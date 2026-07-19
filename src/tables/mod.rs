pub mod alias_enums;
pub mod row_alias;
pub mod row_ambient;
pub mod row_locale;
pub mod row_platform;
pub mod row_reverb;
pub mod row_script_id_lookup;

use std::fmt::Display;
use std::path::Path;
use std::str::FromStr;

pub trait Row {
    fn get_row_name(&self) -> &str;
}

pub fn load_table<T: for<'de> serde::Deserialize<'de>>(path: &Path) -> Result<Vec<T>, String> {
    let mut reader = csv::Reader::from_path(path)
        .map_err(|e| format!("Failed to open file {}: {}", path.display(), e))?;

    let mut rows = Vec::new();
    for result in reader.deserialize() {
        let row = result.map_err(|e| {
            format!(
                "Failed to deserialize row in file {}: {}",
                path.display(),
                e
            )
        })?;
        rows.push(row);
    }
    Ok(rows)
}

/// Like `load_table` but tolerant of comment lines (starting with `#`),
/// trailing commas, and whitespace around values. Needed for files like the
/// alias CSVs that have section comments and varying column counts.
pub fn load_table_relaxed<T: for<'de> serde::Deserialize<'de>>(
    path: &Path,
) -> Result<Vec<T>, String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .comment(Some(b'#'))
        .flexible(true)
        .trim(csv::Trim::All)
        .from_path(path)
        .map_err(|e| format!("Failed to open file {}: {}", path.display(), e))?;

    // Clone the header so we can reuse it when padding short records. Every
    // record gets padded out to header width with empty strings so that
    // serde fields with `#[serde(default)]` receive empty input rather than
    // an "end of row" error — real content frequently trails off early.
    let headers = reader
        .headers()
        .map_err(|e| format!("Failed to read headers in {}: {}", path.display(), e))?
        .clone();
    let header_len = headers.len();

    let mut rows = Vec::new();
    for result in reader.records() {
        let mut record =
            result.map_err(|e| format!("Failed to read row in file {}: {}", path.display(), e))?;

        // Skip blank and comment rows. A row is dropped whenever its first
        // (key) cell — `Name` / `ScriptId` — is empty or begins with a
        // comment marker. This covers blank lines, comma-only spacer rows,
        // rows whose Name was left empty, and commented-out rows, so none of
        // them deserialize into an all-default row with an empty Name (which
        // a downstream consumer would then reject as a malformed alias). The
        // csv crate's built-in comment option only catches `#` as the literal
        // first byte, so the explicit check below also handles quoted cells
        // (`"#section"`) and lines that start with whitespace, post-trim.
        match record.get(0).map(str::trim) {
            None | Some("") => continue,
            Some(f) if f.starts_with('#') || f.starts_with('"') => continue,
            _ => {}
        }

        while record.len() < header_len {
            record.push_field("");
        }
        let row: T = record.deserialize(Some(&headers)).map_err(|e| {
            format!(
                "Failed to deserialize row in file {}: {}",
                path.display(),
                e
            )
        })?;
        rows.push(row);
    }
    Ok(rows)
}

pub fn bool_from_string<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: &str = serde::Deserialize::deserialize(deserializer)?;
    if s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("yes") || s == "1" {
        Ok(true)
    } else if s.eq_ignore_ascii_case("false")
        || s.eq_ignore_ascii_case("no")
        || s == "0"
        || s.is_empty()
    {
        Ok(false)
    } else {
        Err(serde::de::Error::custom(format!("invalid bool: {}", s)))
    }
}

/// Deserializes an empty CSV cell into `None`, otherwise parses the value via `FromStr`.
/// Use on every nullable numeric field in row structs. For integer fields,
/// falls back to parsing as `f64` and truncating — real game CSVs have things
/// like `PriorityMin=-99.95` in int-declared columns, and the format is
/// intentionally lenient about that.
pub fn empty_as_none<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: FromStr + FromF64,
    T::Err: Display,
{
    let s: &str = serde::Deserialize::deserialize(deserializer)?;
    if s.is_empty() {
        return Ok(None);
    }
    if let Ok(v) = s.parse::<T>() {
        return Ok(Some(v));
    }
    // Fallback: float → target.
    if let Ok(f) = s.parse::<f64>() {
        return Ok(Some(T::from_f64(f)));
    }
    Err(serde::de::Error::custom(format!(
        "unparseable numeric value: {}",
        s
    )))
}

/// Helper trait so `empty_as_none` can coerce an f64 fallback into the target
/// numeric type. Integer impls truncate, float impls cast.
pub trait FromF64 {
    fn from_f64(v: f64) -> Self;
}

macro_rules! impl_from_f64 {
    ($($t:ty),*) => {
        $(impl FromF64 for $t { fn from_f64(v: f64) -> Self { v as $t } })*
    };
}
impl_from_f64!(i32, i64, u32, u64, f32, f64);

/// Case-insensitive enum deserializer. Uppercases the input before deserializing,
/// so any CSV value like `bus_fx`, `Bus_Fx`, or `BUS_FX` all resolve to the same variant.
/// The target enum must use `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]`.
/// Empty strings deserialize to `None`.
pub fn opt_enum_upper<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    use serde::de::IntoDeserializer;
    let s: String = serde::Deserialize::deserialize(deserializer)?;
    if s.is_empty() {
        return Ok(None);
    }
    let upper = s.to_uppercase();
    T::deserialize(upper.as_str().into_deserializer())
        .map(Some)
        .map_err(|e: serde::de::value::Error| serde::de::Error::custom(e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct TestRow {
        #[serde(rename = "Name", default)]
        name: String,
        #[serde(rename = "Value", default)]
        #[allow(dead_code)]
        value: String,
    }

    /// A blank line, a comma-only spacer, a row with an empty Name, and the
    /// three comment shapes (`#` at start, whitespace-led `#`, comma-led `#`)
    /// must all be dropped — only the two genuinely-named rows survive. This
    /// is the guard against an empty source row becoming a phantom all-default
    /// alias once column defaults are applied.
    #[test]
    fn relaxed_loader_drops_blank_and_comment_rows() {
        let dir = std::env::temp_dir().join("ultrasound_relaxed_skip_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rows.csv");
        let csv = "Name,Value\n\
                   alpha,1\n\
                   \n\
                   ,,\n\
                   ,has_value_but_no_name\n\
                   # a comment\n\
                   \t# indented comment\n\
                   ,# comma-led comment\n\
                   beta,2\n";
        std::fs::write(&path, csv).unwrap();

        let rows: Vec<TestRow> = load_table_relaxed(&path).unwrap();
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["alpha", "beta"]);
    }
}
