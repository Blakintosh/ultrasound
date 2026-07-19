use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use rayon::prelude::*;

use crate::ambient_bsp::AmbientBsp;
use crate::converter::{
    AliasLooping as CvLooping, AliasStorage as CvStorage, CompressionLevel,
    SoundAssetBankSourceAsset,
};
use crate::duk::Duck;
use crate::env::Env;
use crate::filespec;
use crate::music::MusicSet;
use crate::sound_data_snapshot::SoundDataSnapshot;
use crate::sound_zone_config::{SoundZoneConfig, SoundZoneTableType};
use crate::sz_writer;
use crate::tables::alias_enums::{AliasLooping, AliasStorage};
use crate::tables::load_table_relaxed;
use crate::tables::row_alias::RowAlias;
use crate::tables::row_ambient::RowAmbient;
use crate::tables::row_locale::RowLocale;
use crate::tables::row_reverb::RowReverb;
use crate::tables::row_script_id_lookup::RowScriptIdLookup;

/// Per-source table payload produced by the parallel CSV loader. The
/// optional `CompressionLevel` on the `Aliases` variant carries the
/// source entry's `DefaultAudioCompression` from the SZC so it can be
/// applied to every alias in the vec at merge time. The `Vec<RowScriptIdLookup>`
/// alongside it carries any rows from the scriptid sibling CSV
/// (`<scriptid_dir>/<filename>`); empty when no sibling exists.
enum LoadedSource {
    Aliases(
        Option<CompressionLevel>,
        Vec<RowAlias>,
        Vec<RowScriptIdLookup>,
    ),
    Ambients(Vec<RowAmbient>, Vec<RowReverb>),
    Reverbs(Vec<RowReverb>),
    Duck(Box<Duck>),
    Music(Box<MusicSet>),
}

/// A zone after its source tables have been expanded and every alias has had
/// its FileSpec resolved to concrete source files. `loaded_files` and
/// `streamed_files` are the keyed-by-target-name inputs to the bank builder.
pub struct SoundZone {
    pub name: String,
    pub loaded_files: HashMap<String, SoundAssetBankSourceAsset>,
    pub streamed_files: HashMap<String, SoundAssetBankSourceAsset>,
    pub aliases: Vec<RowAlias>,
    pub ambients: Vec<RowAmbient>,
    pub reverbs: Vec<RowReverb>,
    pub ducks: Vec<Duck>,
    pub music: Vec<MusicSet>,
    pub script_id_lookup: Vec<RowScriptIdLookup>,
}

