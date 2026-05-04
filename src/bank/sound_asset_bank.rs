use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::{
    bank::{bank_entry::BankEntry, bank_header::BankHeader},
    converter::{AliasLooping, AssetFormat, SoundAssetBankConvertedAsset},
    obtainer::SoundAssetObtainer,
    source_asset_cache::Checksum,
    string_hash,
};

const ASSET_NAME_BYTES: usize = 128;

pub struct SoundAssetBankFile {
    pub entry: BankEntry,
    pub name: String,
    pub source_checksum: Checksum,
    pub converted_checksum: Checksum,
}

impl SoundAssetBankFile {
    pub fn new(
        entry: BankEntry,
        name: &str,
        source_checksum: Checksum,
        converted_checksum: Checksum,
    ) -> Self {
        SoundAssetBankFile {
            entry,
            name: name.to_string(),
            source_checksum,
            converted_checksum,
        }
    }
}

pub struct SoundAssetBank {
    file_name: String,
    header: BankHeader,
    bank_files: Vec<SoundAssetBankFile>,
    bank_file_index: HashMap<String, usize>,
}

impl SoundAssetBank {
    pub fn new(
        file_name: &str,
        header: BankHeader,
        entries: &[BankEntry],
        converted_checksums: &[Checksum],
        source_checksums: &[Checksum],
        asset_names: &[&str],
    ) -> Self {
        let count = header.entry_count as usize;
        let mut bank_files: Vec<SoundAssetBankFile> = Vec::with_capacity(count);
        let mut bank_file_index: HashMap<String, usize> = HashMap::with_capacity(count);

        for i in 0..count {
            bank_files.push(SoundAssetBankFile::new(
                entries[i],
                asset_names[i],
                source_checksums[i],
                converted_checksums[i],
            ));
            bank_file_index.insert(asset_names[i].to_string(), i);
        }

        SoundAssetBank {
            file_name: file_name.to_string(),
            header,
            bank_files,
            bank_file_index,
        }
    }

    pub fn new_empty(file_name: &str, header: BankHeader) -> Self {
        SoundAssetBank {
            file_name: file_name.to_string(),
            header,
            bank_files: Vec::with_capacity(0),
            bank_file_index: HashMap::new(),
        }
    }

    pub fn is_sane(&self, size: i64) -> Result<(), String> {
        self.header.is_sane(size)
    }

    /// Gets the file name of this bank.
    pub fn get_file_name(&self) -> &str {
        &self.file_name
    }

    /// Gets the bank files contained in this bank.
    pub fn get_files(&self) -> &[SoundAssetBankFile] {
        &self.bank_files
    }

    /// Sets the dependencies of this bank.
    pub fn set_dependencies(&mut self, dependencies: &[&str]) -> Result<(), String> {
        self.header.set_dependencies(&self.file_name, dependencies)
    }

    pub fn set_zone_name(&mut self, name: &str) {
        self.header.set_zone_name(name);
    }

    pub fn set_platform(&mut self, platform: &str) {
        self.header.set_platform(platform);
    }

    pub fn set_language(&mut self, language: &str) {
        self.header.set_language(language);
    }

    pub fn get_asset(
        &self,
        converted_name: &str,
    ) -> Result<(SoundAssetBankConvertedAsset, Vec<u8>), String> {
        let file_index = self
            .find_file_index(converted_name)
            .ok_or_else(|| format!("Could not find file {} in the bank.", converted_name))?;

        let bank_file = &self.bank_files[*file_index];

        let asset = SoundAssetBankConvertedAsset::new(
            &bank_file.name,
            bank_file.converted_checksum,
            bank_file.source_checksum,
            match bank_file.entry.format {
                0 => AssetFormat::SndAssetFormatPcms16,
                8 => AssetFormat::SndAssetFormatFlac,
                other => return Err(format!("Unknown asset format: {}", other)),
            },
            if bank_file.entry.looping == 1 {
                AliasLooping::Looping
            } else {
                AliasLooping::NonLooping
            },
            bank_file.entry.frame_count as i64,
            bank_file.entry.get_frame_rate(),
            bank_file.entry.channel_count as i32,
            bank_file.entry.envelope_loudness0,
            bank_file.entry.envelope_loudness1,
            bank_file.entry.envelope_loudness2,
            bank_file.entry.envelope_loudness3,
            bank_file.entry.envelope_time1,
            bank_file.entry.envelope_time2,
        );
        let offset = bank_file.entry.offset;
        let size = bank_file.entry.size as usize;
        let mut file = File::open(&self.file_name)
            .map_err(|e| format!("Failed to open {}: {}", self.file_name, e))?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| format!("Failed to seek: {}", e))?;
        let mut bytes = vec![0u8; size];
        file.read_exact(&mut bytes)
            .map_err(|e| format!("Failed to read asset data: {}", e))?;

