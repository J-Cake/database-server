use std::io::{Read, Seek, Write};
use crate::FragmentID;
use crate::rw::{Pointer, RWFragmentStore};
use crate::rwslice::BoundedSection;

impl<Backing: Read + Write + Seek> RWFragmentStore<Backing> {
    // fn alloc_fragment(&mut self, options: impl Into<AllocOptions>) -> Result<WritableFragment<Backing>> {
    //     // 1. At load time, the DB will analyse all leased spaces and create a map of available space.
    //     // 2. When attempting to write a chunk, the database's append-only nature emerges. Using an optional size-hint, the database queries the list of free spaces for a size that could fit.
    //     // 3. After locating a space, the database checks that it really is free (because between load and use, the space may have been occupied.
    //     // 4. This is confirmed by querying the fragment table for the previous fragment.
    //     // 5. If the space is available, then it is returned to the caller.
    //     // 6. If its length does not match the expectation from the space map, a state cache condition is raised, and the database will be reindexed.
    //
    //     let options = options.into();
    //
    //     match options.size_hint {
    //         SizeHint::Sized(size) => {
    //             let size = size.next_multiple_of(PAGE_SIZE as u64);
    //
    //             let (frag, seq) = match options.fragment {
    //                 Some(id) => self
    //                     .header
    //                     .fragment_table
    //                     .iter()
    //                     .filter(|frag| frag.id == id)
    //                     .max_by(|a, b| a.sequence.cmp(&b.sequence))
    //                     .map(|frag| (frag.id, frag.sequence))
    //                     .unwrap_or((self.next_fragment_id(), 0)),
    //                 _ => (self.next_fragment_id(), 0),
    //             };
    //
    //             for (&size, pointers) in self.header.free_space.range_mut(size..) {
    //                 if let Some(ptr) = pointers.pop() {
    //                     return Ok(WritableFragment {
    //                         buf: &mut self.backing,
    //                         fragment: 0,
    //                         sequence: 0,
    //                         ptr,
    //                         size,
    //                     });
    //                 }
    //             }
    //         }
    //         SizeHint::Growable => todo!(),
    //     }
    //
    //     todo!()
    // }

    fn alloc_fragment(&mut self, options: impl Into<AllocOptions>) -> crate::error::Result<WritableFragment<Backing>> {
        
    }
    
    fn next_fragment_id(&mut self) -> FragmentID {
        self.header
            .fragment_table()
            .max_by(|a, b| a.id.cmp(&b.id))
            .map(|frag| frag.id)
            .unwrap_or(1)
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

pub struct WritableFragment<'a, Backing: Read + Write + Seek> {
    buf: &'a mut Backing,

    pub fragment: FragmentID,
    pub sequence: u64,

    ptr: Pointer,
    size: u64,
}

impl<'a, Backing: Read + Write + Seek> WritableFragment<'a, Backing> {
    pub fn blob(&mut self) -> crate::error::Result<BoundedSection<'_, Backing>> {
        BoundedSection::new(self.buf, self.ptr, self.size)
    }

    pub fn commit(self, index: &mut RWFragmentStore<Backing>) -> crate::error::Result<()> {
        index.header.push_fragment(crate::rw::FragmentDescriptor {
            id: self.fragment,
            sequence: self.sequence,
            offset: self.ptr,
            length: self.size,
        })?;

        Ok(())
    }
}