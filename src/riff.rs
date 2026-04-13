use std::{fs::File, io::{BufReader, Cursor, Read, Seek, SeekFrom}};
use crate::units;

pub struct Riff {
    pub file_name: String,
    pub frame_rate: u32,
    pub channel_count: u16,
    pub frame_count: u64,
    pub data_byte_offset: u64,
    pub data_chunk_size: u64,
    pub format: u16,
    pub frame_size: u16
}

impl Riff {
    pub fn load(file_name: &str) -> Result<Self, String> {
        let mut file = File::open(file_name)
            .map_err(|e| format!("Failed to open file {}: {}", file_name, e))?;

        Riff::load_from_file(file_name, &mut file)
    }

    pub fn load_from_file(file_name: &str, input: &mut File) -> Result<Self, String> {
        let metadata = input.metadata()
            .map_err(|e| format!("Failed to get metadata for file {}: {}", file_name, e))?;
        let length = metadata.len();
        let mut reader = BufReader::new(input);
        Self::parse_core(file_name, &mut reader, length)
    }

    /// Parse RIFF headers from an in-memory buffer. Equivalent to `load_from_file`
    /// but skips re-opening the file. Pair with `decode_interleaved_s16_from_slice`
    /// for a full single-read pipeline.
    pub fn parse(file_name: &str, data: &[u8]) -> Result<Self, String> {
        let length = data.len() as u64;
        let mut cursor = Cursor::new(data);
        Self::parse_core(file_name, &mut cursor, length)
    }

    fn parse_core<R: Read + Seek>(file_name: &str, mut reader: &mut R, length: u64) -> Result<Self, String> {
        // Make sure the RIFF file is even possibly parseable.
        if length < 20 {
            return Err(format!("File {} is truncated - too small to be a valid RIFF file.", file_name));
        }

        // Check the RIFF header.
        if read_u32(&mut reader)? != 1179011410u32 {
            return Err(format!("File {} does not start with RIFF header.", file_name));
        }
        // Now make sure the size is OK.
        if read_u32(&mut reader)? as u64 != (length - 8) {
            return Err(format!("File {} has incorrect RIFF chunk size.", file_name));
        }
        // Now check the WAVE header.
        if read_u32(&mut reader)? != 1163280727u32 {
            return Err(format!("File {} does not contain WAVE header.", file_name));
        }
        // Seek out the "fmt " chunk.
        let mut pos = 12; // after WAVE header
        const FOURCC_FMT: u32 = 544501094u32;
        loop {
            if pos >= length {
                return Err(format!("File {} does not contain fmt chunk.", file_name));
            }
            let chunk_id = read_u32(&mut reader)?;

            // We found our guy.
            if chunk_id == FOURCC_FMT {
                break;
            }
            let chunk_size = read_u32(&mut reader)?;

            // Otherwise skip over this chunk, keep looking.
            pos = reader.seek(SeekFrom::Current(chunk_size as i64))
                .map_err(|e| format!("Failed to seek in file {}: {}", file_name, e))?;
        }

        // We're at the fmt chunk, find the length of the chunk and make sure its length fits.
        let fmt_chunk_size = read_u32(&mut reader)? as u64;
        let fmt_data_start = reader.stream_position()
            .map_err(|e| format!("Failed to get current position in file {}: {}", file_name, e))?;

        // Grab the format properties.
        let format = read_u16(&mut reader)?;
        let channel_count = read_u16(&mut reader)?;
        let frame_rate = read_u32(&mut reader)?;
        let _byte_rate = read_u32(&mut reader)?;
        let frame_size = read_u16(&mut reader)?;
        let _bits_per_sample = read_u16(&mut reader)?;

        // Now leave the fmt chunk, and seek out the "data" chunk.
        let fmt_end = fmt_data_start + fmt_chunk_size;
        pos = fmt_end;
        reader.seek(SeekFrom::Start(fmt_end))
            .map_err(|e| format!("Failed to seek in file {}: {}", file_name, e))?;

        const FOURCC_DATA: u32 = 1635017060u32;
        loop {
            if pos >= length {
                return Err(format!("File {} does not contain data chunk.", file_name));
            }
            let chunk_id = read_u32(&mut reader)?;

            // We found our guy.
            if chunk_id == FOURCC_DATA {
                break;
            }
            let chunk_size = read_u32(&mut reader)?;

            // Otherwise skip over this chunk, keep looking.
            pos = reader.seek(SeekFrom::Current(chunk_size as i64))
                .map_err(|e| format!("Failed to seek in file {}: {}", file_name, e))?;
        }

        // At data section, grab its size and offset.
        let data_chunk_size = read_u32(&mut reader)? as u64;
        let data_byte_offset = reader.stream_position()
            .map_err(|e| format!("Failed to get current position in file {}: {}", file_name, e))?;

        Ok(Riff {
            file_name: file_name.to_string(),
            frame_rate,
            channel_count,
            frame_count: data_chunk_size / frame_size as u64,
            data_byte_offset,
            data_chunk_size,
            format,
            frame_size
        })
    }

