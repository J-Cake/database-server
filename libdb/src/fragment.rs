use crate::rw::{FragmentDescriptor, RWFragmentStoreIndex};
use crate::rw::Pointer;
use crate::rw::RWFragmentStore;
use crate::rw::PAGE_SIZE;
use crate::FragmentID;
use std::io::Cursor;
use std::io::Error;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::mem;
use crate::error::global::Inner::FragmentError;

pub trait Buffer: Read + Write + Seek {}

impl<T> Buffer for T where T: Read + Write + Seek {}

impl<Backing: Read + Write + Seek> RWFragmentStore<Backing> {
    pub(crate) fn new_fragment(&mut self, options: impl Into<AllocOptions>) -> crate::error::Result<FragmentHandle<Backing>> {
        let opt = options.into();

        let (frag, seq) = self.next_frag_and_seq(opt.fragment);

        match opt.size_hint {
            SizeHint::Sized(size) => {
                let (ptr, size) = self.header.allocate_fragment(size)?;
                Ok(FragmentHandle {
                    fragment_type: FragmentType::Sized(SizedFragment {
                        index: self,
                        max_size: Some(size.next_multiple_of(PAGE_SIZE as u64)),
                        cursor: 0,
                        fragment: frag,
                        sequence: seq,
                        ptr,
                        size,
                    })
                })
            },
            SizeHint::Growable => {
                Ok(FragmentHandle {
                    fragment_type: FragmentType::Dynamic(DynamicFragment {
                        index: self,
                        buffer_threshold: PAGE_SIZE as u64,
                        fragment: frag,
                        sequence: seq,
                        buffer: InlineBuffer::Buffered(Cursor::new(vec![0; PAGE_SIZE])),
                    })
                })
            }
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
        let seq = self.header.fragment_table()
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
    pub(crate) fragment_type: FragmentType<'a, Backing>,
}

#[derive(Debug)]
pub(crate) enum FragmentType<'a, Backing: Buffer> {
    ReadOnly(ReadonlyFragment<'a, Backing>),
    Sized(SizedFragment<'a, Backing>),
    Dynamic(DynamicFragment<'a, Backing>),
}

impl<'a, Backing: Buffer> FragmentHandle<'a, Backing> {
    pub fn done(self) -> crate::error::Result<()> {
        std::mem::drop(self);
        Ok(())
    }

    fn check_if_readonly_and_copy_if_necessary(&mut self, buf: &[u8]) -> std::io::Result<Option<usize>> {
        // TODO! Implement Copy-on-Write
        todo!()

        // let new = if let FragmentType::ReadOnly(ref mut frag) = self.fragment_type {
        //     log::trace!("Writing to a read-only fragment. Copying to a new writable fragment.");
        //
        //     let offset = frag.stream_position().unwrap_or(0);
        //     let mut new = frag.0.index.new_fragment(AllocOptions::default().size_hint(buf.len() as u64))
        //         .map_err(Error::other)?;
        //
        //     let FragmentType::Sized(sized) = &new.fragment_type else {
        //         return Err(Error::other("Expected a sized fragment"));
        //     };
        //
        //     {
        //         let mut buf = Box::new([0u8; 1024 * 1024]);
        //         while let Ok(n) = frag.read(&mut buf[..]) && n > 0 {
        //             new.write_all(&buf[..n])?;
        //         }
        //     }
        //
        //     new.seek(SeekFrom::Start(offset))?;
        //     FragmentDescriptor {
        //         id: sized.fragment,
        //         sequence: sized.sequence,
        //         offset: sized.ptr,
        //         length: sized.size,
        //     }
        // } else {
        //     return Ok(None)
        // };
        //
        // self.fragment_type = FragmentType::Sized(SizedFragment {
        //     index: match self.fragment_type,
        //     fragment: 0,
        //     sequence: 0,
        //     cursor: 0,
        //     ptr: 0,
        //     size: 0,
        //     max_size: None,
        // });
        //
        // Ok(Some(new.write(buf)?))
    }
}

impl<'a, Backing: Buffer> Read for FragmentHandle<'a, Backing> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self.fragment_type {
            FragmentType::ReadOnly(ref mut frag) => frag.read(buf),
            FragmentType::Sized(ref mut frag) => frag.read(buf),
            FragmentType::Dynamic(ref mut frag) => frag.read(buf),
        }
    }
}

impl<'a, Backing: Buffer> Write for FragmentHandle<'a, Backing> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(len) = self.check_if_readonly_and_copy_if_necessary(buf) {
            return Ok(len);
        }

        match self.fragment_type {
            FragmentType::ReadOnly(ref mut frag) => unreachable!(),
            FragmentType::Sized(ref mut frag) => frag.write(buf),
            FragmentType::Dynamic(ref mut frag) => frag.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self.fragment_type {
            FragmentType::ReadOnly(..) => {
                log::trace!("Flushing a read-only fragment. This does nothing.");
                Ok(())
            },
            FragmentType::Sized(ref mut frag) => frag.flush(),
            FragmentType::Dynamic(ref mut frag) => frag.flush(),
        }
    }
}

impl<'a, Backing: Buffer> Seek for FragmentHandle<'a, Backing> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match self.fragment_type {
            FragmentType::ReadOnly(ref mut frag) => frag.0.seek(pos),
            FragmentType::Sized(ref mut frag) => frag.seek(pos),
            FragmentType::Dynamic(ref mut frag) => frag.seek(pos),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ReadonlyFragment<'a, Backing: Buffer> (pub(crate) SizedFragment<'a, Backing>);

impl<'a, Backing: Read + Write + Seek> Read for ReadonlyFragment<'a, Backing> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl<'a, Backing: Read + Write + Seek> Seek for ReadonlyFragment<'a, Backing> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.0.seek(pos)
    }
}

