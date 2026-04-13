use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct RowReverb {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "MasterReturn")]
    pub master_return: i32,
    #[serde(rename = "EarlyInputLpf")]
    pub early_input_lpf: f32,
    #[serde(rename = "EarlyFeedback")]
    pub early_feedback: f32,
    #[serde(rename = "EarlySmear")]
    pub early_smear: f32,
    #[serde(rename = "EarlyBaseDelayMs")]
    pub early_base_delay_ms: i32,
    #[serde(rename = "EarlyPreDelayMs")]
    pub early_pre_delay_ms: i32,
    #[serde(rename = "EarlyReturn")]
    pub early_return: i32,
    #[serde(rename = "NearInputLpf")]
    pub near_input_lpf: f32,
    #[serde(rename = "NearFeedback")]
    pub near_feedback: f32,
    #[serde(rename = "NearReturn")]
    pub near_return: f32,
    #[serde(rename = "NearLowDamp")]
    pub near_low_damp: f32,
    #[serde(rename = "NearHighDamp")]
    pub near_high_damp: f32,
    #[serde(rename = "NearDecayTime")]
    pub near_decay_time: f32,
    #[serde(rename = "NearSmear")]
    pub near_smear: f32,
    #[serde(rename = "NearPreDelayMs")]
    pub near_pre_delay_ms: i32,
    #[serde(rename = "FarInputLpf")]
    pub far_input_lpf: f32,
    #[serde(rename = "FarFeedback")]
    pub far_feedback: f32,
    #[serde(rename = "FarReturn")]
    pub far_return: f32,
    #[serde(rename = "FarLowDamp")]
    pub far_low_damp: f32,
    #[serde(rename = "FarHighDamp")]
    pub far_high_damp: f32,
    #[serde(rename = "FarDecayTime")]
    pub far_decay_time: f32,
    #[serde(rename = "FarSmear")]
    pub far_smear: f32,
    #[serde(rename = "FarPreDelayMs")]
    pub far_pre_delay_ms: i32,
}

impl crate::tables::Row for RowReverb {
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
    fn load_real_reverb_csv() {
        let rows: Vec<RowReverb> =
            load_table(Path::new("test_data/common_reverb.csv")).expect("load");
        assert!(!rows.is_empty(), "should have at least one row");
        println!("Loaded {} reverbs", rows.len());
        for r in rows.iter().take(3) {
            println!("  {} master_return={} near_decay={}", r.name, r.master_return, r.near_decay_time);
        }
    }
}
