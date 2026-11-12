// SPDX-License-Identifier: GPL-3.0-or-later
//! Videostrom der Bildschirmsynchronisierung (TCP): 128-Byte-Kopf + Nutzdaten,
//! AES-CTR-verschlüsselte H.264/H.265-Einheiten. Port von `raop_rtp_mirror.c`
//! und `mirror_buffer.c`, mit Längenprüfungen für alle Werte aus dem Netz.
use crate::control;
use crate::crypto::{sha512, CtrStream};
use crate::media::Media;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const MAX_PAYLOAD: usize = 16 * 1024 * 1024;
const NAL_START: [u8; 4] = [0, 0, 0, 1];

/// Schlüssel und IV des Videostroms aus dem Audio-Schlüssel und der Stream-ID.
pub fn stream_keys(aes_key: &[u8; 16], stream_connection_id: u64) -> ([u8; 16], [u8; 16]) {
    let key = sha512(&[format!("AirPlayStreamKey{stream_connection_id}").as_bytes(), aes_key]);
    let iv = sha512(&[format!("AirPlayStreamIV{stream_connection_id}").as_bytes(), aes_key]);
    (key[..16].try_into().unwrap(), iv[..16].try_into().unwrap())
}

/// Wandelt längenpräfixierte NAL-Einheiten (AVCC) in Annex-B mit Startcodes um.
/// Gibt die Anzahl der Einheiten zurück oder `None` bei ungültigen Daten.
pub fn avcc_to_annexb(data: &mut [u8]) -> Option<usize> {
    let mut pos = 0;
    let mut count = 0;
    while pos < data.len() {
        let header = data.get(pos..pos + 4)?;
        let len = u32::from_be_bytes(header.try_into().ok()?) as usize;
        if len == 0 || len > data.len() - pos - 4 {
            return None;
        }
        data[pos..pos + 4].copy_from_slice(&NAL_START);
        if data[pos + 4] & 0x80 != 0 {
            return None; // forbidden_zero_bit
        }
        pos += 4 + len;
        count += 1;
    }
    Some(count)
}

/// Parameter-Sätze aus einem unverschlüsselten Typ-1-Paket als Annex-B.
/// Gibt (ist H.265, Daten) zurück.
pub fn parse_parameter_sets(payload: &[u8]) -> Option<(bool, Vec<u8>)> {
    let be16 = |data: &[u8], at: usize| -> Option<usize> {
        Some(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?) as usize)
    };
    if payload.get(4..8) == Some(b"hvc1".as_slice()) {
        // H.265: VPS/SPS/PPS ab Offset 0x75, je mit Kennung a0/a1/a2 00 01 + Länge
        let mut out = Vec::new();
        let mut at = 0x75;
        for marker in [0xa0u8, 0xa1, 0xa2] {
            if payload.get(at..at + 3)? != [marker, 0x00, 0x01] {
                return None;
            }
            let size = be16(payload, at + 3)?;
            let unit = payload.get(at + 5..at + 5 + size)?;
            out.extend_from_slice(&NAL_START);
            out.extend_from_slice(unit);
            at += 5 + size;
        }
        Some((true, out))
    } else {
        let sps_size = be16(payload, 6)?;
        let sps = payload.get(8..8 + sps_size)?;
        let pps_size = be16(payload, sps_size + 9)?;
        let pps = payload.get(sps_size + 11..sps_size + 11 + pps_size)?;
        let mut out = Vec::with_capacity(sps_size + pps_size + 8);
        out.extend_from_slice(&NAL_START);
        out.extend_from_slice(sps);
        out.extend_from_slice(&NAL_START);
        out.extend_from_slice(pps);
        Some((false, out))
    }
}

pub struct MirrorStream {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl MirrorStream {
    /// Öffnet den Video-Port und verarbeitet die eine Verbindung des Senders.
    pub fn start(port: u16, remote: std::net::IpAddr, aes_key: [u8; 16], stream_connection_id: u64, media: Arc<Media>, on_end: impl FnOnce() + Send + 'static) -> Result<(Self, u16), String> {
        let listener = TcpListener::bind(("0.0.0.0", port))
            .or_else(|_| TcpListener::bind(("0.0.0.0", 0)))
            .map_err(|e| format!("Video-Port: {e}"))?;
        let actual = listener.local_addr().map_err(|e| e.to_string())?.port();
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let (key, iv) = stream_keys(&aes_key, stream_connection_id);
        let thread = std::thread::spawn(move || {
            let mut cipher = CtrStream::new(&key, &iv);
            let mut session = Session::default();
            while !stop_thread.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, peer)) => {
                        // Nur das verbundene Gerät darf Video senden.
                        if peer.ip() != remote {
                            control::log(&format!("Videoport: Verbindung von fremder Adresse {} abgewiesen", peer.ip()));
                            continue;
                        }
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
                        let _ = stream.set_nodelay(true);
                        let ended = run_stream(stream, &mut cipher, &mut session, &media, &stop_thread);
                        if ended == StreamEnd::Fatal {
                            break;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => break,
                }
            }
            on_end();
        });
        Ok((Self { stop, thread: Some(thread) }, actual))
    }
}

impl Drop for MirrorStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Default)]
struct Session {
    codec: Option<bool>,
    parameter_sets: Option<(u64, Vec<u8>)>,
    suspended: bool,
}

#[derive(PartialEq, Eq)]
enum StreamEnd {
    Closed,
    Fatal,
}

