#![feature(btree_cursors)]
extern crate core;

use crate::rw::RWFragmentStore;
use error::*;
use std::collections::HashMap;
use std::fs::File;
use std::fs::OpenOptions;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;

pub mod error;
mod rw;
mod rwslice;
mod store;

pub struct Database {
    backing: RWFragmentStore<File>,
}

impl Database {
    pub fn open(index: impl AsRef<Path>) -> Result<Self> {
        if index.as_ref().exists() {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(index)?;
            
            Ok(Self { backing: RWFragmentStore::new(file)? })
        } else {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .truncate(true)
                .open(index)?;
            
            Ok(Self { backing: RWFragmentStore::blank(file)? })
        }
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
