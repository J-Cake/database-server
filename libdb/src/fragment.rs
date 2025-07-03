use crate::rw::FragmentDescriptor;
use crate::rw::Pointer;
use crate::rw::RWFragmentStore;
use crate::rw::RWFragmentStoreIndex;
use crate::rw::PAGE_SIZE;
use crate::FragmentID;
use std::io::{BufReader, BufWriter, Cursor};
use std::io::Error;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;

pub trait Buffer: Read + Write + Seek {}

impl<T> Buffer for T where T: Read + Write + Seek {}

impl<Backing: Read + Write + Seek> RWFragmentStore<Backing> {
    pub fn new_fragment(&mut self, options: impl Into<AllocOptions>) -> crate::error::Result<FragmentHandle<Backing>> {
        let opt = options.into();

        let (frag, seq) = self.next_frag_and_seq(opt.fragment);

        match opt.size_hint {
            SizeHint::Sized(size) => {
                let (ptr, size) = self.header.allocate_fragment(size)?;
                Ok(FragmentHandle {
                    index: self,

                    id: frag,
                    sequence: seq,

                    fragment_type: FragmentType::Sized(SizedFragment {
                        max_size: Some(size.next_multiple_of(PAGE_SIZE as u64)),
                        cursor: 0,
                        ptr,
                        size,
                    }),
                })
            }
            SizeHint::Growable => Ok(FragmentHandle {
                index: self,

                id: frag,
                sequence: seq,

                fragment_type: FragmentType::Dynamic(DynamicFragment {
                    buffer_threshold: PAGE_SIZE as u64,
                    buffer: InlineBuffer::Buffered(Cursor::new(vec![0; PAGE_SIZE])),
                }),
            }),
        }
    }

    fn next_fragment_id(&mut self) -> FragmentID {
        self.header
            .fragment_table()
            .max_by(|a, b| a.id.cmp(&b.id))
            .map(|frag| frag.id)
            .unwrap_or(1)
    }

    fn next_frag_and_seq(&mut self, frag: Option<FragmentID>) -> (FragmentID, u64) {
        let frag = frag.unwrap_or(self.next_fragment_id());
        let seq = self
            .header
            .fragment_table()
            .filter(|i| i.id == frag)
            .max_by(|a, b| a.sequence.cmp(&b.sequence))
            .map(|frag| frag.sequence)
            .unwrap_or(0);

        (frag, seq + 1)
    }
}

#[derive(Default)]
pub struct AllocOptions {
    size_hint: SizeHint,
    fragment: Option<FragmentID>,
}

#[derive(Default)]
pub enum SizeHint {
    Sized(u64),
    #[default]
    Growable,
}

impl AllocOptions {
    pub fn size_hint(mut self, size: u64) -> Self {
        self.size_hint = SizeHint::Sized(size);
        return self;
    }

    pub fn growable(mut self) -> Self {
        self.size_hint = SizeHint::Growable;
        return self;
    }

    pub fn fragment(mut self, fragment: FragmentID) -> Self {
        self.fragment = Some(fragment);
        return self;
    }
}

#[derive(Debug)]
pub struct FragmentHandle<'a, Backing: Buffer> {
    pub(crate) fragment_type: FragmentType,

    pub id: FragmentID,
    pub(crate) sequence: u64,

    pub(crate) index: &'a mut RWFragmentStore<Backing>,
}

#[derive(Debug, Clone)]
pub(crate) enum FragmentType {
    ReadOnly(SizedFragment),
    Sized(SizedFragment),
    Dynamic(DynamicFragment),
}

impl<'a, Backing: Buffer> FragmentHandle<'a, Backing> {
    pub fn done(self) -> crate::error::Result<()> {
        std::mem::drop(self);
        Ok(())
    }

    pub fn size(&self) -> usize {
        match self.fragment_type {
            FragmentType::ReadOnly(SizedFragment { size, .. }) | FragmentType::Sized(SizedFragment { size, .. }) => size as usize,
            FragmentType::Dynamic(DynamicFragment { buffer: InlineBuffer::Buffered(ref buf), .. }) => buf.get_ref().len(),
            FragmentType::Dynamic(DynamicFragment { buffer: InlineBuffer::WriteThrough(.., size), .. }) => size as usize,
        }
    }
}

