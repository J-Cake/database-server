#![feature(btree_cursors)]
extern crate core;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use error::*;

pub mod error;
mod store;
mod rw;

pub struct Database {
    index: Index
}

impl Database {
    pub fn open(index: impl AsRef<Path>) -> Result<Self> {
        let index = Index {
            magic: 0,
            version: 0,
            name: "".to_string(),
            fragments: Default::default(),
        };

        Ok(Self { index })
    }
}

pub struct Index {
    magic: u32,
    version: u32,
    name: String,
    fragments: HashMap<FragmentID, PathBuf>
}

pub type FragmentID = u32;

pub struct Fragment {
    id: FragmentID,
    hash: FragmentHash,
    timestamp: SystemTime,
    sequence: u64,
    data: Vec<u8>
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
    Collection {
        expected_length: u64,
        continuation: Option<FragmentID>,

        page: Vec<FragmentID>
    }
}