use std::fs;
use std::path::Path;

use crate::{
    asset_types::AssetEnvelope,
    bank::bank_entry::BankEntry,
    decoded_audio::DecodedAudio,
    flac,
    ogg,
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
    let source_checksum = Checksum::from_data(&data);

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
            return Err(format!("Unsupported audio format in {}: {}", asset.source_name, riff.format));
        }
        if riff.frame_size / riff.channel_count != 2 {
            return Err(format!(
                "Unsupported bit depth (requires 16-bit) in {}: {}",
                asset.source_name, riff.frame_size * 8 / riff.channel_count
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
        return Err(format!("Unsupported sample rate in {}: {}", asset.source_name, audio.frame_rate));
    }
    if audio.channel_count > 2 {
        return Err(format!(
            "Unsupported channel count (requires 1-2 channels) in {}: {}",
            asset.source_name, audio.channel_count
        ));
    }

    let envelope = AssetEnvelope::envelope_extract(audio.frame_count, audio.channel_count, &audio.samples);

    // Resample + FLAC encode. Empty audio → empty FLAC payload.
    let flac_out = if audio.frame_count == 0 {
        Vec::new()
    } else {
        let mut frame_rate = audio.frame_rate as i32;
        let mut frame_count = audio.frame_count as i32;
        let pcm = if frame_rate != 48000 {
            frame_count = (audio.frame_count * 48000 / frame_rate as u64) as i32;
            let resampled = Riff::resize(&audio.samples, audio.channel_count as u32, frame_count as u32);
            frame_rate = 48000;
            resampled
        } else {
            audio.samples
        };
        flac::encode(&asset.source_name, audio.channel_count as i32, frame_rate, frame_count, &pcm)?
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