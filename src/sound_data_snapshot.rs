use std::collections::HashMap;

use crate::converted_asset_cache::ConvertedAssetCache;
use crate::env::Env;
use crate::source_asset_cache::SourceAssetCache;
use crate::tables::row_alias::RowAlias;
use crate::tables::row_locale::RowLocale;
use crate::tables::row_platform::RowPlatform;
use crate::tables::{load_table, load_table_relaxed};

/// Per-run global state. Holds everything loaded once at startup and borrowed
/// mutably through the pipeline (source cache gets updated; template map is
/// read-only after construction).
pub struct SoundDataSnapshot {
    pub env: Env,
    pub platforms: Vec<RowPlatform>,
    pub locales: Vec<RowLocale>,
    pub alias_templates: HashMap<String, RowAlias>,
    pub source_asset_cache: SourceAssetCache,
    pub converted_asset_cache: ConvertedAssetCache,
}

impl SoundDataSnapshot {
    pub fn new(env: Env) -> Result<Self, String> {
        let platforms =
            load_table::<RowPlatform>(&env.get_sound_globals_dir().join("platform.csv"))?;
        let locales = load_table::<RowLocale>(&env.get_sound_globals_dir().join("locale.csv"))?;

        let alias_templates = load_alias_templates(&env)?;

        Ok(Self {
            env,
            platforms,
            locales,
            alias_templates,
            source_asset_cache: SourceAssetCache::new(),
            converted_asset_cache: ConvertedAssetCache::new(),
        })
    }

    pub fn get_platform(&self, name: &str) -> Option<&RowPlatform> {
        self.platforms.iter().find(|p| p.platform == name)
    }

    pub fn get_locale(&self, name: &str) -> Option<&RowLocale> {
        self.locales.iter().find(|l| l.name == name)
    }
}

/// Load every CSV under the templates dir and merge into one name → RowAlias map.
/// Each file may define multiple templates (one per row); the `Name` column is the key.
fn load_alias_templates(env: &Env) -> Result<HashMap<String, RowAlias>, String> {
    let dir = env.get_sound_alias_template_dir();
    let mut map = HashMap::new();

    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Ok(map), // No templates dir — fine, nothing to load.
    };

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read templates dir: {}", e))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("csv") {
            continue;
        }
        let rows: Vec<RowAlias> = load_table_relaxed(&path)?;
        for row in rows {
            // Template name lookup is case-insensitive — normalize keys to
            // lowercase on both insert and lookup.
            map.insert(row.name.to_ascii_lowercase(), row);
        }
    }

    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Standalone test: loads the template CSV we have in test_data directly,
    /// bypassing Env, to prove the parser handles the templates file shape.
    #[test]
    fn load_template_csv_directly() {
        let path = PathBuf::from("test_data/template_rottsky.csv");
        let rows: Vec<RowAlias> = load_table_relaxed(&path).expect("load templates");
        assert!(!rows.is_empty(), "should have at least one template row");
        println!("Loaded {} template rows", rows.len());
        for r in rows.iter().take(3) {
            println!("  template '{}' file_spec='{}'", r.name, r.file_spec);
        }
    }
}
