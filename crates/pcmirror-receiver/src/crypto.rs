// SPDX-License-Identifier: GPL-3.0-or-later
//! Kryptografische Bausteine (Ersatz für UxPlays OpenSSL-Hülle `crypto.c`).
use aes::cipher::{BlockDecryptMut, KeyIvInit, StreamCipher, StreamCipherSeek};
use aes::Aes128;
use aes_gcm::aead::consts::U16;
use aes_gcm::aead::AeadInPlace;
use aes_gcm::{AesGcm, KeyInit, Nonce, Tag};
use sha2::{Digest, Sha512};

pub type Aes128Ctr = ctr::Ctr128BE<Aes128>;
type Aes128Gcm16 = AesGcm<Aes128, U16, U16>;

/// SHA-512 über mehrere Teile (UxPlays `sha_init/sha_update/sha_final`).
pub fn sha512(parts: &[&[u8]]) -> [u8; 64] {
    let mut hasher = Sha512::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

/// Die ersten 16 Byte von SHA-512(salt || secret).
pub fn derive16(salt: &[u8], secret: &[u8]) -> [u8; 16] {
    sha512(&[salt, secret])[..16].try_into().unwrap()
}

/// AES-128-CTR mit fortlaufendem Zustand wie EVP-CTR in OpenSSL; zusätzlich der
/// Blockversatz für `aes_ctr_start_fresh_block`.
pub struct CtrStream {
    cipher: Aes128Ctr,
    block_offset: usize,
}

impl CtrStream {
    pub fn new(key: &[u8; 16], iv: &[u8; 16]) -> Self {
        Self {
            cipher: Aes128Ctr::new(key.into(), iv.into()),
            block_offset: 0,
        }
    }

    pub fn apply(&mut self, data: &mut [u8]) {
        self.cipher.apply_keystream(data);
        self.block_offset = (self.block_offset + data.len()) % 16;
    }

    /// Überspringt den Rest des angefangenen Blocks.
    pub fn start_fresh_block(&mut self) {
        if self.block_offset != 0 {
            let pos: u64 = self.cipher.current_pos();
            self.cipher.seek(pos + (16 - self.block_offset) as u64);
            self.block_offset = 0;
        }
    }

    pub fn seek(&mut self, position: u64) {
        self.cipher.seek(position);
        self.block_offset = (position % 16) as usize;
    }
}

/// AES-128-CBC-Entschlüsselung ohne Padding; nur ganze Blöcke, der Rest bleibt
/// unverschlüsselt (AirPlay-Audio).
pub fn aes_cbc_decrypt_blocks(key: &[u8; 16], iv: &[u8; 16], data: &mut [u8]) {
    let full = data.len() / 16 * 16;
    let mut decryptor = cbc::Decryptor::<Aes128>::new(key.into(), iv.into());
    for block in data[..full].chunks_exact_mut(16) {
        decryptor.decrypt_block_mut(block.into());
    }
}

/// AES-128-GCM mit 16-Byte-IV (wie UxPlays `gcm_encrypt`). Gibt (Chiffre, Tag) zurück.
pub fn gcm_encrypt(key: &[u8; 16], iv: &[u8; 16], plain: &[u8]) -> (Vec<u8>, [u8; 16]) {
    let cipher = Aes128Gcm16::new(key.into());
    let mut buffer = plain.to_vec();
    let tag = cipher
        .encrypt_in_place_detached(Nonce::<U16>::from_slice(iv), b"", &mut buffer)
        .expect("GCM-Verschlüsselung");
    (buffer, tag.into())
}

/// Entschlüsselt und prüft den Tag; `None` bei falschem Tag.
pub fn gcm_decrypt(key: &[u8; 16], iv: &[u8; 16], cipher_text: &[u8], tag: &[u8; 16]) -> Option<Vec<u8>> {
    let cipher = Aes128Gcm16::new(key.into());
    let mut buffer = cipher_text.to_vec();
    cipher
        .decrypt_in_place_detached(Nonce::<U16>::from_slice(iv), b"", &mut buffer, Tag::<U16>::from_slice(tag))
        .ok()?;
    Some(buffer)
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut out);
    out
}

/// Zufällige vierstellige PIN 1..9999 (wie `random_pin`).
pub fn random_pin() -> u16 {
    loop {
        let bytes = random_bytes::<2>();
        let value = u16::from_le_bytes(bytes) % 10000;
        if value != 0 {
            return value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gcm_round_trip_and_tag_check() {
        let key = [7u8; 16];
        let iv = [9u8; 16];
        let (cipher, tag) = gcm_encrypt(&key, &iv, b"hallo welt 32 byte lang........");
        assert_eq!(gcm_decrypt(&key, &iv, &cipher, &tag).unwrap(), b"hallo welt 32 byte lang........");
        let mut bad = tag;
        bad[0] ^= 1;
        assert!(gcm_decrypt(&key, &iv, &cipher, &bad).is_none());
    }

    /// Vergleich mit OpenSSL (`tests/gcm_vectors.c`): 16-Byte-IV wie bei Apple.
    #[test]
    fn gcm_matches_openssl() {
        let key: [u8; 16] = std::array::from_fn(|i| (i * 3 + 1) as u8);
        let iv: [u8; 16] = std::array::from_fn(|i| (i * 5 + 2) as u8);
        let plain: Vec<u8> = (0..32).map(|i| (i * 7 + 3) as u8).collect();
        let (cipher, tag) = gcm_encrypt(&key, &iv, &plain);
        let expected = include_str!("../tests/gcm_vectors.txt");
        let (ct, tg) = expected.trim().split_once(' ').unwrap();
        let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        assert_eq!(hex(&cipher), ct);
        assert_eq!(hex(&tag), tg);
    }

    #[test]
    fn ctr_fresh_block_skips_rest() {
        let key = [1u8; 16];
        let iv = [2u8; 16];
        let mut a = CtrStream::new(&key, &iv);
        let mut first = [0u8; 5];
        a.apply(&mut first);
        a.start_fresh_block();
        let mut second = [0u8; 16];
        a.apply(&mut second);
        let mut b = CtrStream::new(&key, &iv);
        let mut whole = [0u8; 32];
        b.apply(&mut whole);
        assert_eq!(&whole[..5], &first);
        assert_eq!(&whole[16..], &second);
    }
}
