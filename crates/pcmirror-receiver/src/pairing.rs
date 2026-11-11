// SPDX-License-Identifier: GPL-3.0-or-later
//! Geräteidentität (Ed25519), Pair-Setup/-Verify und PIN-Kopplung.
//! Entspricht UxPlays `pairing.c` und dem Schlüsselteil von `crypto.c`.
use crate::crypto::{derive16, gcm_decrypt, gcm_encrypt, CtrStream};
use crate::srp;
use base64::Engine;
use ed25519_dalek::pkcs8::{DecodePrivateKey, EncodePrivateKey};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use std::path::Path;
use x25519_dalek::{PublicKey, StaticSecret};

const SALT_KEY: &[u8] = b"Pair-Verify-AES-Key";
const SALT_IV: &[u8] = b"Pair-Verify-AES-IV";

/// Dauerhafte Identität des Empfängers. Liest bzw. schreibt die PEM-Datei im
/// selben PKCS#8-Format wie OpenSSL (`receiver.pem` aus der C-Fassung bleibt gültig).
pub struct Identity {
    key: SigningKey,
}

impl Identity {
    /// Gibt (Identität, neu erzeugt) zurück.
    pub fn load_or_create(path: &Path) -> (Self, bool) {
        if let Ok(pem) = std::fs::read_to_string(path) {
            if let Ok(key) = SigningKey::from_pkcs8_pem(&pem) {
                return (Self { key }, false);
            }
        }
        let key = SigningKey::generate(&mut rand_core::OsRng);
        if let Ok(pem) = key.to_pkcs8_pem(Default::default()) {
            let _ = std::fs::write(path, pem.as_bytes());
        }
        (Self { key }, true)
    }

    #[cfg(test)]
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self { key: SigningKey::from_bytes(&seed) }
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.key.verifying_key().to_bytes()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Status {
    Initial,
    Setup,
    Handshake,
    Finished,
}

/// Pairing-Zustand einer Verbindung.
pub struct Session {
    status: Status,
    ecdh_ours: Option<StaticSecret>,
    ecdh_theirs: [u8; 32],
    ed_theirs: [u8; 32],
    ecdh_secret: Option<[u8; 32]>,
    username: String,
    client_pk: Option<[u8; 32]>,
    srp: Option<srp::Server>,
    srp_session_key: Option<[u8; srp::SESSION_KEY_SIZE]>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            status: Status::Initial,
            ecdh_ours: None,
            ecdh_theirs: [0; 32],
            ed_theirs: [0; 32],
            ecdh_secret: None,
            username: String::new(),
            client_pk: None,
            srp: None,
            srp_session_key: None,
        }
    }
}

impl Session {
    pub fn set_setup(&mut self) {
        self.status = Status::Setup;
    }

    /// `pairing_session_check_handshake_status == 0`
    pub fn ready_for_verify(&self) -> bool {
        matches!(self.status, Status::Setup | Status::Handshake)
    }

    /// Gemeinsames Geheimnis, sobald Pair-Verify begonnen hat.
    pub fn ecdh_secret(&self) -> Option<[u8; 32]> {
        if self.status == Status::Initial {
            None
        } else {
            self.ecdh_secret
        }
    }

    fn keys(&self) -> ([u8; 16], [u8; 16]) {
        let secret = self.ecdh_secret.unwrap_or([0; 32]);
        (derive16(SALT_KEY, &secret), derive16(SALT_IV, &secret))
    }

    /// Pair-Verify Schritt 1: Gibt unseren X25519-Schlüssel und die verschlüsselte
    /// Signatur zurück (32 + 64 Byte).
    pub fn verify_start(&mut self, identity: &Identity, ecdh_theirs: &[u8; 32], ed_theirs: &[u8; 32]) -> Option<Vec<u8>> {
        if self.status == Status::Finished {
            return None;
        }
        let ours = StaticSecret::random_from_rng(rand_core::OsRng);
        let our_public = PublicKey::from(&ours).to_bytes();
        let secret = ours.diffie_hellman(&PublicKey::from(*ecdh_theirs)).to_bytes();
        self.ecdh_ours = Some(ours);
        self.ecdh_theirs = *ecdh_theirs;
        self.ed_theirs = *ed_theirs;
        self.ecdh_secret = Some(secret);
        self.status = Status::Handshake;

        let mut message = [0u8; 64];
        message[..32].copy_from_slice(&our_public);
        message[32..].copy_from_slice(ecdh_theirs);
        let mut signature = identity.key.sign(&message).to_bytes();
        let (key, iv) = self.keys();
        CtrStream::new(&key, &iv).apply(&mut signature);

        let mut out = Vec::with_capacity(96);
        out.extend_from_slice(&our_public);
        out.extend_from_slice(&signature);
        Some(out)
    }

    /// Pair-Verify Schritt 2: prüft die Signatur des Clients.
    pub fn verify_finish(&mut self, encrypted_signature: &[u8; 64]) -> bool {
        if self.status != Status::Handshake {
            return false;
        }
        let Some(ours) = &self.ecdh_ours else { return false };
        let (key, iv) = self.keys();
        let mut stream = CtrStream::new(&key, &iv);
        stream.seek(64); // ein „Leerlauf“ für unsere eigene Signatur
        let mut signature = *encrypted_signature;
        stream.apply(&mut signature);

        let mut message = [0u8; 64];
        message[..32].copy_from_slice(&self.ecdh_theirs);
        message[32..].copy_from_slice(&PublicKey::from(ours).to_bytes());
        let Ok(their_key) = VerifyingKey::from_bytes(&self.ed_theirs) else {
            return false;
        };
        if their_key.verify(&message, &Signature::from_bytes(&signature)).is_err() {
            return false;
        }
        self.status = Status::Finished;
        true
    }

