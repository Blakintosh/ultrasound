/// Format-agnostic container for decoded audio data.
/// Both WAV (via `Riff`) and FLAC decode paths produce this.
pub struct DecodedAudio {
    pub samples: Vec<i16>,
    pub frame_rate: u32,
    pub channel_count: u16,
    pub frame_count: u64,
}
