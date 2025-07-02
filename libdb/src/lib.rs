#![feature(btree_cursors)]
#![feature(exact_size_is_empty)]
#![feature(sized_hierarchy)]
#![feature(assert_matches)]
extern crate core;

use crate::rw::RWFragmentStore;
use error::*;
use std::io::{Read, Seek, Write};
use std::time::SystemTime;

pub mod error;
mod rw;
pub mod store;
mod fragment;

#[derive(Debug)]
pub struct Database<Backing: Read + Write + Seek> {
    data_source: RWFragmentStore<Backing>,
}

impl<Backing: Read + Write + Seek> Database<Backing> {
    #[inline]
    pub fn new(backing: Backing) -> Result<Self> {
        Ok(Self { data_source: RWFragmentStore::new(backing)? })
    }

    /// Populates the backing buffer with the initial data, completely overwriting any existing data.
    ///
    /// **Please use this function extremely carefully.**
    ///
    pub fn destructive_reinitialise(mut backing: Backing, _danger: Danger) -> Result<()> {
        log::warn!("Destructively reinitialising database.");
        RWFragmentStore::blank(&mut backing)?;

        Ok(())
    }
    
    // pub fn new_fragment(&'_ mut self) -> Result<FragmentHandle<'_, Backing>> {
    //     self.backing.new_fragment(AllocOptions::default())
    // }

    /// Provides low-level access to the underlying backing object. **Not recommended for daily use**.
    pub fn data_source(&self) -> &RWFragmentStore<Backing> {
        &self.data_source
    }

    /// Provides low-level access to the underlying backing object. **Not recommended for daily use**.
    pub fn data_source_mut(&mut self) -> &mut RWFragmentStore<Backing> {
        &mut self.data_source
    }

    pub fn backing(&self) -> &Backing {
        &self.data_source.backing
    }

    pub fn backing_mut(&mut self) -> &mut Backing {
        &mut self.data_source.backing
    }

    pub fn flush(&mut self) -> Result<()> {
        Ok(self.data_source.backing.flush()?)
    }

    pub fn open_fragment(&mut self, id: FragmentID) -> Result<FragmentHandle<'_, Backing>> {
        self.data_source.open_fragment(id)
    }
}

pub type FragmentID = u64;

pub struct Fragment {
    id: FragmentID,
    hash: FragmentHash,
    timestamp: SystemTime,
    sequence: u64,
    data: Vec<u8>,
}

impl Fragment {
    pub fn validate_hash(self) -> Result<Self> {
        log::warn!("hash not verified - not implemented");
        Ok(self)
    }

    pub(crate) fn compute_hash(&self) -> Result<[u8; 32]> {
        log::warn!("hash not verified - not implemented");
        Ok([0u8; 32])
    }
}

pub type FragmentHash = [u8; 32];

pub type UnixTimeMs = u64;

pub enum Value {
    // Indicates a deleted value
    Tombstone,

    // Indicates an empty or null value
    Nothing,

    // Raw bytes for the user to interpret
    Blob(Vec<u8>),

    // A collection of other fragments
    Collection { expected_length: u64, continuation: Option<FragmentID>, page: Vec<FragmentID> },
}

pub struct Danger;

pub use fragment::AllocOptions;
pub use crate::fragment::FragmentHandle;
use crate::store::FragmentStore;