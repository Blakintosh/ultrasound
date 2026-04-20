use std::fs;
use std::path::Path;

use std::str::FromStr;

use serde::Deserialize;

use crate::{
    asset_types::AssetEnvelope,
    bank::bank_entry::BankEntry,
    decoded_audio::DecodedAudio,
    flac, ogg,
    riff::Riff,
    source_asset_cache::Checksum,
    tables::{row_locale::RowLocale, row_platform::RowPlatform},
};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AliasLooping {
    Looping,
    NonLooping,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AliasStorage {
    Loaded,
    Streamed,
    Primed,
}

pub enum AssetFormat {
    SndAssetFormatPcms16 = 0,
    SndAssetFormatFlac = 8,
}

/// Per-asset lossy-compression budget. Picks a bit-depth truncation level
/// and a silence-gate threshold. Lower levels preserve more detail; higher
/// levels shrink the bank at the cost of noise floor and quiet tails.
#[derive(Copy, Clone, Debug, Default, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(try_from = "String")]
pub enum CompressionLevel {
    #[default]
    None,
    Low,
    Medium,
    High,
    Extreme,
}

impl CompressionLevel {
    /// Mask AND-ed with each i16 sample to discard low-order bits. `None`
    /// leaves the sample untouched.
    pub fn truncate_mask(self) -> i16 {
        match self {
            CompressionLevel::None => !0x0000,
            CompressionLevel::Low => !0x0003,     // 14-bit
            CompressionLevel::Medium => !0x000F,  // 12-bit
            CompressionLevel::High => !0x003F,    // 10-bit
            CompressionLevel::Extreme => !0x00FF, //  8-bit
        }
    }

    /// Silence-gate peak-amplitude threshold. Returns `None` for levels that
    /// skip the gate entirely. Scaled roughly to 4× the truncation step so
    /// the gate only ever chops content that's already near or below the
    /// quantisation noise floor.
    pub fn silence_gate_threshold(self) -> Option<i16> {
        match self {
            CompressionLevel::None => None,
            CompressionLevel::Low => Some(128),      // ~-48 dB
            CompressionLevel::Medium => Some(256),   // ~-42 dB
            CompressionLevel::High => Some(256),     // ~-42 dB
            CompressionLevel::Extreme => Some(1024), // ~-30 dB
        }
    }

    /// Frame count per gate window. Fixed across levels — 128 frames ≈
    /// 2.7 ms at 48 kHz, the shortest "silence" worth detecting.
    pub const GATE_WINDOW_FRAMES: usize = 128;

    /// Stable byte encoding of the level's *actual* compression parameters
    /// (truncation mask, gate window, gate on/off, gate threshold). Fed into
    /// the source checksum so that any retune — switching an asset's level
    /// *or* changing what a level means in code — invalidates the bank entry
    /// and forces reconversion. Update this if you add a new dimension.
    pub fn recipe_fingerprint(self) -> [u8; 13] {
        let mask = self.truncate_mask().to_le_bytes();
        let gate_window = (Self::GATE_WINDOW_FRAMES as u64).to_le_bytes();
        let (has_gate, threshold) = match self.silence_gate_threshold() {
            Some(t) => (1u8, t),
            None => (0u8, 0),
        };
        let threshold = threshold.to_le_bytes();
        [
            mask[0],
            mask[1],
            gate_window[0],
            gate_window[1],
            gate_window[2],
            gate_window[3],
            gate_window[4],
            gate_window[5],
            gate_window[6],
            gate_window[7],
            has_gate,
            threshold[0],
            threshold[1],
        ]
    }
}

impl FromStr for CompressionLevel {
    type Err = String;

    /// Case-invariant. Accepts the five canonical names only. Used by the
    /// top-level SZC deserializer via `TryFrom<String>` and by the alias
    /// CSV column parser after its own alias handling.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" => Ok(Self::None),
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "extreme" => Ok(Self::Extreme),
            other => Err(format!(
                "unknown compression level '{}' (expected None, Low, Medium, High, Extreme)",
                other
            )),
        }
    }
}