impl SoundZone {
    pub fn generate(
        snapshot: &mut SoundDataSnapshot,
        config: &SoundZoneConfig,
        platform_name: &str,
        language_name: &str,
    ) -> Result<Self, String> {
        let platform = snapshot
            .get_platform(platform_name)
            .ok_or_else(|| format!("unknown platform '{}'", platform_name))?
            .clone();
        let language = snapshot
            .get_locale(language_name)
            .ok_or_else(|| format!("unknown language '{}'", language_name))?
            .clone();

        let mut zone = SoundZone {
            name: config.name.clone(),
            loaded_files: HashMap::new(),
            streamed_files: HashMap::new(),
            aliases: Vec::new(),
            ambients: Vec::new(),
            reverbs: Vec::new(),
            ducks: Vec::new(),
            music: Vec::new(),
            script_id_lookup: Vec::new(),
        };

        // 1. Parallel CSV source loading. Each config source is independent;
        //    load them all concurrently, then merge serially into the zone.
        let env = &snapshot.env;
        let reverb_lookup = &snapshot.reverb_lookup;
        let include_shared_reverbs = language.is_shared;
        let loaded_sources: Vec<LoadedSource> = config
            .sources
            .par_iter()
            .map(|src| -> Result<LoadedSource, String> {
                match src.source_type {
                    SoundZoneTableType::Alias => {
                        let alias_path = env.get_sound_alias_dir().join(&src.filename);
                        let aliases = load_table_relaxed(&alias_path)?;
                        // Optional sibling: <scriptid_dir>/<source.filename>.
                        // Baseline pairs alias CSVs with same-named scriptid
                        // CSVs to produce the per-zone scriptid lookup. Only
                        // a small number of zones actually have these.
                        let scriptid_path = env.get_script_id_dir().join(&src.filename);
                        let script_ids = if scriptid_path.exists() {
                            load_table_relaxed::<RowScriptIdLookup>(&scriptid_path)?
                        } else {
                            Vec::new()
                        };
                        Ok(LoadedSource::Aliases(
                            src.default_audio_compression,
                            aliases,
                            script_ids,
                        ))
                    }
                    SoundZoneTableType::Ambient => {
                        let path = env.get_sound_ambient_dir().join(&src.filename);
                        let ambients: Vec<RowAmbient> = load_table_relaxed(&path)?;
                        let reverbs = if include_shared_reverbs {
                            collect_ambient_reverbs(&ambients, reverb_lookup)
                                .map_err(|e| format!("{}: {}", path.display(), e))?
                        } else {
                            Vec::new()
                        };
                        Ok(LoadedSource::Ambients(ambients, reverbs))
                    }
                    SoundZoneTableType::Radverb => {
                        if src.filename.trim().is_empty() {
                            return Ok(LoadedSource::Reverbs(Vec::new()));
                        }
                        let path = env.get_sound_reverb_dir().join(&src.filename);
                        Ok(LoadedSource::Reverbs(
                            crate::tables::row_reverb::load_reverb_table_with_metadata(&path)?,
                        ))
                    }
                    SoundZoneTableType::Duck => {
                        let path = env.get_duck_source_dir().join(&src.filename);
                        Ok(LoadedSource::Duck(Box::new(Duck::load(&path)?)))
                    }
                    SoundZoneTableType::Music => {
                        let path = env.get_music_dir().join(&src.filename);
                        Ok(LoadedSource::Music(Box::new(MusicSet::load(&path)?)))
                    }
                }
            })
            .collect::<Result<Vec<_>, String>>()?;

        for src in loaded_sources {
            match src {
                LoadedSource::Aliases(source_default, mut v, script_ids) => {
                    // Apply the source-entry default to any alias whose own
                    // CompressionLevel column is blank. This pre-populates
                    // the row before template expansion, so the precedence
                    // becomes: alias column > source default > template >
                    // zone default. An alias row that explicitly sets
                    // CompressionLevel still wins.
                    if let Some(level) = source_default {
                        for alias in v.iter_mut() {
                            if alias.compression_level.is_none() {
                                alias.compression_level = Some(level);
                            }
                        }
                    }
                    zone.aliases.extend(v);
                    zone.script_id_lookup.extend(script_ids);
                }
                LoadedSource::Ambients(ambients, reverbs) => {
                    zone.ambients.extend(ambients);
                    zone.reverbs.extend(reverbs);
                }
                LoadedSource::Reverbs(v) => zone.reverbs.extend(v),
                LoadedSource::Duck(d) => zone.ducks.push(*d),
                LoadedSource::Music(m) => zone.music.push(*m),
            }
        }

        // 2. Expand templates and validate every alias in parallel.
        let templates = &snapshot.alias_templates;
        zone.aliases
            .par_iter_mut()
            .try_for_each(|alias| -> Result<(), String> {
                alias.expand_template(templates)?;
                // FileSource/FileTarget columns are generated output, not
                // source-alias input. A source CSV can contain stale values
                // left by an earlier conversion; retaining them makes the
                // emitted .alias.sz reference nonexistent sustain/release
                // assets. Rebuild the fields exclusively from this run's
                // resolved FileSpecs below.
                alias.clear_resolved_file_fields();
                alias.validate_after_template()?;
                Ok(())
            })?;

        // 2.5 Dedupe identical rows. See `dedup_aliases` below for the
        //     full rationale and key composition.
        dedup_aliases(&mut zone.aliases);

        // 3. Parallel filespec expansion. Each alias independently produces
        //    pending bank assets; merge serially into the two HashMaps,
        //    rejecting duplicate targets that would need different output.
        let env = &snapshot.env;
        let platform_ref = &platform;
        let language_ref = &language;
        let per_alias: Vec<Vec<PendingBankAsset>> =
            zone.aliases
                .par_iter_mut()
                .map(|alias| -> Result<Vec<_>, String> {
                    if alias.file_spec.is_empty()
                        && alias.file_spec_sustain.is_empty()
                        && alias.file_spec_release.is_empty()
                    {
                        return Ok(Vec::new());
                    }
                    if !alias_for_platform(alias, platform_name) {
                        return Ok(Vec::new());
                    }
                    let storage_enum = alias.storage.clone().ok_or_else(|| {
                        format!("alias '{}' has FileSpec but no Storage", alias.name)
                    })?;
                    let looping_enum = alias.looping.clone().ok_or_else(|| {
                        format!("alias '{}' has FileSpec but no Looping", alias.name)
                    })?;
                    let is_streamed = matches!(storage_enum, AliasStorage::Streamed);
                    let compression = alias.compression.unwrap_or(0);
                    // Per-alias override wins; otherwise fall back to the
                    // zone's DefaultAudioCompression.
                    let compression_level = alias
                        .compression_level
                        .unwrap_or(config.default_audio_compression);

                    // Clone spec strings so we don't keep an immutable borrow
                    // on `alias` across the writeback below.
                    let specs: [(String, Option<AliasLooping>); 3] = [
                        (alias.file_spec.clone(), None),
                        (alias.file_spec_sustain.clone(), Some(AliasLooping::Looping)),
                        (alias.file_spec_release.clone(), None),
                    ];

                    let mut out = Vec::new();
                    for (idx, (spec, override_loop)) in specs.iter().enumerate() {
                        if spec.is_empty() {
                            continue;
                        }
                        let resolved = filespec::expand(
                            env,
                            alias,
                            platform_ref,
                            language_ref,
                            spec,
                            override_loop.clone(),
                        )?;

                        // Populate FileSource / FileTarget on the alias row so
                        // the emitted .sz binds the alias to its encoded asset.
                        // We keep a 1:1 mapping with the input CSV row, so use
                        // the first resolved file as the canonical (source,
                        // target) pair for this row.
                        if let Some(rf) = resolved.first() {
                            let source_str = rf.source_path.to_string_lossy().into_owned();
                            let target_str = rf.target_name.clone();
                            match idx {
                                0 => {
                                    alias.file_source = source_str;
                                    alias.file_target = target_str;
                                }
                                1 => {
                                    alias.file_source_sustain = source_str;
                                    alias.file_target_sustain = target_str;
                                }
                                2 => {
                                    alias.file_source_release = source_str;
                                    alias.file_target_release = target_str;
                                }
                                _ => unreachable!("specs has exactly 3 entries"),
                            }
                        }

                        let effective_looping = override_loop
                            .clone()
                            .unwrap_or_else(|| looping_enum.clone());
                        for rf in resolved {
                            let source_name = rf.source_path.to_string_lossy().into_owned();
                            let converted_name = rf.target_name.clone();
                            out.push(PendingBankAsset {
                                alias_name: alias.name.clone(),
                                is_streamed,
                                converted_name: converted_name.clone(),
                                asset: SoundAssetBankSourceAsset {
                                    source_name,
                                    looping: to_converter_looping(&effective_looping),
                                    storage: to_converter_storage(&storage_enum),
                                    compression,
                                    compression_level,
                                    locale: language_ref.clone(),
                                    platform: platform_ref.clone(),
                                    converted_name,
                                },
                            });
                        }
                    }
                    Ok(out)
                })
                .collect::<Result<Vec<_>, String>>()?;

        let mut loaded_origins: HashMap<String, String> = HashMap::new();
        let mut streamed_origins: HashMap<String, String> = HashMap::new();
        for group in per_alias {
            for pending in group {
                let (dest, origins) = if pending.is_streamed {
                    (&mut zone.streamed_files, &mut streamed_origins)
                } else {
                    (&mut zone.loaded_files, &mut loaded_origins)
                };
                insert_bank_asset(dest, origins, pending)?;
            }
        }

        // NOTE: source asset cache is deliberately *not* populated here.
        // `update_bank` populates only the sources needed by its `to_add` set,
        // so unchanged assets skip envelope extraction + RIFF decode entirely
        // on warm-cache runs.

        Ok(zone)
    }
}

