use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use rayon::prelude::*;

use crate::bank::bank_header::BankHeader;
use crate::bank::sound_asset_bank::SoundAssetBank;
use crate::converter::{SoundAssetBankConvertedAsset, convert_source_inline};
use crate::obtainer::SoundAssetObtainer;
use crate::sound_data_snapshot::SoundDataSnapshot;
use crate::sound_zone::SoundZone;
use crate::source_asset_cache::Checksum;
use crate::string_hash;

/// Soft cap on a loaded bank (`.sabl`). Exact engine limit is unknown;
/// 600 MB is a conservative guess used only to surface a warning.
#[cfg(any())]
const SABL_LIMIT_BYTES: u64 = 600 * 1_000 * 1_000;
/// Soft cap on a streamed bank (`.sabs`). Exact engine limit is unknown.
#[cfg(any())]
const SABS_LIMIT_BYTES: u64 = 4 * 1_000 * 1_000 * 1_000;
/// Percent-of-limit at which we print a warning with a compression-tuning
/// hint.
#[cfg(any())]
const BANK_WARN_THRESHOLD_PCT: u64 = 85;

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
    // An entry is considered stale if hashing the current source file
    // produces a checksum that differs from the one the bank recorded when
    // it was last built. This catches both in-place edits AND alias swaps
    // that repoint an existing alias at a different source file (mtime
    // alone would miss the latter when the new source is older than the
    // bank). Stale entries get replaced (removed by hash + re-added by
    // name). On the first build the bank is empty and everything becomes
    // to_add regardless.
    let desired_hashes: HashSet<u32> = source_map
        .keys()
        .map(|name| string_hash::hash(name))
        .collect();

    // name_hash → stored source checksum from the existing bank.
    let existing_checksums: HashMap<u32, Checksum> = bank
        .get_files()
        .iter()
        .map(|f| (f.entry.name, f.source_checksum))
        .collect();

    let mut to_remove: HashSet<u32> = HashSet::new();
    // Orphaned entries: in bank but no longer referenced by the zone.
    for f in bank.get_files() {
        let hash = f.entry.name;
        if !desired_hashes.contains(&hash) {
            to_remove.insert(hash);
        }
    }

    // Hash every desired source in parallel. Only read+hash — no decode.
    // For sources whose checksum already matches the bank we skip entirely;
    // the dominant cost here is the read, and it only applies to entries
    // we would otherwise have walked past silently. The hash mixes in the
    // asset's compression-level fingerprint so retuning a level — or
    // reassigning an alias to a different level — invalidates the bank
    // entry even when the source file bytes are unchanged.
    let desired_names: Vec<&String> = source_map.keys().collect();
    let hashed: Vec<Result<(String, Checksum), String>> = desired_names
        .par_iter()
        .map(|name| {
            let src = source_map
                .get(*name)
                .ok_or_else(|| format!("zone source lookup failed for '{}'", name))?;
            let data = fs::read(&src.source_name)
                .map_err(|e| format!("read {}: {}", src.source_name, e))?;
            let fingerprint = src.compression_level.recipe_fingerprint();
            Ok((
                (*name).clone(),
                Checksum::from_data_with_recipe(&data, &fingerprint),
            ))
        })
        .collect();

    let mut current_checksums: HashMap<String, Checksum> =
        HashMap::with_capacity(desired_names.len());
    for r in hashed {
        let (name, sum) = r?;
        current_checksums.insert(name, sum);
    }

    let mut to_add: HashSet<String> = HashSet::new();
    for name in source_map.keys() {
        let name_hash = string_hash::hash(name);
        let current = current_checksums.get(name).copied();
        match existing_checksums.get(&name_hash) {
            None => {
                to_add.insert(name.clone());
            }
            Some(stored) => {
                if current.map(|c| c != *stored).unwrap_or(true) {
                    to_remove.insert(name_hash);
                    to_add.insert(name.clone());
                }
            }
        }
    }

    if to_add.is_empty() && to_remove.is_empty() {
        println!("  {}.{}: up to date", base_name, ext);
        // Still deploy in case the ship copy is missing.
        deploy(&cache_path, &ship_path)?;
        // Bank size warnings are disabled until the real limits are known.
        // check_bank_size(&cache_path, streamed, &base_name, ext, source_map)?;
        return Ok(());
    }
    println!(
        "  {}.{}: +{} / -{}",
        base_name,
        ext,
        to_add.len(),
        to_remove.len()
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
    // Bank size warnings are disabled until the real limits are known.
    // check_bank_size(&cache_path, streamed, &base_name, ext, source_map)?;
    Ok(())
}

/// Warn if the bank is within the warn threshold of its (soft) size cap.
/// The exact engine limit isn't known, so this never errors — it just
/// surfaces a hint so the user can tune DefaultAudioCompression before
/// they hit a wall in-game.
#[cfg(any())]
fn check_bank_size(
    cache_path: &PathBuf,
    streamed: bool,
    base_name: &str,
    ext: &str,
    source_map: &HashMap<String, SoundAssetBankSourceAsset>,
) -> Result<(), String> {
    let size = match fs::metadata(cache_path) {
        Ok(m) => m.len(),
        Err(_) => return Ok(()),
    };
    if size == 0 {
        return Ok(());
    }

    let limit = if streamed {
        SABS_LIMIT_BYTES
    } else {
        SABL_LIMIT_BYTES
    };
    let pct = size.saturating_mul(100) / limit;
    if pct < BANK_WARN_THRESHOLD_PCT {
        return Ok(());
    }

    let max_level = source_map
        .values()
        .map(|src| src.compression_level)
        .max()
        .unwrap_or(CompressionLevel::None);

    if max_level == CompressionLevel::Extreme {
        return Ok(());
    }

    eprintln!(
        "^3warning: {}.{} is at {}% ({:.1} MB) of the {} MB estimated maximum safe bank size. highest compression level in use is {:?}; consider raising DefaultAudioCompression in the .szc, or tweak individual aliases",
        base_name,
        ext,
        pct,
        size as f64 / 1_000_000.0,
        limit / 1_000_000,
        max_level
    );
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
    fn get_asset(&mut self, name: &str) -> Result<(SoundAssetBankConvertedAsset, Vec<u8>), String> {
        self.converted
            .remove(name)
            .ok_or_else(|| format!("obtainer: no pre-converted asset for '{}'", name))
    }

    fn fatal_error(&self) -> bool {
        false
    }
}