#[derive(Debug)]
pub(crate) struct SizedFragment<'a, Backing: Buffer> {
    pub(crate) index: &'a mut RWFragmentStore<Backing>,

    pub(crate) fragment: FragmentID,
    pub(crate) sequence: u64,

    pub(crate) cursor: u64,

    pub(crate) ptr: Pointer,
    pub(crate) size: u64,
    pub(crate) max_size: Option<u64>,
}

impl<'a, Backing: Read + Write + Seek> Read for SizedFragment<'a, Backing> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let start = self.index.backing.stream_position()?;
        self.index.backing.seek(SeekFrom::Start(self.ptr + self.cursor))?;
        let read = self.index.backing.read(buf)?;
        self.index.backing.seek(SeekFrom::Start(start))?;
        Ok(read)
    }
}

impl<'a, Backing: Read + Write + Seek> Write for SizedFragment<'a, Backing> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let start = self.index.backing.stream_position()?;
        self.index.backing.seek(SeekFrom::Start(self.ptr + self.cursor))?;
        let written = self.index.backing.write(buf)?;
        self.index.backing.seek(SeekFrom::Start(start))?;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        // Question: Should this function be used to commit the results to the database?
        // The database treats commits as a finalisation over a write action, meaning the fragment should not be overwritten.
        self.index.backing.flush()
    }
}

impl<'a, Backing: Read + Write + Seek> Seek for SizedFragment<'a, Backing> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        // 1) decide the upper‐bound for this fragment
        let bound = self.max_size.unwrap_or(self.size);

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
                b_i64
                    .checked_add(offset)
                    .ok_or_else(invalid)?
            }
            SeekFrom::Current(offset) => {
                (self.cursor as i64)
                    .checked_add(offset)
                    .ok_or_else(invalid)?
            }
        };

        // 4) now reject anything outside [0..=bound]
        if new_rel_i64 < 0 || (new_rel_i64 as u64) > bound {
            return Err(invalid());
        }

        // 5) commit and return the *relative* offset
        self.cursor = new_rel_i64 as u64;
        Ok(self.cursor)
    }
}

/// The dynamic fragment is a continuously-growable writable section of the backing buffer. Its job is to permit one to write an unknown amount of data to the database.
/// It will attempt to buffer as much as it possibly can in order to reutilise free space in the database. Doing this "intelligently" requires a compromise between accuracy and memory-usage.
/// A threshold is defined, which, when exceeded, will allocate a fragment at the database's end, and will continue writing into that fragment, until the writer is dropped.
/// If the write is finished before the threshold is exceeded, a fragment as close to the volume of buffered data as possible will be allocated and populated.
#[derive(Debug)]
pub(crate) struct DynamicFragment<'a, Backing: Buffer> {
    index: &'a mut RWFragmentStore<Backing>,
    buffer_threshold: u64,

    fragment: FragmentID,
    sequence: u64,

    buffer: InlineBuffer
}