    pub fn load_interleaved_s16(&self) -> Result<Vec<i16>, String> {
        let mut file = File::open(&self.file_name)
            .map_err(|e| format!("Failed to open file {}: {}", self.file_name, e))?;

        self.load_interleaved_s16_from_file(&mut file, 0, self.frame_count)
    }

    /// Decode PCM samples directly from an in-memory WAV buffer (paired with
    /// `Riff::parse`). Avoids reopening the file.
    pub fn decode_interleaved_s16_from_slice(&self, data: &[u8]) -> Result<Vec<i16>, String> {
        let start = self.data_byte_offset as usize;
        let declared_end = start + self.data_chunk_size as usize;
        // Some WAVs overstate the data chunk size (the header claims more
        // bytes than actually follow the `data` FourCC). Read whatever bytes
        // are present and zero-pad the rest out to the declared length so
        // the tail becomes silence instead of a hard error.
        let available_end = declared_end.min(data.len());
        let declared_len = declared_end - start;
        let sample_count = declared_len / 2;
        let mut out = Vec::with_capacity(sample_count);
        let bytes = &data[start..available_end];
        for c in bytes.chunks_exact(2) {
            out.push(i16::from_le_bytes([c[0], c[1]]));
        }
        while out.len() < sample_count {
            out.push(0);
        }
        Ok(out)
    }

