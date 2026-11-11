// SPDX-License-Identifier: GPL-3.0-or-later
//! Tonstrom (RTP über UDP): AES-CBC-entschlüsselt, nach Sequenznummer sortiert,
//! fehlende Pakete werden einmal neu angefordert. Port von `raop_rtp.c`/`raop_buffer.c`.
use crate::crypto::aes_cbc_decrypt_blocks;
use crate::media::Media;
use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Wie viele Pakete höchstens auf ein fehlendes gewartet wird.
const REORDER_WINDOW: u16 = 16;
const MAX_PACKET: usize = 2048;

/// Sortierpuffer nach Sequenznummer (mit 16-Bit-Überlauf).
#[derive(Default)]
pub struct Reorder {
    next: Option<u16>,
    pending: BTreeMap<u32, Vec<u8>>,
    base: u16,
}

fn seq_diff(a: u16, b: u16) -> i16 {
    a.wrapping_sub(b) as i16
}

impl Reorder {
    pub fn flush(&mut self, next: Option<u16>) {
        self.pending.clear();
        self.next = next;
        if let Some(n) = next {
            self.base = n;
        }
    }

    fn key(&self, seq: u16) -> u32 {
        // Abstand zur Basis, damit die Reihenfolge über den Überlauf stimmt
        (seq.wrapping_sub(self.base) as u32) & 0xffff
    }

    /// Fügt ein Paket ein und gibt alle nun lückenlos lieferbaren Pakete zurück.
    /// `missing` erhält (erste fehlende Nummer, Anzahl), wenn eine Lücke entsteht.
    pub fn insert(&mut self, seq: u16, payload: Vec<u8>, missing: &mut Option<(u16, u16)>) -> Vec<Vec<u8>> {
        let next = *self.next.get_or_insert_with(|| {
            self.base = seq;
            seq
        });
        if seq_diff(seq, next) < 0 {
            return Vec::new(); // zu spät oder doppelt
        }
        if seq_diff(seq, next) > REORDER_WINDOW as i16 * 4 {
            // großer Sprung: neu beginnen
            self.flush(Some(seq));
        }
        let key = self.key(seq);
        if seq_diff(seq, self.next.unwrap()) > 0 && self.pending.is_empty() {
            *missing = Some((self.next.unwrap(), seq.wrapping_sub(self.next.unwrap())));
        }
        self.pending.entry(key).or_insert(payload);
        let mut out = Vec::new();
        loop {
            let next = self.next.unwrap();
            let key = self.key(next);
            if let Some(p) = self.pending.remove(&key) {
                out.push(p);
                self.next = Some(next.wrapping_add(1));
                continue;
            }
            // Lücke: nach REORDER_WINDOW Paketen aufgeben und überspringen
            if let Some((&newest, _)) = self.pending.iter().next_back() {
                if newest.wrapping_sub(key) >= REORDER_WINDOW as u32 {
                    self.next = Some(next.wrapping_add(1));
                    continue;
                }
            }
            break;
        }
        // Basis nachführen, damit Schlüssel klein bleiben
        if self.pending.is_empty() {
            self.base = self.next.unwrap();
        }
        out
    }
}

pub struct AudioStream {
    stop: Arc<AtomicBool>,
    threads: Vec<std::thread::JoinHandle<()>>,
    reorder: Arc<Mutex<Reorder>>,
    pub data_port: u16,
    pub control_port: u16,
}

