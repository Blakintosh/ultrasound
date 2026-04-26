use std::{collections::HashMap, fs, time::UNIX_EPOCH};

use crate::{
    bank::sound_asset_bank::SoundAssetBank,
    source_asset_cache::Checksum,
    string_hash,
    tables::{row_locale::RowLocale, row_platform::RowPlatform},
};

pub struct ConvertedAssetCache {
    assets: HashMap<String, ConvertedAssetEntry>,
    banks: Vec<SoundAssetBank>,
    name: String,
}

impl ConvertedAssetCache {
    pub fn new() -> Self {
        ConvertedAssetCache {
            name: "".to_string(),
            assets: HashMap::new(),
            banks: Vec::new(),
        }
    }

    pub fn get_entry(
        &self,
        asset_name: &str,
        source_checksum: Checksum,
    ) -> Option<&ConvertedAssetEntry> {
        self.assets.get(&Self::get_asset_id(
            string_hash::hash(asset_name),
            source_checksum,
        ))
    }

    pub fn get_asset_id(asset_hash: u32, source_checksum: Checksum) -> String {
        format!("{} {}", asset_hash, source_checksum.to_hex())
    }

    fn load_bank(
        &mut self,
        platform: &RowPlatform,
        language: &RowLocale,
        file_name: &str,
    ) -> Result<(), String> {
        let bank = SoundAssetBank::load(file_name, platform.converted_asset_version);
        self.add_bank(&platform.platform, &language.name, bank)
    }

    pub fn add_bank(
        &mut self,
        platform: &str,
        language: &str,
        bank: SoundAssetBank,
    ) -> Result<(), String> {
        let file_name = bank.get_file_name().to_string();

        if !fs::exists(&file_name)
            .map_err(|e| format!("Could not check file {}: {}", file_name, e))?
        {
            return Ok(());
        }

        let metadata =
            fs::metadata(&file_name).map_err(|e| format!("Failed to stat {}: {}", file_name, e))?;

        if let Err(reason) = bank.is_sane(metadata.len() as i64) {
            eprintln!("Insane sound bank {}: {}", file_name, reason);
            let _ = fs::remove_file(&file_name);
            return Ok(());
        }

        let last_write_time = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Register every asset in the bank as a cache entry.
        for file in bank.get_files() {
            let entry = ConvertedAssetEntry {
                platform: platform.to_string(),
                language: language.to_string(),
                bank_file_name: file_name.clone(),
                asset_hash: file.entry.name,
                source_checksum: file.source_checksum,
                converted_checksum: file.converted_checksum,
                last_write_time,
            };
            self.assets.insert(entry.asset_id(), entry);
        }

        self.banks.push(bank);
        Ok(())
    }

    pub fn get_banks(&self) -> &[SoundAssetBank] {
        &self.banks
    }

    pub fn get_asset_count(&self) -> usize {
        self.assets.len()
    }
}

pub struct ConvertedAssetEntry {
    pub platform: String,
    pub language: String,
    pub bank_file_name: String,
    pub asset_hash: u32,
    pub source_checksum: Checksum,
    pub converted_checksum: Checksum,
    pub last_write_time: u64,
}

impl ConvertedAssetEntry {
    pub fn asset_id(&self) -> String {
        format!("{} {}", self.asset_hash, self.source_checksum.to_hex())
    }
}
