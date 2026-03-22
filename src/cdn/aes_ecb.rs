use aes::Aes128;
use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyInit};
use ecb::{Decryptor, Encryptor};

use crate::error::{Error, Result};

type Aes128EcbEnc = Encryptor<Aes128>;
type Aes128EcbDec = Decryptor<Aes128>;

/// Encrypt plaintext with AES-128-ECB (PKCS7 padding).
pub fn encrypt_aes_ecb(plaintext: &[u8], key: &[u8; 16]) -> Vec<u8> {
    let enc = Aes128EcbEnc::new_from_slice(key).expect("invalid key length");
    enc.encrypt_padded_vec_mut::<cipher::block_padding::Pkcs7>(plaintext)
}

/// Decrypt ciphertext with AES-128-ECB (PKCS7 padding).
pub fn decrypt_aes_ecb(ciphertext: &[u8], key: &[u8; 16]) -> Result<Vec<u8>> {
    let dec = Aes128EcbDec::new_from_slice(key).expect("invalid key length");
    dec.decrypt_padded_vec_mut::<cipher::block_padding::Pkcs7>(ciphertext)
        .map_err(|e| Error::Aes(format!("AES-ECB decryption failed: {e}")))
}

/// Compute AES-128-ECB ciphertext size with PKCS7 padding.
pub fn aes_ecb_padded_size(plaintext_size: usize) -> usize {
    ((plaintext_size + 1 + 15) / 16) * 16
}

/// Parse an AES key from base64.
///
/// Two encodings exist in the wild:
/// - base64(raw 16 bytes) — images
/// - base64(32-char hex string) — file/voice/video
pub fn parse_aes_key(aes_key_base64: &str) -> Result<[u8; 16]> {
    use base64::Engine;
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};

    let decoded = STANDARD
        .decode(aes_key_base64)
        .or_else(|_| STANDARD_NO_PAD.decode(aes_key_base64))
        .or_else(|_| URL_SAFE.decode(aes_key_base64))
        .or_else(|_| URL_SAFE_NO_PAD.decode(aes_key_base64))
        .map_err(|e| Error::Aes(format!("base64 decode failed (tried all variants): {e}")))?;

    if decoded.len() == 16 {
        let mut key = [0u8; 16];
        key.copy_from_slice(&decoded);
        return Ok(key);
    }

    if decoded.len() == 32 {
        let hex_str = std::str::from_utf8(&decoded)
            .map_err(|_| Error::Aes("aes_key 32-byte decode is not valid UTF-8".into()))?;
        let bytes = hex::decode(hex_str)
            .map_err(|e| Error::Aes(format!("aes_key hex decode failed: {e}")))?;
        if bytes.len() == 16 {
            let mut key = [0u8; 16];
            key.copy_from_slice(&bytes);
            return Ok(key);
        }
    }

    Err(Error::Aes(format!(
        "aes_key must decode to 16 raw bytes or 32-char hex, got {} bytes",
        decoded.len()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = [0x42u8; 16];
        let plaintext = b"hello, weixin ilink!";
        let ciphertext = encrypt_aes_ecb(plaintext, &key);
        let decrypted = decrypt_aes_ecb(&ciphertext, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn padded_size() {
        assert_eq!(aes_ecb_padded_size(0), 16);
        assert_eq!(aes_ecb_padded_size(1), 16);
        assert_eq!(aes_ecb_padded_size(15), 16);
        assert_eq!(aes_ecb_padded_size(16), 32);
    }

    #[test]
    fn parse_key_raw_16() {
        use base64::Engine;
        use base64::engine::general_purpose::STANDARD;
        let raw = [0xABu8; 16];
        let b64 = STANDARD.encode(raw);
        let key = parse_aes_key(&b64).unwrap();
        assert_eq!(key, raw);
    }

    #[test]
    fn parse_key_hex_32() {
        use base64::Engine;
        use base64::engine::general_purpose::STANDARD;
        let raw = [0xCDu8; 16];
        let hex_str = hex::encode(raw);
        let b64 = STANDARD.encode(hex_str.as_bytes());
        let key = parse_aes_key(&b64).unwrap();
        assert_eq!(key, raw);
    }
}
