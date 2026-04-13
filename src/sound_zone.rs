use std::collections::HashMap;

use rayon::prelude::*;

use crate::converter::{
    AliasLooping as CvLooping, AliasStorage as CvStorage, SoundAssetBankSourceAsset,
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

/// Per-source table payload produced by the parallel CSV loader.
enum LoadedSource {
    Aliases(Vec<RowAlias>),
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
                        Ok(LoadedSource::Aliases(load_table_relaxed(&path)?))
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
                LoadedSource::Aliases(v) => zone.aliases.extend(v),
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

        // 3. Parallel filespec expansion. Each alias independently produces a
        //    list of (is_streamed, converted_name, SoundAssetBankSourceAsset)
        //    entries; merge serially into the two HashMaps with first-seen-wins.
        let env = &snapshot.env;
        let platform_ref = &platform;
        let language_ref = &language;
        let per_alias: Vec<Vec<(bool, String, SoundAssetBankSourceAsset)>> = zone
            .aliases
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

                let specs: [(&str, Option<AliasLooping>); 3] = [
                    (alias.file_spec.as_str(), None),
                    (alias.file_spec_sustain.as_str(), Some(AliasLooping::Looping)),
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
                        out.push((
                            is_streamed,
                            converted_name.clone(),
                            SoundAssetBankSourceAsset {
                                source_name,
                                looping: to_converter_looping(&effective_looping),
                                storage: to_converter_storage(&storage_enum),
                                compression,
                                locale: language_ref.clone(),
                                platform: platform_ref.clone(),
                                converted_name,
                            },
                        ));
                    }
                }
                Ok(out)
            })
            .collect::<Result<Vec<_>, String>>()?;

        for group in per_alias {
            for (is_streamed, converted_name, asset) in group {
                let dest = if is_streamed {
                    &mut zone.streamed_files
                } else {
                    &mut zone.loaded_files
                };
                dest.entry(converted_name).or_insert(asset);
            }
        }

        // NOTE: source asset cache is deliberately *not* populated here.
        // `update_bank` populates only the sources needed by its `to_add` set,
        // so unchanged assets skip envelope extraction + RIFF decode entirely
        // on warm-cache runs.

        Ok(zone)
    }
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
