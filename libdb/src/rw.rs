use crate::error::FragmentError;
use crate::error::Result;
use crate::store::FragmentStore;
use crate::Fragment;
use crate::FragmentID;
use std::collections::BTreeMap;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::time::Duration;
use std::time::SystemTime;

pub (crate) const PAGE_SIZE: usize = 4096;

pub trait Storage<Backing>
where
    Backing: Read + Write + Seek,
    Self: Sized, {
    /// Read a value from a buffer.
    /// Assumes the buffer's cursor is set to the beginning of the structure it aims to read. May fail silently if not.
    fn read(buffer: Backing) -> Result<Self>;

    /// Writes a value into the buffer.
    /// Assumes the buffer's cursor is set to the beginning of the structure it aims to write. May fail silently if not.
    fn write(&mut self, buffer: Backing) -> Result<()>;
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
impl<Backing: Read + Write + Seek> Storage<Backing> for Fragment {
    fn read(mut source: Backing) -> Result<Self> {
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

    /// This function writes the contents of the fragment to disk. It is assumed that the instance has already allocated space for it to do so.
    fn write(&mut self, mut source: Backing) -> Result<()> {
        self.sequence += 1;

        let mut buffer = vec![0u8; Fragment::size()];

        buffer[0..4].copy_from_slice(&FRAG_MAGIC);
        buffer[4..12].copy_from_slice(&self.id.to_le_bytes());
        buffer[12..20].copy_from_slice(&self.sequence.to_le_bytes());
        buffer[20..52].copy_from_slice(&self.compute_hash()?);
        buffer[52..60].copy_from_slice(&self.timestamp.duration_since(SystemTime::UNIX_EPOCH)?.as_millis().to_le_bytes());
        buffer[60..68].copy_from_slice(&self.data.capacity().to_le_bytes());
        buffer[68..76].copy_from_slice(&self.data.len().to_le_bytes());

        source.write_all(&buffer)?;
        source.write_all(&self.data)?;

        Ok(())
    }
}

impl KnownSize for Fragment {
    fn size() -> usize {
        84
    }
}

#[derive(Debug)]
pub struct RWFragmentStore<Backing: Read + Write + Seek> {
    pub(crate) backing: Backing,
    pub(crate) header: RWFragmentStoreIndex,
}

impl<Backing: Read + Write + Seek> RWFragmentStore<Backing> {
    pub fn new(mut backing: Backing) -> Result<Self> {
        Ok(Self {
            header: RWFragmentStoreIndex::read(&mut backing)?,
            backing,
        })
    }

    pub fn blank(backing: Backing) -> Result<Self> {
        Self {
            header: RWFragmentStoreIndex {
                version: 0,
                root_fragment: 0,
                free_space: Default::default(),
                fragment_table_offset: PAGE_SIZE as Pointer,
                fragment_table_parts: vec![FragmentTablePart {
                    continuation: 0,
                    fragments: vec![FragmentDescriptor {
                        id: 0,
                        sequence: 0,
                        offset: 2 * PAGE_SIZE as Pointer,
                        length: PAGE_SIZE as u64,
                    }],
                }],
                end: 3 * PAGE_SIZE as Pointer,
            },
            backing,
        }
        .save()
    }

    fn save(mut self) -> Result<Self> {
        self.header.write(&mut self.backing)?;

        Ok(self)
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
#[derive(Debug)]
pub(crate) struct RWFragmentStoreIndex {
    version: u32,
    root_fragment: FragmentID,
    pub(crate) free_space: BTreeMap<u64, Vec<Pointer>>,
    fragment_table_offset: Pointer,
    fragment_table_parts: Vec<FragmentTablePart>,

    /// Keeps a reference to the end of the backing buffer. Is useful when appending a new chunk.
    pub(crate) end: Pointer,
}

impl<Backing: Read + Write + Seek> Storage<Backing> for RWFragmentStoreIndex {
    fn read(mut source: Backing) -> Result<Self> {
        source.seek(SeekFrom::Start(0))?;
        let mut buffer = vec![0u8; Self::size()];
        source.read_exact(&mut buffer)?;

        if buffer[0..4] != RWFS_MAGIC {
            return Err(FragmentError::InvalidMagic.into());
        }

        let fragment_table_offset = u64::from_le_bytes(buffer[16..24].try_into()?);
        source.seek(SeekFrom::Start(fragment_table_offset))?;

        let root_fragment = FragmentID::from_le_bytes(buffer[8..16].try_into()?);
        let mut prev_offset = fragment_table_offset;
        let mut end = fragment_table_offset;
        let mut fragment_table_parts = vec![];

        loop {
            let mut chunk = FragmentTablePart::read(&mut source)?;

            end = end.max(prev_offset + chunk.cap() as Pointer);

            source.seek(SeekFrom::Start(chunk.continuation))?;
            prev_offset = chunk.continuation;

            let continuation = chunk.continuation;
            fragment_table_parts.push(chunk);

            if continuation == 0 {
                break;
            }
        }

        let mut free_space = BTreeMap::new();

        /*
        # Algorithm:
        1. The end of the root fragment is manually inserted into the vector
        2. The fragment list is iterated. The previous fragment is determined by
         */

        let mut slots = fragment_table_parts.iter().flat_map(|i| i.fragments.iter()).collect::<Vec<_>>();
        slots.sort_by_key(|slot| slot.offset);

        // Iterate over the slots pair-wise. Their gaps are appended to the `free_space` map.

        for i in slots.windows(2) {
            if let [a, b] = *i {
                end = end.max(a.offset + a.length).max(b.offset + b.length);

                let offset = (a.offset + a.length).next_multiple_of(PAGE_SIZE as u64);

                if b.offset > offset {
                    let size = (b.offset - offset).next_multiple_of(PAGE_SIZE as u64) - PAGE_SIZE as u64;

                    if size > 0 {
                        free_space.entry(size).or_insert_with(Vec::new).push(offset);
                    }
                }
            }
        }

        Ok(Self {
            version: u32::from_le_bytes(buffer[4..8].try_into()?),
            root_fragment,
            free_space,
            fragment_table_offset,
            fragment_table_parts,
            end,
        })
    }

    fn write(&mut self, mut source: Backing) -> Result<()> {
        source.seek(SeekFrom::Start(0))?;

        let mut buf = vec![0u8; Self::size()];

        buf[0..4].copy_from_slice(&RWFS_MAGIC);
        buf[4..8].copy_from_slice(&self.version.to_le_bytes());
        buf[8..16].copy_from_slice(&self.root_fragment.to_le_bytes());
        buf[16..24].copy_from_slice(&self.fragment_table_offset.to_le_bytes());

        source.write_all(&buf)?;

        // TODO: Write fragment table
        source.seek(SeekFrom::Start(self.fragment_table_offset))?;

        for part in &mut self.fragment_table_parts {
            part.write(&mut source)?;
            source.seek(SeekFrom::Start(part.continuation))?;
        }

        // Let the OS coalesce adjacent writes - no .flush()
        Ok(())
    }
}

impl KnownSize for RWFragmentStoreIndex {
    fn size() -> usize {
        24
    }
}

impl RWFragmentStoreIndex {
    pub fn allocate_fragment(&mut self, min_size: u64) -> Result<(Pointer, u64)> {
        let size = min_size.next_multiple_of(PAGE_SIZE as u64);

        let ptr = self
            .free_space
            .range_mut(size..)
            .next()
            .and_then(|(_, i)| i.pop())
            .unwrap_or(self.end)
            .next_multiple_of(PAGE_SIZE as u64);

        self.end = self.end.max(ptr + size);

        Ok((ptr, size))
    }
 
    fn mk_fragment_table_part(&mut self) -> Result<&mut FragmentTablePart> {
        let consumed = self.fragment_table().count() * FragmentDescriptor::size();

        let to_allocate = (consumed.next_multiple_of(PAGE_SIZE) as f64).sqrt().ceil().powi(2) as u64;
        let (ptr, size) = self.allocate_fragment(to_allocate)?;

        if let Some(last) = self.fragment_table_parts.last_mut() {
            last.continuation = ptr;
        } else {
            self.fragment_table_offset = ptr;
        }

        assert!(size >= to_allocate);
        self.fragment_table_parts
            .push(FragmentTablePart { continuation: 0, fragments: vec![] });

        self.fragment_table_parts
            .last_mut()
            .ok_or(FragmentError::FailedToCreateNewFragmentTablePart.into())
    }

    pub fn fragment_table(&self) -> impl Iterator<Item = &FragmentDescriptor> {
        self.fragment_table_parts.iter().flat_map(|part| part.fragments.iter())
    }

    pub fn fragment_table_mut(&mut self) -> impl Iterator<Item = &mut FragmentDescriptor> {
        self.fragment_table_parts.iter_mut().flat_map(|part| part.fragments.iter_mut())
    }

    pub fn push_fragment_descriptor(&mut self, fragment: FragmentDescriptor) -> Result<()> {
        match self.fragment_table_parts.last_mut() {
            Some(last) if last.len() < last.cap() => last.fragments.push(fragment),
            _ => self.mk_fragment_table_part()?.fragments.push(fragment),
        }

        Ok(())
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
#[derive(Debug, Clone)]
pub(crate) struct FragmentDescriptor {
    pub(crate) id: FragmentID,
    pub(crate) sequence: u64,
    pub(crate) offset: u64,
    pub(crate) length: u64,
}

impl<Backing: Read + Write + Seek> Storage<Backing> for FragmentDescriptor {
    fn read(mut source: Backing) -> Result<Self> {
        let mut buffer = vec![0u8; Self::size()];
        source.read_exact(&mut buffer)?;

        Ok(Self {
            id: FragmentID::from_le_bytes(buffer[0..8].try_into()?),
            sequence: u64::from_le_bytes(buffer[8..16].try_into()?),
            offset: u64::from_le_bytes(buffer[16..24].try_into()?),
            length: u64::from_le_bytes(buffer[24..32].try_into()?),
        })
    }

    fn write(&mut self, mut source: Backing) -> Result<()> {
        source.write_all(&self.id.to_le_bytes())?;
        source.write_all(&self.sequence.to_le_bytes())?;
        source.write_all(&self.offset.to_le_bytes())?;
        source.write_all(&self.length.to_le_bytes())?;

        Ok(())
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
#[derive(Debug)]
struct FragmentTablePart {
    continuation: Pointer, // We'll accept the use of null-pointers here because they're space efficient.
    fragments: Vec<FragmentDescriptor>,
}

impl<Backing: Read + Write + Seek> Storage<Backing> for FragmentTablePart {
    fn read(mut source: Backing) -> Result<Self> {
        let mut buffer = vec![0u8; Self::size()];
        source.read_exact(&mut buffer)?;

        let (cap, len) = (u64::from_le_bytes(buffer[8..16].try_into()?), u64::from_le_bytes(buffer[16..24].try_into()?));

        if len > cap {
            return Err(FragmentError::LengthExceedsCapacity.into());
        }

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

    fn write(&mut self, mut source: Backing) -> Result<()> {
        source.write_all(self.continuation.to_le_bytes().as_slice())?;
        source.write_all(self.fragments.capacity().to_le_bytes().as_slice())?;
        source.write_all(self.fragments.len().to_le_bytes().as_slice())?;

        for frag in &mut self.fragments {
            frag.write(&mut source)?;
        }

        Ok(())
    }
}

impl KnownSize for FragmentTablePart {
    fn size() -> usize {
        24
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
