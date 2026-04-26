use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct MusicSet {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "StateArray", default)]
    pub state_array: Vec<MusicState>,
    #[serde(rename = "StateNames", default)]
    pub state_names: Vec<String>,
    #[serde(rename = "LoopIndex", default)]
    pub loop_index: i32,
    #[serde(rename = "FileName", default)]
    pub file_name: String,
}

#[derive(Debug, Deserialize)]
pub struct MusicState {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "IntroAsset", default)]
    pub intro_asset: Option<MusicAsset>,
    #[serde(rename = "ExitAsset", default)]
    pub exit_asset: Option<MusicAsset>,
    #[serde(rename = "LoopAssets", default)]
    pub loop_assets: Vec<MusicAsset>,
    #[serde(rename = "LoopIndex", default)]
    pub loop_index: i32,
    #[serde(rename = "IsRandom", default)]
    pub is_random: bool,
    #[serde(rename = "IsSequential", default)]
    pub is_sequential: bool,
    #[serde(rename = "SkipPreviousExit", default)]
    pub skip_previous_exit: bool,
    #[serde(rename = "Order", default)]
    pub order: i32,
}

#[derive(Debug, Deserialize)]
pub struct MusicAsset {
    #[serde(rename = "SourceWaveName", default)]
    pub source_wave_name: String,
    #[serde(rename = "TargetWaveName", default)]
    pub target_wave_name: String,
    #[serde(rename = "AliasName", default)]
    pub alias_name: String,
    #[serde(rename = "Volume", default)]
    pub volume: String,
    #[serde(rename = "BPM", default)]
    pub bpm: i32,
    #[serde(rename = "AssetType", default)]
    pub asset_type: i32,
    #[serde(rename = "Looping", default)]
    pub looping: bool,
    #[serde(rename = "LoopNumber", default)]
    pub loop_number: i32,
    #[serde(rename = "LoopStartOffset", default)]
    pub loop_start_offset: u32,
    #[serde(rename = "CompleteLoop", default)]
    pub complete_loop: bool,
    #[serde(rename = "RemoveAfterPlay", default)]
    pub remove_after_play: bool,
    #[serde(rename = "PlayAsFirstRandom", default)]
    pub play_as_first_random: bool,
    #[serde(rename = "Order", default)]
    pub order: i32,
    #[serde(rename = "Channels", default)]
    pub channels: i32,
    #[serde(rename = "CompleteOnStop", default)]
    pub complete_on_stop: bool,
    #[serde(rename = "StartSync", default)]
    pub start_sync: bool,
    #[serde(rename = "StartSyncBeats", default)]
    pub start_sync_beats: i32,
    #[serde(rename = "StartDelayBeats", default)]
    pub start_delay_beats: i32,
    #[serde(rename = "StartFadeBeats", default)]
    pub start_fade_beats: i32,
    #[serde(rename = "StopSync", default)]
    pub stop_sync: bool,
    #[serde(rename = "StopSyncBeats", default)]
    pub stop_sync_beats: i32,
    #[serde(rename = "StopDelayBeats", default)]
    pub stop_delay_beats: i32,
    #[serde(rename = "StopFadeBeats", default)]
    pub stop_fade_beats: i32,
    #[serde(rename = "StartOffsetFrames", default)]
    pub start_offset_frames: i32,
    #[serde(rename = "Meter", default)]
    pub meter: i32,
    #[serde(rename = "Template", default)]
    pub template: String,
    #[serde(rename = "ParentStateName", default)]
    pub parent_state_name: String,
}

impl MusicSet {
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Err(format!("Music file does not exist: {}", path.display()));
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
    fn load_real_mus() {
        let set = MusicSet::load(Path::new("test_data/zm_karelia.mus")).expect("load");
        assert!(!set.name.is_empty());
        assert!(!set.state_array.is_empty());
        println!(
            "Loaded music set '{}' with {} states, {} state names, loop_index={}",
            set.name,
            set.state_array.len(),
            set.state_names.len(),
            set.loop_index
        );
        for s in set.state_array.iter().take(3) {
            println!("  state {} ({} loop assets)", s.name, s.loop_assets.len());
        }
    }
}
