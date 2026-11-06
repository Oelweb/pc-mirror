// SPDX-License-Identifier: GPL-3.0-or-later
//! Steuerung des AirPlay-Empfängers `uxplay.exe` als eigener Prozess.
//!
//! Befehle laufen über die geerbte stdin-Pipe (STOP, MUTE, SNAPSHOT, IPAD_FILL),
//! Statusereignisse kommen als `PCMIRROR|…`-Zeilen über stdout zurück.
//! Ein Absturz im Codec reißt so die Oberfläche nicht mit.
use crate::events::{self, Event};
use crate::settings::{self, Settings};
use crate::win::{Job, RawHwnd};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(25);
const STOP_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_LINE: usize = 64 * 1024;

/// Rohereignisse aus den Lese-Threads.
#[derive(Debug, Clone, PartialEq)]
pub enum ReceiverEvent {
    Ready,
    Frame(u32, u32),
    Idle,
    Pin(String),
    Muted(bool),
    Snapshot(bool),
    IpadFill(bool),
    Line(String),
    Exited { code: Option<i32> },
}

/// Aufbereitete Meldungen für die Oberfläche.
#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    Ready,
    Frame(u32, u32),
    Idle,
    Pin(String),
    Snapshot(bool),
    Log(String),
    Error {
        message: &'static str,
        details: String,
    },
    Stopped,
}

pub fn executable() -> PathBuf {
    exe_dir().join("uxplay.exe")
}

/// Paketwurzel: flaches Velopack-Paket oder alte `bin/`-Struktur.
pub fn package_root() -> PathBuf {
    let dir = exe_dir();
    if dir.join("lib/gstreamer-1.0").is_dir() {
        dir
    } else {
        dir.parent().map(PathBuf::from).unwrap_or(dir)
    }
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_default()
}

#[derive(Default)]
pub struct Receiver {
    generation: u64,
    stdin: Option<ChildStdin>,
    job: Option<Job>,
    exited: Option<Arc<AtomicBool>>,
    running: bool,
    ready: bool,
    expected_stop: bool,
    desired_mute: bool,
    started: Option<Instant>,
    stop_requested: Option<Instant>,
    details: String,
    reported_problem: bool,
}