impl SoundZone {
    /// Write the per-zone `.sz` sidecar files alongside the cache/ship banks.
    ///
    /// Always emits `<zone>.<lang>.alias.sz` (sorted, baseline column set).
    /// For shared locales (`is_shared`), additionally emits the reverb,
    /// ambient, ducklist, and musiclist sidecars.
    ///
    /// Outputs the engine consumes — `memory.sz`, `assetcount.sz`, and
    /// `assets.sz` — are produced by `update_bank` once the loaded bank has
    /// finished, since they describe the bank's post-build contents.
    ///
    /// Two emissions are intentionally deferred:
    /// * `scriptid.sz` — the script-id table type isn't loaded by this port
    ///   yet (no `Scriptid` variant in `SoundZoneTableType`).
    /// * `<zone>.ambientgeometry.json` — requires a port of `AmbientBsp`
    ///   that doesn't exist here yet.
    pub fn write_outputs(
        &self,
        env: &Env,
        config: &SoundZoneConfig,
        locale: &RowLocale,
    ) -> Result<(), String> {
        let dir = env.get_zone_output_dir(&locale.name);
        let path_for =
            |kind: &str| dir.join(format!("{}.{}.{}.sz", config.name, locale.name, kind));

        let mut aliases: Vec<&RowAlias> = self.aliases.iter().collect();
        aliases.sort_by(|a, b| compare_alias(a, b));
        sz_writer::write_alias_table(&path_for("alias"), aliases.iter().copied())?;

        if locale.is_shared {
            let mut reverbs: Vec<&RowReverb> = self.reverbs.iter().collect();
            // Reverbs sort by descending name — almost certainly an
            // accidental inversion in the spec, but it's the order
            // downstream tools have always seen, so we mirror it exactly.
            reverbs.sort_by(|a, b| b.name.cmp(&a.name));
            sz_writer::write_reverb_table(&path_for("reverb"), reverbs.iter().copied())?;

            let mut ambients: Vec<&RowAmbient> = self.ambients.iter().collect();
            ambients.sort_by(|a, b| a.name.cmp(&b.name));
            sz_writer::write_ambient_table(&path_for("ambient"), ambients.iter().copied())?;

            // Script-ID lookup. Sorted by (ScriptId, AliasName) — matches
            // the baseline `Row.Compare` ordering for a key of `ScriptId`
            // and falls back to alias name as a stable tie-break.
            let mut script_ids: Vec<&RowScriptIdLookup> = self.script_id_lookup.iter().collect();
            script_ids.sort_by(|a, b| {
                a.script_id
                    .cmp(&b.script_id)
                    .then_with(|| a.alias_name.cmp(&b.alias_name))
            });
            sz_writer::write_scriptid_table(&path_for("scriptid"), script_ids.iter().copied())?;

            // Duck names referenced from rows + zone-config Ducks list. The
            // baseline does a globals lookup against an index of every loaded
            // .duk; this port only knows about ducks declared as Sources, so
            // we pass through the names verbatim. Empty / `default` are
            // baseline sentinels meaning "no duck".
            let mut duck_names: BTreeSet<String> = BTreeSet::new();
            for a in &self.aliases {
                let d = a.duck.trim();
                if !d.is_empty() && !d.eq_ignore_ascii_case("default") {
                    duck_names.insert(d.to_string());
                }
            }
            for am in &self.ambients {
                let d = am.duck.trim();
                if !d.is_empty() && !d.eq_ignore_ascii_case("default") {
                    duck_names.insert(d.to_string());
                }
            }
            for d in &config.ducks {
                let stem = Path::new(d)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(d.as_str())
                    .trim();
                if !stem.is_empty() {
                    duck_names.insert(stem.to_string());
                }
            }
            sz_writer::write_name_list(&path_for("ducklist"), &duck_names)?;

            let mut music_names: BTreeSet<String> = BTreeSet::new();
            for m in &config.music_files {
                let stem = Path::new(m)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(m.as_str())
                    .trim();
                if !stem.is_empty() {
                    music_names.insert(stem.to_string());
                }
            }
            sz_writer::write_name_list(&path_for("musiclist"), &music_names)?;

            // Ambient geometry sidecar. Always written into the shared zone
            // dir (no language infix, `.json` rather than `.sz`) — gated on
            // the zone actually referencing a map. Phase 1: only the
            // `Triggers[]` array is populated; `Nodes/Planes/Volumes` are
            // emitted as empty arrays to match the baseline JSON shape so
            // downstream parsers don't choke on missing keys.
            if !config.map_file.trim().is_empty() {
                let map_relative = config.map_file.trim_start_matches(['/', '\\']);
                let map_path = env.get_maps_source_dir().join(map_relative);
                let bsp = AmbientBsp::from_file(&map_path)?;
                let json = bsp.to_json()?;
                let geom_dir = env.get_zone_output_dir("all");
                let geom_path = geom_dir.join(format!("{}.ambientgeometry.json", config.name));
                sz_writer::write_plain_text(&geom_path, &json)?;
            }
        }

        Ok(())
    }
}

