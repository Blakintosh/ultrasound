use std::collections::HashMap;

use rayon::prelude::*;

use crate::converter::{
    AliasLooping as CvLooping, AliasStorage as CvStorage, CompressionLevel,
    SoundAssetBankSourceAsset,
};
use crate::duk::Duck;
use crate::filespec;
use crate::music::MusicSet;
use crate::sound_data_snapshot::SoundDataSnapshot;
use crate::sound_zone_config::{SoundZoneConfig, SoundZoneTableType};
use crate::tables::alias_enums::{AliasLooping, AliasStorage};
use crate::tables::load_table_relaxed;
use crate::tables::row_alias::RowAlias;
use crate::tables::row_ambient::RowAmbient;
use crate::tables::row_reverb::RowReverb;

/// Per-source table payload produced by the parallel CSV loader. The
/// optional `CompressionLevel` on the `Aliases` variant carries the
/// source entry's `DefaultAudioCompression` from the SZC so it can be
/// applied to every alias in the vec at merge time.
enum LoadedSource {
    Aliases(Option<CompressionLevel>, Vec<RowAlias>),
    Ambients(Vec<RowAmbient>),
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
        };

        // 1. Parallel CSV source loading. Each config source is independent;
        //    load them all concurrently, then merge serially into the zone.
        let env = &snapshot.env;
        let loaded_sources: Vec<LoadedSource> = config
            .sources
            .par_iter()
            .map(|src| -> Result<LoadedSource, String> {
                match src.source_type {
                    SoundZoneTableType::Alias => {
                        let path = env.get_sound_alias_dir().join(&src.filename);
                        Ok(LoadedSource::Aliases(
                            src.default_audio_compression,
                            load_table_relaxed(&path)?,
                        ))
                    }
                    SoundZoneTableType::Ambient => {
                        let path = env.get_sound_ambient_dir().join(&src.filename);
                        Ok(LoadedSource::Ambients(load_table_relaxed(&path)?))
                    }
                    SoundZoneTableType::Radverb => {
                        let path = env.get_sound_reverb_dir().join(&src.filename);
                        Ok(LoadedSource::Reverbs(load_table_relaxed(&path)?))
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
                LoadedSource::Aliases(source_default, mut v) => {
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
                }
                LoadedSource::Ambients(v) => zone.ambients.extend(v),
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
                alias.validate_after_template()?;
                Ok(())
            })?;

        // 3. Parallel filespec expansion. Each alias independently produces
        //    pending bank assets; merge serially into the two HashMaps,
        //    rejecting duplicate targets that would need different output.
        let env = &snapshot.env;
        let platform_ref = &platform;
        let language_ref = &language;
        let per_alias: Vec<Vec<PendingBankAsset>> =
            zone.aliases
                .par_iter()
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

                    let specs: [(&str, Option<AliasLooping>); 3] = [
                        (alias.file_spec.as_str(), None),
                        (
                            alias.file_spec_sustain.as_str(),
                            Some(AliasLooping::Looping),
                        ),
                        (alias.file_spec_release.as_str(), None),
                    ];

                    let mut out = Vec::new();
                    for (spec, override_loop) in specs {
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
}
