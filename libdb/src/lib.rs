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
    backing: RWFragmentStore<Backing>,
}

impl<Backing: Read + Write + Seek> Database<Backing> {
    #[inline]
    pub fn new(backing: Backing) -> Result<Self> {
        Ok(Self { backing: RWFragmentStore::new(backing)? })
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
    pub fn backing(&self) -> &RWFragmentStore<Backing> {
        &self.backing
    }

    /// Provides low-level access to the underlying backing object. **Not recommended for daily use**.
    pub fn backing_mut(&mut self) -> &mut RWFragmentStore<Backing> {
        &mut self.backing
    }

    pub fn flush(&mut self) -> Result<()> {
        Ok(self.backing.backing.flush()?)
    }
    
    pub fn open_fragment(&mut self, id: FragmentID) -> Result<FragmentHandle<'_, Backing>> {
        self.backing.open_fragment(id)
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
use crate::fragment::FragmentHandle;
use crate::store::FragmentStore;