        Ok((asset, bytes))
    }

    pub fn compact(&mut self, strip_names: bool) -> Result<(), String> {
        if self.bank_files.is_empty() || self.total_free_space() == 0 {
            return Ok(());
        }

        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.file_name)
            .map_err(|e| format!("Failed to open {}: {}", self.file_name, e))?;

        // Invalidate on disk first so a crash mid-compact leaves an obviously-bad file.
        let mut scratch = BankHeader::new();
        scratch.invalidate();
        file.seek(SeekFrom::Start(0))
            .map_err(|e| format!("Failed to seek: {}", e))?;
        file.write_all(&scratch.to_bytes())
            .map_err(|e| format!("Failed to write: {}", e))?;

        // Walk entries, shifting any that aren't at the expected offset.
        let mut position = std::mem::size_of::<BankHeader>() as u64;
        for i in 0..self.bank_files.len() {
            let entry_offset = self.bank_files[i].entry.offset;
            let entry_size = self.bank_files[i].entry.size as u64;

            if entry_offset != position {
                copy_range(&mut file, entry_offset, position, entry_size)?;
                self.bank_files[i].entry.offset = position;
            }
            position = self.bank_files[i].entry.offset + entry_size;
        }

        self.write_metadata(&mut file, position, strip_names)?;
        self.write_header(&mut file)?;

        Ok(())
    }

    fn find_file_index(&self, converted_name: &str) -> Option<&usize> {
        self.bank_file_index.get(converted_name)
    }

    /// Returns the total number of unused bytes between the header and packed
    /// asset blobs — i.e. gaps left behind by removed entries.
    fn total_free_space(&self) -> i64 {
        let mut total: i64 = 0;
        let mut offset = std::mem::size_of::<BankHeader>() as i64;
        for file in &self.bank_files {
            let entry_offset = file.entry.offset as i64;
            if entry_offset != offset {
                total += entry_offset - offset;
            }
            offset = entry_offset + file.entry.size as i64;
        }
        total
    }

    /// Loads a bank file. On parse failure, returns an empty invalidated
    /// bank at the same path *without* deleting the existing file. The
    /// update_bank pipeline is responsible for deciding whether to overwrite
    /// a corrupt bank — this function must never destroy data by itself.
    pub fn load(file_name: &str, converted_asset_version: i32) -> Self {
        match Self::try_load(file_name, converted_asset_version) {
            Ok(bank) => bank,
            Err(_reason) => {
                let mut header = BankHeader::new();
                header.invalidate();
                Self::new_empty(file_name, header)
            }
        }
    }

    fn remove_files(
        &mut self,
        file: &mut File,
        to_remove: &HashSet<u32>,
        strip_names: bool,
    ) -> Result<(), String> {
        self.invalidate_header(file)?;

        // Validate every requested removal actually exists in the bank.
        let present: HashSet<u32> = self.bank_files.iter().map(|f| f.entry.name).collect();
        for hash in to_remove {
            if !present.contains(hash) {
                return Err(format!(
                    "Can't remove a file that isn't in the bank: {}",
                    hash
                ));
            }
        }

        // Retain only entries whose name hash isn't in to_remove.
        // (copy the packed field to a local before taking a reference)
        self.bank_files.retain(|f| {
            let name = f.entry.name;
            !to_remove.contains(&name)
        });

        // Rebuild the name → index map since positions shifted.
        self.bank_file_index.clear();
        for (i, f) in self.bank_files.iter().enumerate() {
            self.bank_file_index.insert(f.name.clone(), i);
        }

        // Position is the end of the last remaining entry, or just past the header if empty.
        let position = if let Some(last) = self.bank_files.last() {
            last.entry.offset + last.entry.size as u64
        } else {
            std::mem::size_of::<BankHeader>() as u64
        };

        self.write_metadata(file, position, strip_names)?;
        self.write_header(file)?;

        Ok(())
    }

    fn add_files(
        &mut self,
        file: &mut File,
        converter: &mut dyn SoundAssetObtainer,
        to_add: &HashSet<String>,
        converted_asset_version: i32,
        strip_names: bool,
    ) -> Result<(), String> {
        self.invalidate_header(file)?;
        self.header.converted_asset_version = converted_asset_version;

        let mut position = if let Some(last) = self.bank_files.last() {
            let offset = last.entry.offset;
            let size = last.entry.size as u64;
            offset + size
        } else {
            std::mem::size_of::<BankHeader>() as u64
        };

        file.seek(SeekFrom::Start(position))
            .map_err(|e| format!("Failed to seek: {}", e))?;

        // Sorted alphabetically for deterministic output.
        let mut sorted: Vec<&String> = to_add.iter().collect();
        sorted.sort();

        // Batch per-entry blob writes through a BufWriter so 8k+ small
        // write_all syscalls coalesce into a handful of bulk writes.
        {
            let mut writer = BufWriter::with_capacity(4 * 1024 * 1024, &mut *file);
            for name in sorted {
                let (asset, data) = converter.get_asset(name)?;
                if converter.fatal_error() {
                    return Ok(());
                }

                let format_byte = match asset.format {
                    AssetFormat::SndAssetFormatPcms16 => 0,
                    AssetFormat::SndAssetFormatFlac => 8,
                };
                let looping_byte = if matches!(asset.looping, AliasLooping::Looping) {
                    1
                } else {
                    0
                };

                let entry = BankEntry::new(
                    string_hash::hash(name),
                    data.len() as u32,
                    asset.frame_count as u32,
                    0,
                    position,
                    BankEntry::lookup_frame_rate_index(asset.frame_rate),
                    asset.channel_count as u8,
                    looping_byte,
                    format_byte,
                    asset.envelope_loudness0,
                    asset.envelope_loudness1,
                    asset.envelope_loudness2,
                    asset.envelope_loudness3,
                    asset.envelope_time1,
                    asset.envelope_time2,
                );

                let source_checksum = asset.source_checksum;
                let converted_checksum = asset.converted_checksum;
                self.bank_files.push(SoundAssetBankFile::new(
                    entry,
                    name,
                    source_checksum,
                    converted_checksum,
                ));
                self.bank_file_index
                    .insert(name.clone(), self.bank_files.len() - 1);

                writer
                    .write_all(&data)
                    .map_err(|e| format!("Failed to write asset data: {}", e))?;
                position += data.len() as u64;
            }
            writer
                .flush()
                .map_err(|e| format!("Failed to flush blob writer: {}", e))?;
        }

        let end_position = file
            .stream_position()
            .map_err(|e| format!("Failed to get position: {}", e))?;
        self.write_metadata(file, end_position, strip_names)?;
        self.write_header(file)?;

        Ok(())
    }

    pub fn modify(
        &mut self,
        converter: &mut dyn SoundAssetObtainer,
        to_remove: &HashSet<u32>,
        to_add: &HashSet<String>,
        converted_asset_version: i32,
        strip_names: bool,
    ) -> Result<(), String> {
        if to_remove.is_empty() && to_add.is_empty() {
            return Ok(());
        }

        {
            let mut file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(&self.file_name)
                .map_err(|e| format!("Failed to open {}: {}", self.file_name, e))?;

            self.remove_files(&mut file, to_remove, strip_names)?;
            self.add_files(
                &mut file,
                converter,
                to_add,
                converted_asset_version,
                strip_names,
            )?;
        }

        // Post-condition: file should be a sane bank after modification.
        let file_size = std::fs::metadata(&self.file_name)
            .map_err(|e| format!("Failed to stat: {}", e))?
            .len() as i64;
        SoundAssetBank::load(&self.file_name, converted_asset_version)
            .is_sane(file_size)
            .map_err(|e| format!("Bank was insane after modification: {}", e))?;

        self.compact(strip_names)?;

        let file_size = std::fs::metadata(&self.file_name)
            .map_err(|e| format!("Failed to stat: {}", e))?
            .len() as i64;
        SoundAssetBank::load(&self.file_name, converted_asset_version)
            .is_sane(file_size)
            .map_err(|e| format!("Bank was insane after compact: {}", e))?;

        Ok(())
    }

    /// Write a sane bank file even when there are no asset changes to drive
    /// [`modify`]. This matters for empty localized banks: the linker still
    /// expects the `.sabl`/`.sabs` files to exist, even if they contain zero
    /// entries.
    pub fn write_current(
        &mut self,
        converted_asset_version: i32,
        strip_names: bool,
    ) -> Result<(), String> {
        {
            let mut file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&self.file_name)
                .map_err(|e| format!("Failed to open {}: {}", self.file_name, e))?;

            self.header.converted_asset_version = converted_asset_version;
            let position = if let Some(last) = self.bank_files.last() {
                last.entry.offset + last.entry.size as u64
            } else {
                std::mem::size_of::<BankHeader>() as u64
            };
            self.write_metadata(&mut file, position, strip_names)?;
            self.write_header(&mut file)?;
        }

        let file_size = std::fs::metadata(&self.file_name)
            .map_err(|e| format!("Failed to stat: {}", e))?
            .len() as i64;
        SoundAssetBank::load(&self.file_name, converted_asset_version)
            .is_sane(file_size)
            .map_err(|e| format!("Bank was insane after write: {}", e))?;

        Ok(())
    }

    fn invalidate_header(&self, file: &mut File) -> Result<(), String> {
        let mut bank_header = BankHeader::new();
        bank_header.invalidate();

        file.seek(SeekFrom::Start(0))
            .map_err(|e| format!("Failed to seek: {}", e))?;

        let bytes = bank_header.to_bytes();
        file.write_all(&bytes)
            .map_err(|e| format!("Failed to write invalidated header: {}", e))?;

        Ok(())
    }

    fn try_load(file_name: &str, converted_asset_version: i32) -> Result<Self, String> {
        if !Path::new(file_name).exists() {
            let mut header = BankHeader::new();
            header.invalidate();
            return Ok(Self::new_empty(file_name, header));
        }

        let mut file =
            File::open(file_name).map_err(|e| format!("Failed to open {}: {}", file_name, e))?;
        let file_size = file
            .metadata()
            .map_err(|e| format!("Failed to read metadata for {}: {}", file_name, e))?
            .len() as i64;

        let mut header_bytes = vec![0u8; std::mem::size_of::<BankHeader>()];
        file.read_exact(&mut header_bytes)
            .map_err(|e| format!("Failed to read header: {}", e))?;
        let header = BankHeader::from_bytes(&header_bytes)?;

        header.is_sane(file_size)?;

        if header.converted_checksum_offset == 0
            || header.source_checksum_offset == 0
            || header.asset_name_offset == 0
        {
            return Err("Bank is missing required offsets".to_string());
        }

        if converted_asset_version != 0 && header.converted_asset_version != converted_asset_version
        {
            return Err("Converted asset version mismatch".to_string());
        }

        let entry_count = header.entry_count as usize;

        let entries: Vec<BankEntry> =
            read_packed_array(&mut file, header.entry_offset as u64, entry_count)?;
        let converted_checksums: Vec<Checksum> = read_packed_array(
            &mut file,
            header.converted_checksum_offset as u64,
            entry_count,
        )?;
        let source_checksums: Vec<Checksum> =
            read_packed_array(&mut file, header.source_checksum_offset as u64, entry_count)?;

        file.seek(SeekFrom::Start(header.asset_name_offset as u64))
            .map_err(|e| format!("Failed to seek to asset names: {}", e))?;
        let mut name_bytes = vec![0u8; entry_count * ASSET_NAME_BYTES];
        file.read_exact(&mut name_bytes)
            .map_err(|e| format!("Failed to read asset names: {}", e))?;

        let asset_names: Vec<String> = (0..entry_count)
            .map(|i| {
                let slice = &name_bytes[i * ASSET_NAME_BYTES..(i + 1) * ASSET_NAME_BYTES];
                let end = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
                String::from_utf8_lossy(&slice[..end]).into_owned()
            })
            .collect();

        for i in 1..entry_count {
            if entries[i - 1].offset > entries[i].offset {
                return Err("Bank entries were not sorted by offset".to_string());
            }
        }

        let mut seen: HashSet<&str> = HashSet::with_capacity(entry_count);
        for name in &asset_names {
            if !seen.insert(name.as_str()) {
                return Err(format!("Bank has duplicate asset {}", name));
            }
        }

        let name_refs: Vec<&str> = asset_names.iter().map(|s| s.as_str()).collect();
        Ok(Self::new(
            file_name,
            header,
            &entries,
            &converted_checksums,
            &source_checksums,
            &name_refs,
        ))
    }

    fn write_header(&mut self, file: &mut File) -> Result<(), String> {
        self.header.fix(
            self.bank_files.len().try_into().unwrap(),
            file.metadata()
                .map_err(|e| format!("Failed to get metadata: {}", e))?
                .len() as i64,
        );

        file.seek(SeekFrom::Start(0))
            .map_err(|e| format!("Failed to seek: {}", e))?;

        let bytes = self.header.to_bytes();
        file.write_all(&bytes)
            .map_err(|e| format!("Failed to write: {}", e))?;

        Ok(())
    }

    fn write_metadata(
        &mut self,
        file: &mut File,
        position: u64,
        strip_names: bool,
    ) -> Result<(), String> {
        file.seek(SeekFrom::Start(align(position, 2048)))
            .map_err(|e| format!("Failed to seek: {}", e))?;

        self.header.entry_count = self.bank_files.len() as u32;

        // Extract the three parallel arrays from bank_files and write them.
        let entries: Vec<BankEntry> = self.bank_files.iter().map(|f| f.entry).collect();
        let converted: Vec<Checksum> = self
            .bank_files
            .iter()
            .map(|f| f.converted_checksum)
            .collect();
        let source: Vec<Checksum> = self.bank_files.iter().map(|f| f.source_checksum).collect();

        let (entry_bytes, entry_offset) = write_array(file, &entries)?;
        self.header.entry_offset = entry_offset as i64;

        let (converted_bytes, converted_offset) = write_array(file, &converted)?;
        self.header.converted_checksum_offset = converted_offset as i64;

        let (source_bytes, source_offset) = write_array(file, &source)?;
        self.header.source_checksum_offset = source_offset as i64;

        // Align before the name table.
        let name_offset = align(
            file.stream_position()
                .map_err(|e| format!("Failed to get position: {}", e))?,
            2048,
        );
        file.seek(SeekFrom::Start(name_offset))
            .map_err(|e| format!("Failed to seek: {}", e))?;
        self.header.asset_name_offset = name_offset as i64;

        // Build the name table: entry_count * 128 bytes, each slot null-padded ASCII.
        let mut name_bytes = vec![0u8; self.bank_files.len() * ASSET_NAME_BYTES];
        if !strip_names {
            for (i, f) in self.bank_files.iter().enumerate() {
                let bytes = f.name.as_bytes();
                let len = bytes.len().min(ASSET_NAME_BYTES);
                let base = i * ASSET_NAME_BYTES;
                name_bytes[base..base + len].copy_from_slice(&bytes[..len]);
            }
        }
        file.write_all(&name_bytes)
            .map_err(|e| format!("Failed to write name table: {}", e))?;

        // Take copies of the array fields as they're packed.
        let zone_name = self.header.zone_name;
        let platform = self.header.platform;
        let language = self.header.language;
        let dependencies = self.header.dependencies;

        self.header.bank_checksum = compute_content_checksum(
            &entry_bytes,
            &converted_bytes,
            &source_bytes,
            &name_bytes,
            &zone_name,
            &platform,
            &language,
            &dependencies,
        );

        let end = file
            .stream_position()
            .map_err(|e| format!("Failed to get position: {}", e))?;
        file.set_len(end)
            .map_err(|e| format!("Failed to set length: {}", e))?;
        self.header.file_size = end as i64;

        Ok(())
    }
}