impl<'a, Backing: Buffer> Read for FragmentHandle<'a, Backing> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self.fragment_type {
            FragmentType::ReadOnly(ref mut frag) | FragmentType::Sized(ref mut frag) => {
                let start = self.index.backing.stream_position()?;
                self.index.backing.seek(SeekFrom::Start(frag.ptr + frag.cursor))?;
                let read = self.index.backing.read(buf)?;
                self.index.backing.seek(SeekFrom::Start(start))?;
                frag.cursor += read as u64;
                Ok(read)
            }
            FragmentType::Dynamic(DynamicFragment {
                buffer: InlineBuffer::Buffered(ref mut cursor), ..
            }) => cursor.read(buf),
            FragmentType::Dynamic(DynamicFragment { buffer: InlineBuffer::WriteThrough(..), .. }) => self.index.backing.read(buf),
        }
    }
}

impl<'a, Backing: Buffer> Write for FragmentHandle<'a, Backing> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let FragmentType::ReadOnly(SizedFragment { size, .. }) = self.fragment_type {
            let alloc = AllocOptions::default()
                .size_hint(size)
                .growable()
                .fragment(self.id);

            self.fragment_type = self.index
                .new_fragment(alloc)
                .map_err(Error::other)?
                .fragment_type
                .clone();
        }

        match self.fragment_type {
            FragmentType::ReadOnly(ref mut frag) => {
                log::trace!("FragmentType is still ReadOnly after write. There's probably something seriously wrong.");
                unreachable!()
            },
            FragmentType::Sized(ref mut frag) => {
                let start = self.index.backing.stream_position()?;
                self.index.backing.seek(SeekFrom::Start(frag.ptr + frag.cursor))?;
                let written = self.index.backing.write(buf)?;
                self.index.backing.seek(SeekFrom::Start(start))?;
                frag.cursor += written as u64;
                Ok(written)
            },
            FragmentType::Dynamic(ref mut frag) => {
                if let InlineBuffer::Buffered(ref mut cursor) = frag.buffer
                    && cursor.get_ref().len() + buf.len() > frag.buffer_threshold as usize {

                    // Switch to write-through mode.
                    // 1. Allocate a new fragment
                    // 2. Copy the buffer over to the fragment
                    // 3. Switch to write-through mode

                    // let (ptr, size) = self.index.header
                    //     .allocate_fragment(cursor.get_ref().len() as u64)
                    //     .map_err(Error::other)?;

                    self.index.backing.write_all(cursor.get_mut())?;
                    frag.buffer = InlineBuffer::WriteThrough(self.index.header.end.next_multiple_of(PAGE_SIZE as u64), (cursor.get_ref().len() + buf.len()) as u64);
                }

                match frag.buffer {
                    InlineBuffer::Buffered(ref mut cursor) => cursor.write(buf),
                    InlineBuffer::WriteThrough(..) => self.index.backing.write(buf),
                }
            },
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let FragmentType::ReadOnly(..) = self.fragment_type {
            log::trace!("Flushing a read-only fragment. This does nothing.");
        }

        self.index.backing.flush()
    }
}

impl<'a, Backing: Buffer> Seek for FragmentHandle<'a, Backing> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match self.fragment_type {
            FragmentType::ReadOnly(ref mut sized) | FragmentType::Sized(ref mut sized) => {
                // 1) decide the upper‐bound for this fragment
                let bound = sized.max_size.unwrap_or(sized.size);

                // 2) build a reusable constructor for our InvalidInput error
                let invalid = || Error::new(ErrorKind::InvalidInput, "seek beyond fragment bounds");

                // 3) compute the *signed* new relative position inside [0..=bound]
                let new_rel_i64 = match pos {
                    SeekFrom::Start(off) => {
                        // Try to convert the u64 → i64
                        i64::try_from(off).map_err(|_| invalid())?
                    }
                    SeekFrom::End(offset) => {
                        // bound as i64 plus the signed offset
                        let b_i64 = i64::try_from(bound).map_err(|_| invalid())?;
                        b_i64.checked_add(offset).ok_or_else(invalid)?
                    }
                    SeekFrom::Current(offset) => (sized.cursor as i64).checked_add(offset).ok_or_else(invalid)?,
                };

                // 4) now reject anything outside [0..=bound]
                if new_rel_i64 < 0 || (new_rel_i64 as u64) > bound {
                    return Err(invalid());
                }

                // 5) commit and return the *relative* offset
                sized.cursor = new_rel_i64 as u64;
                Ok(sized.cursor)
            }

            FragmentType::Dynamic(DynamicFragment { buffer: InlineBuffer::Buffered(ref mut cursor), .. }) => cursor.seek(pos),
            FragmentType::Dynamic(DynamicFragment { buffer: InlineBuffer::WriteThrough(..), .. }) => self.index.backing.seek(pos),
        }
    }
}

