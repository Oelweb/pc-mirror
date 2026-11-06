// SPDX-License-Identifier: GPL-3.0-or-later
//! Rotierende Logdatei (5 MiB) plus Ringpuffer für die Diagnoseansicht.
//! Auch Meldungen von Velopack laufen über die `log`-Fassade hierher.
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

const MAX_SIZE: u64 = 5 * 1024 * 1024;
const RING: usize = 500;

struct Logger {
    file: Mutex<Option<File>>,
    ring: Mutex<VecDeque<String>>,
}

static LOGGER: Logger = Logger {
    file: Mutex::new(None),
    ring: Mutex::new(VecDeque::new()),
};
static REVISION: AtomicU64 = AtomicU64::new(0);

pub fn path() -> PathBuf {
    crate::settings::data_dir()
        .join("logs")
        .join("pc-mirror.log")
}

/// Öffnet die Logdatei. Fehler bedeuten: Ordner nicht beschreibbar.
pub fn init() -> Result<(), String> {
    let file_path = path();
    let folder = file_path.parent().unwrap();
    fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    if fs::metadata(&file_path)
        .map(|m| m.len() > MAX_SIZE)
        .unwrap_or(false)
    {
        let previous = file_path.with_extension("log.previous");
        let _ = fs::remove_file(&previous);
        let _ = fs::rename(&file_path, &previous);
    }
    // Vorhandenes Ende der Datei in die Diagnoseansicht übernehmen.
    if let Ok(text) = fs::read_to_string(&file_path) {
        let mut ring = LOGGER.ring.lock().unwrap();
        for line in text
            .lines()
            .rev()
            .take(RING)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            ring.push_back(line.to_owned());
        }
    }
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)
        .map_err(|e| e.to_string())?;
    *LOGGER.file.lock().unwrap() = Some(file);
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);
    Ok(())
}

/// Schreibt eine Zeile mit Zeitstempel.
pub fn line(text: &str) {
    let stamped = format!("{}  {}", timestamp(), text.trim_end());
    if let Some(file) = LOGGER.file.lock().unwrap().as_mut() {
        let _ = writeln!(file, "{stamped}");
        let _ = file.flush();
    }
    let mut ring = LOGGER.ring.lock().unwrap();
    for part in stamped.lines() {
        if ring.len() == RING {
            ring.pop_front();
        }
        ring.push_back(part.to_owned());
    }
    REVISION.fetch_add(1, Ordering::Relaxed);
}

pub fn revision() -> u64 {
    REVISION.load(Ordering::Relaxed)
}

/// Neueste Zeilen zuerst.
pub fn recent_newest_first() -> String {
    let ring = LOGGER.ring.lock().unwrap();
    ring.iter().rev().cloned().collect::<Vec<_>>().join("\n")
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            line(&format!(
                "{} [{}] {}",
                record.level(),
                record.target(),
                record.args()
            ));
        }
    }
    fn flush(&self) {}
}

/// Lokale Zeit im ISO-Format, ohne zusätzliche Abhängigkeit.
fn timestamp() -> String {
    use windows::Win32::Foundation::SYSTEMTIME;
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let time: SYSTEMTIME = unsafe { GetLocalTime() };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}",
        time.wYear,
        time.wMonth,
        time.wDay,
        time.wHour,
        time.wMinute,
        time.wSecond,
        time.wMilliseconds
    )
}
