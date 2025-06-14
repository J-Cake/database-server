use crate::error::FragmentError;
use crate::error::Result;
use crate::store::FragmentStore;
use crate::Fragment;
use crate::FragmentID;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::time::Duration;
use std::time::SystemTime;

pub trait Storage
where
    Self: Sized, {
    /// Read a value from a buffer.
    /// Assumes the buffer's cursor is to the beginning of the structure it aims to read. May fail silently if not.
    fn read<Buffer: Read + Seek>(buffer: Buffer) -> Result<Self>;
    fn write<Buffer: Write + Seek>(self) -> Result<()>;
}

pub trait KnownSize {
    fn size() -> usize;
}

const FRAG_MAGIC: [u8; 4] = *b"FRAG";

impl Storage for Fragment {
    fn read<Backing: Read + Seek>(mut source: Backing) -> Result<Self> {
        // Binary Layout
        // 0-4 [4B]: magic
        // 4-12 [8B]: id
        // 12-20 [8B]: sequence
        // 20-52 [32B]: hash
        // 52-60 [8B]: timestamp
        // 60-68 [8B]: capacity
        // 68-76 [8B]: length
        let mut buffer = vec![0u8; Fragment::size()];
        source.read_exact(&mut buffer)?;

        if buffer[0..4] != FRAG_MAGIC {
            return Err(FragmentError::InvalidMagic.into());
        }

        let (cap, len) = (u64::from_le_bytes(buffer[60..68].try_into()?), u64::from_le_bytes(buffer[68..76].try_into()?));

        if len > cap {
            return Err(FragmentError::LengthExceedsCapacity.into());
        }

        let mut data = vec![0u8; cap as usize];
        source.read_exact(&mut data)?;
        data.truncate(len as usize);

        Fragment {
            id: FragmentID::from_le_bytes(buffer[4..12].try_into()?),
            sequence: u64::from_le_bytes(buffer[12..20].try_into()?),
            hash: buffer[20..52].try_into()?,
            timestamp: SystemTime::UNIX_EPOCH + Duration::from_millis(u64::from_le_bytes(buffer[52..60].try_into()?)),
            data,
        }
        .validate_hash()
    }

    fn write<Backing: Write + Seek>(self) -> Result<()> {
        todo!()
    }
}

impl KnownSize for Fragment {
    fn size() -> usize {
        84
    }
}

pub struct RWFragmentStore<Backing: Read + Write + Seek> {
    backing: Backing,
    header: RWFragmentStoreIndex,
    fragment_table: Vec<FragmentDescriptor>,
}

impl<Backing: Read + Write + Seek> FragmentStore for RWFragmentStore<Backing> {
    fn read_fragment(&mut self, id: FragmentID) -> Result<Fragment> {
        if let Some(frag) = self
            .fragment_table
            .iter()
            .filter(|frag| frag.id == id)
            .max_by(|i, j| i.sequence.cmp(&j.sequence))
        {
            self.backing.seek(SeekFrom::Start(frag.offset))?;
            Fragment::read(&mut self.backing)
        } else {
            FragmentError::not_found(id)
        }
    }

    fn write_fragment(&mut self, fragment: impl AsRef<Fragment>) -> Result<()> {
        todo!()
    }
}

impl<Backing: Read + Write + Seek> RWFragmentStore<Backing> {
    pub fn new(mut backing: Backing) -> Result<Self> {
        Ok(Self {
            header: RWFragmentStoreIndex::read(&mut backing)?,
            fragment_table: RWFragmentStore::<Backing>::read_fragment_table(&mut backing)?,

            backing,
        })
    }

    fn update_fragment_table(&mut self) -> Result<()> {
        self.fragment_table = Self::read_fragment_table(&mut self.backing)?;

        Ok(())
    }

