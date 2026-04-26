use serde::{Deserialize, Deserializer};

use crate::converter::CompressionLevel;
use crate::tables::alias_enums::{
    AliasBehavior, AliasBus, AliasFluxType, AliasLimitType, AliasLooping, AliasStorage,
};
use crate::tables::{bool_from_string, empty_as_none, opt_enum_upper};

/// CSV cell parser for the optional per-alias `CompressionLevel` override.
///
/// Returns `None` when the cell is absent / blank / `default` / `yes`,
/// which tells [`crate::sound_zone::SoundZone::generate`] to fall back to
/// the zone's `DefaultAudioCompression`. Returns `Some(level)` only when
/// the cell explicitly names a level; `no` is accepted as a friendly
/// spelling of `None` so designers can disable compression on a single
/// alias without thinking about enum names.
fn alias_compression_level<'de, D>(deserializer: D) -> Result<Option<CompressionLevel>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: String = Deserialize::deserialize(deserializer)?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    match trimmed.to_ascii_lowercase().as_str() {
        "default" | "yes" => Ok(None),
        "no" => Ok(Some(CompressionLevel::None)),
        other => other
            .parse::<CompressionLevel>()
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}

#[derive(Debug, Deserialize)]
pub struct RowAlias {
    #[serde(rename = "Name", default)]
    pub name: String,

    #[serde(rename = "Behavior", default, deserialize_with = "opt_enum_upper")]
    pub behavior: Option<AliasBehavior>,

    #[serde(rename = "Storage", default, deserialize_with = "opt_enum_upper")]
    pub storage: Option<AliasStorage>,

    #[serde(rename = "FileSpec", default)]
    pub file_spec: String,

    #[serde(rename = "FileSpecSustain", default)]
    pub file_spec_sustain: String,

    #[serde(rename = "FileSpecRelease", default)]
    pub file_spec_release: String,

    #[serde(rename = "Template", default)]
    pub template: String,

    #[serde(rename = "Loadspec", default)]
    pub loadspec: String,

    #[serde(rename = "Secondary", default)]
    pub secondary: String,

    #[serde(rename = "SustainAlias", default)]
    pub sustain_alias: String,

    #[serde(rename = "ReleaseAlias", default)]
    pub release_alias: String,

    #[serde(rename = "Bus", default, deserialize_with = "opt_enum_upper")]
    pub bus: Option<AliasBus>,

    #[serde(rename = "VolumeGroup", default)]
    pub volume_group: String,

    #[serde(rename = "DuckGroup", default)]
    pub duck_group: String,

    #[serde(rename = "Duck", default)]
    pub duck: String,

    #[serde(rename = "ReverbSend", default, deserialize_with = "empty_as_none")]
    pub reverb_send: Option<i32>,

    #[serde(rename = "CenterSend", default, deserialize_with = "empty_as_none")]
    pub center_send: Option<i32>,

    #[serde(rename = "VolMin", default, deserialize_with = "empty_as_none")]
    pub vol_min: Option<i32>,

    #[serde(rename = "VolMax", default, deserialize_with = "empty_as_none")]
    pub vol_max: Option<i32>,

    #[serde(rename = "DistMin", default, deserialize_with = "empty_as_none")]
    pub dist_min: Option<i32>,

    #[serde(rename = "DistMaxDry", default, deserialize_with = "empty_as_none")]
    pub dist_max_dry: Option<i32>,

    #[serde(rename = "DistMaxWet", default, deserialize_with = "empty_as_none")]
    pub dist_max_wet: Option<i32>,

    #[serde(rename = "DryMinCurve", default)]
    pub dry_min_curve: String,

    #[serde(rename = "DryMaxCurve", default)]
    pub dry_max_curve: String,

    #[serde(rename = "WetMinCurve", default)]
    pub wet_min_curve: String,

    #[serde(rename = "WetMaxCurve", default)]
    pub wet_max_curve: String,

    #[serde(rename = "LimitCount", default, deserialize_with = "empty_as_none")]
    pub limit_count: Option<i32>,

    #[serde(rename = "LimitType", default, deserialize_with = "opt_enum_upper")]
    pub limit_type: Option<AliasLimitType>,

