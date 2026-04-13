use serde::Deserialize;
use super::Row;

#[derive(Clone, Deserialize)]
pub struct RowPlatform {
    #[serde(rename = "Platform")]
    pub platform: String,

    #[serde(rename = "ConvertedAssetVersion")]
    pub converted_asset_version: i32,

    #[serde(rename = "ConvertThreadCount")]
    pub convert_thread_count: i32,

    #[serde(rename = "CompressionScale")]
    pub compression_scale: f32,
}

impl RowPlatform {
    pub fn scale_compression(&self, value: i32) -> i32 {
        ((value as f32) * (100.0 * self.compression_scale).floor() / 100.0).floor() as i32
    }

    pub fn get_extension(&self) -> String {
        format!(".{}.snd", self.platform)
    }
}

impl Row for RowPlatform {
    fn get_row_name(&self) -> &str {
        &self.platform
    }
}