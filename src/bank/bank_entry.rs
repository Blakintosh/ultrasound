
#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct BankEntry {
    pub name: u32,
    pub size: u32,
    pub frame_count: u32,
    pub order: u32,
    pub offset: u64,
    pub frame_rate_index: u8,
    pub channel_count: u8,
    pub looping: u8,
    pub format: u8,
    pub envelope_loudness0: u8,
    pub envelope_loudness1: u8,
    pub envelope_loudness2: u8,
    pub envelope_loudness3: u8,
    pub envelope_time1: u16,
    pub envelope_time2: u16,
}

impl BankEntry {
    pub fn new(name: u32, size: u32, frame_count: u32, order: u32, offset: u64, frame_rate_index: u8, channel_count: u8, looping: u8, format: u8, envelope_loudness0: u8, envelope_loudness1: u8, envelope_loudness2: u8, envelope_loudness3: u8, envelope_time1: u16, envelope_time2: u16) -> BankEntry {
        BankEntry {
            name,
            size,
            frame_count,
            order,
            offset,
            frame_rate_index,
            channel_count,
            looping,
            format,
            envelope_loudness0,
            envelope_loudness1,
            envelope_loudness2,
            envelope_loudness3,
            envelope_time1,
            envelope_time2
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let expected = std::mem::size_of::<Self>();
        if bytes.len() < expected {
            return Err(format!("BankEntry buffer too small: {} < {}", bytes.len(), expected));
        }
        Ok(unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const Self) })
    }

    pub fn get_frame_rate(&self) -> i32 {
        match self.frame_rate_index {
            0 => 8000,
            1 => 12000,
            2 => 16000,
            3 => 24000,
            4 => 32000,
            5 => 44100,
            6 => 48000,
            7 => 96000,
            8 => 192000,
            _ => 0,
        }
    }

    pub fn lookup_frame_rate_index(sample_rate: i32) -> u8 {
        match sample_rate {
            8000 => 0,
            12000 => 1,
            16000 => 2,
            24000 => 3,
            32000 => 4,
            44100 => 5,
            48000 => 6,
            96000 => 7,
            192000 => 8,
            _ => u8::MAX,
        }
    }
}