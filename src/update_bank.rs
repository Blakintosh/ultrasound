use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};


use rayon::prelude::*;

use crate::bank::bank_header::BankHeader;
use crate::bank::sound_asset_bank::SoundAssetBank;
use crate::converter::{convert_source_inline, SoundAssetBankConvertedAsset};
use crate::obtainer::SoundAssetObtainer;
use crate::sound_data_snapshot::SoundDataSnapshot;
use crate::sound_zone::SoundZone;
use crate::string_hash;

/// Build (or update) a bank file for one zone+platform+language+storage-class.
/// Runs once for the loaded bank (`streamed=false`, `.sabl`) and once for the
/// streamed bank (`streamed=true`, `.sabs`).
pub fn update_bank(
    snapshot: &SoundDataSnapshot,
    zone: &SoundZone,
    platform_name: &str,
    language_name: &str,
    streamed: bool,
) -> Result<(), String> {
    let platform = snapshot
        .get_platform(platform_name)
        .ok_or_else(|| format!("unknown platform '{}'", platform_name))?
        .clone();
    let language = snapshot
        .get_locale(language_name)
        .ok_or_else(|| format!("unknown language '{}'", language_name))?
        .clone();

    let source_map = if streamed {
        &zone.streamed_files
    } else {
        &zone.loaded_files
    };

    let ext = if streamed { "sabs" } else { "sabl" };
    let base_name = format!("{}.{}", zone.name, language.deploy_name);

    let cache_dir = snapshot.env.get_cache_bank_dir(&language.cache_name);
    let ship_dir = snapshot.env.get_deploy_bank_dir(&language.deploy_name);
    fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("create cache dir {}: {}", cache_dir.display(), e))?;
    fs::create_dir_all(&ship_dir)
        .map_err(|e| format!("create ship dir {}: {}", ship_dir.display(), e))?;

    let cache_path: PathBuf = cache_dir.join(format!("{}.{}", base_name, ext));
    let ship_path: PathBuf = ship_dir.join(format!("{}.{}", base_name, ext));

    // 1. Load existing bank if present, else start from an invalidated empty one.
    let mut bank = if cache_path.exists() {
        SoundAssetBank::load(
            cache_path.to_str().unwrap(),
            platform.converted_asset_version,
        )
    } else {
        let mut hdr = BankHeader::new();
        hdr.set_zone_name(&zone.name);
        hdr.set_platform(&platform.platform);
        hdr.set_language(&language.deploy_name);
        hdr.invalidate();
        SoundAssetBank::new_empty(cache_path.to_str().unwrap(), hdr)
    };

    // 2. Diff against zone's desired asset list.
    //
    // An entry is considered stale if the source .wav's mtime is newer than
    // the bank file's own mtime, meaning it was edited since the bank was
    // last written. Stale entries get replaced (removed by hash + re-added
    // by name). If the bank has no entries (first build), skip the stat
    // guard entirely and treat everything as to_add.
    let desired_hashes: HashSet<u32> = source_map
        .keys()
        .map(|name| string_hash::hash(name))
        .collect();

    let bank_mtime: Option<SystemTime> = if bank.get_files().is_empty() {
        None
    } else {
        fs::metadata(&cache_path)
            .and_then(|m| m.modified())
            .ok()
    };

    let existing_hashes: HashSet<u32> = bank
        .get_files()
        .iter()
        .map(|f| { let h = f.entry.name; h })
        .collect();

    let mut to_remove: HashSet<u32> = HashSet::new();
    // Orphaned entries: in bank but no longer referenced by the zone.
    for f in bank.get_files() {
        let hash = f.entry.name;
        if !desired_hashes.contains(&hash) {
            to_remove.insert(hash);
        }
    }

    let mut to_add: HashSet<String> = HashSet::new();
    for name in source_map.keys() {
        let name_hash = string_hash::hash(name);
        let in_bank = existing_hashes.contains(&name_hash);
        if !in_bank {
            to_add.insert(name.clone());
            continue;
        }
        // Entry exists in the bank — check staleness by source mtime.
        let Some(bank_t) = bank_mtime else {
            // Bank had entries but we couldn't stat it; safest is to reconvert.
            to_remove.insert(name_hash);
            to_add.insert(name.clone());
            continue;
        };
        let Some(src) = source_map.get(name) else { continue };
        let src_t = fs::metadata(&src.source_name)
            .and_then(|m| m.modified())
            .unwrap_or(UNIX_EPOCH);
        if src_t > bank_t {
            to_remove.insert(name_hash);
            to_add.insert(name.clone());
        }
    }

    if to_add.is_empty() && to_remove.is_empty() {
        println!("  {}.{}: up to date", base_name, ext);
        // Still deploy in case the ship copy is missing.
        deploy(&cache_path, &ship_path)?;
        return Ok(());
    }
    println!(
        "  {}.{}: +{} / -{}",
        base_name, ext, to_add.len(), to_remove.len()
    );

    // 3. One-shot parallel convert: each worker does fs::read → RIFF parse →
    //    PCM decode → envelope → resample → FLAC encode in a single pass over
    //    one buffer. No shared source cache; workers are fully independent.
    let converted: HashMap<String, (SoundAssetBankConvertedAsset, Vec<u8>)> = to_add
        .par_iter()
        .map(|name| {
            let src = source_map
                .get(name)
                .ok_or_else(|| format!("obtainer: no source asset for '{}'", name))?;
            let out = convert_source_inline(src)?;
            Ok((name.clone(), out))
        })
        .collect::<Result<HashMap<_, _>, String>>()?;

    let mut obtainer = PreConvertedObtainer { converted };

    bank.modify(
        &mut obtainer,
        &to_remove,
        &to_add,
        platform.converted_asset_version,
        false,
    )?;

    // 4. Deploy cache → ship.
    deploy(&cache_path, &ship_path)?;
    Ok(())
}

fn deploy(cache: &PathBuf, ship: &PathBuf) -> Result<(), String> {
    fs::copy(cache, ship)
        .map(|_| ())
        .map_err(|e| format!("deploy {} → {}: {}", cache.display(), ship.display(), e))
}

/// Lookup-only obtainer over a pre-converted asset map. Conversion happens up
/// front in parallel (see `update_bank`); this just drains the map as
/// `bank.modify` asks for each entry by name. Each name is visited exactly
/// once by the bank writer, so `remove` is safe.
struct PreConvertedObtainer {
    converted: HashMap<String, (SoundAssetBankConvertedAsset, Vec<u8>)>,
}

impl SoundAssetObtainer for PreConvertedObtainer {
    fn get_asset(
        &mut self,
        name: &str,
    ) -> Result<(SoundAssetBankConvertedAsset, Vec<u8>), String> {
        self.converted
            .remove(name)
            .ok_or_else(|| format!("obtainer: no pre-converted asset for '{}'", name))
    }

    fn fatal_error(&self) -> bool {
        false
    }
}