impl AudioStream {
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        remote: IpAddr,
        remote_control_port: u16,
        data_port: u16,
        control_port: u16,
        key: [u8; 16],
        iv: [u8; 16],
        ct: u8,
        media: Arc<Media>,
    ) -> Result<Self, String> {
        let bind = |port: u16| UdpSocket::bind(("0.0.0.0", port)).or_else(|_| UdpSocket::bind(("0.0.0.0", 0)));
        let data = bind(data_port).map_err(|e| format!("Audio-Datenport: {e}"))?;
        let control = bind(control_port).map_err(|e| format!("Audio-Steuerport: {e}"))?;
        let data_port = data.local_addr().map_err(|e| e.to_string())?.port();
        let control_port = control.local_addr().map_err(|e| e.to_string())?.port();
        data.set_read_timeout(Some(Duration::from_millis(250))).ok();
        control.set_read_timeout(Some(Duration::from_millis(250))).ok();
        media.start_audio(ct)?;

        let stop = Arc::new(AtomicBool::new(false));
        let reorder = Arc::new(Mutex::new(Reorder::default()));
        let control = Arc::new(control);
        let resend_seq = Arc::new(AtomicI32::new(0));
        let remote_control = (remote_control_port != 0).then(|| SocketAddr::new(remote, remote_control_port));
        let deliver = {
            let media = media.clone();
            move |frames: Vec<Vec<u8>>| {
                for frame in frames {
                    media.push_audio(&frame);
                }
            }
        };
        let decrypt = move |packet: &[u8]| -> Option<(u16, Vec<u8>)> {
            if packet.len() < 12 {
                return None;
            }
            let seq = u16::from_be_bytes([packet[2], packet[3]]);
            let mut payload = packet[12..].to_vec();
            aes_cbc_decrypt_blocks(&key, &iv, &mut payload);
            Some((seq, payload))
        };

        let mut threads = Vec::new();
        {
            // Datenkanal
            let (stop, reorder, control, resend_seq, deliver) =
                (stop.clone(), reorder.clone(), control.clone(), resend_seq.clone(), deliver.clone());
            threads.push(std::thread::spawn(move || {
                let mut buffer = [0u8; MAX_PACKET];
                let mut last_resend = Instant::now() - Duration::from_secs(1);
                while !stop.load(Ordering::SeqCst) {
                    let Ok((len, _)) = data.recv_from(&mut buffer) else { continue };
                    let packet = &buffer[..len];
                    if len < 12 || packet[1] & 0x7f != 0x60 {
                        continue;
                    }
                    let Some((seq, payload)) = decrypt(packet) else { continue };
                    let mut missing = None;
                    let frames = reorder.lock().unwrap().insert(seq, payload, &mut missing);
                    deliver(frames);
                    if let (Some((first, count)), Some(addr)) = (missing, remote_control) {
                        if count > 0 && count < 64 && last_resend.elapsed() > Duration::from_millis(20) {
                            last_resend = Instant::now();
                            let ours = resend_seq.fetch_add(1, Ordering::SeqCst) as u16;
                            let mut request = [0x80, 0x55 | 0x80, 0, 0, 0, 0, 0, 0];
                            request[2..4].copy_from_slice(&ours.to_be_bytes());
                            request[4..6].copy_from_slice(&first.to_be_bytes());
                            request[6..8].copy_from_slice(&count.to_be_bytes());
                            let _ = control.send_to(&request, addr);
                        }
                    }
                }
            }));
        }
        {
            // Steuerkanal: Synchronisation (0x54) und neu gesendete Pakete (0x56)
            let (stop, reorder, control) = (stop.clone(), reorder.clone(), control.clone());
            threads.push(std::thread::spawn(move || {
                let mut buffer = [0u8; MAX_PACKET];
                while !stop.load(Ordering::SeqCst) {
                    let Ok((len, _)) = control.recv_from(&mut buffer) else { continue };
                    let packet = &buffer[..len];
                    if len >= 16 && packet[1] & 0x7f == 0x56 {
                        let inner = &packet[4..];
                        if inner[1] & 0x7f != 0x60 {
                            continue;
                        }
                        let Some((seq, payload)) = decrypt(inner) else { continue };
                        let mut missing = None;
                        let frames = reorder.lock().unwrap().insert(seq, payload, &mut missing);
                        deliver(frames);
                    }
                }
            }));
        }
        Ok(Self { stop, threads, reorder, data_port, control_port })
    }

    /// FLUSH mit optionaler nächster Sequenznummer.
    pub fn flush(&self, next: Option<u16>) {
        self.reorder.lock().unwrap().flush(next);
    }
}

impl Drop for AudioStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reorders_and_skips_gaps() {
        let mut r = Reorder::default();
        let mut missing = None;
        assert_eq!(r.insert(10, vec![10], &mut missing).len(), 1);
        assert_eq!(r.insert(12, vec![12], &mut missing).len(), 0);
        assert_eq!(missing, Some((11, 1)));
        let out = r.insert(11, vec![11], &mut None);
        assert_eq!(out, vec![vec![11], vec![12]]);
        // Überlauf der Sequenznummer
        let mut r = Reorder::default();
        r.insert(65534, vec![1], &mut None);
        r.insert(65535, vec![2], &mut None);
        assert_eq!(r.insert(0, vec![3], &mut None), vec![vec![3]]);
        // Lücke, die nicht gefüllt wird, wird nach dem Fenster übersprungen
        let mut r = Reorder::default();
        r.insert(100, vec![0], &mut None);
        let mut delivered = 0;
        for s in 102..(102 + REORDER_WINDOW + 1) {
            delivered += r.insert(s, vec![0], &mut None).len();
        }
        assert!(delivered > 0, "Lücke bei 101 muss übersprungen werden");
        // Doppelte/alte Pakete werden ignoriert
        assert!(r.insert(100, vec![0], &mut None).is_empty());
    }
}
