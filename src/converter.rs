use std::fs;

use crate::{
    asset_types::AssetEnvelope,
    bank::bank_entry::BankEntry,
    flac::encode,
    riff::Riff,
    source_asset_cache::Checksum,
    tables::{row_locale::RowLocale, row_platform::RowPlatform},
};


pub enum AliasLooping {
    Looping,
    NonLooping
}

pub enum AliasStorage {
    Loaded,
    Streamed,
    Primed
}

pub enum AssetFormat {
    SndAssetFormatPcms16 = 0,
    SndAssetFormatFlac = 8
}

pub struct SoundAssetBankSourceAsset {
    pub source_name: String,
    pub looping: AliasLooping,
    pub storage: AliasStorage,
    pub compression: i32,
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

/// Single-pass source → converted asset. Reads the source WAV exactly once,
/// computes source checksum + RIFF parse + PCM decode + envelope + resample +
/// FLAC encode from the same buffer. Replaces the Phase 8.6 split where
/// `SourceAssetCache::update_sources` and `convert_asset` each re-read the file.
pub fn convert_source_inline(
    asset: &SoundAssetBankSourceAsset,
) -> Result<(SoundAssetBankConvertedAsset, Vec<u8>), String> {
    if asset.platform.platform != "pc" {
        return Err(format!("Unsupported platform: {}", asset.platform.platform));
    }

    let data = fs::read(&asset.source_name)
        .map_err(|e| format!("Failed to read file {}: {}", asset.source_name, e))?;
    let source_checksum = Checksum::from_data(&data);

    let riff = Riff::parse(&asset.source_name, &data)?;

    if riff.format != 1 && riff.format != 65534 {
        return Err(format!("Unsupported audio format in {}: {}", asset.source_name, riff.format));
    }
    if BankEntry::lookup_frame_rate_index(riff.frame_rate as i32) == u8::MAX {
        return Err(format!("Unsupported sample rate in {}: {}", asset.source_name, riff.frame_rate));
    }
    if riff.frame_size / riff.channel_count != 2 {
        return Err(format!(
            "Unsupported bit depth (requires 16-bit) in {}: {}",
            asset.source_name, riff.frame_size * 8 / riff.channel_count
        ));
    }
    if riff.channel_count > 2 {
        return Err(format!(
            "Unsupported channel count (requires 1-2 channels) in {}: {}",
            asset.source_name, riff.channel_count
        ));
    }

    let samples = if riff.frame_count == 0 {
        Vec::new()
    } else {
        riff.decode_interleaved_s16_from_slice(&data)?
    };
    drop(data);

    let envelope = AssetEnvelope::envelope_extract(&riff, &samples);

    // Resample + FLAC encode. Empty audio → empty FLAC payload.
    let flac = if riff.frame_count == 0 {
        Vec::new()
    } else {
        let mut frame_rate = riff.frame_rate as i32;
        let mut frame_count = riff.frame_count as i32;
        let pcm = if frame_rate != 48000 {
            frame_count = (riff.frame_count * 48000 / frame_rate as u64) as i32;
            let resampled = Riff::resize(&samples, riff.channel_count as u32, frame_count as u32);
            frame_rate = 48000;
            resampled
        } else {
            samples
        };
        encode(&asset.source_name, riff.channel_count as i32, frame_rate, frame_count, &pcm)?
    };

    let converted_frame_count = if riff.frame_rate != 48000 {
        (riff.frame_count * 48000 / riff.frame_rate as u64) as i64
    } else {
        riff.frame_count as i64
    };

    Ok((
        SoundAssetBankConvertedAsset {
            name: asset.converted_name.clone(),
            converted_checksum: Checksum::from_data(&flac),
            source_checksum,
            format: AssetFormat::SndAssetFormatFlac,
            looping: match asset.looping {
                AliasLooping::Looping => AliasLooping::Looping,
                AliasLooping::NonLooping => AliasLooping::NonLooping,
            },
            frame_count: converted_frame_count,
            frame_rate: 48000,
            channel_count: riff.channel_count as i32,
            envelope_loudness0: envelope.left[0] as u8,
            envelope_loudness1: envelope.left[1] as u8,
            envelope_loudness2: envelope.left[2] as u8,
            envelope_loudness3: envelope.left[3] as u8,
            envelope_time1: (u16::MAX as f64 * envelope.time[1]) as u16,
            envelope_time2: (u16::MAX as f64 * envelope.time[2]) as u16,
        },
        flac,
    ))
}