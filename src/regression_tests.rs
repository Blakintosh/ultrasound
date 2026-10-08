use crate::{
    asset_types::AssetEnvelope, flac, riff::Riff, sound_zone, tables::row_alias::RowAlias,
};

fn wav(block_align: u16, odd_junk: bool) -> Vec<u8> {
    let mut bytes = b"RIFF\0\0\0\0WAVE".to_vec();
    if odd_junk {
        bytes.extend(b"JUNK");
        bytes.extend(1u32.to_le_bytes());
        bytes.extend([42, 0]);
    }
    bytes.extend(b"fmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(48000u32.to_le_bytes());
    bytes.extend(96000u32.to_le_bytes());
    bytes.extend(block_align.to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(4u32.to_le_bytes());
    bytes.extend(100i16.to_le_bytes());
    bytes.extend((-100i16).to_le_bytes());
    let size = bytes.len() as u32 - 8;
    bytes[4..8].copy_from_slice(&size.to_le_bytes());
    bytes
}

#[test]
fn synthetic_pcm_wav_decodes_exactly() {
    let bytes = wav(2, false);
    let riff = Riff::parse("synthetic.wav", &bytes).unwrap();
    assert_eq!(riff.frame_count, 2);
    assert_eq!(riff.frame_rate, 48000);
    assert_eq!(riff.channel_count, 1);
    assert_eq!(
        riff.decode_interleaved_s16_from_slice(&bytes).unwrap(),
        [100, -100]
    );
}

#[test]
fn stereo_data_with_a_partial_frame_decodes_whole_frames() {
    let mut bytes = wav(4, false);
    bytes[22..24].copy_from_slice(&2u16.to_le_bytes());
    let data_size = bytes.len() - 8;
    bytes[data_size..data_size + 4].copy_from_slice(&6u32.to_le_bytes());
    bytes.extend(7i16.to_le_bytes());
    let size = bytes.len() as u32 - 8;
    bytes[4..8].copy_from_slice(&size.to_le_bytes());

    let riff = Riff::parse("partial.wav", &bytes).unwrap();
    assert_eq!(riff.frame_count, 1);
    assert_eq!(
        riff.decode_interleaved_s16_from_slice(&bytes).unwrap(),
        [100, -100]
    );
}

#[test]
fn truncated_wav_header_names_the_file() {
    let mut bytes = wav(2, false)[..30].to_vec();
    let size = bytes.len() as u32 - 8;
    bytes[4..8].copy_from_slice(&size.to_le_bytes());
    let Err(err) = Riff::parse("cut.wav", &bytes) else {
        panic!("a truncated header parsed")
    };
    assert!(err.contains("cut.wav"), "{err}");
}

#[test]
fn invalid_wav_dimensions_and_fmt_lengths_return_errors() {
    for (offset, replacement) in [
        (22, 0u16.to_le_bytes().to_vec()),
        (24, 0u32.to_le_bytes().to_vec()),
        (16, 12u32.to_le_bytes().to_vec()),
        (16, 200u32.to_le_bytes().to_vec()),
    ] {
        let mut bytes = wav(2, false);
        bytes[offset..offset + replacement.len()].copy_from_slice(&replacement);
        assert!(Riff::parse("invalid.wav", &bytes).is_err());
    }
}

#[test]
fn odd_chunks_after_fmt_and_short_stereo_audio_work() {
    let mut bytes = wav(2, false);
    bytes.splice(36..36, b"JUNK\x01\0\0\0x\0".iter().copied());
    let size = bytes.len() as u32 - 8;
    bytes[4..8].copy_from_slice(&size.to_le_bytes());
    let parsed = Riff::parse("post-fmt.wav", &bytes).unwrap();
    assert_eq!(
        parsed.decode_interleaved_s16_from_slice(&bytes).unwrap(),
        [100, -100]
    );
    assert_eq!(
        Riff::resize(&[-32768, 32767], 2, 3),
        [-32768, 32767, -32768, 32767, -32768, 32767]
    );
    assert!(Riff::resize(&[], 1, 3).is_empty());
}

#[test]
fn short_envelope_handles_silence_and_full_scale() {
    assert_eq!(AssetEnvelope::envelope_extract(0, 1, &[]).left, [0.0; 4]);
    assert_eq!(
        AssetEnvelope::envelope_extract(1, 1, &[-32768]).left,
        [100.0; 4]
    );
}

#[test]
fn flac_rejects_inconsistent_pcm_dimensions() {
    assert!(flac::encode("short", 2, 48000, 2, &[100]).is_err());
    assert!(flac::encode("zero-channels", 0, 48000, 2, &[]).is_err());
    assert!(flac::encode("negative-frames", 1, 48000, -1, &[]).is_err());
}

#[test]
fn conversion_uses_the_hashed_buffer_without_reopening_the_source() {
    use crate::converter::{
        AliasLooping, AliasStorage, CompressionLevel, SoundAssetBankSourceAsset,
        convert_source_data,
    };
    use crate::source_asset_cache::Checksum;
    let asset = SoundAssetBankSourceAsset {
        source_name: "this-source-is-not-on-disk.wav".into(),
        converted_name: "test.LN100.pc.snd".into(),
        looping: AliasLooping::NonLooping,
        storage: AliasStorage::Loaded,
        compression: 100,
        compression_level: CompressionLevel::None,
        locale: crate::tables::row_locale::RowLocale {
            name: "all".into(),
            search_name: "all".into(),
            deploy_name: "all".into(),
            cache_name: "all".into(),
            is_shared: true,
            compression_scale: 1.0,
        },
        platform: crate::tables::row_platform::RowPlatform {
            platform: "pc".into(),
            converted_asset_version: 14,
            convert_thread_count: 1,
            compression_scale: 1.0,
        },
    };
    let data = wav(2, false);
    let checksum =
        Checksum::from_data_with_recipe(&data, &asset.compression_level.recipe_fingerprint());
    let (converted, encoded) = convert_source_data(&asset, data, checksum).unwrap();
    assert_eq!(converted.source_checksum, checksum);
    assert_eq!(
        flac::decode("converted", &encoded).unwrap().samples,
        [100, -100]
    );
}

#[test]
fn dedup_preserves_platform_and_compression_variants() {
    let mut reader = csv::Reader::from_reader(
        b"Name,FileSpec,PlatformMask,CompressionLevel\na,tone.wav,pc,None\na,tone.wav,console,None\na,tone.wav,pc,High\na,tone.wav,pc,None\n".as_slice(),
    );
    let mut aliases: Vec<RowAlias> = reader.deserialize().map(Result::unwrap).collect();
    sound_zone::dedup_aliases(&mut aliases);
    assert_eq!(aliases.len(), 3);
    assert_eq!(aliases[0].platform_mask, "pc");
    assert_eq!(aliases[1].platform_mask, "console");
    assert_eq!(
        aliases[2].compression_level,
        Some(crate::converter::CompressionLevel::High)
    );
}

#[test]
fn synthetic_flac_preserves_channels_frames_and_samples() {
    for channels in [1, 2] {
        let pcm: Vec<i16> = (0..4096 * channels)
            .map(|i| (i % 30001 - 15000) as i16)
            .collect();
        let bytes = flac::encode("synthetic", channels, 48000, 4096, &pcm).unwrap();
        let audio = flac::decode("synthetic", &bytes).unwrap();
        assert_eq!(audio.frame_count, 4096);
        assert_eq!(audio.channel_count, channels as u16);
        assert_eq!(audio.frame_rate, 48000);
        assert_eq!(audio.samples, pcm);
    }
}

#[test]
fn synthetic_vorbis_decodes_audible_mono_and_stereo() {
    use std::num::{NonZeroU8, NonZeroU32};
    use vorbis_rs::VorbisEncoderBuilder;
    for channels in [1, 2] {
        let signal: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.08).sin() * 0.4).collect();
        let mut encoded = Vec::new();
        let mut encoder = VorbisEncoderBuilder::new(
            NonZeroU32::new(48000).unwrap(),
            NonZeroU8::new(channels).unwrap(),
            &mut encoded,
        )
        .unwrap()
        .build()
        .unwrap();
        encoder
            .encode_audio_block(&vec![signal.as_slice(); channels as usize])
            .unwrap();
        encoder.finish().unwrap();
        let audio = crate::ogg::decode("synthetic.ogg", &encoded).unwrap();
        assert_eq!(audio.channel_count, channels as u16);
        assert_eq!(audio.frame_rate, 48000);
        assert_eq!(
            audio.samples.len(),
            audio.frame_count as usize * channels as usize
        );
        assert!(audio.frame_count >= 4800 && audio.frame_count <= 6000);
        assert!(audio.samples.iter().any(|s| s.unsigned_abs() > 5000));
    }
}

