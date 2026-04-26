use std::os::raw::c_void;

use crate::decoded_audio::DecodedAudio;

// -- Encoder callback type --------------------------------------------------

type FlacEncWriteCallback = extern "C" fn(
    encoder: *mut c_void,
    buffer: *const u8,
    bytes: usize,
    samples: u32,
    current_frame: u32,
    client_data: *mut c_void,
) -> i32;

// -- Decoder callback types -------------------------------------------------

/// FLAC__StreamDecoderReadStatus: 0 = continue, 1 = end_of_stream, 2 = abort
type FlacDecReadCallback = extern "C" fn(
    decoder: *mut c_void,
    buffer: *mut u8,
    bytes: *mut usize,
    client_data: *mut c_void,
) -> i32;

/// FLAC__StreamDecoderWriteStatus: 0 = continue, 1 = abort
type FlacDecWriteCallback = extern "C" fn(
    decoder: *mut c_void,
    frame: *const FlacFrame,
    buffer: *const *const i32,
    client_data: *mut c_void,
) -> i32;

type FlacDecErrorCallback =
    extern "C" fn(decoder: *mut c_void, status: i32, client_data: *mut c_void);

type FlacDecMetadataCallback =
    extern "C" fn(decoder: *mut c_void, metadata: *const c_void, client_data: *mut c_void);

// -- Minimal FLAC frame header for the write callback -----------------------

#[repr(C)]
struct FlacFrameHeader {
    blocksize: u32,
    sample_rate: u32,
    channels: u32,
    channel_assignment: i32,
    bits_per_sample: u32,
    number_type: i32,
    number: u64, // union — we only need the size
    crc: u8,
}

#[repr(C)]
struct FlacFrame {
    header: FlacFrameHeader,
    // subframes and footer follow but we don't need them
}

// -- FFI bindings into libFLAC.dll ------------------------------------------

unsafe extern "C" {
    // Encoder
    unsafe fn FLAC__stream_encoder_new() -> *mut c_void;
    unsafe fn FLAC__stream_encoder_delete(encoder: *mut c_void);
    unsafe fn FLAC__stream_encoder_set_channels(encoder: *mut c_void, channels: u32) -> i32;
    unsafe fn FLAC__stream_encoder_set_bits_per_sample(encoder: *mut c_void, bps: u32) -> i32;
    unsafe fn FLAC__stream_encoder_set_sample_rate(encoder: *mut c_void, sample_rate: u32) -> i32;
    unsafe fn FLAC__stream_encoder_set_compression_level(encoder: *mut c_void, level: u32) -> i32;
    unsafe fn FLAC__stream_encoder_set_blocksize(encoder: *mut c_void, blocksize: u32) -> i32;
    unsafe fn FLAC__stream_encoder_set_do_mid_side_stereo(
        encoder: *mut c_void,
        do_mid_side_stereo: i32,
    ) -> i32;
    unsafe fn FLAC__stream_encoder_set_total_samples_estimate(
        encoder: *mut c_void,
        total_samples_estimate: u64,
    ) -> i32;
    unsafe fn FLAC__stream_encoder_set_metadata(
        encoder: *mut c_void,
        metadata: *mut c_void,
        num_blocks: u32,
    ) -> i32;
    unsafe fn FLAC__stream_encoder_init_stream(
        encoder: *mut c_void,
        write_callback: FlacEncWriteCallback,
        seek_callback: *const c_void,
        tell_callback: *const c_void,
        metadata_callback: *const c_void,
        client_data: *mut c_void,
    ) -> i32;
    unsafe fn FLAC__stream_encoder_process_interleaved(
        encoder: *mut c_void,
        buffer: *const i32,
        samples: u32,
    ) -> i32;
    unsafe fn FLAC__stream_encoder_finish(encoder: *mut c_void) -> i32;

    // Decoder
    unsafe fn FLAC__stream_decoder_new() -> *mut c_void;
    unsafe fn FLAC__stream_decoder_delete(decoder: *mut c_void);
    unsafe fn FLAC__stream_decoder_init_stream(
        decoder: *mut c_void,
        read_callback: FlacDecReadCallback,
        seek_callback: *const c_void,
        tell_callback: *const c_void,
        length_callback: *const c_void,
        eof_callback: *const c_void,
        write_callback: FlacDecWriteCallback,
        metadata_callback: FlacDecMetadataCallback,
        error_callback: FlacDecErrorCallback,
        client_data: *mut c_void,
    ) -> i32;
    unsafe fn FLAC__stream_decoder_process_until_end_of_stream(decoder: *mut c_void) -> i32;
    unsafe fn FLAC__stream_decoder_finish(decoder: *mut c_void) -> i32;
    unsafe fn FLAC__stream_decoder_get_sample_rate(decoder: *mut c_void) -> u32;
    unsafe fn FLAC__stream_decoder_get_channels(decoder: *mut c_void) -> u32;
    unsafe fn FLAC__stream_decoder_get_bits_per_sample(decoder: *mut c_void) -> u32;
    unsafe fn FLAC__stream_decoder_get_total_samples(decoder: *mut c_void) -> u64;
}