    pub fn load_interleaved_s16_from_file(&self, input: &mut File, start_frame: u64, frame_count: u64) -> Result<Vec<i16>, String> {
        let mut reader = BufReader::new(input);

        let byte_count = self.get_data_length(frame_count);
        // Seek to the start data offset.
        reader.seek(SeekFrom::Start(self.get_data_offset(start_frame)))
            .map_err(|e| format!("Failed to seek in file {}: {}", self.file_name, e))?;

        // Read the data into a zero-initialised buffer via a short-read. If
        // the WAV overstates its data chunk size the tail remains zeroed,
        // so header/body mismatches seen in the wild resolve to silence
        // rather than a hard error.
        let mut buffer = vec![0u8; byte_count as usize];
        let mut filled = 0usize;
        while filled < buffer.len() {
            match reader.read(&mut buffer[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(e) => return Err(format!("Failed to read data from file {}: {}", self.file_name, e)),
            }
        }

        // Now transform the byte buffer into a i16 buffer.
        let result: Vec<i16> = buffer.chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        Ok(result)
    }

    pub fn peak_value_db(data: &[i16]) -> f64 {
        let mut peak = 0.0;
        for &sample in data {
            let abs_sample = sample.abs() as f64;
            if abs_sample > peak {
                peak = abs_sample;
            }
        }
        units::linear_to_db_fs(peak / 32768.0)
    }

    pub fn compute_loudness(&self, data: &[i16]) -> (f64, f64, f64) {
        let peak = Riff::peak_value_db(data);
        let mut rms = -100000.0;

        for i in 0..self.channel_count {
            let sample = self.windowed_rms_db(data, i, 0.005f64);
            if sample > rms {
                rms = sample;
            }
        }
        (peak, rms, peak - rms)
    }

    pub fn windowed_rms_db(&self, data: &[i16], channel: u16, window_time: f64) -> f64 {
        let mut peak_rms = 0.0;
        let window_size = (window_time * self.frame_rate as f64) as usize;
        let total_frames = data.len() / self.channel_count as usize;

        if window_size > total_frames {
            return units::linear_to_db_fs(0.0);
        }

        let mut offset = 0;
        while offset + window_size < total_frames {
            let mut sum_squares = 0.0;
            for i in 0..window_size {
                let sample = data[(i + offset) * self.channel_count as usize + channel as usize] as f64 / 32768.0;
                sum_squares += sample * sample;
            }
            let rms = (sum_squares / window_size as f64).sqrt();
            if rms > peak_rms {
                peak_rms = rms;
            }
            offset += window_size / 5;
        }
        units::linear_to_db_fs(peak_rms)
    }

    pub fn resize(input: &[i16], channel_count: u32, output_frame_count: u32) -> Vec<i16> {
        let ch = channel_count as usize;
        let out_frames = output_frame_count as usize;
        let in_frames = input.len() / ch;
        let mut result = vec![0i16; out_frames * ch];

        for channel in 0..ch {
            for out_i in 0..out_frames {
                // Where does this output sample fall in the input?
                let pos = out_i as f64 / out_frames as f64 * in_frames as f64;
                let base_frame = pos as usize;

                // Grab 6 surrounding sample indices (wrapping around)
                let idx = [
                    (base_frame + in_frames - 2) % in_frames,
                    (base_frame + in_frames - 1) % in_frames,
                    (base_frame + in_frames) % in_frames,
                    (base_frame + in_frames + 1) % in_frames,
                    (base_frame + in_frames + 2) % in_frames,
                    (base_frame + in_frames + 3) % in_frames,
                ];

                // Interpolate using the 6-point polynomial
                let sample = poly(
                    pos % 1.0,
                    input[idx[0] * ch + channel] as f64,
                    input[idx[1] * ch + channel] as f64,
                    input[idx[2] * ch + channel] as f64,
                    input[idx[3] * ch + channel] as f64,
                    input[idx[4] * ch + channel] as f64,
                    input[idx[5] * ch + channel] as f64,
                ).clamp(-32767.0, 32767.0);

                result[out_i * ch + channel] = sample as i16;
            }
        }

        result
    }

    fn get_data_offset(&self, start_frame: u64) -> u64 {
        self.data_byte_offset + self.get_data_length(start_frame)
    }

    fn get_data_length(&self, frame_count: u64) -> u64 {
        frame_count * self.channel_count as u64 * 2
    }
}

fn read_u32(reader: &mut impl Read) -> Result<u32, String> {
    let mut buffer = [0u8; 4];
    reader.read_exact(&mut buffer)
        .map_err(|e| format!("Failed to read u32: {}", e))?;
    Ok(u32::from_le_bytes(buffer))
}

fn read_u16(reader: &mut impl Read) -> Result<u16, String> {
    let mut buffer = [0u8; 2];
    reader.read_exact(&mut buffer)
        .map_err(|e| format!("Failed to read u16: {}", e))?;
    Ok(u16::from_le_bytes(buffer))
}

fn poly(x: f64, y_2: f64, y_1: f64, y0: f64, y1: f64, y2: f64, y3: f64) -> f64 {
    let base = x - 0.5;
    let a = y1 + y0;
    let b = y1 - y0;
    let c = y2 + y_1;
    let d = y2 - y_1;
    let e = y3 + y_2;
    let f = y3 - y_2;

    let g = a * 0.426859834093794 + c * 0.0723812351117003 + e * 0.00075893079450573;
    let h = b * 0.358317723488933 + d * 0.204516445547583 + f * 0.00562658797241955;
    let i = a * -0.217009177221292 + c * 0.200513765940862 + e * 0.0164954112804021;
    let j = b * -0.25112715343741 + d * 0.0422302599220046 + f * 0.0248872747299513;
    let k = a * 0.0416694667353327 + c * -0.0625042011435699 + e * 0.020834734408418;

    (((((b * 0.0834979923567504 + d * -0.0417491284163099 + f * 0.00834987866042734) * base + k) * base + j) * base + i) * base + h) * base + g
}