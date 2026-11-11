// SPDX-License-Identifier: GPL-3.0-or-later
//! FairPlay-Setup für AirPlay (fp-setup). Rust-Port von UxPlays `fairplay_playfair.c`.
//! Die Unterschiede zum Original sind Sicherheitsprüfungen: Modus-Bytes vom Client
//! werden vor dem Tabellenzugriff geprüft.

mod playfair;
mod tables;

static REPLY_MESSAGE: &[u8; 4 * 142] = include_bytes!("tables/reply_message.bin");
const FP_HEADER: [u8; 12] = [0x46, 0x50, 0x4c, 0x59, 0x03, 0x01, 0x04, 0x00, 0x00, 0x00, 0x00, 0x14];

#[derive(Default)]
pub struct FairPlay {
    keymsg: Option<Box<[u8; 164]>>,
}

impl FairPlay {
    /// Erste Phase (16 Byte Anfrage → 142 Byte Antwort).
    pub fn setup(&mut self, request: &[u8]) -> Option<Vec<u8>> {
        if request.len() != 16 || request[4] != 0x03 {
            return None;
        }
        let mode = request[14] as usize;
        if mode > 3 {
            return None;
        }
        self.keymsg = None;
        Some(REPLY_MESSAGE[mode * 142..mode * 142 + 142].to_vec())
    }

    /// Zweite Phase (164 Byte Anfrage → 32 Byte Antwort).
    pub fn handshake(&mut self, request: &[u8]) -> Option<Vec<u8>> {
        if request.len() != 164 || request[4] != 0x03 {
            return None;
        }
        let keymsg: [u8; 164] = request.try_into().ok()?;
        self.keymsg = Some(Box::new(keymsg));
        let mut response = Vec::with_capacity(32);
        response.extend_from_slice(&FP_HEADER);
        response.extend_from_slice(&request[144..164]);
        Some(response)
    }

    /// Entschlüsselt den AES-Schlüssel aus `ekey` (72 Byte).
    pub fn decrypt(&self, ekey: &[u8]) -> Option<[u8; 16]> {
        let keymsg = self.keymsg.as_ref()?;
        let ekey: &[u8; 72] = ekey.get(..72)?.try_into().ok()?;
        playfair::decrypt(keymsg, ekey)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vergleichswerte aus der C-Fassung (UxPlay `playfair_decrypt`), erzeugt mit
    /// `tests/fairplay_vectors.c`: Eingaben aus einem festen Zufallsgenerator.
    #[test]
    fn matches_c_reference_vectors() {
        let text = include_str!("../../tests/fairplay_vectors.txt");
        let mut checked = 0;
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            let keymsg = hex(parts[0]);
            let ekey = hex(parts[1]);
            let expected = hex(parts[2]);
            let mut fp = FairPlay::default();
            let mut setup = [0u8; 16];
            setup[4] = 3;
            setup[14] = keymsg[12];
            assert!(fp.setup(&setup).is_some());
            assert_eq!(fp.handshake(&keymsg).unwrap()[12..], keymsg[144..164]);
            assert_eq!(fp.decrypt(&ekey).unwrap().to_vec(), expected, "Fall {checked}");
            checked += 1;
        }
        assert!(checked >= 64, "zu wenige Vergleichswerte: {checked}");
    }

    #[test]
    fn rejects_invalid_modes() {
        let mut fp = FairPlay::default();
        let mut setup = [0u8; 16];
        setup[4] = 3;
        setup[14] = 9;
        assert!(fp.setup(&setup).is_none());
        let mut keymsg = [0u8; 164];
        keymsg[4] = 3;
        keymsg[12] = 7;
        fp.handshake(&keymsg).unwrap();
        assert!(fp.decrypt(&[0u8; 72]).is_none());
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }
}
