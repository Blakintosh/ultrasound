use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use rayon::prelude::*;

use crate::bank::bank_header::BankHeader;
use crate::bank::sound_asset_bank::SoundAssetBank;
use crate::converter::{SoundAssetBankConvertedAsset, convert_source_data};
use crate::obtainer::SoundAssetObtainer;
use crate::sound_data_snapshot::SoundDataSnapshot;
use crate::sound_zone::SoundZone;
use crate::source_asset_cache::Checksum;
use crate::string_hash;
use crate::sz_writer;

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
    output_env: &crate::env::Env,
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
    let ship_dir = output_env.get_deploy_bank_dir(&language.deploy_name);
    fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("create cache dir {}: {}", cache_dir.display(), e))?;
    fs::create_dir_all(&ship_dir)
        .map_err(|e| format!("create ship dir {}: {}", ship_dir.display(), e))?;

    let cache_path: PathBuf = cache_dir.join(format!("{}.{}", base_name, ext));
    let ship_path: PathBuf = ship_dir.join(format!("{}.{}", base_name, ext));

    let cache_name = cache_path
        .to_str()
        .ok_or_else(|| format!("path is not valid Unicode: {}", cache_path.display()))?;

    // 1. Load existing bank if present, else start from an invalidated empty one.
    let mut bank = if cache_path.exists() {
        SoundAssetBank::load(cache_name, platform.converted_asset_version)
    } else {
        let mut hdr = BankHeader::new();
        hdr.set_zone_name(&zone.name);
        hdr.set_platform(&platform.platform);
        hdr.set_language(&language.deploy_name);
        hdr.invalidate();
        SoundAssetBank::new_empty(cache_name, hdr)
    };
    let cache_valid = fs::metadata(&cache_path)
        .map(|metadata| bank.is_sane(metadata.len() as i64).is_ok())
        .unwrap_or(false);
    bank.set_zone_name(&zone.name);
    bank.set_platform(&platform.platform);
    bank.set_language(&language.deploy_name);

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

    // Retain a source buffer only when its checksum requires conversion.
    let validation_path = cache_path.with_extension(format!("{ext}.usn.json"));
    let validation = crate::source_validation::SourceValidation::load(&validation_path);
    let desired_names: Vec<&String> = source_map.keys().collect();
    let converted: HashMap<String, (SoundAssetBankConvertedAsset, Vec<u8>)> = desired_names
        .par_iter()
        .map(|name| -> Result<Option<_>, String> {
            let src = &source_map[*name];
            let (checksum, data) = validation.read(&src.source_name,
                &src.compression_level.recipe_fingerprint(),
                existing_checksums.get(&string_hash::hash(name)).copied())?;
            if existing_checksums.get(&string_hash::hash(name)) == Some(&checksum) {
                return Ok(None);
            }
            let output = convert_source_data(src, data.expect("changed sources are read"), checksum)?;
            Ok(Some(((*name).clone(), output)))
        })
        .collect::<Result<Vec<_>, String>>()?
        .into_iter()
        .flatten()
        .collect();
    let to_add: HashSet<String> = converted.keys().cloned().collect();
    for name in &to_add {
        let hash = string_hash::hash(name);
        if existing_checksums.contains_key(&hash) {
            to_remove.insert(hash);
        }
    }

    let unchanged = match validation.reused() {
        0 => String::new(),
        reused => format!(" ({reused} sources unchanged, per the NTFS journal)"),
    };
    let no_changes = to_add.is_empty() && to_remove.is_empty();
    if no_changes {
        if !cache_valid {
            println!("  {}.{}: create empty{}", base_name, ext, unchanged);
            bank.write_current(platform.converted_asset_version, false)?;
        } else {
            println!("  {}.{}: up to date{}", base_name, ext, unchanged);
        }
    } else {
        println!(
            "  {}.{}: +{} / -{}{}",
            base_name,
            ext,
            to_add.len(),
            to_remove.len(),
            unchanged
        );

        let mut obtainer = PreConvertedObtainer { converted };

        bank.modify(
            &mut obtainer,
            &to_remove,
            &to_add,
            platform.converted_asset_version,
            false,
        )?;
    }

    // 4. Deploy cache → ship (covers a missing ship copy on otherwise warm
    //    runs as well as the post-modify path).
    let deployed_path = snapshot.env.get_deploy_bank_dir(&language.deploy_name)
        .join(format!("{}.{}", base_name, ext));
    if !crate::staged_outputs::files_equal(&cache_path, &deployed_path)? {
        deploy(&cache_path, &ship_path)?;
    }

    // 5. Loaded-bank sidecars. Only the loaded variant emits these; the
    //    streamed bank doesn't have an asset/memory manifest in baseline.
    //    Always re-emit to keep the files consistent with the bank on disk
    //    even when nothing changed (e.g. the manifest was deleted by hand).
    if !streamed {
        let dir = output_env.get_zone_output_dir(&language.name);
        let path_for =
            |kind: &str| dir.join(format!("{}.{}.{}.sz", zone.name, language.name, kind));

        let total_bytes: u64 = bank.get_files().iter().map(|f| f.entry.size as u64).sum();
        let count = bank.get_files().len();

        sz_writer::write_plain_text(&path_for("memory"), &total_bytes.to_string())?;
        sz_writer::write_plain_text(&path_for("assetcount"), &count.to_string())?;

        let names: Vec<&str> = bank.get_files().iter().map(|f| f.name.as_str()).collect();
        sz_writer::write_assets_list(&path_for("assets"), &names)?;
    }

    // Bank size warnings are disabled until the real limits are known.
    validation.save(&validation_path);
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