/// Drop alias rows that duplicate a row already kept earlier in the vec.
///
/// Alias CSVs commonly share rows across files (e.g. a weapon's base foley
/// is repeated verbatim in its `_v21` variant CSV) and authors also
/// copy-paste rows within a single CSV (e.g. nine empty-FileSpec config
/// rows followed by nine specific-FileSpec variants). Dedup keeps the
/// first occurrence keyed by name + the three FileSpec columns + all four
/// `(ContextType, ContextValue)` pairs — anything matching that key
/// downstream is dropped.
///
/// **The context fields MUST be part of the key.** A single alias name
/// often has multiple rows that share a FileSpec but route to a different
/// audio variant via context (e.g. `train=city` for ADS, `train=country`
/// for hipfire, both pointing at the same hammer .wav). Collapsing those
/// would drop one of the contexts and silence that playback path at
/// runtime — see the `dedup_aliases_preserves_context_distinct_rows`
/// regression test.
pub(crate) fn dedup_aliases(aliases: &mut Vec<RowAlias>) {
    type AliasDedupKey = (
        String, String, String, String, // name + 3 file specs
        String, String, String, String, // 2 of the 4 context pairs (flattened)
        String, String, String, String, // remaining 2 context pairs
    );
    let mut seen: HashSet<AliasDedupKey> = HashSet::new();
    aliases.retain(|a| {
        seen.insert((
            a.name.clone(),
            a.file_spec.clone(),
            a.file_spec_sustain.clone(),
            a.file_spec_release.clone(),
            a.context_type.clone(),
            a.context_value.clone(),
            a.context_type_1.clone(),
            a.context_value_1.clone(),
            a.context_type_2.clone(),
            a.context_value_2.clone(),
            a.context_type_3.clone(),
            a.context_value_3.clone(),
        ))
    });
}

