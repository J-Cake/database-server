use std::collections::{BTreeMap, BTreeSet, Bound};
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

const PAGE_SIZE: usize = 4096;

pub trait Storage
where
    Self: Sized, {
    /// Read a value from a buffer.
    /// Assumes the buffer's cursor is set to the beginning of the structure it aims to read. May fail silently if not.
    fn read<Buffer: Read + Seek>(buffer: Buffer) -> Result<Self>;
    fn write<Buffer: Write + Seek>(&self, buffer: Buffer) -> Result<()>;
}

pub trait KnownSize {
    fn size() -> usize;
}

const FRAG_MAGIC: [u8; 4] = *b"FRAG";

/// Implements binary serialization for the `Fragment` type.
///
/// This layout is used to persist a fragment and reconstruct it from storage.
/// It assumes that the buffer is already correctly seeked to the beginning of a fragment.
///
/// # Binary Layout (Little-Endian)
/// ```text
/// Offset  Size     Field
/// -------------------------------
/// 0       4 B      Magic number ("FRAG")
/// 4       8 B      Fragment ID
/// 12      8 B      Sequence number
/// 20      32 B     Hash (e.g., Blake3)
/// 52      8 B      Timestamp (ms since UNIX_EPOCH)
/// 60      8 B      Capacity (allocated bytes)
/// 68      8 B      Length (actual data bytes)
/// 76+     N B      Fragment data (up to `capacity`)
/// ```
///
/// Notes:
/// - Magic number is checked for early validation.
/// - The hash is expected to match the actual content of the fragment data.
/// - If `length > capacity`, deserialization will fail.
/// - The `read` implementation allocates `capacity` but truncates to `length` to avoid reading undefined bytes.
///
/// This format balances safety and space-efficiency but relies on external logic
/// to ensure correct buffer positioning and integrity guarantees.
impl Storage for Fragment {
    fn read<Backing: Read + Seek>(mut source: Backing) -> Result<Self> {
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

    fn write<Backing: Write + Seek>(&self, source: Backing) -> Result<()> {
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
}

impl<Backing: Read + Write + Seek> FragmentStore for RWFragmentStore<Backing> {
    fn read_fragment(&mut self, id: FragmentID) -> Result<Fragment> {
        if let Some(frag) = self
            .header
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
            backing,
        })
    }
}

const RWFS_MAGIC: [u8; 4] = *b"RWFS";

/// Represents the root index for the database.
///
/// This structure holds metadata required to locate and access the core components
/// of the fragment store, including the root fragment and the fragment table.
///
/// # Binary Layout
/// ```text
/// Offset  Size    Field
/// -----------------------------
/// 0       4 B     Magic number
/// 4       4 B     Version
/// 8       8 B     Root fragment ID
/// 16      8 B     Fragment table pointer (start of first chunk)
/// ```
///
/// All values are encoded in little-endian format.
struct RWFragmentStoreIndex {
    version: u32,
    root_fragment: FragmentID,
    free_space: BTreeMap<u64, Vec<Pointer>>,
    fragment_table_offset: Pointer,
    fragment_table: Vec<FragmentDescriptor>,
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

        if buffer[0..4] != RWFS_MAGIC {
            return Err(FragmentError::InvalidMagic.into());
        }

        let mut fragment_table = Vec::new();
        let fragment_table_offset = u64::from_le_bytes(buffer[16..24].try_into()?);
        source.seek(SeekFrom::Start(fragment_table_offset))?;

        let root_fragment = FragmentID::from_le_bytes(buffer[8..16].try_into()?);

        loop {
            let mut chunk = FragmentTablePart::read(&mut source)?;
            fragment_table.append(&mut chunk.fragments);

            if chunk.continuation == 0 {
                break;
            }

            source.seek(SeekFrom::Start(chunk.continuation))?;
        }

        let mut free_space = BTreeMap::new();

        /*
        # Algorithm:
        1. The end of the root fragment is manually inserted into the vector
        2. The fragment list is iterated. The previous fragment is determined by
         */

        let mut slots = fragment_table.iter().collect::<Vec<_>>();
        slots.sort_by_key(|slot| slot.offset);

        // Iterate over the slots pair-wise. Their gaps are appended to the `free_space` map.

        for i in slots.windows(2) {
            if let [a, b] = *i {
                let offset = (a.offset + a.length).next_multiple_of(PAGE_SIZE as u64);
        
                if b.offset > offset {
                    let size = (b.offset - offset).next_multiple_of(PAGE_SIZE as u64) - PAGE_SIZE as u64;
        
                    if size > 0 {
                        free_space.entry(size)
                            .or_insert_with(Vec::new)
                            .push(offset);
                    }
                }
            }
        }