// These assert the intended behavior. The audit runner requires each test to
// fail at its named assertion until its fix promotes it to the ordinary suite.
mod known_bugs {
    use super::*;

    #[test]
    fn default_encoder_preserves_unprocessed_samples() {
        let pcm = vec![1001i16; 2048];
        let encoded = flac::encode("unprocessed", 1, 48000, 2048, &pcm).unwrap();
        let decoded = flac::decode("unprocessed", &encoded).unwrap();
        assert_eq!(
            decoded.samples[0], pcm[0],
            "B008: unprocessed samples should retain their low bit"
        );
    }

    #[test]
    fn zero_block_alignment_returns_error() {
        let result = std::panic::catch_unwind(|| Riff::parse("zero.wav", &wav(0, false)));
        assert!(
            matches!(result, Ok(Err(_))),
            "B001: invalid alignment must return an error without panicking"
        );
    }

    #[test]
    fn padded_odd_riff_chunk_is_accepted() {
        assert!(
            Riff::parse("odd.wav", &wav(2, true)).is_ok(),
            "B002: valid padded RIFF chunk must parse"
        );
    }

    #[test]
    fn short_envelope_uses_same_units_as_long_envelope() {
        let short = AssetEnvelope::envelope_extract(512, 1, &vec![16384; 512]);
        let long = AssetEnvelope::envelope_extract(1024, 1, &vec![16384; 1024]);
        assert!(
            (short.left[0] - long.left[0]).abs() < 0.01,
            "B003: equal amplitude must use equal envelope units"
        );
    }

    #[test]
    fn one_frame_resample_does_not_panic() {
        let result = std::panic::catch_unwind(|| Riff::resize(&[100], 1, 2));
        assert!(
            matches!(result, Ok(ref samples) if samples == &[100, 100]),
            "B004: one-frame resampling must preserve the sample without panic"
        );
    }

    #[test]
    fn distinct_alias_settings_survive_deduplication() {
        let mut reader = csv::Reader::from_reader(
            b"Name,FileSpec,Storage,VolMin\na,tone.wav,LOADED,70\na,tone.wav,STREAMED,80\n"
                .as_slice(),
        );
        let mut aliases: Vec<RowAlias> = reader.deserialize().map(Result::unwrap).collect();
        sound_zone::dedup_aliases(&mut aliases);
        assert_eq!(
            aliases.len(),
            2,
            "B005: different alias settings must not be silently discarded"
        );
    }
}
