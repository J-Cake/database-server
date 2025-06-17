use std::io::{self, Read, Write, Seek, SeekFrom};
use crate::Result;

pub struct BoundedSection<'a, T: Read + Write + Seek> {
    inner: &'a mut T,
    base_offset: u64,
    max_len: u64,
    cursor: u64,
}

impl<'a, T: Read + Write + Seek> BoundedSection<'a, T> {
    pub fn new(inner: &'a mut T, base_offset: u64, max_len: u64) -> Result<Self> {
        inner.seek(SeekFrom::Start(base_offset))?;
        Ok(Self {
            inner,
            base_offset,
            max_len,
            cursor: 0,
        })
    }

    fn clamp_len(&self, len: usize) -> usize {
        let remaining = self.max_len.saturating_sub(self.cursor);
        len.min(remaining as usize)
    }
}

impl<'a, T: Read + Write + Seek> Read for BoundedSection<'a, T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = self.clamp_len(buf.len());
        let read = self.inner.read(&mut buf[..len])?;
        self.cursor += read as u64;
        Ok(read)
    }
}

impl<'a, T: Read + Write + Seek> Write for BoundedSection<'a, T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let len = self.clamp_len(buf.len());
        let written = self.inner.write(&buf[..len])?;
        self.cursor += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<'a, T: Read + Write + Seek> Seek for BoundedSection<'a, T> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let new_cursor = match pos {
            SeekFrom::Start(n) => n,
            SeekFrom::End(n) => {
                let end = self.max_len as i64;
                (end + n).try_into().map_err(|_| io::ErrorKind::InvalidInput)?
            }
            SeekFrom::Current(n) => {
                let cur = self.cursor as i64;
                (cur + n).try_into().map_err(|_| io::ErrorKind::InvalidInput)?
            }
        };

        if new_cursor > self.max_len {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Seek beyond fragment"));
        }

        self.inner.seek(SeekFrom::Start(self.base_offset + new_cursor))?;
        self.cursor = new_cursor;
        Ok(self.cursor)
    }
}
