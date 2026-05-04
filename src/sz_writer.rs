//! Per-zone `.sz` sidecar writers.
//!
//! These emit the sidecar layout expected next to the `.sabl`/`.sabs` banks,
//! so the downstream tools (zone build, in-engine loaders) can find their
//! inputs at the conventional paths. Output is CRLF-terminated to match the
//! Windows reference output byte layout.

use std::collections::BTreeSet;
use std::fmt::Display;
use std::fs;
use std::path::Path;

use csv::{Terminator, WriterBuilder};

use crate::tables::row_alias::RowAlias;
use crate::tables::row_ambient::RowAmbient;
use crate::tables::row_reverb::RowReverb;
use crate::tables::row_script_id_lookup::RowScriptIdLookup;

/// Render a primitive optional value: `None` → empty cell; `Some` → its
/// `Display` form. Used for every nullable numeric column.
fn opt_str<T: Display>(v: &Option<T>) -> String {
    match v {
        Some(x) => x.to_string(),
        None => String::new(),
    }
}

fn opt_enum<T, F: Fn(&T) -> &'static str>(v: &Option<T>, f: F) -> String {
    match v {
        Some(x) => f(x).to_string(),
        None => String::new(),
    }
}

fn bool_str(b: bool) -> &'static str {
    if b { "True" } else { "False" }
}

fn opt_bool_str(v: &Option<bool>) -> String {
    match v {
        Some(b) => bool_str(*b).to_string(),
        None => String::new(),
    }
}

fn ensure_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("create dir {}: {}", parent.display(), e))?;
    }
    Ok(())
}

fn write_text_crlf(path: &Path, body: &str) -> Result<(), String> {
    ensure_parent(path)?;
    fs::write(path, body).map_err(|e| format!("write {}: {}", path.display(), e))
}

/// Write a single-column "Name" .sz list (used for ducklist / musiclist /
/// assets). Always sorted; always prepended with a `Name` header line.
pub fn write_name_list(path: &Path, names: &BTreeSet<String>) -> Result<(), String> {
    let mut buf = String::with_capacity(8 + names.iter().map(|n| n.len() + 2).sum::<usize>());
    buf.push_str("Name\r\n");
    for n in names {
        buf.push_str(n);
        buf.push_str("\r\n");
    }
    // Strip the trailing CRLF — baseline `string.Join(NewLine, ...)` doesn't
    // emit a terminator on the last entry.
    if buf.ends_with("\r\n") {
        buf.truncate(buf.len() - 2);
    }
    write_text_crlf(path, &buf)
}

pub fn write_plain_text(path: &Path, body: &str) -> Result<(), String> {
    write_text_crlf(path, body)
}

/// Write the loaded-bank assets list: one converted-asset name per line, no
/// header. Order matches `bank.get_files()` to match baseline behavior.
pub fn write_assets_list(path: &Path, names: &[&str]) -> Result<(), String> {
    let mut buf = String::with_capacity(names.iter().map(|n| n.len() + 2).sum::<usize>());
    for (i, n) in names.iter().enumerate() {
        if i != 0 {
            buf.push_str("\r\n");
        }
        buf.push_str(n);
    }
    write_text_crlf(path, &buf)
}

/// Build a `csv::Writer` configured to match the baseline byte layout: no
/// quoting unless required, CRLF terminators.
fn open_csv(path: &Path) -> Result<csv::Writer<std::fs::File>, String> {
    ensure_parent(path)?;
    WriterBuilder::new()
        .terminator(Terminator::CRLF)
        .quote_style(csv::QuoteStyle::Necessary)
        .from_path(path)
        .map_err(|e| format!("create csv {}: {}", path.display(), e))
}

fn finish_csv(w: csv::Writer<std::fs::File>, path: &Path) -> Result<(), String> {
    w.into_inner()
        .map_err(|e| format!("flush csv {}: {}", path.display(), e))?
        .sync_all()
        .map_err(|e| format!("sync csv {}: {}", path.display(), e))
}

