use std::collections::HashMap;

use crate::bank::sound_asset_bank::SoundAssetBank;
use crate::converter::SoundAssetBankConvertedAsset;

pub trait SoundAssetObtainer {
    fn get_asset(&mut self, name: &str) -> Result<(SoundAssetBankConvertedAsset, Vec<u8>), String>;
    fn fatal_error(&self) -> bool;
}

/// Resolves assets from a set of existing bank files. Used when deploying or
/// rebuilding — you hand it banks you already have, and it forwards asset
/// lookups into the first bank that contains each name.
pub struct SoundAssetBankObtainer<'a> {
    banks: Vec<&'a SoundAssetBank>,
    index: HashMap<String, usize>,
}

impl<'a> SoundAssetBankObtainer<'a> {
    pub fn new(banks: Vec<&'a SoundAssetBank>) -> Self {
        let mut index = HashMap::new();
        for (i, bank) in banks.iter().enumerate() {
            for file in bank.get_files() {
                index.entry(file.name.clone()).or_insert(i);
            }
        }
        Self { banks, index }
    }
}

impl<'a> SoundAssetObtainer for SoundAssetBankObtainer<'a> {
    fn get_asset(&mut self, name: &str) -> Result<(SoundAssetBankConvertedAsset, Vec<u8>), String> {
        let bank_idx = *self
            .index
            .get(name)
            .ok_or_else(|| format!("Asset {} not found in any bank", name))?;
        self.banks[bank_idx].get_asset(name)
    }

    fn fatal_error(&self) -> bool {
        false
    }
}