    #[serde(
        rename = "EntityLimitCount",
        default,
        deserialize_with = "empty_as_none"
    )]
    pub entity_limit_count: Option<i32>,

    #[serde(
        rename = "EntityLimitType",
        default,
        deserialize_with = "opt_enum_upper"
    )]
    pub entity_limit_type: Option<AliasLimitType>,

    #[serde(rename = "PitchMin", default, deserialize_with = "empty_as_none")]
    pub pitch_min: Option<i32>,

    #[serde(rename = "PitchMax", default, deserialize_with = "empty_as_none")]
    pub pitch_max: Option<i32>,

    #[serde(rename = "PriorityMin", default, deserialize_with = "empty_as_none")]
    pub priority_min: Option<i32>,

    #[serde(rename = "PriorityMax", default, deserialize_with = "empty_as_none")]
    pub priority_max: Option<i32>,

    #[serde(
        rename = "PriorityThresholdMin",
        default,
        deserialize_with = "empty_as_none"
    )]
    pub priority_threshold_min: Option<f32>,

    #[serde(
        rename = "PriorityThresholdMax",
        default,
        deserialize_with = "empty_as_none"
    )]
    pub priority_threshold_max: Option<f32>,

    #[serde(
        rename = "AmplitudePriority",
        default,
        deserialize_with = "bool_from_string"
    )]
    pub amplitude_priority: bool,

    #[serde(rename = "PanType", default)]
    pub pan_type: String,

    #[serde(rename = "Pan", default)]
    pub pan: String,

    #[serde(rename = "Futz", default, deserialize_with = "bool_from_string")]
    pub futz: bool,

    #[serde(rename = "Looping", default, deserialize_with = "opt_enum_upper")]
    pub looping: Option<AliasLooping>,

    #[serde(rename = "RandomizeType", default)]
    pub randomize_type: String,

    #[serde(rename = "Probability", default, deserialize_with = "empty_as_none")]
    pub probability: Option<f32>,

    #[serde(rename = "StartDelay", default, deserialize_with = "empty_as_none")]
    pub start_delay: Option<i32>,

    #[serde(rename = "EnvelopMin", default, deserialize_with = "empty_as_none")]
    pub envelop_min: Option<i32>,

    #[serde(rename = "EnvelopMax", default, deserialize_with = "empty_as_none")]
    pub envelop_max: Option<i32>,

    #[serde(rename = "EnvelopPercent", default, deserialize_with = "empty_as_none")]
    pub envelop_percent: Option<i32>,

    #[serde(rename = "OcclusionLevel", default, deserialize_with = "empty_as_none")]
    pub occlusion_level: Option<f32>,

    #[serde(rename = "IsBig", default, deserialize_with = "bool_from_string")]
    pub is_big: bool,

    #[serde(rename = "DistanceLpf", default, deserialize_with = "bool_from_string")]
    pub distance_lpf: bool,

    #[serde(rename = "FluxType", default, deserialize_with = "opt_enum_upper")]
    pub flux_type: Option<AliasFluxType>,

    #[serde(rename = "FluxTime", default, deserialize_with = "empty_as_none")]
    pub flux_time: Option<i32>,

    #[serde(rename = "Subtitle", default)]
    pub subtitle: String,

    #[serde(rename = "Doppler", default, deserialize_with = "bool_from_string")]
    pub doppler: bool,

    #[serde(rename = "ContextType", default)]
    pub context_type: String,

    #[serde(rename = "ContextValue", default)]
    pub context_value: String,

    #[serde(rename = "ContextType1", default)]
    pub context_type_1: String,

    #[serde(rename = "ContextValue1", default)]
    pub context_value_1: String,

    #[serde(rename = "ContextType2", default)]
    pub context_type_2: String,

    #[serde(rename = "ContextValue2", default)]
    pub context_value_2: String,

    #[serde(rename = "ContextType3", default)]
    pub context_type_3: String,

    #[serde(rename = "ContextValue3", default)]
    pub context_value_3: String,

    #[serde(rename = "Timescale", default, deserialize_with = "bool_from_string")]
    pub timescale: bool,

    #[serde(rename = "IsMusic", default, deserialize_with = "bool_from_string")]
    pub is_music: bool,

    #[serde(rename = "IsCinematic", default, deserialize_with = "bool_from_string")]
    pub is_cinematic: bool,

    #[serde(rename = "FadeIn", default, deserialize_with = "empty_as_none")]
    pub fade_in: Option<i32>,

    #[serde(rename = "FadeOut", default, deserialize_with = "empty_as_none")]
    pub fade_out: Option<i32>,

    #[serde(rename = "Pauseable", default, deserialize_with = "bool_from_string")]
    pub pauseable: bool,

    #[serde(
        rename = "StopOnEntDeath",
        default,
        deserialize_with = "bool_from_string"
    )]
    pub stop_on_ent_death: bool,

    #[serde(rename = "Compression", default, deserialize_with = "empty_as_none")]
    pub compression: Option<i32>,

    /// Per-alias lossy-compression override. `None` (the field is absent
    /// from the CSV, or the cell is blank, or the cell says `default` /
    /// `yes`) means "defer to the zone's `DefaultAudioCompression`". A
    /// `Some(level)` explicitly overrides the zone default — including
    /// `Some(None)` via the literal `no` / `none`, which forces no lossy
    /// processing even when the zone default is aggressive.
    #[serde(
        rename = "CompressionLevel",
        default,
        deserialize_with = "alias_compression_level"
    )]
    pub compression_level: Option<CompressionLevel>,

    #[serde(rename = "StopOnPlay", default)]
    pub stop_on_play: String,

    #[serde(rename = "DopplerScale", default, deserialize_with = "empty_as_none")]
    pub doppler_scale: Option<f32>,

    #[serde(rename = "FutzPatch", default)]
    pub futz_patch: String,

    #[serde(rename = "VoiceLimit", default, deserialize_with = "bool_from_string")]
    pub voice_limit: bool,

    #[serde(
        rename = "IgnoreMaxDist",
        default,
        deserialize_with = "bool_from_string"
    )]
    pub ignore_max_dist: bool,

    #[serde(
        rename = "NeverPlayTwice",
        default,
        deserialize_with = "bool_from_string"
    )]
    pub never_play_twice: bool,

    #[serde(
        rename = "ContinuousPan",
        default,
        deserialize_with = "bool_from_string"
    )]
    pub continuous_pan: bool,

    #[serde(rename = "FileSource", default)]
    pub file_source: String,

    #[serde(rename = "FileSourceSustain", default)]
    pub file_source_sustain: String,

    #[serde(rename = "FileSourceRelease", default)]
    pub file_source_release: String,

    #[serde(rename = "FileTarget", default)]
    pub file_target: String,

    #[serde(rename = "FileTargetSustain", default)]
    pub file_target_sustain: String,

    #[serde(rename = "FileTargetRelease", default)]
    pub file_target_release: String,

    #[serde(rename = "Platform", default)]
    pub platform: String,

    #[serde(rename = "Language", default)]
    pub language: String,

    #[serde(rename = "OutputDevices", default)]
    pub output_devices: String,

    #[serde(rename = "PlatformMask", default)]
    pub platform_mask: String,

    #[serde(rename = "WiiUMono", default, deserialize_with = "bool_from_string")]
    pub wii_u_mono: bool,

    #[serde(rename = "StopAlias", default)]
    pub stop_alias: String,

    #[serde(rename = "DistanceLpfMin", default, deserialize_with = "empty_as_none")]
    pub distance_lpf_min: Option<i32>,

    #[serde(rename = "DistanceLpfMax", default, deserialize_with = "empty_as_none")]
    pub distance_lpf_max: Option<i32>,

    #[serde(rename = "FacialAnimationName", default)]
    pub facial_animation_name: String,

    #[serde(
        rename = "RestartContextLoops",
        default,
        deserialize_with = "bool_from_string"
    )]
    pub restart_context_loops: bool,

    #[serde(rename = "SilentInCPZ", default, deserialize_with = "bool_from_string")]
    pub silent_in_cpz: bool,

    #[serde(
        rename = "ContextFailsafe",
        default,
        deserialize_with = "bool_from_string"
    )]
    pub context_failsafe: bool,

    #[serde(rename = "GPAD", default, deserialize_with = "bool_from_string")]
    pub gpad: bool,

    #[serde(rename = "GPADOnly", default, deserialize_with = "bool_from_string")]
    pub gpad_only: bool,

    #[serde(rename = "MuteVoice", default, deserialize_with = "bool_from_string")]
    pub mute_voice: bool,

    #[serde(rename = "MuteMusic", default, deserialize_with = "bool_from_string")]
    pub mute_music: bool,

    #[serde(rename = "RowSourceFileName", default)]
    pub row_source_file_name: String,

    #[serde(rename = "RowSourceShortName", default)]
    pub row_source_short_name: String,

    #[serde(
        rename = "RowSourceLineNumber",
        default,
        deserialize_with = "empty_as_none"
    )]
    pub row_source_line_number: Option<i32>,
}