/// Column headers + per-row cells for `RowAlias`. Order matches the baseline
/// `RowAlias` declaration so downstream consumers don't have to reorder.
/// `CompressionLevel` is intentionally omitted: it's a Rust addition that the
/// baseline schema does not carry.
const ALIAS_COLUMNS: &[&str] = &[
    "Name",
    "Behavior",
    "Storage",
    "FileSpec",
    "FileSpecSustain",
    "FileSpecRelease",
    "Template",
    "Loadspec",
    "Secondary",
    "SustainAlias",
    "ReleaseAlias",
    "Bus",
    "VolumeGroup",
    "DuckGroup",
    "Duck",
    "ReverbSend",
    "CenterSend",
    "VolMin",
    "VolMax",
    "DistMin",
    "DistMaxDry",
    "DistMaxWet",
    "DryMinCurve",
    "DryMaxCurve",
    "WetMinCurve",
    "WetMaxCurve",
    "LimitCount",
    "LimitType",
    "EntityLimitCount",
    "EntityLimitType",
    "PitchMin",
    "PitchMax",
    "PriorityMin",
    "PriorityMax",
    "PriorityThresholdMin",
    "PriorityThresholdMax",
    "AmplitudePriority",
    "PanType",
    "Pan",
    "Futz",
    "Looping",
    "RandomizeType",
    "Probability",
    "StartDelay",
    "EnvelopMin",
    "EnvelopMax",
    "EnvelopPercent",
    "OcclusionLevel",
    "IsBig",
    "DistanceLpf",
    "FluxType",
    "FluxTime",
    "Subtitle",
    "Doppler",
    "ContextType",
    "ContextValue",
    "ContextType1",
    "ContextValue1",
    "ContextType2",
    "ContextValue2",
    "ContextType3",
    "ContextValue3",
    "Timescale",
    "IsMusic",
    "IsCinematic",
    "FadeIn",
    "FadeOut",
    "Pauseable",
    "StopOnEntDeath",
    "Compression",
    "StopOnPlay",
    "DopplerScale",
    "FutzPatch",
    "VoiceLimit",
    "IgnoreMaxDist",
    "NeverPlayTwice",
    "ContinuousPan",
    "FileSource",
    "FileSourceSustain",
    "FileSourceRelease",
    "FileTarget",
    "FileTargetSustain",
    "FileTargetRelease",
    "Platform",
    "Language",
    "OutputDevices",
    "PlatformMask",
    "WiiUMono",
    "StopAlias",
    "DistanceLpfMin",
    "DistanceLpfMax",
    "FacialAnimationName",
    "RestartContextLoops",
    "SilentInCPZ",
    "ContextFailsafe",
    "GPAD",
    "GPADOnly",
    "MuteVoice",
    "MuteMusic",
    "RowSourceFileName",
    "RowSourceShortName",
    "RowSourceLineNumber",
];

