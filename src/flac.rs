use std::os::raw::c_void;

type FlacWriteCallback = extern "C" fn(
    encoder: *mut c_void,
    buffer: *const u8,
    bytes: usize,
    samples: u32,
    current_frame: u32,
    client_data: *mut c_void,
) -> i32;

// FFI bindings into libFLAC.dll 
unsafe extern "C" {
    unsafe fn FLAC__stream_encoder_new() -> *mut c_void;
    unsafe fn FLAC__stream_encoder_delete(encoder: *mut c_void);
    unsafe fn FLAC__stream_encoder_set_channels(encoder: *mut c_void, channels: u32) -> i32;
    unsafe fn FLAC__stream_encoder_set_bits_per_sample(encoder: *mut c_void, bps: u32) -> i32;
    unsafe fn FLAC__stream_encoder_set_sample_rate(encoder: *mut c_void, sample_rate: u32) -> i32;
    unsafe fn FLAC__stream_encoder_set_compression_level(encoder: *mut c_void, level: u32) -> i32;
    unsafe fn FLAC__stream_encoder_set_blocksize(encoder: *mut c_void, blocksize: u32) -> i32;
    unsafe fn FLAC__stream_encoder_set_do_mid_side_stereo(encoder: *mut c_void, do_mid_side_stereo: i32) -> i32;
    unsafe fn FLAC__stream_encoder_set_total_samples_estimate(encoder: *mut c_void, total_samples_estimate: u64) -> i32;
    unsafe fn FLAC__stream_encoder_set_metadata(encoder: *mut c_void, metadata: *mut c_void, num_blocks: u32) -> i32;
    unsafe fn FLAC__stream_encoder_init_stream(encoder: *mut c_void, write_callback: FlacWriteCallback, seek_callback: *const c_void, tell_callback: *const c_void, metadata_callback: *const c_void, client_data: *mut c_void) -> i32;
    unsafe fn FLAC__stream_encoder_process_interleaved(encoder: *mut c_void, buffer: *const i32, samples: u32) -> i32;
    unsafe fn FLAC__stream_encoder_finish(encoder: *mut c_void) -> i32;
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

extern "C" fn write_callback(                                                                                               
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

pub fn encode(source_name: &str, channel_count: i32, sample_rate: i32, frame_count: i32, pcm_data: &[i16]) -> Result<Vec<u8>, String> {
    let total_samples = frame_count * channel_count;

    let mask = if cfg!(feature = "flac_hifi") { !0 } else { !1 };

    unsafe {
        let encoder = FlacEncoder {
            ptr: FLAC__stream_encoder_new()
        };

        if encoder.ptr.is_null() {
            return Err(format!("Failed to create FLAC encoder for {}", source_name));
        }

        let compression_level = if cfg!(feature = "flac_hifi") { 5 } else { 16 };

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
            write_callback,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            &mut output as *mut Vec<u8> as *mut c_void,
        );

        if init_result != 0 {
            return Err(format!("Failed to initialize FLAC encoder: error code {}", init_result));
        }

        // Convert 16-bit samples to 32-bit for libFLAC
        let samples_32: Vec<i32> = pcm_data.iter().map(|&s| (s & mask) as i32).collect();

        if FLAC__stream_encoder_process_interleaved(encoder.ptr, samples_32.as_ptr(), frame_count as u32) == 0 {
            return Err("Failed to encode FLAC data".to_string());
        }

        if FLAC__stream_encoder_finish(encoder.ptr) == 0 {
            return Err("Failed to finalize FLAC encoding".to_string());
        }

        Ok(output)
    }
}