/// Sort order on emitted alias rows: name, file_spec, file_target, then
/// the four context pairs, then template. Empty strings sort before
/// populated ones — null/empty is shortest, so it sorts first.
fn compare_alias(a: &RowAlias, b: &RowAlias) -> Ordering {
    macro_rules! cmp_field {
        ($field:ident) => {
            match a.$field.cmp(&b.$field) {
                Ordering::Equal => {}
                other => return other,
            }
        };
    }
    cmp_field!(name);
    cmp_field!(file_spec);
    cmp_field!(file_target);
    cmp_field!(context_type);
    cmp_field!(context_value);
    cmp_field!(context_type_1);
    cmp_field!(context_value_1);
    cmp_field!(context_type_2);
    cmp_field!(context_value_2);
    cmp_field!(context_type_3);
    cmp_field!(context_value_3);
    cmp_field!(template);
    Ordering::Equal
}

fn collect_ambient_reverbs(
    ambients: &[RowAmbient],
    reverb_lookup: &HashMap<String, RowReverb>,
) -> Result<Vec<RowReverb>, String> {
    let mut reverbs = Vec::new();

    for ambient in ambients {
        let name = ambient.reverb.trim();
        if name.is_empty() {
            continue;
        }

        let key = name.to_ascii_lowercase();
        let reverb = reverb_lookup.get(&key).ok_or_else(|| {
            format!(
                "unknown reverb '{}' referenced by ambient '{}'",
                name, ambient.name
            )
        })?;
        reverbs.push(reverb.clone());
    }

    Ok(reverbs)
}

struct PendingBankAsset {
    alias_name: String,
    is_streamed: bool,
    converted_name: String,
    asset: SoundAssetBankSourceAsset,
}