struct FlacEncoder {
    ptr: *mut c_void,
}

impl Drop for FlacEncoder {
    fn drop(&mut self) {
        unsafe {
            FLAC__stream_encoder_delete(self.ptr);
        }
    }
}

struct FlacDecoder {
    ptr: *mut c_void,
}

impl Drop for FlacDecoder {
    fn drop(&mut self) {
        unsafe {
            FLAC__stream_decoder_delete(self.ptr);
        }
    }
}

// -- Encoder callbacks ------------------------------------------------------

extern "C" fn enc_write_callback(
    _encoder: *mut c_void,
    buffer: *const u8,
    bytes: usize,
    _samples: u32,
    _current_frame: u32,
    client_data: *mut c_void,
) -> i32 {
    unsafe {
        let output = &mut *(client_data as *mut Vec<u8>);
        let slice = std::slice::from_raw_parts(buffer, bytes);
        output.extend_from_slice(slice);
    }
    0
}

// -- STREAMINFO parser ------------------------------------------------------

struct StreamInfo {
    sample_rate: u32,
    channels: u32,
    bits_per_sample: u32,
    total_samples: u64,
}

fn parse_streaminfo(source_name: &str, data: &[u8]) -> Result<StreamInfo, String> {
    if data.len() < 4 + 4 + 34 || &data[..4] != b"fLaC" {
        return Err(format!("Not a FLAC file: {}", source_name));
    }
    // Block header at data[4..8]: bit 0 = last-flag, bits 1..7 = type, then 24-bit length.
    let block_type = data[4] & 0x7F;
    if block_type != 0 {
        return Err(format!(
            "First metadata block is not STREAMINFO in {}",
            source_name
        ));
    }
    let body = &data[8..8 + 34];

    let sample_rate =
        ((body[10] as u32) << 12) | ((body[11] as u32) << 4) | ((body[12] as u32) >> 4);
    let channels = (((body[12] >> 1) & 0x07) as u32) + 1;
    let bits_per_sample = ((((body[12] & 0x01) << 4) | ((body[13] >> 4) & 0x0F)) as u32) + 1;
    let total_samples = (((body[13] & 0x0F) as u64) << 32)
        | ((body[14] as u64) << 24)
        | ((body[15] as u64) << 16)
        | ((body[16] as u64) << 8)
        | (body[17] as u64);

    Ok(StreamInfo {
        sample_rate,
        channels,
        bits_per_sample,
        total_samples,
    })
}

// -- Decoder callbacks ------------------------------------------------------

struct DecodeState {
    input: *const [u8],
    offset: usize,
    output: Vec<i16>,
    channels: u32,
    bits_per_sample: u32,
    sample_rate: u32,
    total_samples: u64,
    error: Option<String>,
}