/// Liest genau `buffer.len()` Byte; Zeitüberschreitungen werden wiederholt,
/// solange nicht gestoppt wird.
fn read_full(stream: &mut TcpStream, buffer: &mut [u8], stop: &AtomicBool) -> std::io::Result<bool> {
    let mut filled = 0;
    while filled < buffer.len() {
        if stop.load(Ordering::SeqCst) {
            return Ok(false);
        }
        match stream.read(&mut buffer[filled..]) {
            Ok(0) => return Ok(false),
            Ok(n) => filled += n,
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(true)
}

fn run_stream(mut stream: TcpStream, cipher: &mut CtrStream, session: &mut Session, media: &Media, stop: &AtomicBool) -> StreamEnd {
    let mut header = [0u8; 128];
    loop {
        match read_full(&mut stream, &mut header, stop) {
            Ok(true) => {}
            Ok(false) => return StreamEnd::Closed,
            Err(_) => return StreamEnd::Closed,
        }
        let payload_size = u32::from_le_bytes(header[0..4].try_into().unwrap()) as usize;
        if payload_size > MAX_PAYLOAD {
            control::log(&format!("Videostrom: Paket zu groß ({payload_size} Byte), Verbindung beendet"));
            return StreamEnd::Fatal;
        }
        let timestamp = u64::from_le_bytes(header[8..16].try_into().unwrap());
        let mut payload = vec![0u8; payload_size];
        match read_full(&mut stream, &mut payload, stop) {
            Ok(true) => {}
            _ => return StreamEnd::Closed,
        }
        match header[4] {
            0x00 => {
                // Verschlüsselte Bilddaten: Schlüsselstrom läuft über alle Pakete weiter.
                cipher.apply(&mut payload);
                let Some(_) = avcc_to_annexb(&mut payload) else {
                    control::log("Videostrom: ungültige Bilddaten verworfen");
                    session.parameter_sets = None;
                    continue;
                };
                let mut frame = Vec::with_capacity(payload.len() + 256);
                if let Some((ts, sets)) = session.parameter_sets.take() {
                    if ts == timestamp {
                        frame.extend_from_slice(&sets);
                    }
                }
                frame.extend_from_slice(&payload);
                media.push_video(&frame, crate::media::ntp_to_ns(timestamp));
            }
            0x01 => {
                let option = header[6];
                if matches!(option, 0x56 | 0x5e) && !session.suspended {
                    session.suspended = true;
                    media.video_paused(true);
                    control::log("Videostrom pausiert (Gerät gesperrt oder App im Hintergrund)");
                } else if matches!(option, 0x16 | 0x1e) && session.suspended {
                    session.suspended = false;
                    media.video_paused(false);
                }
                if payload.is_empty() {
                    control::log("Videostrom: Codec nicht unterstützt (H.265 nicht freigegeben)");
                    return StreamEnd::Fatal;
                }
                let Some((h265, sets)) = parse_parameter_sets(&payload) else {
                    control::log("Videostrom: ungültige Parameter-Sätze");
                    continue;
                };
                match session.codec {
                    None => {
                        let width = f32::from_le_bytes(header[56..60].try_into().unwrap());
                        let height = f32::from_le_bytes(header[60..64].try_into().unwrap());
                        control::log(&format!(
                            "Videostrom: {} {}×{}",
                            if h265 { "H.265" } else { "H.264" },
                            width as u32,
                            height as u32
                        ));
                        if let Err(error) = media.set_video_codec(h265) {
                            control::log(&format!("Videoausgabe konnte nicht starten: {error}"));
                            return StreamEnd::Fatal;
                        }
                        session.codec = Some(h265);
                    }
                    Some(current) if current != h265 => {
                        control::log("Videostrom: unzulässiger Codecwechsel");
                        return StreamEnd::Fatal;
                    }
                    _ => {}
                }
                session.parameter_sets = Some((timestamp, sets));
            }
            0x02 | 0x05 => {} // Lebenszeichen und Statistik
            other => control::log(&format!("Videostrom: unbekannter Pakettyp {other:#04x}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avcc_conversion_and_bounds() {
        let mut data = vec![0, 0, 0, 2, 0x65, 0x88, 0, 0, 0, 1, 0x41];
        assert_eq!(avcc_to_annexb(&mut data), Some(2));
        assert_eq!(data, vec![0, 0, 0, 1, 0x65, 0x88, 0, 0, 0, 1, 0x41]);
        let mut bad = vec![0, 0, 0, 9, 0x65];
        assert_eq!(avcc_to_annexb(&mut bad), None);
        let mut forbidden = vec![0, 0, 0, 1, 0x80];
        assert_eq!(avcc_to_annexb(&mut forbidden), None);
        let mut empty_unit = vec![0, 0, 0, 0];
        assert_eq!(avcc_to_annexb(&mut empty_unit), None);
    }

    #[test]
    fn parses_h264_parameter_sets() {
        // avcC-ähnlich: 6 Byte Kopf, SPS-Länge, SPS, 1 Byte Anzahl, PPS-Länge, PPS
        let mut payload = vec![1, 0x64, 0, 0x28, 0xff, 0xe1, 0, 3, 0x67, 1, 2, 1, 0, 2, 0x68, 9];
        let (h265, sets) = parse_parameter_sets(&payload).unwrap();
        assert!(!h265);
        assert_eq!(sets, vec![0, 0, 0, 1, 0x67, 1, 2, 0, 0, 0, 1, 0x68, 9]);
        payload.truncate(10);
        assert!(parse_parameter_sets(&payload).is_none(), "abgeschnittene Daten");
    }

    #[test]
    fn stream_keys_follow_formula() {
        let (key, iv) = stream_keys(&[1u8; 16], 42);
        assert_eq!(key.to_vec(), sha512(&[b"AirPlayStreamKey42", &[1u8; 16]])[..16].to_vec());
        assert_eq!(iv.to_vec(), sha512(&[b"AirPlayStreamIV42", &[1u8; 16]])[..16].to_vec());
    }
}