impl Receiver {
    pub fn start(&mut self, settings: &Settings, host: RawHwnd, test_pattern: bool) -> Vec<Notice> {
        if self.running {
            return Vec::new();
        }
        self.generation += 1;
        self.ready = false;
        self.expected_stop = false;
        self.desired_mute = settings.muted;
        self.details.clear();
        self.reported_problem = false;
        self.stop_requested = None;

        let exe = executable();
        if !exe.is_file() {
            return vec![Notice::Error {
                message: "uxplay.exe fehlt. Bitte die App vollständig neu installieren.",
                details: exe.display().to_string(),
            }];
        }
        let data = settings::data_dir();
        let config = data.join("receiver.conf");
        if let Err(error) = std::fs::create_dir_all(&data)
            .and_then(|_| std::fs::write(&config, "# PC-Mirror manages receiver options.\n"))
        {
            return vec![Notice::Error {
                message: "Die Empfänger-Konfiguration konnte nicht erstellt werden.",
                details: error.to_string(),
            }];
        }

        let root = package_root();
        let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let path = format!(
            "{};{system_root}\\System32;{system_root}",
            exe_dir().display()
        );
        let arguments = if test_pattern {
            vec!["--pcmirror-selftest".into()]
        } else {
            settings.receiver_arguments(&config)
        };
        let mut command = Command::new(&exe);
        command
            .args(arguments)
            .env("PATH", path)
            .env("GST_PLUGIN_SYSTEM_PATH_1_0", root.join("lib/gstreamer-1.0"))
            .env("GST_PLUGIN_PATH_1_0", "")
            .env(
                "GST_PLUGIN_SCANNER_1_0",
                root.join("libexec/gstreamer-1.0/gst-plugin-scanner.exe"),
            )
            .env("GST_REGISTRY_1_0", data.join("gstreamer-registry.bin"))
            .env("PCMIRROR_EMBEDDED", "1")
            .env("PCMIRROR_HWND", host.to_string())
            .env(
                "PCMIRROR_IPAD_FILL",
                if settings.ipad_fill { "1" } else { "0" },
            )
            .current_dir(&data)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW);

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                return vec![Notice::Error {
                    message: "Der AirPlay-Empfänger konnte nicht gestartet werden. Bitte die App vollständig neu installieren.",
                    details: error.to_string(),
                }]
            }
        };
        let stdin = child.stdin.take();
        let mut notices = Vec::new();
        self.job = Job::new();
        match &self.job {
            Some(job) => {
                if let Err(error) = job.assign(child.id()) {
                    notices.push(Notice::Log(format!(
                        "WARN: Empfänger konnte nicht dem Prozess-Job zugeordnet werden: {error}"
                    )));
                }
            }
            None => notices.push(Notice::Log(
                "WARN: Prozessüberwachung konnte nicht initialisiert werden.".into(),
            )),
        }

        let generation = self.generation;
        let readers: Vec<_> = [
            child
                .stdout
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
            child
                .stderr
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
        ]
        .into_iter()
        .flatten()
        .map(|stream| std::thread::spawn(move || read_lines(stream, generation)))
        .collect();
        let exited = Arc::new(AtomicBool::new(false));
        let exit_flag = exited.clone();
        std::thread::spawn(move || {
            for reader in readers {
                let _ = reader.join();
            }
            let code = child.wait().ok().and_then(|status| status.code());
            exit_flag.store(true, Ordering::SeqCst);
            events::post(Event::Receiver {
                generation,
                event: ReceiverEvent::Exited { code },
            });
        });

        self.stdin = stdin;
        self.exited = Some(exited);
        self.running = true;
        self.started = Some(Instant::now());
        notices.push(Notice::Log(if test_pattern {
            "Synthetischer Bildtest gestartet; kein AirPlay-Test.".into()
        } else {
            format!("AirPlay-Empfänger startet: {}", settings.name)
        }));
        notices
    }

    /// Verarbeitet ein Ereignis der aktuellen Prozessgeneration.
    pub fn handle(&mut self, generation: u64, event: ReceiverEvent) -> Vec<Notice> {
        if generation != self.generation {
            return Vec::new();
        }
        match event {
            ReceiverEvent::Ready => {
                self.ready = true;
                self.started = None;
                let mut notices = self.mute(self.desired_mute);
                notices.insert(0, Notice::Ready);
                notices
            }
            ReceiverEvent::Frame(w, h) => vec![Notice::Frame(w, h)],
            ReceiverEvent::Idle => vec![Notice::Idle],
            ReceiverEvent::Pin(pin) => vec![Notice::Pin(pin)],
            ReceiverEvent::Muted(muted) => vec![Notice::Log(format!(
                "Ton {}",
                if muted { "aus" } else { "an" }
            ))],
            ReceiverEvent::Snapshot(ok) => vec![Notice::Snapshot(ok)],
            ReceiverEvent::IpadFill(fill) => vec![Notice::Log(format!(
                "iPad-Darstellung: {}",
                if fill {
                    "Fenster ausfüllen"
                } else {
                    "Ganzes Bild"
                }
            ))],
            ReceiverEvent::Line(line) => {
                self.details.push('\n');
                self.details.push_str(&line);
                if self.details.len() > 12_000 {
                    let cut = self.details.len() - 12_000;
                    let cut = (cut..self.details.len())
                        .find(|&i| self.details.is_char_boundary(i))
                        .unwrap_or(0);
                    self.details.drain(..cut);
                }
                let mut notices = vec![Notice::Log(line.clone())];
                if !self.reported_problem && is_problem(&line, self.ready) {
                    self.reported_problem = true;
                    notices.push(Notice::Error {
                        message: "Der Empfänger meldet einen schweren Fehler. Details und Hilfen stehen in der Diagnose.",
                        details: line,
                    });
                }
                notices
            }
            ReceiverEvent::Exited { code } => {
                let mut notices = Vec::new();
                if !self.expected_stop {
                    notices.push(Notice::Error {
                        message: "Der Empfänger wurde unerwartet beendet. Bitte Diagnose prüfen oder neu starten.",
                        details: format!("Exit {}\n{}", code.map_or("Absturz".into(), |c| c.to_string()), self.details),
                    });
                }
                self.running = false;
                self.ready = false;
                self.stdin = None;
                self.job = None;
                self.exited = None;
                self.started = None;
                self.stop_requested = None;
                notices.push(Notice::Stopped);
                notices
            }
        }
    }

    /// Zeitüberschreitungen prüfen; regelmäßig aus dem UI-Timer aufrufen.
    pub fn tick(&mut self) -> Vec<Notice> {
        if let Some(started) = self.started {
            if started.elapsed() > STARTUP_TIMEOUT && !self.ready {
                self.started = None;
                let mut notices = vec![Notice::Error {
                    message: "Der Empfänger startet nicht rechtzeitig. Bitte Diagnose öffnen und erneut starten.",
                    details: self.details.clone(),
                }];
                notices.extend(self.stop());
                return notices;
            }
        }
        if let Some(requested) = self.stop_requested {
            if requested.elapsed() > STOP_TIMEOUT && self.running {
                self.stop_requested = None;
                if let Some(job) = &self.job {
                    job.terminate();
                }
                return vec![Notice::Log(
                    "Empfänger reagierte nicht auf STOP und wurde beendet.".into(),
                )];
            }
        }
        Vec::new()
    }

    pub fn stop(&mut self) -> Vec<Notice> {
        self.expected_stop = true;
        self.started = None;
        if !self.running {
            return vec![Notice::Stopped];
        }
        if self.stop_requested.is_none() {
            self.stop_requested = Some(Instant::now());
        }
        self.command("STOP")
    }

    pub fn mute(&mut self, muted: bool) -> Vec<Notice> {
        self.desired_mute = muted;
        self.command(if muted { "MUTE 1" } else { "MUTE 0" })
    }

    pub fn snapshot(&mut self, path: &std::path::Path) -> Vec<Notice> {
        self.command(&format!("SNAPSHOT {}", path.display()))
    }

    pub fn set_ipad_fill(&mut self, fill: bool) -> Vec<Notice> {
        self.command(if fill { "IPAD_FILL 1" } else { "IPAD_FILL 0" })
    }

    fn command(&mut self, line: &str) -> Vec<Notice> {
        let Some(stdin) = self.stdin.as_mut() else {
            return Vec::new();
        };
        if line.contains(['\n', '\r']) {
            return Vec::new();
        }
        let result = stdin
            .write_all(format!("{line}\n").as_bytes())
            .and_then(|_| stdin.flush());
        match result {
            Ok(()) => Vec::new(),
            Err(error) if !self.expected_stop => vec![Notice::Error {
                message: "Der Empfänger reagiert nicht auf die Steuerung. Bitte neu starten.",
                details: error.to_string(),
            }],
            Err(_) => Vec::new(),
        }
    }

    /// Beim Beenden der App: kurz auf sauberes Ende warten, dann hart beenden.
    pub fn shutdown(&mut self) {
        self.expected_stop = true;
        if !self.running {
            return;
        }
        let _ = self.command("STOP");
        self.stdin = None; // schließt die Pipe: UxPlay endet auch dadurch
        let deadline = Instant::now() + Duration::from_millis(1200);
        while Instant::now() < deadline {
            if self
                .exited
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::SeqCst))
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if let Some(job) = self.job.take() {
            job.terminate();
        }
        self.running = false;
    }
}

