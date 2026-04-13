use serde::Deserialize;
use super::Row;

#[derive(Clone, Deserialize)]
pub struct RowLocale {
    #[serde(rename = "Name")]
    pub name: String,

    #[serde(rename = "SearchName")]
    pub search_name: String,

    #[serde(rename = "DeployName")]
    pub deploy_name: String,

    #[serde(rename = "CacheName")]
    pub cache_name: String,

    #[serde(rename = "IsShared", deserialize_with = "super::bool_from_string")]
    pub is_shared: bool,

    #[serde(rename = "CompressionScale")]
    pub compression_scale: f32,
}

impl RowLocale {
    pub fn scale_compression(&self, value: i32) -> i32 {
        ((value as f32) * (100.0 * self.compression_scale).floor() / 100.0).floor() as i32
    }
}

impl Row for RowLocale {
    fn get_row_name(&self) -> &str {
        &self.name
    }
}