    fn read_fragment_table<Buf: Read + Seek>(mut backing: Buf) -> Result<Vec<FragmentDescriptor>> {
        backing.seek(SeekFrom::Current(16))?;
        let mut offset = [0u8; 8];
        backing.read_exact(&mut offset)?;

        let mut index = u64::from_le_bytes(offset);
        let mut result = Vec::new();

        loop {
            backing.seek(SeekFrom::Start(index))?;
            let part = FragmentTablePart::read(&mut backing)?;

            result.extend(part.fragments);

            if part.continuation != 0 {
                index = part.continuation;
            } else {
                return Ok(result);
            }
        }
    }
}

const RWFS_MAGIC: [u8; 4] = *b"RWFS";

pub struct RWFragmentStoreIndex {
    version: u32,
    root_fragment: FragmentID,
    fragment_table: Pointer,
}

impl KnownSize for RWFragmentStoreIndex {
    fn size() -> usize {
        24
    }
}

impl Storage for RWFragmentStoreIndex {
    fn read<Backing: Read + Seek>(mut source: Backing) -> Result<Self> {
        source.seek(SeekFrom::Start(0))?;
        let mut buffer = vec![0u8; 24];
        source.read_exact(&mut buffer)?;

        if buffer[0..4] == RWFS_MAGIC {
            Ok(Self {
                version: u32::from_le_bytes(buffer[4..8].try_into()?),
                root_fragment: FragmentID::from_le_bytes(buffer[8..16].try_into()?),
                fragment_table: u64::from_le_bytes(buffer[16..24].try_into()?),
            })
        } else {
            Err(FragmentError::InvalidMagic.into())
        }
    }

    fn write<Backing: Write + Seek>(self) -> Result<()> {
        todo!()
    }
}

pub type Pointer = u64;

pub struct FragmentDescriptor {
    id: FragmentID,
    sequence: u64,
    offset: u64,
    // length comes from the fragment itself
}

impl Storage for FragmentDescriptor {
    fn read<Backing: Read + Seek>(mut source: Backing) -> Result<Self> {
        let mut buffer = vec![0u8; 24];
        source.read_exact(&mut buffer)?;

        Ok(Self {
            id: FragmentID::from_le_bytes(buffer[0..8].try_into()?),
            sequence: u64::from_le_bytes(buffer[8..16].try_into()?),
            offset: u64::from_le_bytes(buffer[16..24].try_into()?),
        })
    }

    fn write<Backing: Write + Seek>(self) -> Result<()> {
        todo!()
    }
}

impl KnownSize for FragmentDescriptor {
    fn size() -> usize {
        24
    }
}

pub struct FragmentTablePart {
    continuation: Pointer, // We'll accept the use of null-pointers here because they're space efficient.
    fragments: Vec<FragmentDescriptor>,
}

impl Storage for FragmentTablePart {
    fn read<Backing: Read + Seek>(mut source: Backing) -> Result<Self> {
        let mut buffer = vec![0u8; 24];
        source.read_exact(&mut buffer)?;

        let (cap, len) = (u64::from_le_bytes(buffer[8..16].try_into()?), u64::from_le_bytes(buffer[16..24].try_into()?));

        if len > cap {
            return Err(FragmentError::LengthExceedsCapacity.into());
        }

        let mut buffer = vec![0u8; cap as usize * FragmentDescriptor::size()];
        source.read_exact(&mut buffer)?;

        let mut fragments = Vec::with_capacity(len as usize);

        for _ in 0..len {
            fragments.push(FragmentDescriptor::read(&mut source)?);
            // source.seek(SeekFrom::Current(FragmentDescriptor::size() as i64))?;
        }

        Ok(Self {
            continuation: u64::from_le_bytes(buffer[0..8].try_into()?),
            fragments,
        })
    }

    fn write<Backing: Write + Seek>(self) -> Result<()> {
        todo!()
    }
}

impl FragmentTablePart {
    pub fn len(&self) -> usize {
        self.fragments.len()
    }

    pub fn cap(&self) -> usize {
        self.fragments.capacity()
    }
}
