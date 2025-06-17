use std::io::{Read, Seek, Write};
use std::path::PathBuf;
use crate::{Fragment, FragmentID};
use crate::error::{FragmentError, Result};

pub trait FragmentStore {

    fn read_fragment(&mut self, id: FragmentID) -> Result<Fragment>;
    fn write_fragment(&mut self, fragment: impl AsRef<Fragment>) -> Result<()>;

}
