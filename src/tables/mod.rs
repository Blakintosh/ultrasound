pub mod row_locale;
pub mod row_platform;
pub mod row_ambient;
pub mod row_reverb;
pub mod alias_enums;
pub mod row_alias;

use std::fmt::Display;
use std::path::Path;
use std::str::FromStr;

pub trait Row {
    fn get_row_name(&self) -> &str;
}

pub fn load_table<T: for<'de> serde::Deserialize<'de>>(path: &Path) -> Result<Vec<T>, String> {
    let mut reader = csv::Reader::from_path(path).map_err(|e| format!("Failed to open file {}: {}", path.display(), e))?;

    let mut rows = Vec::new();
    for result in reader.deserialize() {
        let row = result.map_err(|e| format!("Failed to deserialize row in file {}: {}", path.display(), e))?;
        rows.push(row);
    }
    Ok(rows)
}

/// Like `load_table` but tolerant of comment lines (starting with `#`),
/// trailing commas, and whitespace around values. Needed for files like the
/// alias CSVs that have section comments and varying column counts.
pub fn load_table_relaxed<T: for<'de> serde::Deserialize<'de>>(path: &Path) -> Result<Vec<T>, String> {
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
        let mut record = result.map_err(|e| format!("Failed to read row in file {}: {}", path.display(), e))?;

        // Skip comment rows. The csv crate's built-in comment option only
        // catches records where `#` is literally the first byte, so it misses
        // quoted cells (`"#section"`) and lines that start with whitespace.
        // Check the first field after parsing + trimming to catch both cases.
        if record.get(0).map(|f| f.starts_with('#')).unwrap_or(false) {
            continue;
        }

        while record.len() < header_len {
            record.push_field("");
        }
        let row: T = record
            .deserialize(Some(&headers))
            .map_err(|e| format!("Failed to deserialize row in file {}: {}", path.display(), e))?;
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
    } else if s.eq_ignore_ascii_case("false") || s.eq_ignore_ascii_case("no") || s == "0" || s.is_empty() {
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
    Err(serde::de::Error::custom(format!("unparseable numeric value: {}", s)))
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