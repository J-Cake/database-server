use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use crate::{Fragment, FragmentID};
use crate::error::{FragmentError, Result};
use crate::fragment::{FragmentHandle, FragmentType, ReadonlyFragment, SizedFragment};
use crate::rw::RWFragmentStore;

pub trait FragmentStore<Backing: Read + Write + Seek> {
    fn open_fragment(&mut self, fragment: FragmentID) -> Result<FragmentHandle<'_, Backing>>;
}

impl<Backing: Read + Write + Seek> FragmentStore<Backing> for RWFragmentStore<Backing> {
    fn open_fragment(&'_ mut self, fragment: FragmentID) -> Result<FragmentHandle<'_, Backing>> {
        if let Some(frag) = self.header.fragment_table()
            .filter(|i| i.id == fragment)
            .max_by_key(|i| i.sequence)
            .cloned() {

            Ok(FragmentHandle {
                fragment_type: FragmentType::ReadOnly(ReadonlyFragment(SizedFragment {
                    index: self,

                    fragment: frag.id,
                    sequence: frag.sequence,

                    cursor: 0,
                    ptr: frag.offset,
                    size: frag.length,

                    max_size: Some(0), // Disable writes
                }))
            })
        } else {
            Err(FragmentError::NoFound(fragment).into())
        }
    }
}