fn alias_row(r: &RowAlias) -> Vec<String> {
    vec![
        r.name.clone(),
        opt_enum(&r.behavior, |x| x.as_str()),
        opt_enum(&r.storage, |x| x.as_str()),
        r.file_spec.clone(),
        r.file_spec_sustain.clone(),
        r.file_spec_release.clone(),
        r.template.clone(),
        r.loadspec.clone(),
        r.secondary.clone(),
        r.sustain_alias.clone(),
        r.release_alias.clone(),
        opt_enum(&r.bus, |x| x.as_str()),
        r.volume_group.clone(),
        r.duck_group.clone(),
        r.duck.clone(),
        opt_str(&r.reverb_send),
        opt_str(&r.center_send),
        opt_str(&r.vol_min),
        opt_str(&r.vol_max),
        opt_str(&r.dist_min),
        opt_str(&r.dist_max_dry),
        opt_str(&r.dist_max_wet),
        r.dry_min_curve.clone(),
        r.dry_max_curve.clone(),
        r.wet_min_curve.clone(),
        r.wet_max_curve.clone(),
        opt_str(&r.limit_count),
        opt_enum(&r.limit_type, |x| x.as_str()),
        opt_str(&r.entity_limit_count),
        opt_enum(&r.entity_limit_type, |x| x.as_str()),
        opt_str(&r.pitch_min),
        opt_str(&r.pitch_max),
        opt_str(&r.priority_min),
        opt_str(&r.priority_max),
        opt_str(&r.priority_threshold_min),
        opt_str(&r.priority_threshold_max),
        opt_bool_str(&r.amplitude_priority),
        r.pan_type.clone(),
        r.pan.clone(),
        opt_bool_str(&r.futz),
        opt_enum(&r.looping, |x| x.as_str()),
        r.randomize_type.clone(),
        opt_str(&r.probability),
        opt_str(&r.start_delay),
        opt_str(&r.envelop_min),
        opt_str(&r.envelop_max),
        opt_str(&r.envelop_percent),
        opt_str(&r.occlusion_level),
        opt_bool_str(&r.is_big),
        opt_bool_str(&r.distance_lpf),
        opt_enum(&r.flux_type, |x| x.as_str()),
        opt_str(&r.flux_time),
        r.subtitle.clone(),
        opt_bool_str(&r.doppler),
        r.context_type.clone(),
        r.context_value.clone(),
        r.context_type_1.clone(),
        r.context_value_1.clone(),
        r.context_type_2.clone(),
        r.context_value_2.clone(),
        r.context_type_3.clone(),
        r.context_value_3.clone(),
        opt_bool_str(&r.timescale),
        opt_bool_str(&r.is_music),
        opt_bool_str(&r.is_cinematic),
        opt_str(&r.fade_in),
        opt_str(&r.fade_out),
        opt_bool_str(&r.pauseable),
        opt_bool_str(&r.stop_on_ent_death),
        opt_str(&r.compression),
        r.stop_on_play.clone(),
        opt_str(&r.doppler_scale),
        r.futz_patch.clone(),
        opt_bool_str(&r.voice_limit),
        opt_bool_str(&r.ignore_max_dist),
        opt_bool_str(&r.never_play_twice),
        opt_bool_str(&r.continuous_pan),
        r.file_source.clone(),
        r.file_source_sustain.clone(),
        r.file_source_release.clone(),
        r.file_target.clone(),
        r.file_target_sustain.clone(),
        r.file_target_release.clone(),
        r.platform.clone(),
        r.language.clone(),
        r.output_devices.clone(),
        r.platform_mask.clone(),
        opt_bool_str(&r.wii_u_mono),
        r.stop_alias.clone(),
        opt_str(&r.distance_lpf_min),
        opt_str(&r.distance_lpf_max),
        r.facial_animation_name.clone(),
        opt_bool_str(&r.restart_context_loops),
        opt_bool_str(&r.silent_in_cpz),
        opt_bool_str(&r.context_failsafe),
        opt_bool_str(&r.gpad),
        opt_bool_str(&r.gpad_only),
        opt_bool_str(&r.mute_voice),
        opt_bool_str(&r.mute_music),
        r.row_source_file_name.clone(),
        r.row_source_short_name.clone(),
        opt_str(&r.row_source_line_number),
    ]
}

pub fn write_alias_table<'a, I>(path: &Path, rows: I) -> Result<(), String>
where
    I: IntoIterator<Item = &'a RowAlias>,
{
    let mut w = open_csv(path)?;
    w.write_record(ALIAS_COLUMNS)
        .map_err(|e| format!("alias header {}: {}", path.display(), e))?;
    for r in rows {
        let cells = alias_row(r);
        w.write_record(&cells)
            .map_err(|e| format!("alias row {}: {}", path.display(), e))?;
    }
    finish_csv(w, path)
}

const REVERB_COLUMNS: &[&str] = &[
    "Name",
    "MasterReturn",
    "EarlyInputLpf",
    "EarlyFeedback",
    "EarlySmear",
    "EarlyBaseDelayMs",
    "EarlyPreDelayMs",
    "EarlyReturn",
    "NearInputLpf",
    "NearFeedback",
    "NearReturn",
    "NearLowDamp",
    "NearHighDamp",
    "NearDecayTime",
    "NearSmear",
    "NearPreDelayMs",
    "FarInputLpf",
    "FarFeedback",
    "FarReturn",
    "FarLowDamp",
    "FarHighDamp",
    "FarDecayTime",
    "FarSmear",
    "FarPreDelayMs",
    // Inherited Row metadata. Baseline emits these on every row table even
    // though our struct doesn't currently track them — see comment above
    // `ALIAS_COLUMNS` for the same reason.
    "RowSourceFileName",
    "RowSourceShortName",
    "RowSourceLineNumber",
];

