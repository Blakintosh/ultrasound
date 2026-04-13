use serde::Deserialize;
use crate::tables::bool_from_string;

#[derive(Debug, Deserialize)]
pub struct RowAmbient {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Loadspec", default)]
    pub loadspec: String,
    #[serde(rename = "DefaultRoom", default, deserialize_with = "bool_from_string")]
    pub default_room: bool,
    #[serde(rename = "Reverb", default = "default_reverb")]
    pub reverb: String,
    #[serde(rename = "ReverbDryLevel", default = "default_one")]
    pub reverb_dry_level: f32,
    #[serde(rename = "ReverbWetLevel", default = "default_one")]
    pub reverb_wet_level: f32,
    #[serde(rename = "Loop")]
    pub loop_: String,
    #[serde(rename = "Duck", default = "default_default")]
    pub duck: String,
    #[serde(rename = "EntityContextType0", default)]
    pub entity_context_type_0: String,
    #[serde(rename = "EntityContextValue0", default)]
    pub entity_context_value_0: String,
    #[serde(rename = "EntityContextType1", default)]
    pub entity_context_type_1: String,
    #[serde(rename = "EntityContextValue1", default)]
    pub entity_context_value_1: String,
    #[serde(rename = "EntityContextType2", default)]
    pub entity_context_type_2: String,
    #[serde(rename = "EntityContextValue2", default)]
    pub entity_context_value_2: String,
    #[serde(rename = "GlobalContextType", default)]
    pub global_context_type: String,
    #[serde(rename = "GlobalContextValue", default)]
    pub global_context_value: String,
}

fn default_reverb() -> String { "default".to_string() }
fn default_default() -> String { "default".to_string() }
fn default_one() -> f32 { 1.0 }

impl crate::tables::Row for RowAmbient {
    fn get_row_name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use crate::tables::load_table;

    #[test]
    fn load_real_ambient_csv() {
        let rows: Vec<RowAmbient> =
            load_table(Path::new("test_data/ambient_zm_karelia.csv")).expect("load");
        assert!(!rows.is_empty(), "should have at least one row");
        println!("Loaded {} ambients", rows.len());
        for r in rows.iter().take(3) {
            println!("  {} reverb={} loop={} default_room={}", r.name, r.reverb, r.loop_, r.default_room);
        }
    }
}