/// Nur eindeutig schwere Fehler zeigen den roten Hinweis.
/// (Behebt: früher löste jede Logzeile mit „error“/„WARNING“ einen Fehlalarm aus.)
pub fn is_problem(line: &str, ready: bool) -> bool {
    const FATAL: [&str; 10] = [
        "dnssd_register",
        "could not initialize dnssd",
        "error initializing raop",
        "raop_set failed",
        "could not create native video child",
        "synthetic pipeline failed",
        "address already in use",
        "failed to bind",
        "unable to bind",
        "could not set control handler",
    ];
    let lower = line.to_lowercase();
    if FATAL.iter().any(|pattern| lower.contains(pattern)) {
        return true;
    }
    // Vor der Bereitschaft verhindert ein Fehler den Start ohnehin.
    !ready && (lower.contains("error") || lower.contains("failed")) && !lower.contains("warn")
}

pub fn parse_line(line: &str) -> ReceiverEvent {
    let Some(start) = line.find("PCMIRROR|") else {
        return ReceiverEvent::Line(line.to_owned());
    };
    let parts: Vec<&str> = line[start..].split('|').collect();
    let number = |i: usize| {
        parts
            .get(i)
            .and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or(0)
    };
    let flag = |i: usize| parts.get(i).is_some_and(|v| v.trim() == "1");
    match parts.get(1).map(|s| s.trim()) {
        Some("READY") => ReceiverEvent::Ready,
        Some("FRAME") => ReceiverEvent::Frame(number(2), number(3)),
        Some("IDLE") => ReceiverEvent::Idle,
        Some("PIN") => ReceiverEvent::Pin(parts.get(2).unwrap_or(&"").trim().to_owned()),
        Some("MUTED") => ReceiverEvent::Muted(flag(2)),
        Some("SNAPSHOT") => ReceiverEvent::Snapshot(parts.get(2).is_some_and(|v| v.trim() == "OK")),
        Some("IPAD_FILL") => ReceiverEvent::IpadFill(flag(2)),
        _ => ReceiverEvent::Line(line.to_owned()),
    }
}