impl<'a, Backing: Buffer> Read for DynamicFragment<'a, Backing> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self.buffer {
            InlineBuffer::Buffered(ref mut cursor) => cursor.read(buf),
            InlineBuffer::WriteThrough(..) => self.index.backing.read(buf),
        }
    }
}

impl<'a, Backing: Buffer> Write for DynamicFragment<'a, Backing> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {

        if let InlineBuffer::Buffered(ref mut cursor) = self.buffer && cursor.get_ref().len() + buf.len() > self.buffer_threshold as usize {
            // Switch to write-through mode.
            // 1. Allocate a new fragment
            // 2. Copy the buffer over to the fragment
            // 3. Switch to write-through mode

            // let (ptr, size) = self.index.header
            //     .allocate_fragment(cursor.get_ref().len() as u64)
            //     .map_err(Error::other)?;

            self.index.backing.write_all(cursor.get_mut())?;
            self.buffer = InlineBuffer::WriteThrough(self.index.header.end.next_multiple_of(PAGE_SIZE as u64), (cursor.get_ref().len() + buf.len()) as u64);
        }

        match self.buffer {
            InlineBuffer::Buffered(ref mut cursor) => cursor.write(buf),
            InlineBuffer::WriteThrough(..) => self.index.backing.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self.buffer {
            InlineBuffer::Buffered(ref mut cursor) => cursor.flush(),
            InlineBuffer::WriteThrough(..) => self.index.backing.flush(),
        }
    }
}

impl<'a, Backing: Buffer> Seek for DynamicFragment<'a, Backing> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match self.buffer {
            InlineBuffer::Buffered(ref mut cursor) => cursor.seek(pos),
            InlineBuffer::WriteThrough(..) => self.index.backing.seek(pos),
        }
    }
}

impl<'a, Backing: Buffer> Drop for DynamicFragment<'a, Backing> {
    fn drop(&mut self) {
        match &self.buffer {
            InlineBuffer::Buffered(buf) => {
                let (ptr, size) = self.index.header
                    .allocate_fragment(buf.get_ref().len() as u64)
                    .expect("Closing fragment failed. The fragment was not written.");

                self.index.backing.seek(SeekFrom::Start(ptr)).expect("Failed to flush dynamic fragment");
                self.index.backing.write_all(buf.get_ref()).expect("Failed to flush dynamic fragment");
                self.index.header.push_fragment_descriptor(FragmentDescriptor {
                    id: self.fragment,
                    sequence: self.sequence,
                    offset: ptr,
                    length: size,
                }).expect("Closing fragment failed. The database is in a corrupt state.");
            },
            InlineBuffer::WriteThrough(ptr, size) => self.index.header.push_fragment_descriptor(FragmentDescriptor {
                id: self.fragment,
                sequence: self.sequence,
                offset: *ptr,
                length: *size,
            }).expect("Closing fragment failed. The database is in a corrupt state."),
        }
    }
}

#[derive(Debug)]
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

        let mut backing = RWFragmentStore::blank(&mut backing)
            .map_err(Error::other)?;

        {
            let mut fragment = SizedFragment {
                index: &mut backing,
                fragment: 1,
                sequence: 1,
                cursor: 0,
                ptr: 256,
                size: 100,
                max_size: None,
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

        let mut backing = RWFragmentStore::blank(&mut backing)
            .map_err(Error::other)?;

        let mut fragment = SizedFragment {
            index: &mut backing,
            fragment: 1,
            sequence: 1,
            cursor: 0,
            ptr: 256,
            size: 100,
            max_size: None,
        };

        fragment.write_all(b"hello world")?;
        fragment.flush()?;

        assert_eq!(backing.backing.get_ref()[256..267], *b"hello world");

        Ok(())
    }

    #[test]
    pub fn test_dynamic_fragment() -> crate::error::Result<()> {
        let mut store = RWFragmentStore::new(Cursor::new(vec![0; 1024]))?;
        
        {
            let mut frag = store.new_fragment(AllocOptions::default()
                .size_hint(100))?;

            assert!(frag.write_all(&[0u8; 101]).is_err());
            assert!(frag.write_all(b"Hello World!").is_ok());
        }

        Ok(())
    }
}