extern "C" fn dec_read_callback(
    _decoder: *mut c_void,
    buffer: *mut u8,
    bytes: *mut usize,
    client_data: *mut c_void,
) -> i32 {
    unsafe {
        let state = &mut *(client_data as *mut DecodeState);
        let input = &*state.input;
        let requested = *bytes;
        let remaining = input.len().saturating_sub(state.offset);
        if remaining == 0 {
            *bytes = 0;
            return 1; // end of stream
        }
        let to_copy = requested.min(remaining);
        std::ptr::copy_nonoverlapping(input.as_ptr().add(state.offset), buffer, to_copy);
        state.offset += to_copy;
        *bytes = to_copy;
        0 // continue
    }
}

extern "C" fn dec_write_callback(
    _decoder: *mut c_void,
    frame: *const FlacFrame,
    buffer: *const *const i32,
    client_data: *mut c_void,
) -> i32 {
    unsafe {
        let state = &mut *(client_data as *mut DecodeState);
        let header = &(*frame).header;
        let blocksize = header.blocksize as usize;
        let channels = state.channels as usize;

        for i in 0..blocksize {
            for ch in 0..channels {
                let channel_buf = *buffer.add(ch);
                let sample_i32 = *channel_buf.add(i);
                let sample_i16 = sample_i32.clamp(-32768, 32767) as i16;
                state.output.push(sample_i16);
            }
        }
        0 // continue
    }
}

extern "C" fn dec_metadata_callback(
    _decoder: *mut c_void,
    _metadata: *const c_void,
    _client_data: *mut c_void,
) {
    // We read metadata via getter functions after init, nothing to do here.
}

extern "C" fn dec_error_callback(_decoder: *mut c_void, status: i32, client_data: *mut c_void) {
    unsafe {
        let state = &mut *(client_data as *mut DecodeState);
        if state.error.is_none() {
            state.error = Some(format!("FLAC decode error: status {}", status));
        }
    }
}

pub fn encode(
    source_name: &str,
    channel_count: i32,
    sample_rate: i32,
    frame_count: i32,
    pcm_data: &[i16],
) -> Result<Vec<u8>, String> {
    let total_samples = frame_count * channel_count;

    let mask = if cfg!(feature = "flac_hifi") { !0 } else { !1 };

    unsafe {
        let encoder = FlacEncoder {
            ptr: FLAC__stream_encoder_new(),
        };

        if encoder.ptr.is_null() {
            return Err(format!("Failed to create FLAC encoder for {}", source_name));
        }

        let compression_level = if cfg!(feature = "flac_hifi") { 5 } else { 8 };

        FLAC__stream_encoder_set_compression_level(encoder.ptr, compression_level);
        FLAC__stream_encoder_set_bits_per_sample(encoder.ptr, 16);

        FLAC__stream_encoder_set_channels(encoder.ptr, channel_count as u32);
        FLAC__stream_encoder_set_sample_rate(encoder.ptr, sample_rate as u32);
        FLAC__stream_encoder_set_total_samples_estimate(encoder.ptr, frame_count as u64);
        FLAC__stream_encoder_set_blocksize(encoder.ptr, 1024);
        FLAC__stream_encoder_set_do_mid_side_stereo(encoder.ptr, 0);
        FLAC__stream_encoder_set_metadata(encoder.ptr, std::ptr::null_mut(), 0);

        let mut output: Vec<u8> = Vec::new();

        let init_result = FLAC__stream_encoder_init_stream(
            encoder.ptr,
            enc_write_callback,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            &mut output as *mut Vec<u8> as *mut c_void,
        );

        if init_result != 0 {
            return Err(format!(
                "Failed to initialize FLAC encoder: error code {}",
                init_result
            ));
        }

        // Convert 16-bit samples to 32-bit for libFLAC
        let samples_32: Vec<i32> = pcm_data.iter().map(|&s| (s & mask) as i32).collect();

        if FLAC__stream_encoder_process_interleaved(
            encoder.ptr,
            samples_32.as_ptr(),
            frame_count as u32,
        ) == 0
        {
            return Err("Failed to encode FLAC data".to_string());
        }

        if FLAC__stream_encoder_finish(encoder.ptr) == 0 {
            return Err("Failed to finalize FLAC encoding".to_string());
        }

        Ok(output)
    }
}

