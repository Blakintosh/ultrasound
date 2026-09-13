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
fn synthetic_flac_preserves_channels_frames_and_current_quantization() {
    for channels in [1, 2] {
        let pcm: Vec<i16> = (0..4096 * channels)
            .map(|i| (i % 30001 - 15000) as i16)
            .collect();
        let bytes = flac::encode("synthetic", channels, 48000, 4096, &pcm).unwrap();
        let audio = flac::decode("synthetic", &bytes).unwrap();
        assert_eq!(audio.frame_count, 4096);
        assert_eq!(audio.channel_count, channels as u16);
        assert_eq!(audio.frame_rate, 48000);
        let mask = if cfg!(feature = "flac_hifi") { !0 } else { !1 };
        assert_eq!(
            audio.samples,
            pcm.iter().map(|s| s & mask).collect::<Vec<_>>()
        );
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
    #[cfg(not(feature = "flac_hifi"))]
    #[ignore = "known bug B008; default encoder changes samples without a lossy prepass"]
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
    #[ignore = "known bug B001; run scripts/audit.py --known-bugs"]
    fn zero_block_alignment_returns_error() {
        let result = std::panic::catch_unwind(|| Riff::parse("zero.wav", &wav(0, false)));
        assert!(
            matches!(result, Ok(Err(_))),
            "B001: invalid alignment must return an error without panicking"
        );
    }

    #[test]
    #[ignore = "known bug B002; run scripts/audit.py --known-bugs"]
    fn padded_odd_riff_chunk_is_accepted() {
        assert!(
            Riff::parse("odd.wav", &wav(2, true)).is_ok(),
            "B002: valid padded RIFF chunk must parse"
        );
    }

    #[test]
    #[ignore = "known bug B003; run scripts/audit.py --known-bugs"]
    fn short_envelope_uses_same_units_as_long_envelope() {
        let short = AssetEnvelope::envelope_extract(512, 1, &vec![16384; 512]);
        let long = AssetEnvelope::envelope_extract(1024, 1, &vec![16384; 1024]);
        assert!(
            (short.left[0] - long.left[0]).abs() < 0.01,
            "B003: equal amplitude must use equal envelope units"
        );
    }

    #[test]
    #[ignore = "known bug B004; debug arithmetic reproducer"]
    fn one_frame_resample_does_not_panic() {
        let result = std::panic::catch_unwind(|| Riff::resize(&[100], 1, 2));
        assert!(
            matches!(result, Ok(ref samples) if samples == &[100, 100]),
            "B004: one-frame resampling must preserve the sample without panic"
        );
    }

    #[test]
    #[ignore = "known bug B005; run scripts/audit.py --known-bugs"]
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
