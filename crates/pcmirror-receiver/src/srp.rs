// SPDX-License-Identifier: GPL-3.0-or-later
//! SRP-6a (SHA-1, 2048-Bit-Gruppe aus RFC 5054) in Apples Variante für die
//! PIN-Kopplung. Entspricht UxPlays `srp.c` mit `APPLE_VARIANT` und `rfc5054_compat`.
use num_bigint::BigUint;
use sha1::{Digest, Sha1};

const N_HEX: &str = "AC6BDB41324A9A9BF166DE5E1389582FAF72B6651987EE07FC3192943DB56050A37329CBB4A099ED8193E0757767A13DD52312AB4B03310DCD7F48A9DA04FD50E8083969EDB767B0CF6095179A163AB3661A05FBD5FAAAE82918A9962F0B93B855F97993EC975EEAA80D740ADBF4FF747359D041D5C33EA71D281E446B14773BCA97B43A23FB801676BD207A436C6481F1D2B9078717461A5B9D32E688F87748544523B524B0D57D5EA77A2775D2ECFA032CFBDBF52FB3786160279004E57AE6AF874E7303CE53299CCC041C7BC308D82A5698F3A8D0C38271AE35F8E9DBFBB694B5C803D89F7AE435DE236D525F54759B65E372FCD68EF20FA7111F9E4AFF73";

pub const SALT_SIZE: usize = 16;
pub const SESSION_KEY_SIZE: usize = 40;

fn n() -> BigUint {
    BigUint::parse_bytes(N_HEX.as_bytes(), 16).unwrap()
}

fn g() -> BigUint {
    BigUint::from(2u8)
}