fn insert_bank_asset(
    files: &mut HashMap<String, SoundAssetBankSourceAsset>,
    origins: &mut HashMap<String, String>,
    pending: PendingBankAsset,
) -> Result<(), String> {
    if let Some(existing) = files.get(&pending.converted_name) {
        if compatible_bank_asset(existing, &pending.asset) {
            return Ok(());
        }

        let existing_alias = origins
            .get(&pending.converted_name)
            .map(String::as_str)
            .unwrap_or("<unknown>");
        return Err(format!(
            "duplicate bank target '{}' has conflicting conversion settings: alias '{}' uses source '{}' with {:?}, but alias '{}' uses source '{}' with {:?}. Use the same CompressionLevel for aliases that resolve to the same bank target, or make their generated target names distinct.",
            pending.converted_name,
            existing_alias,
            existing.source_name,
            existing.compression_level,
            pending.alias_name,
            pending.asset.source_name,
            pending.asset.compression_level,
        ));
    }

    origins.insert(pending.converted_name.clone(), pending.alias_name);
    files.insert(pending.converted_name, pending.asset);
    Ok(())
}

fn compatible_bank_asset(
    existing: &SoundAssetBankSourceAsset,
    incoming: &SoundAssetBankSourceAsset,
) -> bool {
    existing.source_name == incoming.source_name
        && existing.looping == incoming.looping
        && existing.storage == incoming.storage
        && existing.compression_level == incoming.compression_level
}

fn alias_for_platform(alias: &RowAlias, platform: &str) -> bool {
    if alias.platform_mask.trim().is_empty() {
        return true;
    }
    alias
        .platform_mask
        .split_whitespace()
        .any(|p| p.eq_ignore_ascii_case(platform))
}

fn to_converter_looping(l: &AliasLooping) -> CvLooping {
    match l {
        AliasLooping::Looping => CvLooping::Looping,
        AliasLooping::Nonlooping => CvLooping::NonLooping,
    }
}

