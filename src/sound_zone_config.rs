use std::collections::HashSet;
use std::path::Path;

use serde::Deserialize;

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