    /// Öffentlicher Ed25519-Schlüssel des Clients aus Pair-Verify (Base64).
    pub fn client_ed_key_base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.ed_theirs)
    }

    // ------------------------------------------------------------ PIN (SRP)

    /// pair-setup-pin Schritt 1: Gibt (Salz, öffentliches B) zurück.
    pub fn srp_start(&mut self, username: &str, pin: &str) -> Option<(Vec<u8>, Vec<u8>)> {
        if username.len() > 24 {
            return None;
        }
        let server = srp::Server::new(username, pin);
        let result = (server.salt().to_vec(), server.public_key());
        self.username = username.to_owned();
        self.srp = Some(server);
        self.srp_session_key = None;
        Some(result)
    }

    /// pair-setup-pin Schritt 2: prüft den Beweis, gibt unseren 20-Byte-Beweis zurück.
    pub fn srp_verify(&mut self, client_a: &[u8], client_proof: &[u8]) -> Option<[u8; 20]> {
        let server = self.srp.as_ref()?;
        match server.verify(client_a, client_proof) {
            Some(verified) => {
                self.srp_session_key = Some(verified.session_key);
                Some(verified.proof)
            }
            None => {
                self.srp = None;
                None
            }
        }
    }

    /// pair-setup-pin Schritt 3: entschlüsselt den Client-Schlüssel und gibt unseren
    /// verschlüsselt zurück (epk, authTag).
    pub fn srp_confirm(&mut self, identity: &Identity, epk: &[u8; 32], auth_tag: &[u8; 16]) -> Option<([u8; 32], [u8; 16])> {
        let session_key = self.srp_session_key.take()?;
        self.srp = None;
        let aes_key = derive16(b"Pair-Setup-AES-Key", &session_key);
        let mut aes_iv = derive16(b"Pair-Setup-AES-IV", &session_key);
        aes_iv[15] = aes_iv[15].wrapping_add(1);
        let client_pk = gcm_decrypt(&aes_key, &aes_iv, epk, auth_tag)?;
        self.client_pk = Some(client_pk.try_into().ok()?);
        aes_iv[15] = aes_iv[15].wrapping_add(1);
        let (cipher, tag) = gcm_encrypt(&aes_key, &aes_iv, &identity.public_key());
        Some((cipher.try_into().ok()?, tag))
    }

    /// Gerätekennung und Schlüssel des Clients nach erfolgreicher PIN-Kopplung.
    pub fn paired_client(&self) -> Option<(String, String)> {
        let pk = self.client_pk?;
        Some((self.username.clone(), base64::engine::general_purpose::STANDARD.encode(pk)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vollständiges Pair-Verify mit einem unabhängig geschriebenen Client.
    #[test]
    fn pair_verify_round_trip() {
        let identity = Identity::from_seed([3u8; 32]);
        let client_ed = SigningKey::from_bytes(&[9u8; 32]);
        let client_x = StaticSecret::from([5u8; 32]);
        let client_x_pub = PublicKey::from(&client_x).to_bytes();

        let mut session = Session::default();
        session.set_setup();
        let reply = session
            .verify_start(&identity, &client_x_pub, &client_ed.verifying_key().to_bytes())
            .unwrap();
        assert_eq!(reply.len(), 96);
        let server_x: [u8; 32] = reply[..32].try_into().unwrap();
        let secret = client_x.diffie_hellman(&PublicKey::from(server_x)).to_bytes();
        let key = derive16(SALT_KEY, &secret);
        let iv = derive16(SALT_IV, &secret);
        let mut stream = CtrStream::new(&key, &iv);

        // Client prüft die Server-Signatur …
        let mut server_sig: [u8; 64] = reply[32..].try_into().unwrap();
        stream.apply(&mut server_sig);
        let mut message = [0u8; 64];
        message[..32].copy_from_slice(&server_x);
        message[32..].copy_from_slice(&client_x_pub);
        VerifyingKey::from_bytes(&identity.public_key())
            .unwrap()
            .verify(&message, &Signature::from_bytes(&server_sig))
            .expect("Server-Signatur gültig");

        // … und signiert selbst.
        let mut message = [0u8; 64];
        message[..32].copy_from_slice(&client_x_pub);
        message[32..].copy_from_slice(&server_x);
        let mut client_sig = client_ed.sign(&message).to_bytes();
        stream.apply(&mut client_sig);
        assert!(session.verify_finish(&client_sig));
        assert!(session.ecdh_secret().is_some());

        // Manipulierte Signatur wird abgewiesen
        let mut session2 = Session::default();
        session2.set_setup();
        session2.verify_start(&identity, &client_x_pub, &client_ed.verifying_key().to_bytes()).unwrap();
        assert!(!session2.verify_finish(&[0u8; 64]));
    }

    #[test]
    fn identity_survives_pem_round_trip() {
        let dir = std::env::temp_dir().join(format!("pcmirror-id-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("receiver.pem");
        let _ = std::fs::remove_file(&path);
        let (first, created) = Identity::load_or_create(&path);
        assert!(created);
        let (second, created) = Identity::load_or_create(&path);
        assert!(!created);
        assert_eq!(first.public_key(), second.public_key());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Die vorhandene OpenSSL-PEM-Datei der C-Fassung wird gelesen (falls vorhanden).
    #[test]
    fn reads_openssl_pem_if_present() {
        if let Ok(base) = std::env::var("LOCALAPPDATA") {
            let path = Path::new(&base).join("PC-Mirror").join("receiver.pem");
            if let Ok(pem) = std::fs::read_to_string(&path) {
                assert!(SigningKey::from_pkcs8_pem(&pem).is_ok(), "OpenSSL-PEM muss lesbar sein");
            }
        }
    }
}