fn reverb_row(r: &RowReverb) -> Vec<String> {
    vec![
        r.name.clone(),
        r.master_return.to_string(),
        r.early_input_lpf.to_string(),
        r.early_feedback.to_string(),
        r.early_smear.to_string(),
        r.early_base_delay_ms.to_string(),
        r.early_pre_delay_ms.to_string(),
        r.early_return.to_string(),
        r.near_input_lpf.to_string(),
        r.near_feedback.to_string(),
        r.near_return.to_string(),
        r.near_low_damp.to_string(),
        r.near_high_damp.to_string(),
        r.near_decay_time.to_string(),
        r.near_smear.to_string(),
        r.near_pre_delay_ms.to_string(),
        r.far_input_lpf.to_string(),
        r.far_feedback.to_string(),
        r.far_return.to_string(),
        r.far_low_damp.to_string(),
        r.far_high_damp.to_string(),
        r.far_decay_time.to_string(),
        r.far_smear.to_string(),
        r.far_pre_delay_ms.to_string(),
        r.row_source_file_name.clone(),
        r.row_source_short_name.clone(),
        opt_str(&r.row_source_line_number),
    ]
}

pub fn write_reverb_table<'a, I>(path: &Path, rows: I) -> Result<(), String>
where
    I: IntoIterator<Item = &'a RowReverb>,
{
    let mut w = open_csv(path)?;
    w.write_record(REVERB_COLUMNS)
        .map_err(|e| format!("reverb header {}: {}", path.display(), e))?;
    for r in rows {
        w.write_record(&reverb_row(r))
            .map_err(|e| format!("reverb row {}: {}", path.display(), e))?;
    }
    finish_csv(w, path)
}

const AMBIENT_COLUMNS: &[&str] = &[
    "Name",
    "DefaultRoom",
    "Reverb",
    "ReverbDryLevel",
    "ReverbWetLevel",
    "Loop",
    "Duck",
    "EntityContextType0",
    "EntityContextValue0",
    "EntityContextType1",
    "EntityContextValue1",
    "EntityContextType2",
    "EntityContextValue2",
    "GlobalContextType",
    "GlobalContextValue",
    "RowSourceFileName",
    "RowSourceShortName",
    "RowSourceLineNumber",
];

fn ambient_row(r: &RowAmbient) -> Vec<String> {
    vec![
        r.name.clone(),
        bool_str(r.default_room).to_string(),
        r.reverb.clone(),
        r.reverb_dry_level.to_string(),
        r.reverb_wet_level.to_string(),
        r.loop_.clone(),
        r.duck.clone(),
        r.entity_context_type_0.clone(),
        r.entity_context_value_0.clone(),
        r.entity_context_type_1.clone(),
        r.entity_context_value_1.clone(),
        r.entity_context_type_2.clone(),
        r.entity_context_value_2.clone(),
        r.global_context_type.clone(),
        r.global_context_value.clone(),
        String::new(), // RowSourceFileName — not tracked
        String::new(), // RowSourceShortName — not tracked
        String::new(), // RowSourceLineNumber — not tracked
    ]
}

const SCRIPTID_COLUMNS: &[&str] = &[
    "ScriptId",
    "AliasName",
    "RowSourceFileName",
    "RowSourceShortName",
    "RowSourceLineNumber",
];

fn scriptid_row(r: &RowScriptIdLookup) -> Vec<String> {
    vec![
        r.script_id.clone(),
        r.alias_name.clone(),
        r.row_source_file_name.clone(),
        r.row_source_short_name.clone(),
        opt_str(&r.row_source_line_number),
    ]
}

/// Write `<zone>.<lang>.scriptid.sz`. Emits the five-column header in all
/// cases (a zone with no scriptid sibling CSVs gets a header-only file —
/// downstream consumers always expect the file to exist).
pub fn write_scriptid_table<'a, I>(path: &Path, rows: I) -> Result<(), String>
where
    I: IntoIterator<Item = &'a RowScriptIdLookup>,
{
    let mut w = open_csv(path)?;
    w.write_record(SCRIPTID_COLUMNS)
        .map_err(|e| format!("scriptid header {}: {}", path.display(), e))?;
    for r in rows {
        w.write_record(&scriptid_row(r))
            .map_err(|e| format!("scriptid row {}: {}", path.display(), e))?;
    }
    finish_csv(w, path)
}

