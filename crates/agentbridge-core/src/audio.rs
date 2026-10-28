const PCM_FORMAT_TAG: u16 = 1;
const FMT_CHUNK_LEN: u32 = 16;
const CHANNELS: u16 = 1;
const BITS_PER_SAMPLE: u16 = 16;
const HEADER_LEN: usize = 44;
/// RIFF size excludes the 8-byte "RIFF"+size prefix.
const RIFF_SIZE_OFFSET: u32 = 36;

/// Wraps raw 16-bit little-endian mono PCM (what ElevenLabs returns for `pcm_*`) in a WAV header.
pub fn pcm16_mono_to_wav(pcm: &[u8], sample_rate: u32) -> Vec<u8> {
    let block_align = CHANNELS * BITS_PER_SAMPLE / 8;
    let byte_rate = sample_rate * u32::from(block_align);
    let data_len = pcm.len() as u32;

    let mut out = Vec::with_capacity(HEADER_LEN + pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(RIFF_SIZE_OFFSET + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&FMT_CHUNK_LEN.to_le_bytes());
    out.extend_from_slice(&PCM_FORMAT_TAG.to_le_bytes());
    out.extend_from_slice(&CHANNELS.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(pcm);
    out
}

/// Sample rate of raw PCM as providers label it, e.g. `audio/L16;codec=pcm;rate=24000`.
pub fn pcm_rate(mime: &str) -> Option<u32> {
    mime.split(';')
        .filter_map(|param| param.trim().strip_prefix("rate="))
        .find_map(|rate| rate.trim().parse().ok())
}

/// Speech providers return either a finished WAV file or raw 16-bit mono PCM; both become WAV.
pub fn ensure_wav(bytes: Vec<u8>, default_rate: u32, mime: &str) -> Vec<u8> {
    if bytes.starts_with(b"RIFF") {
        return bytes;
    }
    pcm16_mono_to_wav(&bytes, pcm_rate(mime).unwrap_or(default_rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_pcm_rate_and_keeps_existing_wav() {
        assert_eq!(pcm_rate("audio/L16;codec=pcm;rate=24000"), Some(24_000));
        assert_eq!(pcm_rate("audio/pcm"), None);
        let wav = pcm16_mono_to_wav(&[0u8; 4], 8_000);
        assert_eq!(ensure_wav(wav.clone(), 24_000, "audio/wav"), wav);
        let wrapped = ensure_wav(vec![0u8; 4], 24_000, "audio/pcm; rate=16000");
        assert_eq!(
            u32::from_le_bytes(wrapped[24..28].try_into().unwrap()),
            16_000
        );
    }

    #[test]
    fn builds_valid_header() {
        let wav = pcm16_mono_to_wav(&[0u8; 100], 44_100);
        assert_eq!(wav.len(), HEADER_LEN + 100);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()), 136);
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 44_100);
        assert_eq!(u32::from_le_bytes(wav[28..32].try_into().unwrap()), 88_200);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 100);
    }
}
