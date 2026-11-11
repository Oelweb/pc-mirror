// SPDX-License-Identifier: GPL-3.0-or-later
//! Steuerkanal zur App: Befehle über stdin, Ereignisse als `PCMIRROR|…` über stdout.
use std::io::{BufRead, Read, Write};
use std::path::Path;
use std::sync::mpsc::Sender;

pub enum Command {
    Stop,
    Mute(bool),
    IpadFill(bool),
    Snapshot(String),
}

/// Gibt ein Ereignis an die App aus (eine Zeile, sofort geleert).
pub fn emit(event: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "PCMIRROR|{event}");
    let _ = out.flush();
}

/// Protokollzeile für das App-Log.
pub fn log(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// Liest Befehle aus stdin. Ende der Pipe (App beendet) gilt als STOP.
pub fn spawn_reader(sender: Sender<Command>) {
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut reader = stdin.lock();
        let mut line = Vec::new();
        loop {
            line.clear();
            // Zeilen sind höchstens 32 KiB lang, sonst wird verworfen.
            match reader.by_ref().take(32 * 1024).read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => {
                    let _ = sender.send(Command::Stop);
                    return;
                }
                Ok(_) => {}
            }
            if !line.ends_with(b"\n") && line.len() >= 32 * 1024 {
                eprintln!("PC-Mirror rejected an oversized command");
                // Rest der überlangen Zeile verwerfen
                let mut rest = Vec::new();
                let _ = reader.read_until(b'\n', &mut rest);
                continue;
            }
            let text = String::from_utf8_lossy(&line);
            let text = text.trim_end_matches(['\r', '\n']);
            let command = match text {
                "STOP" => Command::Stop,
                "MUTE 1" => Command::Mute(true),
                "MUTE 0" => Command::Mute(false),
                "IPAD_FILL 1" => Command::IpadFill(true),
                "IPAD_FILL 0" => Command::IpadFill(false),
                other => match other.strip_prefix("SNAPSHOT ") {
                    Some(path) if !path.is_empty() => Command::Snapshot(path.to_owned()),
                    _ => {
                        eprintln!("PC-Mirror received an unknown control command");
                        continue;
                    }
                },
            };
            if sender.send(command).is_err() {
                return;
            }
        }
    });
}

/// Speichert das letzte Videobild als PPM (P6).
pub fn write_snapshot(path: &str) -> bool {
    let Some((width, height, stride, pixels)) = crate::gst::snapshot_rgb() else {
        return false;
    };
    let mut data = format!("P6\n{width} {height}\n255\n").into_bytes();
    data.reserve(width * height * 3);
    for row in 0..height {
        data.extend_from_slice(&pixels[row * stride..row * stride + width * 3]);
    }
    std::fs::write(Path::new(path), data).is_ok()
}