fn to_converter_storage(s: &AliasStorage) -> CvStorage {
    match s {
        AliasStorage::Loaded => CvStorage::Loaded,
        AliasStorage::Streamed => CvStorage::Streamed,
        AliasStorage::Primed => CvStorage::Primed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::row_locale::RowLocale;
    use crate::tables::row_platform::RowPlatform;

    fn test_asset(
        source_name: &str,
        compression_level: CompressionLevel,
    ) -> SoundAssetBankSourceAsset {
        SoundAssetBankSourceAsset {
            source_name: source_name.to_string(),
            looping: CvLooping::NonLooping,
            storage: CvStorage::Loaded,
            compression: 100,
            compression_level,
            locale: RowLocale {
                name: "all".to_string(),
                search_name: "all".to_string(),
                deploy_name: "all".to_string(),
                cache_name: "all".to_string(),
                is_shared: true,
                compression_scale: 1.0,
            },
            platform: RowPlatform {
                platform: "pc".to_string(),
                converted_asset_version: 14,
                convert_thread_count: 1,
                compression_scale: 1.0,
            },
            converted_name: "shared.LN100.pc.snd".to_string(),
        }
    }

    fn pending(
        alias_name: &str,
        converted_name: &str,
        source_name: &str,
        compression_level: CompressionLevel,
    ) -> PendingBankAsset {
        let mut asset = test_asset(source_name, compression_level);
        asset.converted_name = converted_name.to_string();
        PendingBankAsset {
            alias_name: alias_name.to_string(),
            is_streamed: false,
            converted_name: converted_name.to_string(),
            asset,
        }
    }

    #[test]
    fn duplicate_bank_target_allows_identical_conversion() {
        let mut files = HashMap::new();
        let mut origins = HashMap::new();

        insert_bank_asset(
            &mut files,
            &mut origins,
            pending(
                "first",
                "shared.LN100.pc.snd",
                "sound.wav",
                CompressionLevel::High,
            ),
        )
        .expect("first insert");
        insert_bank_asset(
            &mut files,
            &mut origins,
            pending(
                "second",
                "shared.LN100.pc.snd",
                "sound.wav",
                CompressionLevel::High,
            ),
        )
        .expect("identical duplicate");

        assert_eq!(files.len(), 1);
        assert_eq!(
            origins.get("shared.LN100.pc.snd").map(String::as_str),
            Some("first")
        );
    }

    #[test]
    fn duplicate_bank_target_rejects_conflicting_compression_level() {
        let mut files = HashMap::new();
        let mut origins = HashMap::new();

        insert_bank_asset(
            &mut files,
            &mut origins,
            pending(
                "first",
                "shared.LN100.pc.snd",
                "sound.wav",
                CompressionLevel::Low,
            ),
        )
        .expect("first insert");
        let err = insert_bank_asset(
            &mut files,
            &mut origins,
            pending(
                "second",
                "shared.LN100.pc.snd",
                "sound.wav",
                CompressionLevel::High,
            ),
        )
        .expect_err("conflicting duplicate");

        assert!(err.contains("duplicate bank target"));
        assert!(err.contains("first"));
        assert!(err.contains("second"));
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn ambient_reverbs_preserve_baseline_duplicates() {
        use crate::tables::load_table_relaxed;
        use crate::tables::row_reverb::load_reverb_table_with_metadata;

        let ambients: Vec<RowAmbient> =
            load_table_relaxed(std::path::Path::new("test_data/ambient_zm_karelia.csv"))
                .expect("ambient csv");
        let mut lookup = std::collections::HashMap::new();
        for path in [
            "test_data/common_reverb.csv",
            "test_data/zm_karelia_reverb.csv",
        ] {
            for row in
                load_reverb_table_with_metadata(std::path::Path::new(path)).expect("reverb csv")
            {
                lookup.insert(row.name.to_ascii_lowercase(), row);
            }
        }

        let reverbs = collect_ambient_reverbs(&ambients, &lookup).expect("ambient reverbs");

        assert_eq!(reverbs.len(), ambients.len());
        assert_eq!(
            reverbs
                .iter()
                .filter(|r| r.name == "karelia_green_house")
                .count(),
            3
        );
        assert_eq!(
            reverbs
                .iter()
                .filter(|r| r.name == "global_urban_outdoor")
                .count(),
            1
        );
    }

    /// Regression test for the silent-pistol-mike-in-ADS bug.
    ///
    /// Real baseline data (`weap_mike_fire_plr`, `weap_mike_fire_plr_lfe`)
    /// has multiple rows that share `(name, FileSpec)` but differ only in
    /// `ContextValue` — one with `train=city` (ADS) and one with
    /// `train=country` (hipfire), both pointing at the same hammer .wav.
    /// Both rows must survive dedup so the runtime can pick the right
    /// variant per context. An earlier dedup key omitted the context
    /// columns and silently dropped the ADS rows, leaving those layers
    /// silent in-game.
    ///
    /// This test also locks in the *intended* dedup behaviour: rows that
    /// match on every keyed column (name + 3 file specs + 4 context
    /// pairs) collapse to the first occurrence, but anything that differs
    /// on any of those columns is preserved.
    #[test]
    fn dedup_aliases_preserves_context_distinct_rows() {
        // 12-column header: just the fields the dedup key looks at. Every
        // RowAlias field has `#[serde(default)]`, so all the unspecified
        // columns deserialize to their type defaults (None / "" / etc.).
        let csv_input = "\
Name,FileSpec,FileSpecSustain,FileSpecRelease,ContextType,ContextValue,ContextType1,ContextValue1,ContextType2,ContextValue2,ContextType3,ContextValue3
weap_mike_fire_plr,fire/hammer_01.wav,,,train,city,,,,,,
weap_mike_fire_plr,fire/hammer_01.wav,,,train,country,,,,,,
weap_mike_fire_plr_lfe,fire/lfe.wav,,,train,city,,,,,,
weap_mike_fire_plr_lfe,fire/lfe.wav,,,train,country,,,,,,
weap_dup,fire/dup.wav,,,train,country,,,,,,
weap_dup,fire/dup.wav,,,train,country,,,,,,
weap_secondary_ctx,fire/x.wav,,,train,city,water,over,,,,
weap_secondary_ctx,fire/x.wav,,,train,city,water,under,,,,
weap_template,,,,,,,,,,,
weap_template,,,,,,,,,,,
";
        let path = std::env::temp_dir().join("ultrasound_dedup_aliases.csv");
        std::fs::write(&path, csv_input).expect("write fixture");
        let mut aliases: Vec<RowAlias> =
            crate::tables::load_table_relaxed(&path).expect("load fixture");
        // Ten rows in, dedup runs over them.
        assert_eq!(aliases.len(), 10, "fixture sanity");

        dedup_aliases(&mut aliases);

        // Tally per-name occurrences.
        let count = |name: &str| aliases.iter().filter(|a| a.name == name).count();
        assert_eq!(
            count("weap_mike_fire_plr"),
            2,
            "context-distinct rows on the same FileSpec must both survive (ADS regression)"
        );
        assert_eq!(
            count("weap_mike_fire_plr_lfe"),
            2,
            "lfe variant: same context-distinct preservation rule"
        );
        assert_eq!(
            count("weap_dup"),
            1,
            "byte-identical duplicates collapse to the first occurrence"
        );
        assert_eq!(
            count("weap_secondary_ctx"),
            2,
            "rows differing only in ContextValue1 must both survive"
        );
        assert_eq!(
            count("weap_template"),
            1,
            "empty-FileSpec rows with identical contexts collapse"
        );
        assert_eq!(aliases.len(), 2 + 2 + 1 + 2 + 1);

        // First-occurrence-wins: the surviving mike_fire_plr rows preserve
        // the original input order (city before country).
        let mike_contexts: Vec<&str> = aliases
            .iter()
            .filter(|a| a.name == "weap_mike_fire_plr")
            .map(|a| a.context_value.as_str())
            .collect();
        assert_eq!(mike_contexts, ["city", "country"]);
    }

    /// End-to-end parity check between ultrasound's dedup output and the
    /// committed baseline `zm_karelia.all.alias.sz` for the affected
    /// aliases. Catches the regression at the file-level boundary in
    /// case the dedup pass moves or someone "simplifies" the key later.
    ///
    /// Pulls the row counts directly out of the reference baseline file
    /// rather than hard-coding them — if the baseline ever changes, the
    /// expected counts update with it.
    #[test]
    fn baseline_alias_row_counts_match_for_affected_aliases() {
        const REF_PATH: &str = "test_data/baseline_outputs/zm_karelia/zm_karelia.all.alias.sz";
        const TARGETS: &[&str] = &[
            "weap_mike_fire_plr",
            "weap_mike_fire_plr_lfe",
            "weap_mike_fire_plr_mech",
        ];

        // Count rows per target alias name in the baseline.
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_path(REF_PATH)
            .expect("baseline alias.sz");
        let mut baseline_counts: HashMap<&str, usize> =
            TARGETS.iter().map(|n| (*n, 0)).collect();
        for rec in reader.records() {
            let rec = rec.expect("record");
            if let Some(name) = rec.get(0) {
                if let Some(slot) = baseline_counts.get_mut(name) {
                    *slot += 1;
                }
            }
        }

        // Sanity — these baseline counts come from manual inspection. If
        // the fixture file is ever swapped out for one without these
        // aliases the test would silently pass, so pin a lower bound.
        assert!(
            baseline_counts["weap_mike_fire_plr"] >= 2,
            "baseline must contain at least 2 weap_mike_fire_plr rows (one per train context)"
        );
        assert!(
            baseline_counts["weap_mike_fire_plr_lfe"] >= 2,
            "baseline must contain at least 2 weap_mike_fire_plr_lfe rows"
        );

        // Build a "post-template-expansion" alias list by reading the
        // baseline twice and concatenating (simulating the cross-CSV
        // duplication pattern that's the whole reason dedup exists).
        // After dedup we should land back on exactly the baseline
        // counts. RowAlias doesn't impl Clone, so we read the file
        // twice rather than deep-copying rows in memory.
        let read_targets = || -> Vec<RowAlias> {
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_path(REF_PATH)
                .expect("baseline alias.sz");
            reader
                .deserialize::<RowAlias>()
                .filter_map(|r| {
                    let row = r.expect("deserialize");
                    TARGETS.contains(&row.name.as_str()).then_some(row)
                })
                .collect()
        };
        let mut aliases = read_targets();
        let baseline_only_count = aliases.len();
        aliases.extend(read_targets());
        assert_eq!(
            aliases.len(),
            baseline_only_count * 2,
            "duplicate-injection sanity check"
        );

        dedup_aliases(&mut aliases);

        for &target in TARGETS {
            let actual = aliases.iter().filter(|a| a.name == target).count();
            assert_eq!(
                actual, baseline_counts[target],
                "post-dedup row count for '{}' must match baseline ({} expected, got {})",
                target, baseline_counts[target], actual
            );
        }
    }
}