impl<'a, Backing: Buffer> Drop for FragmentHandle<'a, Backing> {
    fn drop(&mut self) {
        self.flush().expect("Failed to flush");

        match self.fragment_type {
            FragmentType::Dynamic(DynamicFragment { buffer: InlineBuffer::Buffered(ref mut buf), .. }) => {
                let (ptr, size) = self
                    .index
                    .header
                    .allocate_fragment(buf.get_ref().len() as u64)
                    .expect("Closing fragment failed. The fragment was not written.");

                self.index
                    .backing
                    .seek(SeekFrom::Start(ptr))
                    .expect("Failed to flush dynamic fragment");
                self.index
                    .backing
                    .write_all(buf.get_ref())
                    .expect("Failed to flush dynamic fragment");
                self.index
                    .header
                    .push_fragment_descriptor(FragmentDescriptor {
                        id: self.id,
                        sequence: self.sequence,
                        offset: ptr,
                        length: size,
                    })
                    .expect("Closing fragment failed. The database is in a corrupt state.");
            }
            FragmentType::Dynamic(DynamicFragment { buffer: InlineBuffer::WriteThrough(ptr, size), .. }) => self
                .index
                .header
                .push_fragment_descriptor(FragmentDescriptor {
                    id: self.id,
                    sequence: self.sequence,
                    offset: ptr,
                    length: size,
                })
                .expect("Closing fragment failed. The database is in a corrupt state."),
            _ => {}
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct SizedFragment {
    pub(crate) cursor: u64,

    pub(crate) ptr: Pointer,
    pub(crate) size: u64,
    pub(crate) max_size: Option<u64>,
}

/// The dynamic fragment is a continuously-growable writable section of the backing buffer. Its job is to permit one to write an unknown amount of data to the database.
/// It will attempt to buffer as much as it possibly can in order to reutilise free space in the database. Doing this "intelligently" requires a compromise between accuracy and memory-usage.
/// A threshold is defined, which, when exceeded, will allocate a fragment at the database's end, and will continue writing into that fragment, until the writer is dropped.
/// If the write is finished before the threshold is exceeded, a fragment as close to the volume of buffered data as possible will be allocated and populated.
#[derive(Debug, Clone)]
pub(crate) struct DynamicFragment {
    buffer_threshold: u64,

    buffer: InlineBuffer,
}

#[derive(Debug, Clone)]
enum InlineBuffer {
    Buffered(Cursor<Vec<u8>>),
    WriteThrough(Pointer, u64),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_matches::assert_matches;
    use std::io::Cursor;
    use std::io::Result;

    #[test]
    pub fn test_reposition_seek() -> Result<()> {
        let mut backing = Cursor::new(vec![0; 1024]);

        let mut backing = RWFragmentStore::blank(&mut backing).map_err(Error::other)?;

        {
            let mut fragment = FragmentHandle {
                index: &mut backing,

                id: 1,
                sequence: 1,

                fragment_type: FragmentType::Sized(SizedFragment {
                    cursor: 0,
                    ptr: 256,
                    size: 100,
                    max_size: None,
                })
            };

            assert_matches!(fragment.stream_position(), Ok(0));
            assert_matches!(fragment.seek(SeekFrom::Current(50)), Ok(50));
            assert_matches!(fragment.seek(SeekFrom::Current(-50)), Ok(0));
            assert!(fragment.seek(SeekFrom::Current(-50)).is_err());
        }

        assert_matches!(backing.backing.stream_position(), Ok(0));

        Ok(())
    }

    #[test]
    pub fn test_write_fragment() -> Result<()> {
        let mut backing = Cursor::new(vec![0; 1024]);

        let mut backing = RWFragmentStore::blank(&mut backing).map_err(Error::other)?;
        
        {
            let mut fragment = FragmentHandle {
                index: &mut backing,

                id: 1,
                sequence: 1,

                fragment_type: FragmentType::Sized(SizedFragment {
                    cursor: 0,
                    ptr: 256,
                    size: 100,
                    max_size: None,
                }),
            };

            fragment.write_all(b"hello world")?;
            fragment.flush()?;
        }

        assert_eq!(backing.backing.get_ref()[256..267], *b"hello world");

        Ok(())
    }

    #[test]
    pub fn test_dynamic_fragment() -> crate::error::Result<()> {
        let mut store = RWFragmentStore::new(Cursor::new(vec![0; 1024]))?;

        {
            let mut frag = store.new_fragment(AllocOptions::default().size_hint(100))?;

            assert!(frag.write_all(&[0u8; 101]).is_err());
            assert!(frag.write_all(b"Hello World!").is_ok());
        }

        Ok(())
    }
}
