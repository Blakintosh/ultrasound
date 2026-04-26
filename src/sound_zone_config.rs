use std::collections::HashSet;
use std::path::Path;

use serde::Deserialize;

use crate::converter::CompressionLevel;

#[derive(Deserialize, Default)]
pub struct SoundZoneConfig {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Parent", default)]
    pub parent: String,
    #[serde(rename = "IsStandalone", default)]
    pub is_standalone: bool,
    #[serde(rename = "IsShipped", default)]
    pub is_shipped: bool,
    #[serde(rename = "NoStreamBank", default)]
    pub no_stream_bank: bool,
    #[serde(rename = "OptimizePoint", default)]
    pub optimize_point: bool,
    #[serde(rename = "MapFile", default)]
    pub map_file: String,
    #[serde(rename = "Sources", default)]
    pub sources: Vec<SoundZoneSource>,
    #[serde(rename = "Ducks", default)]
    pub ducks: Vec<String>,
    #[serde(rename = "MusicFiles", default)]
    pub music_files: Vec<String>,
    /// Default lossy-compression level applied to every asset in this
    /// zone unless overridden per-alias. Omitted/absent → `None` (no
    /// lossy processing). Valid values (case-invariant): `None`, `Low`,
    /// `Medium`, `High`, `Extreme`.
    #[serde(rename = "DefaultAudioCompression", default)]
    pub default_audio_compression: CompressionLevel,
}

impl SoundZoneConfig {
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Err(format!("Sound zone does not exist: {}", path.display()));
        }

        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

        let mut config: SoundZoneConfig = json5::from_str(&text)
            .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;

        if config.parent == config.name {
            config.parent = String::new();
        }

        Ok(config)
    }
}

#[derive(Deserialize)]
pub struct SoundZoneSource {
    #[serde(rename = "Type")]
    pub source_type: SoundZoneTableType,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Filename", default)]
    pub filename: String,
    #[serde(rename = "Specs", default)]
    pub specs: HashSet<String>,
    /// Source-entry override for the zone's `DefaultAudioCompression`.
    /// Applies to every alias inside this source's CSV that doesn't have
    /// its own `CompressionLevel` column set. Alias row overrides still
    /// win, so per-entry tuning (e.g. VO vs SFX inside the same file) is
    /// still possible. Absent → inherit the zone default.
    #[serde(rename = "DefaultAudioCompression", default)]
    pub default_audio_compression: Option<CompressionLevel>,
}

#[derive(Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SoundZoneTableType {
    Radverb,
    Ambient,
    Alias,
    Duck,
    Music,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_real_szc() {
        let path = Path::new("test_data/zm_karelia.szc");
        let config = SoundZoneConfig::load(path).expect("load");
        assert!(!config.name.is_empty(), "name should be populated");
        println!(
            "Loaded zone '{}' (parent='{}', {} sources, {} ducks, {} music)",
            config.name,
            config.parent,
            config.sources.len(),
            config.ducks.len(),
            config.music_files.len()
        );
    }
}
