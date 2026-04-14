use std::io::Cursor;
use lewton::inside_ogg::OggStreamReader;

use crate::decoded_audio::DecodedAudio;

pub fn decode(source_name: &str, data: &[u8]) -> Result<DecodedAudio, String> {
    let cursor = Cursor::new(data);
    let mut reader = OggStreamReader::new(cursor)
        .map_err(|e| format!("Failed to parse OGG header in {}: {}", source_name, e))?;

    let sample_rate = reader.ident_hdr.audio_sample_rate;
    let channel_count = reader.ident_hdr.audio_channels as u16;

    let mut samples: Vec<i16> = Vec::new();
    loop {
        match reader.read_dec_packet_itl() {
            Ok(Some(packet)) => samples.extend_from_slice(&packet),
            Ok(None) => break,
            Err(e) => return Err(format!("Failed to decode OGG packet in {}: {}", source_name, e)),
        }
    }

    let frame_count = samples.len() as u64 / channel_count as u64;

    Ok(DecodedAudio {
        samples,
        frame_rate: sample_rate,
        channel_count,
        frame_count,
    })
}
