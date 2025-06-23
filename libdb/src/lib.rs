#![feature(btree_cursors)]
#![feature(exact_size_is_empty)]
#![feature(sized_hierarchy)]
extern crate core;

use crate::rw::RWFragmentStore;
use error::*;
use std::collections::HashMap;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::{Read, Seek, Write};
use std::marker::PhantomData;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;

pub mod error;
mod rw;
mod rwslice;
mod store;

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
        RWFragmentStore::blank(&mut backing)?;

        Ok(())
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