fn compute_content_checksum(
    entry_bytes: &[u8],
    converted_bytes: &[u8],
    source_bytes: &[u8],
    name_bytes: &[u8],
    zone_name: &[u8],
    platform: &[u8],
    language: &[u8],
    dependencies: &[u8],
) -> Checksum {
    let total = entry_bytes.len()
        + converted_bytes.len()
        + source_bytes.len()
        + name_bytes.len()
        + zone_name.len()
        + platform.len()
        + language.len()
        + dependencies.len();
    let mut buf = Vec::with_capacity(total);
    for slice in [
        entry_bytes,
        converted_bytes,
        source_bytes,
        name_bytes,
        zone_name,
        platform,
        language,
        dependencies,
    ] {
        buf.extend_from_slice(slice);
    }
    Checksum::from_data(&buf)
}

fn read_packed_array<T: Copy>(
    file: &mut File,
    offset: u64,
    count: usize,
) -> Result<Vec<T>, String> {
    let elem_size = std::mem::size_of::<T>();
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| format!("Failed to seek packed array: {}", e))?;
    let mut buf = vec![0u8; count * elem_size];
    file.read_exact(&mut buf)
        .map_err(|e| format!("Failed to read packed array: {}", e))?;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let elem = unsafe { std::ptr::read_unaligned(buf[i * elem_size..].as_ptr() as *const T) };
        out.push(elem);
    }
    Ok(out)
}