pub fn write_ambient_table<'a, I>(path: &Path, rows: I) -> Result<(), String>
where
    I: IntoIterator<Item = &'a RowAmbient>,
{
    let mut w = open_csv(path)?;
    w.write_record(AMBIENT_COLUMNS)
        .map_err(|e| format!("ambient header {}: {}", path.display(), e))?;
    for r in rows {
        w.write_record(&ambient_row(r))
            .map_err(|e| format!("ambient row {}: {}", path.display(), e))?;
    }
    finish_csv(w, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn name_list_round_trip() {
        let dir = std::env::temp_dir().join("ultrasound_sz_test");
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("ducklist.sz");
        let mut s = BTreeSet::new();
        s.insert("alpha".to_string());
        s.insert("beta".to_string());
        write_name_list(&p, &s).unwrap();
        let read = fs::read_to_string(&p).unwrap();
        assert_eq!(read, "Name\r\nalpha\r\nbeta");
    }

    #[test]
    fn assets_list_no_header() {
        let dir = std::env::temp_dir().join("ultrasound_sz_test_2");
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("assets.sz");
        let names = vec!["foo.snd", "bar.snd"];
        write_assets_list(&p, &names).unwrap();
        let read = fs::read_to_string(&p).unwrap();
        assert_eq!(read, "foo.snd\r\nbar.snd");
    }

    #[test]
    fn alias_column_defaults_are_written() {
        use crate::tables::row_alias::RowAlias;
        use std::collections::HashMap;

        let mut reader = csv::Reader::from_reader("Name\nalias_minimal\n".as_bytes());
        let mut row: RowAlias = reader
            .deserialize()
            .next()
            .expect("row")
            .expect("deserialize");
        row.expand_template(&HashMap::new()).expect("expand");

        let out = std::env::temp_dir()
            .join("ultrasound_sz_test_3")
            .join("alias.sz");
        write_alias_table(&out, [&row]).expect("write");

        let mut reader = csv::Reader::from_path(&out).expect("read");
        let headers = reader.headers().expect("headers").clone();
        let records: Vec<csv::StringRecord> = reader
            .records()
            .collect::<Result<Vec<_>, _>>()
            .expect("records");
        let record = records.first().expect("record");
        let value = |column: &str| {
            let index = headers.iter().position(|h| h == column).unwrap();
            record.get(index).unwrap()
        };

        for (column, expected) in [
            ("Behavior", "DEFAULT"),
            ("Storage", "LOADED"),
            ("Bus", "BUS_FX"),
            ("ReverbSend", "0"),
            ("CenterSend", "0"),
            ("VolMin", "92"),
            ("VolMax", "92"),
            ("DistMin", "0"),
            ("DistMaxDry", "10000"),
            ("DistMaxWet", "10000"),
            ("DryMinCurve", "allon"),
            ("DryMaxCurve", "default"),
            ("WetMinCurve", "allon"),
            ("WetMaxCurve", "default"),
            ("LimitCount", "8"),
            ("LimitType", "PRIORITY"),
            ("EntityLimitCount", "8"),
            ("EntityLimitType", "OLDEST"),
            ("PitchMin", "0"),
            ("PitchMax", "0"),
            ("PriorityMin", "100"),
            ("PriorityMax", "100"),
            ("PriorityThresholdMin", "0.25"),
            ("PriorityThresholdMax", "0.75"),
            ("AmplitudePriority", "False"),
            ("PanType", "2d"),
            ("Pan", "default"),
            ("Futz", "False"),
            ("Looping", "NONLOOPING"),
            ("Probability", "1"),
            ("StartDelay", "0"),
            ("EnvelopMin", "0"),
            ("EnvelopMax", "0"),
            ("EnvelopPercent", "0"),
            ("OcclusionLevel", "0.25"),
            ("IsBig", "False"),
            ("DistanceLpf", "True"),
            ("FluxType", "NONE"),
            ("FluxTime", "0"),
            ("Doppler", "False"),
            ("Timescale", "False"),
            ("IsMusic", "False"),
            ("IsCinematic", "False"),
            ("FadeIn", "0"),
            ("FadeOut", "0"),
            ("Pauseable", "True"),
            ("StopOnEntDeath", "False"),
            ("Compression", "100"),
            ("DopplerScale", "1"),
            ("VoiceLimit", "False"),
            ("IgnoreMaxDist", "False"),
            ("NeverPlayTwice", "False"),
            ("ContinuousPan", "True"),
            ("WiiUMono", "False"),
            ("DistanceLpfMin", "800"),
            ("DistanceLpfMax", "3000"),
            ("RestartContextLoops", "False"),
            ("SilentInCPZ", "False"),
            ("ContextFailsafe", "False"),
            ("GPAD", "False"),
            ("GPADOnly", "False"),
            ("MuteVoice", "False"),
            ("MuteMusic", "False"),
        ] {
            assert_eq!(value(column), expected, "default mismatch for {column}");
        }
    }

    #[test]
    fn alias_explicit_values_override_defaults() {
        use crate::tables::row_alias::RowAlias;
        use std::collections::HashMap;

        let csv = "\
Name,DistanceLpf,Pauseable,ContinuousPan,FluxType,DryMaxCurve,FluxTime,DopplerScale
alias_explicit,no,no,no,left_player,custom_curve,123,2.5
";
        let mut reader = csv::Reader::from_reader(csv.as_bytes());
        let mut row: RowAlias = reader
            .deserialize()
            .next()
            .expect("row")
            .expect("deserialize");
        row.expand_template(&HashMap::new()).expect("expand");

        let out = std::env::temp_dir()
            .join("ultrasound_sz_test_4")
            .join("alias.sz");
        write_alias_table(&out, [&row]).expect("write");

        let mut reader = csv::Reader::from_path(&out).expect("read");
        let headers = reader.headers().expect("headers").clone();
        let record = reader.records().next().expect("record").expect("record");
        let value = |column: &str| {
            let index = headers.iter().position(|h| h == column).unwrap();
            record.get(index).unwrap()
        };

        assert_eq!(value("DistanceLpf"), "False");
        assert_eq!(value("Pauseable"), "False");
        assert_eq!(value("ContinuousPan"), "False");
        assert_eq!(value("FluxType"), "LEFT_PLAYER");
        assert_eq!(value("DryMaxCurve"), "custom_curve");
        assert_eq!(value("FluxTime"), "123");
        assert_eq!(value("DopplerScale"), "2.5");
    }

    /// Byte-level parity tests against the reference files checked in
    /// under `test_data/baseline_outputs/zm_karelia/`. Only the writers
    /// that don't require an actual bank build or full zone resolution are
    /// covered here — `alias.sz`, `reverb.sz`, `ambient.sz`, and the three
    /// bank-derived sidecars (`memory.sz` / `assetcount.sz` / `assets.sz`)
    /// need upstream fixtures we don't have yet, so they're omitted.
    mod baseline_parity {
        use super::*;
        use crate::tables::row_script_id_lookup::RowScriptIdLookup;
        use std::collections::BTreeSet;

        const REF_DIR: &str = "test_data/baseline_outputs/zm_karelia";

        fn tmp(name: &str) -> std::path::PathBuf {
            let dir = std::env::temp_dir().join("ultrasound_parity");
            fs::create_dir_all(&dir).unwrap();
            dir.join(name)
        }

        fn assert_files_equal(actual: &Path, reference: &Path) {
            let a = fs::read(actual).unwrap_or_else(|e| panic!("read {}: {}", actual.display(), e));
            let b = fs::read(reference)
                .unwrap_or_else(|e| panic!("read {}: {}", reference.display(), e));
            assert_eq!(
                a,
                b,
                "byte mismatch — actual={} ({} B) vs reference={} ({} B)",
                actual.display(),
                a.len(),
                reference.display(),
                b.len()
            );
        }

        /// `zm_karelia.all.musiclist.sz` baseline content is just the szc's
        /// `MusicFiles` ([`zm_karelia`]) under a `Name` header. Should be
        /// byte-identical to what `write_name_list` produces.
        #[test]
        fn musiclist() {
            let mut names = BTreeSet::new();
            names.insert("zm_karelia".to_string());
            let out = tmp("zm_karelia.all.musiclist.sz");
            write_name_list(&out, &names).unwrap();
            assert_files_equal(
                &out,
                Path::new(&format!("{}/zm_karelia.all.musiclist.sz", REF_DIR)),
            );
        }

        /// `zm_karelia.all.ducklist.sz` baseline holds 11 duck names sorted
        /// alphabetically under `Name`. Reproducing it directly from the
        /// reference makes this test independent of the upstream alias /
        /// ambient resolution that originally produced the list.
        #[test]
        fn ducklist() {
            let names = [
                "blak_abyss",
                "blak_health_low",
                "cmn_duck_all_but_movie",
                "cmn_duck_underscore",
                "cmn_duck_underscore_and_round",
                "exp_grenade",
                "prj_impact",
                "rottsky_cmn_shot_iw8_plr",
                "wpn_cmn_shot_3p",
                "wpn_cmn_shot_plr",
                "zmb_duck_music_3d",
            ];
            let set: BTreeSet<String> = names.iter().map(|s| s.to_string()).collect();
            let out = tmp("zm_karelia.all.ducklist.sz");
            write_name_list(&out, &set).unwrap();
            assert_files_equal(
                &out,
                Path::new(&format!("{}/zm_karelia.all.ducklist.sz", REF_DIR)),
            );
        }

        /// `zm_karelia.all.scriptid.sz` baseline is header-only: this zone
        /// has no scriptid sibling CSVs.
        #[test]
        fn scriptid_empty() {
            let rows: Vec<RowScriptIdLookup> = Vec::new();
            let out = tmp("zm_karelia.all.scriptid.sz");
            write_scriptid_table(&out, rows.iter()).unwrap();
            assert_files_equal(
                &out,
                Path::new(&format!("{}/zm_karelia.all.scriptid.sz", REF_DIR)),
            );
        }

        /// `memory.sz` is the integer total of bank entry sizes, no
        /// terminator. Bank data isn't fixtured, so we pin against the
        /// reference byte-for-byte by feeding the known total directly.
        #[test]
        fn memory_format() {
            let out = tmp("zm_karelia.all.memory.sz");
            write_plain_text(&out, "522075371").unwrap();
            assert_files_equal(
                &out,
                Path::new(&format!("{}/zm_karelia.all.memory.sz", REF_DIR)),
            );
        }

        /// Same shape as `memory.sz` — proves the formatting (no
        /// terminator, raw `to_string()`) matches baseline.
        #[test]
        fn assetcount_format() {
            let out = tmp("zm_karelia.all.assetcount.sz");
            write_plain_text(&out, "8813").unwrap();
            assert_files_equal(
                &out,
                Path::new(&format!("{}/zm_karelia.all.assetcount.sz", REF_DIR)),
            );
        }

        /// `ambient.sz` parity — same shape as the reverb test. Loads
        /// `ambient_zm_karelia.csv`, writes via our writer, and verifies
        /// every non-RowSource cell matches the baseline byte-for-byte.
        /// All 8 ambient rows appear in both files, so we get full
        /// per-row coverage of:
        /// * Column order (header equality)
        /// * Bool case (`False` for `DefaultRoom`)
        /// * f32 → string formatting on `ReverbDryLevel` / `ReverbWetLevel`
        /// * String passthrough across all the context cells
        /// * Empty-cell rendering for absent context types/values
        #[test]
        fn ambient_columns_and_string_formatting() {
            use crate::tables::load_table_relaxed;
            use crate::tables::row_ambient::RowAmbient;

            let mut rows: Vec<RowAmbient> =
                load_table_relaxed(Path::new("test_data/ambient_zm_karelia.csv"))
                    .expect("ambient csv");
            rows.sort_by(|a, b| a.name.cmp(&b.name));

            let out = tmp("zm_karelia.all.ambient.sz");
            write_ambient_table(&out, rows.iter()).expect("write");

            let mut ref_reader =
                csv::Reader::from_path(format!("{}/zm_karelia.all.ambient.sz", REF_DIR))
                    .expect("ref reader");
            let ref_headers: Vec<String> = ref_reader
                .headers()
                .expect("ref headers")
                .iter()
                .map(String::from)
                .collect();
            let mut ref_by_name: std::collections::HashMap<String, Vec<String>> =
                std::collections::HashMap::new();
            for rec in ref_reader.records() {
                let rec = rec.expect("ref record");
                let cells: Vec<String> = rec.iter().map(String::from).collect();
                ref_by_name.insert(cells[0].clone(), cells);
            }

            let mut our_reader = csv::Reader::from_path(&out).expect("our reader");
            let our_headers: Vec<String> = our_reader
                .headers()
                .expect("our headers")
                .iter()
                .map(String::from)
                .collect();
            assert_eq!(our_headers, ref_headers, "header mismatch");

            // Compare every column except the trailing 3 RowSource fields,
            // which our struct doesn't track yet.
            let compare_range = 0..(our_headers.len() - 3);
            let mut compared = 0;
            for rec in our_reader.records() {
                let rec = rec.expect("our record");
                let cells: Vec<String> = rec.iter().map(String::from).collect();
                let name = &cells[0];
                let ref_cells = ref_by_name
                    .get(name)
                    .unwrap_or_else(|| panic!("missing '{}' in reference", name));
                for i in compare_range.clone() {
                    assert_eq!(
                        cells[i], ref_cells[i],
                        "column {} ({}) for '{}': ours={:?} ref={:?}",
                        i, our_headers[i], name, cells[i], ref_cells[i]
                    );
                }
                compared += 1;
            }
            assert_eq!(compared, rows.len(), "every input row got compared");
        }

        /// `reverb.sz` parity — structural rather than byte-equal:
        /// * The reference contains duplicate rows for reverbs referenced
        ///   by multiple ambients (baseline `reverbs.Add` re-pushes the
        ///   same row each time). Our test writes one row per source
        ///   reverb, so row counts differ.
        /// * Our `RowReverb` struct doesn't track `RowSourceFileName` /
        ///   `RowSourceShortName` / `RowSourceLineNumber`, so those
        ///   columns come through empty.
        /// What this test *does* prove: header order matches, column
        /// count matches, and every numeric cell formats byte-identically
        /// to baseline (catches f32 → string drift like `0` vs `0.0`,
        /// `5.1` vs `5.099999`, `-13` vs `-13.0`).
        #[test]
        fn reverb_columns_and_numeric_formatting() {
            use crate::tables::load_table;
            use crate::tables::row_reverb::RowReverb;

            let karelia: Vec<RowReverb> =
                load_table(Path::new("test_data/zm_karelia_reverb.csv")).expect("karelia csv");
            let common: Vec<RowReverb> =
                load_table(Path::new("test_data/common_reverb.csv")).expect("common csv");

            // The reference output references one reverb from common
            // (`global_urban_outdoor`) plus all 5 from the karelia file.
            // Pull that exact set out of our inputs.
            let wanted: BTreeSet<&str> = [
                "karelia_gas_station",
                "karelia_barn",
                "karelia_bunker_corridor",
                "karelia_bunker_large_room",
                "karelia_green_house",
                "global_urban_outdoor",
            ]
            .into_iter()
            .collect();
            let rows: Vec<&RowReverb> = karelia
                .iter()
                .chain(common.iter())
                .filter(|r| wanted.contains(r.name.as_str()))
                .collect();
            assert_eq!(
                rows.len(),
                wanted.len(),
                "every wanted reverb should be present in the input fixtures"
            );

            let out = tmp("zm_karelia.all.reverb.sz");
            write_reverb_table(&out, rows.iter().copied()).expect("write");

            // Index reference rows by Name → first cell-vector. We compare
            // the numeric range only (cols 1..=23, skipping the trailing
            // RowSource columns that we don't populate).
            let mut ref_reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_path(format!("{}/zm_karelia.all.reverb.sz", REF_DIR))
                .expect("ref reader");
            let ref_headers: Vec<String> = ref_reader
                .headers()
                .expect("ref headers")
                .iter()
                .map(String::from)
                .collect();
            let mut ref_by_name: std::collections::HashMap<String, Vec<String>> =
                std::collections::HashMap::new();
            for rec in ref_reader.records() {
                let rec = rec.expect("ref record");
                let cells: Vec<String> = rec.iter().map(String::from).collect();
                ref_by_name.entry(cells[0].clone()).or_insert(cells);
            }

            let mut our_reader = csv::Reader::from_path(&out).expect("our reader");
            let our_headers: Vec<String> = our_reader
                .headers()
                .expect("our headers")
                .iter()
                .map(String::from)
                .collect();
            assert_eq!(our_headers, ref_headers, "header mismatch");

            const NUMERIC_RANGE: std::ops::RangeInclusive<usize> = 1..=23;
            let mut compared = 0;
            for rec in our_reader.records() {
                let rec = rec.expect("our record");
                let cells: Vec<String> = rec.iter().map(String::from).collect();
                let name = &cells[0];
                let ref_cells = ref_by_name
                    .get(name)
                    .unwrap_or_else(|| panic!("missing '{}' in reference", name));
                for i in NUMERIC_RANGE {
                    assert_eq!(
                        cells[i], ref_cells[i],
                        "column {} ({}) for '{}': ours={:?} ref={:?}",
                        i, our_headers[i], name, cells[i], ref_cells[i]
                    );
                }
                compared += 1;
            }
            assert_eq!(compared, wanted.len(), "every wanted row got compared");
        }
    }
}
