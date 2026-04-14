use std::{collections::{HashMap, HashSet}, fs::{self, Metadata}, path::Path, time::UNIX_EPOCH};
use md5::{Md5, Digest};
use rayon::prelude::*;

use crate::{asset_types::AssetEnvelope, bank::bank_entry::BankEntry, decoded_audio::DecodedAudio, flac, riff::Riff};

pub struct SourceAssetCache {
    assets: HashMap<String, SourceAsset>
}

impl SourceAssetCache {
    pub fn new() -> Self {
        SourceAssetCache {
            assets: HashMap::new()
        }
    }

    pub fn get(&self, file_name: &str) -> Option<&SourceAsset> {
        self.assets.get(file_name)
    }

    pub fn get_source_checksum(&self, file_name: &str) -> Option<Checksum> {
        self.assets.get(file_name).map(|asset| asset.hash)
    }

    pub fn update_source(&mut self, file_name: &str, cleanup_only: bool) -> Result<Option<&SourceAsset>, String> {
        let metadata = match fs::metadata(file_name) {
            Ok(metadata) => metadata,
            Err(_) => {
                // File was deleted, remove from cache.
                self.assets.remove(file_name);
                return Ok(None);
            }
        };

        if cleanup_only {
            // If in cleanup, just get the file if it exists.
            return Ok(self.get(file_name));
        }

        let needs_update = match self.assets.get(file_name) {
            Some(asset) => mtime_newer_than(&metadata, asset.time),
            None => true
        };

        if !needs_update {
            return Ok(self.get(file_name));
        }

        let source_asset = SourceAsset::from_file(file_name, &metadata);

        match source_asset {
            Ok(v) => {
                self.assets.insert(file_name.to_string(), v);
                Ok(self.assets.get(file_name))
            },
            Err(err) => Err(format!("Failed to load source asset from file {}: {}", file_name, err))
        }
    }
    
    pub fn update_sources(&mut self, file_names: HashSet<&str>) -> Result<(), String> {
        // 1. Plan: stat each path, split into rebuild / delete lists.
        let mut work: Vec<(String, Metadata)> = Vec::new();
        let mut deleted: Vec<String> = Vec::new();
        for name in &file_names {
            match fs::metadata(name) {
                Err(_) => deleted.push((*name).to_string()),
                Ok(meta) => {
                    let stale = match self.assets.get(*name) {
                        Some(a) => mtime_newer_than(&meta, a.time),
                        None => true,
                    };
                    if stale {
                        work.push(((*name).to_string(), meta));
                    }
                }
            }
        }

        // 2. Work: rebuild SourceAssets in parallel. from_file is pure over its
        //    borrowed inputs, so workers don't touch self.
        let rebuilt: Vec<Result<(String, SourceAsset), String>> = work
            .par_iter()
            .map(|(name, meta)| {
                SourceAsset::from_file(name, meta)
                    .map(|a| (name.clone(), a))
                    .map_err(|e| format!("Failed to load source asset from file {}: {}", name, e))
            })
            .collect();

        // 3. Merge: propagate errors, apply deletes, insert rebuilds.
        for name in deleted {
            self.assets.remove(&name);
        }
        for r in rebuilt {
            let (name, asset) = r?;
            self.assets.insert(name, asset);
        }
        Ok(())
    }

    pub fn get_sources(&self) -> impl Iterator<Item = &SourceAsset> {
        self.assets.values()
    }

    pub fn cleanup_sources(&mut self) -> Result<(), String> {
        let names: HashSet<String> = self.assets.values().map(|a| a.name.clone()).collect();
        // names owns its strings now — self is free
        for name in &names {
            self.update_source(name, true)?;
        }

        Ok(())
    }
}

pub struct SourceAsset {
    pub envelope_loudness0: u8,
    pub envelope_loudness1: u8,
    pub envelope_loudness2: u8,
    pub envelope_loudness3: u8,
    pub envelope_time1: u16,
    pub envelope_time2: u16,

    pub name: String,
    pub frame_rate: i32,
    pub frame_count: u64,
    pub channel_count: i32,
    pub time: u64,
    pub hash: Checksum
}

impl SourceAsset {
    pub fn from_file(file_name: &str, metadata: &Metadata) -> Result<SourceAsset, String> {
        // Single-pass file read: one open+read feeds MD5, header parse, and PCM decode.
        let data = fs::read(file_name)
            .map_err(|e| format!("Failed to read file {}: {}", file_name, e))?;
        let hash = Checksum::from_data(&data);

        let is_flac = Path::new(file_name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("flac"))
            .unwrap_or(false);

        let audio = if is_flac {
            let decoded = flac::decode(file_name, &data)?;
            drop(data);
            decoded
        } else {
            let riff = Riff::parse(file_name, &data)?;

            if riff.format != 1 && riff.format != 65534 {
                return Err(format!("Unsupported audio format in {}: {}", file_name, riff.format));
            }
            if riff.frame_size / riff.channel_count != 2 {
                return Err(format!("Unsupported bit depth (requires 16-bit) in {}: {}", file_name, riff.frame_size * 8 / riff.channel_count));
            }

            let samples = riff.decode_interleaved_s16_from_slice(&data)?;
            drop(data);

            DecodedAudio {
                samples,
                frame_rate: riff.frame_rate,
                channel_count: riff.channel_count,
                frame_count: riff.frame_count,
            }
        };

        if BankEntry::lookup_frame_rate_index(audio.frame_rate as i32) == u8::MAX {
            return Err(format!("Unsupported sample rate in {}: {}", file_name, audio.frame_rate));
        }
        if audio.channel_count > 2 {
            return Err(format!("Unsupported channel count (requires 1-2 channels) in {}: {}", file_name, audio.channel_count));
        }

        let asset_envelope = AssetEnvelope::envelope_extract(audio.frame_count, audio.channel_count, &audio.samples);

        Ok(SourceAsset {
            name: file_name.to_string(),
            frame_rate: audio.frame_rate as i32,
            frame_count: audio.frame_count,
            channel_count: audio.channel_count as i32,
            hash,
            time: metadata.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0),
            envelope_loudness0: (1.0 * asset_envelope.left[0]) as u8,
            envelope_loudness1: (1.0 * asset_envelope.left[1]) as u8,
            envelope_loudness2: (1.0 * asset_envelope.left[2]) as u8,
            envelope_loudness3: (1.0 * asset_envelope.left[3]) as u8,
            envelope_time1: (u16::MAX as f64 * asset_envelope.time[1]) as u16,
            envelope_time2: (u16::MAX as f64 * asset_envelope.time[2]) as u16,
        })
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct Checksum(pub [u8; 16]);

impl Checksum {
    pub fn from_data(data: &[u8]) -> Self {
        let mut hasher = Md5::new();
        hasher.update(data);

        Checksum(hasher.finalize().into())
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 16 {
            return Err(format!("Checksum buffer too small: {} < 16", bytes.len()));
        }
        let mut arr = [0u8; 16];
        arr.copy_from_slice(&bytes[..16]);
        Ok(Checksum(arr))
    }

    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(32);
        for b in &self.0 {
            use std::fmt::Write;
            write!(s, "{:02X}", b).unwrap();
        }
        s
    }
}

fn mtime_newer_than(meta: &Metadata, stored_secs: u64) -> bool {
    let modified_secs = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    modified_secs > stored_secs + 1  // 1-second tolerance
}