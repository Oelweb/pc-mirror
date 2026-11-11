// SPDX-License-Identifier: GPL-3.0-or-later
//! Zeitabgleich mit dem Sender (AirPlay-1-NTP auf dem Timing-Port). Der Empfänger
//! fragt regelmäßig die Uhr des Senders ab, wie es iOS erwartet; die Wiedergabe
//! selbst läuft ohne Uhrensynchronisation mit minimaler Latenz.
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SECONDS_1900_TO_1970: u64 = 2_208_988_800;

/// Aktuelle Zeit als NTP-Zeitstempel (Q32.32 seit 1900).
pub fn ntp_now() -> u64 {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = now.as_secs() + SECONDS_1900_TO_1970;
    let fraction = ((now.subsec_nanos() as u64) << 32) / 1_000_000_000;
    (seconds << 32) | fraction
}

fn q32_to_nanos(value: u64) -> i128 {
    ((value >> 32) as i128) * 1_000_000_000 + (((value & 0xffff_ffff) as i128 * 1_000_000_000) >> 32)
}

pub struct Ntp {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    /// Geschätzte Differenz Sender − Empfänger in Nanosekunden (nur für Diagnose).
    pub offset: Arc<AtomicI64>,
    pub local_port: u16,
}

impl Ntp {
    pub fn start(remote: SocketAddr, local_port: u16) -> Result<Self, String> {
        let socket = UdpSocket::bind(("0.0.0.0", local_port))
            .or_else(|_| UdpSocket::bind(("0.0.0.0", 0)))
            .map_err(|e| format!("Timing-Port: {e}"))?;
        let local_port = socket.local_addr().map_err(|e| e.to_string())?.port();
        socket.set_read_timeout(Some(Duration::from_millis(500))).ok();
        let stop = Arc::new(AtomicBool::new(false));
        let offset = Arc::new(AtomicI64::new(0));
        let (stop_thread, offset_thread) = (stop.clone(), offset.clone());
        let thread = std::thread::spawn(move || {
            let mut request = [0u8; 32];
            request[0] = 0x80;
            request[1] = 0xd2;
            request[3] = 0x07;
            let mut best_delay = i128::MAX;
            while !stop_thread.load(Ordering::SeqCst) {
                let t1 = ntp_now();
                request[24..32].copy_from_slice(&t1.to_be_bytes());
                if remote.port() != 0 {
                    let _ = socket.send_to(&request, remote);
                }
                let mut response = [0u8; 128];
                if let Ok((len, _)) = socket.recv_from(&mut response) {
                    if len >= 32 && u64::from_be_bytes(response[8..16].try_into().unwrap()) == t1 {
                        let t4 = ntp_now();
                        let t2 = u64::from_be_bytes(response[16..24].try_into().unwrap());
                        let t3 = u64::from_be_bytes(response[24..32].try_into().unwrap());
                        let (t1n, t2n, t3n, t4n) = (q32_to_nanos(t1), q32_to_nanos(t2), q32_to_nanos(t3), q32_to_nanos(t4));
                        let delay = (t4n - t1n) - (t3n - t2n);
                        if delay >= 0 && delay <= best_delay.saturating_mul(2) {
                            best_delay = best_delay.min(delay);
                            let estimate = ((t2n - t1n) + (t3n - t4n)) / 2;
                            offset_thread.store(estimate.clamp(i64::MIN as i128, i64::MAX as i128) as i64, Ordering::SeqCst);
                        }
                        // Antwort gibt t3 als Referenz zurück (wie die C-Fassung)
                        request[8..16].copy_from_slice(&t3.to_be_bytes());
                    }
                }
                for _ in 0..30 {
                    if stop_thread.load(Ordering::SeqCst) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        });
        Ok(Self { stop, thread: Some(thread), offset, local_port })
    }
}

impl Drop for Ntp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ntp_time_round_trip() {
        let t = ntp_now();
        assert!((t >> 32) > SECONDS_1900_TO_1970);
        let nanos = q32_to_nanos(1u64 << 32 | 0x8000_0000);
        assert_eq!(nanos, 1_500_000_000);
    }
}