impl crate::tables::Row for RowAlias {
    fn get_row_name(&self) -> &str {
        &self.name
    }
}

/// Canonicalize a filespec path string: trim whitespace, `/` → `\`, lowercase,
/// strip leading `\`, collapse consecutive `\`. Empty stays empty.
fn normalize_path(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        let c = if ch == '/' { '\\' } else { ch };
        let c = c.to_ascii_lowercase();
        if c == '\\' && out.is_empty() {
            continue; // skip leading
        }
        if c == '\\' && out.ends_with('\\') {
            continue; // collapse
        }
        out.push(c);
    }
    out
}

/// Helper macros for the template merge. For each listed field, if the
/// destination's value is empty/None, copy the value from the source template.
/// `opt` covers `Option<T>`, `str` covers `String`.
macro_rules! merge_opt {
    ($dst:expr, $src:expr, $($field:ident),* $(,)?) => {
        $(
            if $dst.$field.is_none() {
                $dst.$field = $src.$field.clone();
            }
        )*
    };
}
macro_rules! merge_str {
    ($dst:expr, $src:expr, $($field:ident),* $(,)?) => {
        $(
            if $dst.$field.is_empty() {
                $dst.$field = $src.$field.clone();
            }
        )*
    };
}

impl RowAlias {
    /// Merge missing fields from the alias's named template, if any. Also
    /// applies hard-coded post-merge fixups (DistMaxWet ← DistMaxDry,
    /// WetMinCurve ← DryMinCurve, Pan=lfe → ReverbSend=0).
    ///
    /// Returns `Err` if `self.template` is set but not found in the map;
    /// returns `Ok` (no-op) if `self.template` is empty.
    pub fn expand_template(
        &mut self,
        templates: &std::collections::HashMap<String, RowAlias>,
    ) -> Result<(), String> {
        if !self.template.is_empty() {
            // Case-insensitive lookup — real content has mixed-case refs like
            // `UIN_MOD` referencing a template stored as `uin_mod`.
            let tmpl = templates
                .get(&self.template.to_ascii_lowercase())
                .ok_or_else(|| {
                    format!(
                        "alias '{}' references missing template '{}'",
                        self.name, self.template
                    )
                })?;

            merge_str!(
                self,
                tmpl,
                file_spec,
                file_spec_sustain,
                file_spec_release,
                loadspec,
                secondary,
                sustain_alias,
                release_alias,
                volume_group,
                duck_group,
                duck,
                dry_min_curve,
                dry_max_curve,
                wet_min_curve,
                wet_max_curve,
                pan_type,
                pan,
                randomize_type,
                subtitle,
                context_type,
                context_value,
                context_type_1,
                context_value_1,
                context_type_2,
                context_value_2,
                context_type_3,
                context_value_3,
                stop_on_play,
                futz_patch,
                file_source,
                file_source_sustain,
                file_source_release,
                file_target,
                file_target_sustain,
                file_target_release,
                platform,
                language,
                output_devices,
                platform_mask,
                stop_alias,
                facial_animation_name,
            );
            merge_opt!(
                self,
                tmpl,
                behavior,
                storage,
                bus,
                limit_type,
                entity_limit_type,
                looping,
                flux_type,
                reverb_send,
                center_send,
                vol_min,
                vol_max,
                dist_min,
                dist_max_dry,
                dist_max_wet,
                limit_count,
                entity_limit_count,
                pitch_min,
                pitch_max,
                priority_min,
                priority_max,
                priority_threshold_min,
                priority_threshold_max,
                probability,
                start_delay,
                envelop_min,
                envelop_max,
                envelop_percent,
                occlusion_level,
                flux_time,
                fade_in,
                fade_out,
                compression,
                compression_level,
                doppler_scale,
                distance_lpf_min,
                distance_lpf_max,
            );
            // Bool fields: we can't distinguish "unset" from "false" without
            // Option<bool>, so the rule is "if the alias is false, inherit".
            // Matches the common case where CSVs leave the column empty (parsed as false).
            macro_rules! merge_bool {
                ($($field:ident),* $(,)?) => {
                    $(
                        if !self.$field {
                            self.$field = tmpl.$field;
                        }
                    )*
                };
            }
            merge_bool!(
                amplitude_priority,
                futz,
                is_big,
                distance_lpf,
                doppler,
                timescale,
                is_music,
                is_cinematic,
                pauseable,
                stop_on_ent_death,
                voice_limit,
                ignore_max_dist,
                never_play_twice,
                continuous_pan,
                wii_u_mono,
                restart_context_loops,
                silent_in_cpz,
                context_failsafe,
                gpad,
                gpad_only,
                mute_voice,
                mute_music,
            );
        }

        // Hard-coded column defaults. Only the handful that gate the pipeline
        // (filespec expansion / bank writes) — the rest can be added if
        // validation trips on them.
        use crate::tables::alias_enums::{AliasBus, AliasLooping, AliasStorage};
        if self.storage.is_none() {
            self.storage = Some(AliasStorage::Loaded);
        }
        if self.looping.is_none() {
            self.looping = Some(AliasLooping::Nonlooping);
        }
        if self.bus.is_none() {
            self.bus = Some(AliasBus::BusFx);
        }
        if self.compression.is_none() {
            self.compression = Some(100);
        }

        // Post-merge fixups.
        if self.dist_max_wet.is_none() {
            self.dist_max_wet = self.dist_max_dry;
        }
        if self.wet_max_curve.is_empty() {
            self.wet_max_curve = if self.dry_max_curve.is_empty() {
                "default".to_string()
            } else {
                self.dry_max_curve.clone()
            };
        }
        if self.wet_min_curve.is_empty() {
            self.wet_min_curve = if self.dry_min_curve.is_empty() {
                "default".to_string()
            } else {
                self.dry_min_curve.clone()
            };
        }
        if self.pan == "lfe" {
            self.reverb_send = Some(0);
        }

        // Canonicalize every filespec string: collapse consecutive separators,
        // convert `/` → `\`, lowercase, strip leading `\`.
        self.file_spec = normalize_path(&self.file_spec);
        self.file_spec_sustain = normalize_path(&self.file_spec_sustain);
        self.file_spec_release = normalize_path(&self.file_spec_release);

        Ok(())
    }