fn read_lines(stream: Box<dyn Read + Send>, generation: u64) {
    let mut reader = BufReader::new(stream);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match reader
            .by_ref()
            .take(MAX_LINE as u64)
            .read_until(b'\n', &mut buffer)
        {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buffer).trim().to_owned();
                if !line.is_empty() {
                    events::post(Event::Receiver {
                        generation,
                        event: parse_line(&line),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_protocol_lines() {
        assert_eq!(parse_line("PCMIRROR|READY"), ReceiverEvent::Ready);
        assert_eq!(
            parse_line("xx PCMIRROR|FRAME|1920|1080"),
            ReceiverEvent::Frame(1920, 1080)
        );
        assert_eq!(
            parse_line("PCMIRROR|PIN|4711"),
            ReceiverEvent::Pin("4711".into())
        );
        assert_eq!(parse_line("PCMIRROR|MUTED|1"), ReceiverEvent::Muted(true));
        assert_eq!(
            parse_line("PCMIRROR|SNAPSHOT|ERROR"),
            ReceiverEvent::Snapshot(false)
        );
        assert_eq!(
            parse_line("PCMIRROR|IPAD_FILL|0"),
            ReceiverEvent::IpadFill(false)
        );
        assert_eq!(parse_line("normal"), ReceiverEvent::Line("normal".into()));
    }

    #[test]
    fn only_serious_lines_are_problems() {
        assert!(!is_problem("GStreamer-WARNING: something minor", true));
        assert!(!is_problem("decoder error recovered", true));
        assert!(is_problem(
            "dnssd_register_raop failed with error code 5",
            true
        ));
        assert!(is_problem("Error initializing raop!", false));
        assert!(is_problem("bind failed", false));
    }

    #[test]
    fn stale_generation_is_ignored() {
        let mut receiver = Receiver {
            generation: 2,
            ..Default::default()
        };
        assert!(receiver.handle(1, ReceiverEvent::Ready).is_empty());
    }

    #[test]
    fn unexpected_exit_reports_error_then_stopped() {
        let mut receiver = Receiver {
            generation: 1,
            running: true,
            ..Default::default()
        };
        let notices = receiver.handle(1, ReceiverEvent::Exited { code: Some(3) });
        assert!(matches!(notices[0], Notice::Error { .. }));
        assert_eq!(notices[1], Notice::Stopped);
        assert!(!receiver.running);
    }
}