        Ok(Self {
            version: u32::from_le_bytes(buffer[4..8].try_into()?),
            root_fragment,
            free_space,
            fragment_table,
            fragment_table_offset,
        })
    }

    fn write<Backing: Write + Seek>(&self, mut source: Backing) -> Result<()> {
        source.seek(SeekFrom::Start(0))?;
        source.write_all(&RWFS_MAGIC)?;
        source.write_all(&self.version.to_le_bytes())?;
        source.write_all(&self.root_fragment.to_le_bytes())?;
        source.write_all(&self.fragment_table_offset.to_le_bytes())?;
        // Let the OS coalesce adjacent writes - no .flush()
        Ok(())
    }
}

impl RWFragmentStoreIndex {
    fn get_writable_chunk(&self, size_hint: Option<u64>) -> Result<Pointer> {
        // 1. At load time, the DB will analyse all leased spaces and create a map of available space.
        // 2. When attempting to write a chunk, the database's append-only nature emerges. Using an optional size-hint, the database queries the list of free spaces for a size that could fit.
        // 3. After locating a space, the database checks that it really is free (because between load and use, the space may have been occupied.
        // 4. This is confirmed by querying the fragment table for the previous fragment.
        // 5. If the space is available, then it is returned to the caller.
        // 6. If its length does not match the expectation from the space map, a state cache condition is raised, and the database will be reindexed.
    }
}

pub type Pointer = u64;

/// Implements binary serialization for a `FragmentDescriptor`.
///
/// A `FragmentDescriptor` acts as a lightweight index entry into a fragment store,
/// pointing to a specific fragment's metadata and offset within a storage medium.
///
/// # Binary Layout (Little-Endian)
/// ```text
/// Offset  Size     Field
/// -------------------------------
/// 0       8 B      Fragment ID
/// 8       8 B      Sequence number
/// 16      8 B      Offset in backing store (e.g. file or block device)
/// 24      8 B      Number of bytes the fragment contains
/// ```
///
/// Notes:
/// - The actual length of the fragment is determined during fragment deserialization,
///   as it is embedded within the fragment structure itself.
/// - This compact format is useful for constructing fragment tables or indexes,
///   and is intentionally fixed-size (24 bytes) for predictable seeking and storage.
///
/// Ensure that the backing buffer is already seeked correctly prior to invoking `read`.
struct FragmentDescriptor {
    id: FragmentID,
    sequence: u64,
    offset: u64,
    length: u64
}

impl Storage for FragmentDescriptor {
    fn read<Backing: Read + Seek>(mut source: Backing) -> Result<Self> {
        let mut buffer = vec![0u8; FragmentDescriptor::size()];
        source.read_exact(&mut buffer)?;

        Ok(Self {
            id: FragmentID::from_le_bytes(buffer[0..8].try_into()?),
            sequence: u64::from_le_bytes(buffer[8..16].try_into()?),
            offset: u64::from_le_bytes(buffer[16..24].try_into()?),
            length: u64::from_le_bytes(buffer[24..32].try_into()?),
        })
    }

    fn write<Backing: Write + Seek>(&self, source: Backing) -> Result<()> {
        todo!()
    }
}

impl KnownSize for FragmentDescriptor {
    fn size() -> usize {
        32
    }
}

/// A part of the fragment table stored in a linked list-like format.
///
/// Each `FragmentTablePart` represents a chunk in a potentially chained sequence
/// that together forms a complete fragment table. This design allows scalable indexing
/// across backing stores where contiguous allocation isn't guaranteed.
///
/// # Binary Layout (Little-Endian)
/// ```text
/// Offset  Size     Field
/// -------------------------------
/// 0       8 B      Continuation pointer (0 if terminal)
/// 8       8 B      Capacity (max number of descriptors in this chunk)
/// 16      8 B      Length (actual number of descriptors stored)
/// 24..    N × 24 B Fragment descriptors (see `FragmentDescriptor`)
/// ```
///
/// Notes:
/// - `continuation` is a pointer to the next `FragmentTablePart`, or 0 to denote the end.
/// - The layout is fixed-size up to offset 24, after which a variable number of
///   `FragmentDescriptor`s are stored.
/// - `capacity` defines how many descriptors the buffer can hold; `length` is how many
///   are actually present. This allows for preallocation and incremental expansion.
/// - Null (zero) pointers are permitted for `continuation` to denote list termination.
///
/// Ensure the backing buffer is already seeked to the start of a valid table chunk.

struct FragmentTablePart {
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

    fn write<Backing: Write + Seek>(&self, source: Backing) -> Result<()> {
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