impl TryFrom<String> for CompressionLevel {
    type Error = String;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

pub struct SoundAssetBankSourceAsset {
    pub source_name: String,
    pub looping: AliasLooping,
    pub storage: AliasStorage,
    pub compression: i32,
    pub compression_level: CompressionLevel,
    pub locale: RowLocale,
    pub platform: RowPlatform,
    pub converted_name: String,
}

pub struct SoundAssetBankConvertedAsset {
    pub name: String,
    pub converted_checksum: Checksum,
    pub source_checksum: Checksum,
    pub format: AssetFormat,
    pub looping: AliasLooping,
    pub frame_count: i64,
    pub frame_rate: i32,
    pub channel_count: i32,
    pub envelope_loudness0: u8,
    pub envelope_loudness1: u8,
    pub envelope_loudness2: u8,
    pub envelope_loudness3: u8,
    pub envelope_time1: u16,
    pub envelope_time2: u16,
}

impl SoundAssetBankConvertedAsset {
    pub fn new(
        name: &str,
        converted_checksum: Checksum,
        source_checksum: Checksum,
        format: AssetFormat,
        looping: AliasLooping,
        frame_count: i64,
        frame_rate: i32,
        channel_count: i32,
        envelope_loudness0: u8,
        envelope_loudness1: u8,
        envelope_loudness2: u8,
        envelope_loudness3: u8,
        envelope_time1: u16,
        envelope_time2: u16,
    ) -> Self {
        Self {
            name: name.to_string(),
            converted_checksum,
            source_checksum,
            format,
            looping,
            frame_count,
            frame_rate,
            channel_count,
            envelope_loudness0,
            envelope_loudness1,
            envelope_loudness2,
            envelope_loudness3,
            envelope_time1,
            envelope_time2,
        }
    }
}

/// Single-pass source → converted asset. Reads the source file exactly once,
/// computes source checksum + decode + envelope + resample + FLAC encode from
/// the same buffer. Supports both WAV and FLAC input.
pub fn convert_source_inline(
    asset: &SoundAssetBankSourceAsset,
) -> Result<(SoundAssetBankConvertedAsset, Vec<u8>), String> {
    if asset.platform.platform != "pc" {
        return Err(format!("Unsupported platform: {}", asset.platform.platform));
    }

    let data = fs::read(&asset.source_name)
        .map_err(|e| format!("Failed to read file {}: {}", asset.source_name, e))?;
    // Recipe-aware source checksum: mixes the file bytes with the level's
    // compression fingerprint so retuning (or reassigning) a level
    // invalidates the matching bank entry on the next run.
    let source_checksum =
        Checksum::from_data_with_recipe(&data, &asset.compression_level.recipe_fingerprint());

    let ext = Path::new(&asset.source_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let audio = if ext.eq_ignore_ascii_case("flac") {
        let decoded = flac::decode(&asset.source_name, &data)?;
        drop(data);
        decoded
    } else if ext.eq_ignore_ascii_case("ogg") {
        let decoded = ogg::decode(&asset.source_name, &data)?;
        drop(data);
        decoded
    } else {
        let riff = Riff::parse(&asset.source_name, &data)?;

        if riff.format != 1 && riff.format != 65534 {
            return Err(format!(
                "Unsupported audio format in {}: {}",
                asset.source_name, riff.format
            ));
        }
        if riff.frame_size / riff.channel_count != 2 {
            return Err(format!(
                "Unsupported bit depth (requires 16-bit) in {}: {}",
                asset.source_name,
                riff.frame_size * 8 / riff.channel_count
            ));
        }

        let samples = if riff.frame_count == 0 {
            Vec::new()
        } else {
            riff.decode_interleaved_s16_from_slice(&data)?
        };
        drop(data);

        DecodedAudio {
            samples,
            frame_rate: riff.frame_rate,
            channel_count: riff.channel_count,
            frame_count: riff.frame_count,
        }
    };

    // Common validation for both paths.
    if BankEntry::lookup_frame_rate_index(audio.frame_rate as i32) == u8::MAX {
        return Err(format!(
            "Unsupported sample rate in {}: {}",
            asset.source_name, audio.frame_rate
        ));
    }
    if audio.channel_count > 2 {
        return Err(format!(
            "Unsupported channel count (requires 1-2 channels) in {}: {}",
            asset.source_name, audio.channel_count
        ));
    }

    let envelope =
        AssetEnvelope::envelope_extract(audio.frame_count, audio.channel_count, &audio.samples);

    // Resample + FLAC encode. Empty audio → empty FLAC payload.
    let flac_out = if audio.frame_count == 0 {
        Vec::new()
    } else {
        let mut frame_rate = audio.frame_rate as i32;
        let mut frame_count = audio.frame_count as i32;
        let mut pcm = if frame_rate != 48000 {
            frame_count = (audio.frame_count * 48000 / frame_rate as u64) as i32;
            let resampled = Riff::resize(
                &audio.samples,
                audio.channel_count as u32,
                frame_count as u32,
            );
            frame_rate = 48000;
            resampled
        } else {
            audio.samples
        };
        // Lossy pre-passes driven by the asset's compression level. Silence
        // gate first (zero long quiet runs so FLAC encodes them as flat
        // subframes), then bit-depth truncation (raises the noise floor
        // but gives FLAC's predictor much smaller residuals to Rice-code).
        let level = asset.compression_level;

        if let Some(threshold) = level.silence_gate_threshold() {
            silence_gate(
                &mut pcm,
                audio.channel_count as usize,
                CompressionLevel::GATE_WINDOW_FRAMES,
                threshold,
            );
        }

        let mask = level.truncate_mask();
        if mask != !0 {
            for s in &mut pcm {
                *s &= mask;
            }
        }
        flac::encode(
            &asset.source_name,
            audio.channel_count as i32,
            frame_rate,
            frame_count,
            &pcm,
        )?
    };

    let converted_frame_count = if audio.frame_rate != 48000 {
        (audio.frame_count * 48000 / audio.frame_rate as u64) as i64
    } else {
        audio.frame_count as i64
    };

    Ok((
        SoundAssetBankConvertedAsset {
            name: asset.converted_name.clone(),
            converted_checksum: Checksum::from_data(&flac_out),
            source_checksum,
            format: AssetFormat::SndAssetFormatFlac,
            looping: match asset.looping {
                AliasLooping::Looping => AliasLooping::Looping,
                AliasLooping::NonLooping => AliasLooping::NonLooping,
            },
            frame_count: converted_frame_count,
            frame_rate: 48000,
            channel_count: audio.channel_count as i32,
            envelope_loudness0: envelope.left[0] as u8,
            envelope_loudness1: envelope.left[1] as u8,
            envelope_loudness2: envelope.left[2] as u8,
            envelope_loudness3: envelope.left[3] as u8,
            envelope_time1: (u16::MAX as f64 * envelope.time[1]) as u16,
            envelope_time2: (u16::MAX as f64 * envelope.time[2]) as u16,
        },
        flac_out,
    ))
}

/// Windowed silence gate over interleaved i16 PCM. Walks the signal in
/// `window_frames`-frame blocks; within each block, if no sample on any
/// channel exceeds `threshold` in absolute value, zero the entire block.
/// Zero runs compress to almost nothing in FLAC (constant subframes),
/// so this reclaims space from quiet tails without touching loud content.
fn silence_gate(pcm: &mut [i16], channels: usize, window_frames: usize, threshold: i16) {
    if channels == 0 || window_frames == 0 || pcm.is_empty() {
        return;
    }
    let window_samples = window_frames * channels;
    let thr = threshold as i32;
    for window in pcm.chunks_mut(window_samples) {
        let loud = window.iter().any(|s| (*s as i32).abs() >= thr);
        if !loud {
            for s in window.iter_mut() {
                *s = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_fingerprint_encodes_gate_window() {
        let fingerprint = CompressionLevel::High.recipe_fingerprint();

        let mut encoded_window = [0u8; 8];
        encoded_window.copy_from_slice(&fingerprint[2..10]);

        assert_eq!(fingerprint.len(), 13);
        assert_eq!(
            u64::from_le_bytes(encoded_window),
            CompressionLevel::GATE_WINDOW_FRAMES as u64
        );
        assert_eq!(fingerprint[10], 1);
    }
}