fn sha1(parts: &[&[u8]]) -> [u8; 20] {
    let mut h = Sha1::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

/// BN_bn2bin: Big Endian ohne führende Nullen (0 → leer).
fn bytes(n: &BigUint) -> Vec<u8> {
    if n.bits() == 0 {
        Vec::new()
    } else {
        n.to_bytes_be()
    }
}

fn padded(n: &BigUint, len: usize) -> Vec<u8> {
    let raw = bytes(n);
    let mut out = vec![0u8; len.saturating_sub(raw.len())];
    out.extend_from_slice(&raw);
    out
}

/// H(PAD(a) || PAD(b)) mit Länge von N (RFC 5054).
fn h_nn(a: &BigUint, b: &BigUint) -> BigUint {
    let len = bytes(&n()).len();
    BigUint::from_bytes_be(&sha1(&[&padded(a, len), &padded(b, len)]))
}

fn calculate_x(salt: &[u8], username: &str, password: &[u8]) -> BigUint {
    let inner = sha1(&[username.as_bytes(), b":", password]);
    let salt_bn = BigUint::from_bytes_be(salt);
    BigUint::from_bytes_be(&sha1(&[&bytes(&salt_bn), &inner]))
}

/// Apples Sitzungsschlüssel: H(S || 00000000) || H(S || 00000001).
fn session_key(s: &BigUint) -> [u8; SESSION_KEY_SIZE] {
    let sb = bytes(s);
    let mut out = [0u8; SESSION_KEY_SIZE];
    out[..20].copy_from_slice(&sha1(&[&sb, &[0, 0, 0, 0]]));
    out[20..].copy_from_slice(&sha1(&[&sb, &[0, 0, 0, 1]]));
    out
}

fn calculate_m(username: &str, salt: &BigUint, a: &BigUint, b: &BigUint, k: &[u8]) -> [u8; 20] {
    let h_n = sha1(&[&bytes(&n())]);
    let h_g = sha1(&[&bytes(&g())]);
    let mut h_xor = [0u8; 20];
    for i in 0..20 {
        h_xor[i] = h_n[i] ^ h_g[i];
    }
    let h_i = sha1(&[username.as_bytes()]);
    sha1(&[&h_xor, &h_i, &bytes(salt), &bytes(a), &bytes(b), k])
}

/// Serverseite einer Kopplung.
pub struct Server {
    username: String,
    salt: [u8; SALT_SIZE],
    verifier: BigUint,
    private_b: BigUint,
    public_b: BigUint,
}

pub struct Verified {
    /// Antwort an den Client (H_AMK, 20 Byte).
    pub proof: [u8; 20],
    pub session_key: [u8; SESSION_KEY_SIZE],
}

impl Server {
    /// Legt Salz, Verifier und das öffentliche B an.
    pub fn new(username: &str, password: &str) -> Self {
        let mut salt = crate::crypto::random_bytes::<SALT_SIZE>();
        // Ein Salz mit führender Null hätte in der C-Fassung 15 Byte Länge und würde
        // abgewiesen; Apples Client rechnet mit den gesendeten 16 Byte.
        if salt[0] == 0 {
            salt[0] = 1;
        }
        let b = BigUint::from_bytes_be(&crate::crypto::random_bytes::<32>());
        Self::with_values(username, password, salt, b)
    }

    fn with_values(username: &str, password: &str, salt: [u8; SALT_SIZE], private_b: BigUint) -> Self {
        let n = n();
        let x = calculate_x(&salt, username, password.as_bytes());
        let verifier = g().modpow(&x, &n);
        let k = h_nn(&n, &g());
        let public_b = (k * &verifier + g().modpow(&private_b, &n)) % &n;
        Self {
            username: username.to_owned(),
            salt,
            verifier,
            private_b,
            public_b,
        }
    }

    pub fn salt(&self) -> &[u8; SALT_SIZE] {
        &self.salt
    }

    pub fn public_key(&self) -> Vec<u8> {
        bytes(&self.public_b)
    }

    /// Prüft A und den Beweis M des Clients; `None` bei falscher PIN.
    pub fn verify(&self, client_a: &[u8], client_m: &[u8]) -> Option<Verified> {
        let n = n();
        let a = BigUint::from_bytes_be(client_a);
        if client_m.len() < 20 || (&a % &n).bits() == 0 {
            return None;
        }
        let u = h_nn(&a, &self.public_b);
        let s = (&a * self.verifier.modpow(&u, &n)).modpow(&self.private_b, &n);
        let key = session_key(&s);
        let salt = BigUint::from_bytes_be(&self.salt);
        let m = calculate_m(&self.username, &salt, &a, &self.public_b, &key);
        // Vergleich in konstanter Zeit
        let diff = m.iter().zip(&client_m[..20]).fold(0u8, |acc, (x, y)| acc | (x ^ y));
        if diff != 0 {
            return None;
        }
        let proof = sha1(&[&bytes(&a), &m, &key]);
        Some(Verified {
            proof,
            session_key: key,
        })
    }
}

/// Clientseite (wie ein iPhone) für Test- und Diagnosewerkzeuge:
/// gibt (A, M, Sitzungsschlüssel K) zurück.
pub fn client_proof(username: &str, password: &str, salt: &[u8], public_b: &[u8], private_a: &[u8]) -> (Vec<u8>, [u8; 20], [u8; SESSION_KEY_SIZE]) {
    let n = n();
    let a = BigUint::from_bytes_be(private_a);
    let big_a = g().modpow(&a, &n);
    let b = BigUint::from_bytes_be(public_b);
    let u = h_nn(&big_a, &b);
    let k = h_nn(&n, &g());
    let x = calculate_x(salt, username, password.as_bytes());
    let gx = g().modpow(&x, &n);
    let base = (&b + &n * &k - (&k * &gx) % &n) % &n;
    let s = base.modpow(&(&a + &u * &x), &n);
    let key = session_key(&s);
    let m = calculate_m(username, &BigUint::from_bytes_be(salt), &big_a, &b, &key);
    (bytes(&big_a), m, key)
}

/// Erwarteter Server-Beweis H(A | M | K) zur Prüfung auf Clientseite.
pub fn expected_server_proof(public_a: &[u8], m: &[u8; 20], key: &[u8; SESSION_KEY_SIZE]) -> [u8; 20] {
    sha1(&[&bytes(&BigUint::from_bytes_be(public_a)), m, key])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Clientseite nach Apples Variante (unabhängig vom Server implementiert).
    fn client(username: &str, pin: &str, salt: &[u8], public_b: &[u8], a: &BigUint) -> (Vec<u8>, [u8; 20], [u8; 40]) {
        let n = n();
        let big_a = g().modpow(a, &n);
        let b = BigUint::from_bytes_be(public_b);
        let u = h_nn(&big_a, &b);
        let k = h_nn(&n, &g());
        let x = calculate_x(salt, username, pin.as_bytes());
        let gx = g().modpow(&x, &n);
        let base = (&b + &n * &k - (&k * &gx) % &n) % &n;
        let s = base.modpow(&(a + &u * &x), &n);
        let key = session_key(&s);
        let m = calculate_m(username, &BigUint::from_bytes_be(salt), &big_a, &b, &key);
        (bytes(&big_a), m, key)
    }

    #[test]
    fn pin_pairing_round_trip() {
        let server = Server::new("AA:BB:CC:DD:EE:FF", "1234");
        let a = BigUint::from_bytes_be(&crate::crypto::random_bytes::<32>());
        let (big_a, m, key) = client("AA:BB:CC:DD:EE:FF", "1234", server.salt(), &server.public_key(), &a);
        let verified = server.verify(&big_a, &m).expect("richtige PIN");
        assert_eq!(verified.session_key, key);
        let n = BigUint::from_bytes_be(&big_a);
        assert_eq!(verified.proof, sha1(&[&bytes(&n), &m, &key]));

        let (big_a, m, _) = client("AA:BB:CC:DD:EE:FF", "9999", server.salt(), &server.public_key(), &a);
        assert!(server.verify(&big_a, &m).is_none(), "falsche PIN muss scheitern");
        assert!(server.verify(&[0u8; 256], &m).is_none(), "A = 0 muss scheitern");
    }

    /// Vergleich mit der C-Fassung (UxPlay srp.c), erzeugt mit `tests/srp_vectors.c`.
    #[test]
    fn matches_c_reference() {
        let text = include_str!("../tests/srp_vectors.txt");
        let mut lines = text.lines();
        let mut value = |name: &str| -> Vec<u8> {
            let line = lines.next().unwrap();
            let (key, hex) = line.split_once('=').unwrap();
            assert_eq!(key, name);
            (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect()
        };
        let salt = value("s");
        let b = value("b");
        let a = value("A");
        let expected_b = value("B");
        let m = value("M");
        let hamk = value("HAMK");
        let k = value("K");
        let server = Server::with_values("AA:BB:CC:DD:EE:FF", "4711", salt.try_into().unwrap(), BigUint::from_bytes_be(&b));
        assert_eq!(server.public_key(), expected_b);
        let verified = server.verify(&a, &m).expect("C-Beweis gültig");
        assert_eq!(verified.proof.to_vec(), hamk);
        assert_eq!(verified.session_key.to_vec(), k);
    }
}
