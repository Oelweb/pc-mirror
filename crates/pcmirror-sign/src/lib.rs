// SPDX-License-Identifier: GPL-3.0-or-later
//! Detached Ed25519 signatures for the update feed (`releases.win.json.sig`).
//!
//! The SHA-256 values inside the feed protect every package. Signing the feed
//! itself means a compromised web server can no longer hand out its own
//! packages: only the holder of the offline private key can publish.

use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

/// Returns `(private_key, public_key)`, both Base64.
pub fn generate_keypair() -> (String, String) {
    let key = SigningKey::generate(&mut rand_core::OsRng);
    (
        STANDARD.encode(key.to_bytes()),
        STANDARD.encode(key.verifying_key().to_bytes()),
    )
}

pub fn sign(message: &[u8], private_key_b64: &str) -> Result<String, String> {
    let bytes: [u8; 32] = decode_fixed(private_key_b64, "Privater Schlüssel")?;
    let key = SigningKey::from_bytes(&bytes);
    Ok(STANDARD.encode(key.sign(message).to_bytes()))
}

pub fn verify(message: &[u8], signature_b64: &str, public_key_b64: &str) -> Result<(), String> {
    let public: [u8; 32] = decode_fixed(public_key_b64, "Öffentlicher Schlüssel")?;
    let signature: [u8; 64] = decode_fixed(signature_b64, "Signatur")?;
    let key = VerifyingKey::from_bytes(&public)
        .map_err(|e| format!("Öffentlicher Schlüssel ungültig: {e}"))?;
    key.verify_strict(message, &Signature::from_bytes(&signature))
        .map_err(|_| "Signatur des Update-Feeds ist ungültig".to_owned())
}

fn decode_fixed<const N: usize>(value: &str, what: &str) -> Result<[u8; N], String> {
    let bytes = STANDARD
        .decode(value.trim())
        .map_err(|_| format!("{what}: kein gültiges Base64"))?;
    bytes
        .try_into()
        .map_err(|_| format!("{what}: falsche Länge"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_and_rejects_tampering() {
        let (private, public) = generate_keypair();
        let feed = br#"{"Assets":[]}"#;
        let signature = sign(feed, &private).unwrap();
        assert!(verify(feed, &signature, &public).is_ok());
        assert!(verify(br#"{"Assets":[{}]}"#, &signature, &public).is_err());
        let (_, other) = generate_keypair();
        assert!(verify(feed, &signature, &other).is_err());
        assert!(verify(feed, "kein base64", &public).is_err());
    }
}