fn write_array<T: Copy>(file: &mut File, array: &[T]) -> Result<(Vec<u8>, u64), String> {
    let position = file
        .stream_position()
        .map_err(|e| format!("Failed to get position: {}", e))?;

    let new_offset = align(position, 2048);

    file.seek(SeekFrom::Start(new_offset))
        .map_err(|e| format!("Failed to seek: {}", e))?;

    let bytes = slice_as_bytes(array);
    file.write_all(&bytes)
        .map_err(|e| format!("Failed to write: {}", e))?;

    Ok((bytes, new_offset))
}

fn align(value: u64, align: u64) -> u64 {
    value + (align - value % align) % align
}

/// Copies `size` bytes within a single file from `src` to `dst`, using a 64KB buffer.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_real_bank_file() {
        let path = "test_data/core_patch.all.sabl";
        let file_size = std::fs::metadata(path).expect("test file missing").len() as i64;

        let bank = SoundAssetBank::load(path, 0);

        bank.is_sane(file_size).expect("loaded bank should be sane");

        let files = bank.get_files();
        assert!(!files.is_empty(), "bank should contain at least one asset");

        println!("Loaded {} assets from {}", files.len(), path);
        for file in files.iter().take(5) {
            let name = &file.name;
            let size = file.entry.size;
            let offset = file.entry.offset;
            let frame_rate = file.entry.get_frame_rate();
            let channels = file.entry.channel_count;
            println!(
                "  {} @ 0x{:x} ({} bytes, {} Hz, {} ch)",
                name, offset, size, frame_rate, channels
            );
        }

        // Entries should be monotonic by offset.
        for i in 1..files.len() {
            let prev = files[i - 1].entry.offset;
            let curr = files[i].entry.offset;
            assert!(prev <= curr, "entries not sorted at index {}", i);
        }
    }

    #[test]
    fn write_current_creates_empty_sane_bank() {
        let dir = std::env::temp_dir().join("ultrasound_empty_bank_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.en.sabl");
        let _ = std::fs::remove_file(&path);

        let mut header = BankHeader::new();
        header.set_zone_name("empty_zone");
        header.set_platform("pc");
        header.set_language("en");
        header.invalidate();

        let mut bank = SoundAssetBank::new_empty(path.to_str().unwrap(), header);
        bank.write_current(14, false).expect("write empty bank");

        let file_size = std::fs::metadata(&path).unwrap().len() as i64;
        let loaded = SoundAssetBank::load(path.to_str().unwrap(), 14);
        loaded
            .is_sane(file_size)
            .expect("empty bank should be sane");
        assert!(loaded.get_files().is_empty());
    }

    #[test]
    fn round_trip_copy_assets() {
        use crate::obtainer::SoundAssetBankObtainer;

        let src_path = "test_data/core_patch.all.sabl";
        let dst_path = "test_data/round_trip_out.sabl";
        let _ = std::fs::remove_file(dst_path);

        // Load the real source bank.
        let src = SoundAssetBank::load(src_path, 0);
        let src_files = src.get_files();
        assert!(!src_files.is_empty());

        // Pick the first 3 asset names to copy.
        let to_copy: HashSet<String> = src_files.iter().take(3).map(|f| f.name.clone()).collect();
        let expected_count = to_copy.len();

        // Capture ground-truth bytes from source before we start mutating things.
        let expected: Vec<(String, Vec<u8>)> = to_copy
            .iter()
            .map(|name| {
                let (_, data) = src.get_asset(name).expect("source get_asset");
                (name.clone(), data)
            })
            .collect();

        // Build an obtainer around the source bank, then modify a fresh empty dest bank.
        let banks = vec![&src];
        let mut obtainer = SoundAssetBankObtainer::new(banks);

        let mut header = BankHeader::new();
        header.invalidate();
        let mut dst = SoundAssetBank::new_empty(dst_path, header);
        dst.modify(&mut obtainer, &HashSet::new(), &to_copy, 0, false)
            .expect("modify");

        // Reload the destination and verify everything round-tripped.
        let loaded = SoundAssetBank::load(dst_path, 0);
        let file_size = std::fs::metadata(dst_path).unwrap().len() as i64;
        loaded
            .is_sane(file_size)
            .expect("round-tripped bank should be sane");

        let loaded_files = loaded.get_files();
        assert_eq!(
            loaded_files.len(),
            expected_count,
            "expected {} assets, got {}",
            expected_count,
            loaded_files.len()
        );

        for (name, expected_bytes) in &expected {
            let (_, actual_bytes) = loaded
                .get_asset(name)
                .unwrap_or_else(|e| panic!("loaded get_asset({}) failed: {}", name, e));
            assert_eq!(
                &actual_bytes, expected_bytes,
                "asset {} bytes differ after round-trip",
                name
            );
        }

        println!("Round-tripped {} assets successfully", expected_count);
        let _ = std::fs::remove_file(dst_path);
    }
}