    /// Validate the alias after template expansion. Returns the first
    /// validation error found, or `Ok(())`. Context validation is skipped —
    /// it needs a contexts table we don't yet load.
    pub fn validate_after_template(&self) -> Result<(), String> {
        if self.name.contains(' ') {
            return Err(format!("'{}': Name cannot contain a space", self.name));
        }
        if self.file_spec.contains(' ') {
            return Err(format!("'{}': FileSpec cannot contain a space", self.name));
        }
        if self.subtitle.contains(' ') {
            return Err(format!("'{}': Subtitle cannot contain a space", self.name));
        }

        fn check_pair<T: PartialOrd + std::fmt::Debug>(
            name: &str,
            a: Option<T>,
            b: Option<T>,
            a_label: &str,
            b_label: &str,
        ) -> Result<(), String> {
            if let (Some(a), Some(b)) = (a, b) {
                if a > b {
                    return Err(format!(
                        "'{}': {} ({:?}) > {} ({:?})",
                        name, a_label, a, b_label, b
                    ));
                }
            }
            Ok(())
        }

        check_pair(
            &self.name,
            self.dist_min,
            self.dist_max_dry,
            "DistMin",
            "DistMaxDry",
        )?;
        check_pair(
            &self.name,
            self.dist_min,
            self.dist_max_wet,
            "DistMin",
            "DistMaxWet",
        )?;
        check_pair(
            &self.name,
            self.dist_max_dry,
            self.dist_max_wet,
            "DistMaxDry",
            "DistMaxWet",
        )?;
        check_pair(
            &self.name,
            self.pitch_min,
            self.pitch_max,
            "PitchMin",
            "PitchMax",
        )?;
        check_pair(
            &self.name,
            self.envelop_min,
            self.envelop_max,
            "EnvelopMin",
            "EnvelopMax",
        )?;
        check_pair(&self.name, self.vol_min, self.vol_max, "VolMin", "VolMax")?;
        check_pair(
            &self.name,
            self.priority_threshold_min,
            self.priority_threshold_max,
            "PriorityThresholdMin",
            "PriorityThresholdMax",
        )?;

        // 3D alias with PRIORITY limit type must have differing Priority min/max.
        let is_priority_limit = matches!(
            self.limit_type,
            Some(crate::tables::alias_enums::AliasLimitType::Priority)
        ) || matches!(
            self.entity_limit_type,
            Some(crate::tables::alias_enums::AliasLimitType::Priority)
        );
        let is_spatial = self.pan_type == "3d" || self.pan_type == "2.5d";
        if is_priority_limit && is_spatial {
            if self.priority_min == self.priority_max {
                return Err(format!(
                    "'{}': 3D alias with PRIORITY limit type must have differing PriorityMin/PriorityMax",
                    self.name
                ));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::load_table_relaxed;
    use std::path::Path;

    #[test]
    fn expand_and_validate_real_aliases() {
        use std::collections::HashMap;
        let templates: Vec<RowAlias> =
            load_table_relaxed(Path::new("test_data/template_rottsky.csv")).expect("templates");
        let mut tmpl_map: HashMap<String, RowAlias> = HashMap::new();
        for t in templates {
            tmpl_map.insert(t.name.clone(), t);
        }

        let rows: Vec<RowAlias> =
            load_table_relaxed(Path::new("test_data/zm_karelia_sfx.csv")).expect("aliases");

        // Find one row that actually references a template we loaded.
        let sample = rows
            .into_iter()
            .find(|r| !r.template.is_empty() && tmpl_map.contains_key(&r.template));

        // Run expand + validate over every alias in the fixture (most have no
        // template, so they exercise the fixup + validation path only).
        let all: Vec<RowAlias> =
            load_table_relaxed(Path::new("test_data/zm_karelia_sfx.csv")).expect("reload");
        let mut ok = 0usize;
        for mut r in all {
            r.expand_template(&tmpl_map).expect("expand");
            r.validate_after_template()
                .unwrap_or_else(|e| panic!("validate failed: {}", e));
            ok += 1;
        }
        println!("Expanded + validated {} aliases", ok);

        if let Some(mut r) = sample {
            let template_name = r.template.clone();
            let before_storage = r.storage.is_some();
            r.expand_template(&tmpl_map).expect("expand");
            r.validate_after_template().expect("validate");
            println!(
                "Expanded '{}' via template '{}': storage before={}, after={:?}, bus={:?}",
                r.name, template_name, before_storage, r.storage, r.bus
            );
            // Post-fixup guarantees:
            assert!(!r.wet_max_curve.is_empty());
            assert!(!r.wet_min_curve.is_empty());
        } else {
            println!("No alias referencing a known template in fixture — skipping merge assertion");
        }
    }

    #[test]
    fn load_real_alias_csv() {
        let rows: Vec<RowAlias> =
            load_table_relaxed(Path::new("test_data/zm_karelia_sfx.csv")).expect("load");
        assert!(!rows.is_empty(), "should have at least one row");
        println!("Loaded {} aliases", rows.len());
        for r in rows.iter().take(5) {
            println!(
                "  {} file_spec={} bus={:?} storage={:?}",
                r.name, r.file_spec, r.bus, r.storage
            );
        }
    }
}