pub fn decode(source_name: &str, data: &[u8]) -> Result<DecodedAudio, String> {
    unsafe {
        let decoder = FlacDecoder {
            ptr: FLAC__stream_decoder_new(),
        };
        if decoder.ptr.is_null() {
            return Err(format!("Failed to create FLAC decoder for {}", source_name));
        }

        let mut state = DecodeState {
            input: data as *const [u8],
            offset: 0,
            output: Vec::new(),
            channels: 0,
            bits_per_sample: 0,
            sample_rate: 0,
            total_samples: 0,
            error: None,
        };

        let init_result = FLAC__stream_decoder_init_stream(
            decoder.ptr,
            dec_read_callback,
            std::ptr::null(), // seek
            std::ptr::null(), // tell
            std::ptr::null(), // length
            std::ptr::null(), // eof
            dec_write_callback,
            dec_metadata_callback,
            dec_error_callback,
            &mut state as *mut DecodeState as *mut c_void,
        );

        if init_result != 0 {
            return Err(format!(
                "Failed to initialize FLAC decoder for {}: error code {}",
                source_name, init_result
            ));
        }

        // Parse STREAMINFO directly from the file bytes. The libFLAC getters are
        // unreliable here (they returned 0 in practice), and reading the metadata
        // callback's struct cross-FFI is fragile due to union alignment.
        let info = parse_streaminfo(source_name, data)?;
        state.sample_rate = info.sample_rate;
        state.channels = info.channels;
        state.bits_per_sample = info.bits_per_sample;
        state.total_samples = info.total_samples;

        if state.bits_per_sample != 16 {
            FLAC__stream_decoder_finish(decoder.ptr);
            return Err(format!(
                "Unsupported bit depth (requires 16-bit) in {}: {}",
                source_name, state.bits_per_sample
            ));
        }

        if FLAC__stream_decoder_process_until_end_of_stream(decoder.ptr) == 0 {
            FLAC__stream_decoder_finish(decoder.ptr);
            let msg = state.error.unwrap_or_else(|| "unknown error".to_string());
            return Err(format!(
                "Failed to decode FLAC data in {}: {}",
                source_name, msg
            ));
        }

        FLAC__stream_decoder_finish(decoder.ptr);

        if let Some(err) = state.error {
            return Err(format!("FLAC decode error in {}: {}", source_name, err));
        }

        let frame_count = state.output.len() as u64 / state.channels as u64;

        Ok(DecodedAudio {
            samples: state.output,
            frame_rate: state.sample_rate,
            channel_count: state.channels as u16,
            frame_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_FLAC: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/test_data/zm_kar_roundend_normal.flac"
    );

    #[test]
    fn parse_streaminfo_reads_real_file() {
        let data = std::fs::read(SAMPLE_FLAC).expect("sample flac present in repo root");
        let info = parse_streaminfo(SAMPLE_FLAC, &data).expect("streaminfo parses");
        assert_eq!(info.bits_per_sample, 16, "expected 16-bit source");
        assert!(info.channels >= 1 && info.channels <= 2);
        assert!(info.sample_rate > 0);
        assert!(info.total_samples > 0);
    }

    #[test]
    fn decode_real_file_produces_samples() {
        let data = std::fs::read(SAMPLE_FLAC).expect("sample flac present in repo root");
        let audio = decode(SAMPLE_FLAC, &data).expect("decode succeeds");
        assert!(audio.frame_rate > 0);
        assert!(audio.channel_count >= 1 && audio.channel_count <= 2);
        assert!(audio.frame_count > 0);
        assert_eq!(
            audio.samples.len(),
            audio.frame_count as usize * audio.channel_count as usize
        );
    }

    /// Exploratory: measure whether routing PCM through a lossy Vorbis pass
    /// before FLAC-encoding produces a smaller bank payload than FLAC'ing
    /// the raw PCM directly. Not an assertion test — it prints sizes so a
    /// human can eyeball the ratio. Run with:
    ///   cargo test --release lossy_prepass_shrinks_flac -- --nocapture
    #[test]
    fn lossy_prepass_shrinks_flac() {
        use lewton::inside_ogg::OggStreamReader;
        use std::io::Cursor;
        use std::num::{NonZeroU8, NonZeroU32};
        use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoderBuilder};

        let data = std::fs::read(SAMPLE_FLAC).expect("sample flac present");
        let audio = decode(SAMPLE_FLAC, &data).expect("flac decodes");
        let sample_rate = audio.frame_rate as i32;
        let channels = audio.channel_count as i32;
        let frame_count = audio.frame_count as i32;

        // Baseline: encode the raw PCM straight to FLAC at our normal level.
        let baseline = encode(
            SAMPLE_FLAC,
            channels,
            sample_rate,
            frame_count,
            &audio.samples,
        )
        .expect("baseline flac encode");

        // Split interleaved i16 → per-channel f32 in [-1, 1] for vorbis_rs.
        let ch = channels as usize;
        let mut planar: Vec<Vec<f32>> = vec![Vec::with_capacity(frame_count as usize); ch];
        for frame in audio.samples.chunks_exact(ch) {
            for (c, s) in frame.iter().enumerate() {
                planar[c].push(*s as f32 / 32768.0);
            }
        }

        // Encode PCM → Vorbis at several quality rungs, decode back, re-FLAC.
        for quality in [0.1_f32, 0.3, 0.5, 0.7] {
            let mut ogg_bytes: Vec<u8> = Vec::new();
            {
                let mut enc = VorbisEncoderBuilder::new(
                    NonZeroU32::new(sample_rate as u32).unwrap(),
                    NonZeroU8::new(channels as u8).unwrap(),
                    &mut ogg_bytes,
                )
                .expect("vorbis builder")
                .bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr {
                    target_quality: quality,
                })
                .build()
                .expect("vorbis build");

                let block: Vec<&[f32]> = planar.iter().map(|v| v.as_slice()).collect();
                enc.encode_audio_block(&block).expect("vorbis encode block");
                enc.finish().expect("vorbis finish");
            }

            // Decode the OGG back to interleaved i16 with lewton.
            let mut reader = OggStreamReader::new(Cursor::new(&ogg_bytes)).expect("ogg reader");
            let mut round_trip: Vec<i16> = Vec::with_capacity(audio.samples.len());
            while let Some(packet) = reader.read_dec_packet_itl().expect("ogg packet") {
                round_trip.extend_from_slice(&packet);
            }
            let rt_frames = (round_trip.len() / ch) as i32;

            let flac_after = encode(SAMPLE_FLAC, channels, sample_rate, rt_frames, &round_trip)
                .expect("post-vorbis flac encode");

            let pct = 100.0 * flac_after.len() as f64 / baseline.len() as f64;
            println!(
                "quality {:.1}: ogg={}B, flac_after_ogg={}B, baseline_flac={}B, ratio={:.1}%",
                quality,
                ogg_bytes.len(),
                flac_after.len(),
                baseline.len(),
                pct
            );
        }

        // Bit-depth truncation: mask off the lowest N bits of each 16-bit
        // sample and re-encode. No dither — crudest possible requantisation,
        // which is the worst case for audible noise but the best case for
        // FLAC's predictor (trailing zeros are very predictable).
        for drop_bits in [2, 4, 6, 8] {
            let mask: i16 = !((1i16 << drop_bits) - 1);
            let truncated: Vec<i16> = audio.samples.iter().map(|s| *s & mask).collect();
            let flac_trunc = encode(SAMPLE_FLAC, channels, sample_rate, frame_count, &truncated)
                .expect("truncated flac encode");
            let effective_bits = 16 - drop_bits;
            let pct = 100.0 * flac_trunc.len() as f64 / baseline.len() as f64;
            println!(
                "trunc to {}-bit: flac={}B, baseline={}B, ratio={:.1}%",
                effective_bits,
                flac_trunc.len(),
                baseline.len(),
                pct
            );
        }
    }
}
