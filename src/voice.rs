use crate::cdn::download::{download_and_decrypt, download_plain};
use crate::client::ILinkClient;
use crate::error::Result;
use crate::http_client::HttpClient;
use crate::types::VoiceItem;

/// Default sample rate for Weixin voice messages (Hz).
pub const DEFAULT_VOICE_SAMPLE_RATE: u32 = 24_000;

/// Pluggable SILK decoder trait.
///
/// Implement this to provide SILK → PCM decoding.
/// The returned bytes must be raw PCM signed 16-bit little-endian mono audio.
pub trait SilkDecoder: Send + Sync {
    fn decode(&self, silk_data: &[u8], sample_rate: u32) -> std::result::Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>>;
}

/// Built-in SILK decoder using the `silk-codec` crate.
///
/// ```ignore
/// use weixin_ilink_sdk::voice::{DefaultSilkDecoder, download_voice};
/// let voice_data = download_voice(&client, &voice_item, Some(&DefaultSilkDecoder)).await?;
/// ```
pub struct DefaultSilkDecoder;

impl SilkDecoder for DefaultSilkDecoder {
    fn decode(
        &self,
        silk_data: &[u8],
        sample_rate: u32,
    ) -> std::result::Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let pcm = silk_codec::decode_silk(silk_data, sample_rate as i32)?;
        Ok(pcm)
    }
}

/// Download a voice message from CDN, decrypt it, optionally decode SILK to WAV.
///
/// - If `decoder` is provided and decoding succeeds, returns WAV bytes.
/// - Otherwise, returns raw SILK bytes.
pub async fn download_voice<H: HttpClient>(
    client: &ILinkClient<H>,
    voice: &VoiceItem,
    decoder: Option<&dyn SilkDecoder>,
) -> Result<VoiceData> {
    let media = voice
        .media
        .as_ref()
        .ok_or_else(|| crate::error::Error::Other("voice has no media".into()))?;
    let param = media
        .encrypt_query_param
        .as_deref()
        .ok_or_else(|| crate::error::Error::Other("voice has no encrypt_query_param".into()))?;

    let silk_bytes = if let Some(aes_key) = &media.aes_key {
        download_and_decrypt(client, param, aes_key).await?
    } else {
        download_plain(client, param).await?
    };

    if let Some(dec) = decoder {
        let sample_rate = voice.sample_rate.unwrap_or(DEFAULT_VOICE_SAMPLE_RATE);
        match dec.decode(&silk_bytes, sample_rate) {
            Ok(pcm) => {
                let wav = build_wav(&pcm, sample_rate);
                return Ok(VoiceData {
                    data: wav,
                    format: VoiceFormat::Wav,
                });
            }
            Err(e) => {
                tracing::warn!(error = %e, "SILK decode failed, returning raw SILK");
            }
        }
    }

    Ok(VoiceData {
        data: silk_bytes,
        format: VoiceFormat::Silk,
    })
}

/// Result of voice download.
#[derive(Debug)]
pub struct VoiceData {
    pub data: Vec<u8>,
    pub format: VoiceFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceFormat {
    Silk,
    Wav,
}

/// Wrap raw PCM (signed 16-bit LE, mono) in a WAV container.
pub fn build_wav(pcm: &[u8], sample_rate: u32) -> Vec<u8> {
    assert!(pcm.len() <= u32::MAX as usize, "PCM data exceeds WAV 4GB limit");
    let pcm_len = pcm.len() as u32;
    let total_size = 44 + pcm_len;
    let mut buf = Vec::with_capacity(total_size as usize);

    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(total_size - 8).to_le_bytes());
    buf.extend_from_slice(b"WAVE");

    // fmt chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    buf.extend_from_slice(&1u16.to_le_bytes()); // mono
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    buf.extend_from_slice(&2u16.to_le_bytes()); // block align
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

    // data chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&pcm_len.to_le_bytes());
    buf.extend_from_slice(pcm);

    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_valid() {
        let pcm = vec![0u8; 100];
        let wav = build_wav(&pcm, 24000);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
        // data chunk size
        let data_size = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
        assert_eq!(data_size, 100);
        assert_eq!(wav.len(), 144);
    }
}