fn copy_range(file: &mut File, src: u64, dst: u64, size: u64) -> Result<(), String> {
    let mut buf = [0u8; 65536];
    let mut remaining = size;
    let mut src_pos = src;
    let mut dst_pos = dst;

    while remaining > 0 {
        let chunk = remaining.min(buf.len() as u64) as usize;

        file.seek(SeekFrom::Start(src_pos))
            .map_err(|e| format!("Failed to seek src: {}", e))?;
        file.read_exact(&mut buf[..chunk])
            .map_err(|e| format!("Failed to read: {}", e))?;

        file.seek(SeekFrom::Start(dst_pos))
            .map_err(|e| format!("Failed to seek dst: {}", e))?;
        file.write_all(&buf[..chunk])
            .map_err(|e| format!("Failed to write: {}", e))?;

        src_pos += chunk as u64;
        dst_pos += chunk as u64;
        remaining -= chunk as u64;
    }

    Ok(())
}

fn slice_as_bytes<T: Copy>(slice: &[T]) -> Vec<u8> {
    let byte_len = std::mem::size_of_val(slice);
    let mut out = vec![0u8; byte_len];
    unsafe {
        std::ptr::copy_nonoverlapping(slice.as_ptr() as *const u8, out.as_mut_ptr(), byte_len);
    }
    out
}
