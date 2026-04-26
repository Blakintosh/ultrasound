use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Duck {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "DefaultAllOn", default = "default_true")]
    pub default_all_on: bool,
    #[serde(rename = "FadeIn", default)]
    pub fade_in: f32,
    #[serde(rename = "FadeInCurve", default)]
    pub fade_in_curve: String,
    #[serde(rename = "FadeOut", default)]
    pub fade_out: f32,
    #[serde(rename = "FadeOutCurve", default)]
    pub fade_out_curve: String,
    #[serde(rename = "Distance", default)]
    pub distance: i32,
    #[serde(rename = "Length", default)]
    pub length: f32,
    #[serde(rename = "StartDelay", default)]
    pub start_delay: f32,
    #[serde(rename = "UpdateWhilePaused", default)]
    pub update_while_paused: Option<bool>,
    #[serde(rename = "TrackAmplitude", default)]
    pub track_amplitude: Option<bool>,
    #[serde(rename = "DuckAlias", default)]
    pub duck_alias: Option<String>,
    #[serde(rename = "DuckAliasLpf", default)]
    pub duck_alias_lpf: i32,
    #[serde(rename = "DuckAliasAttenuation", default)]
    pub duck_alias_attenuation: i32,
    #[serde(rename = "DisableInSplitScreen", default)]
    pub disable_in_split_screen: bool,
    #[serde(rename = "Values", default)]
    pub values: Vec<DuckValue>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct DuckValue {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "DuckGroup")]
    pub duck_group: String,
    #[serde(rename = "UseDefaultValue", default)]
    pub use_default_value: bool,
    #[serde(rename = "Lpf", default)]
    pub lpf: i32,
    #[serde(rename = "Attenuation", default)]
    pub attenuation: i32,
}

impl Duck {
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Err(format!("Duck file does not exist: {}", path.display()));
        }
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        json5::from_str(&text).map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_real_duk() {
        let duck = Duck::load(Path::new("test_data/blak_health_low.duk")).expect("load");
        assert!(!duck.name.is_empty());
        assert!(!duck.values.is_empty());
        println!(
            "Loaded duck '{}' with {} values",
            duck.name,
            duck.values.len()
        );
        for v in duck.values.iter().take(3) {
            println!(
                "  {} group={} lpf={} atten={}",
                v.name, v.duck_group, v.lpf, v.attenuation
            );
        }
    }
}
