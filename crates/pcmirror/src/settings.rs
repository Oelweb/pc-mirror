// SPDX-License-Identifier: GPL-3.0-or-later
//! Benutzereinstellungen in `%LOCALAPPDATA%\PC-Mirror\settings.ini`.
//! Das Format bleibt kompatibel zu den Dateien der früheren Qt-Version.
use crate::secret;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

pub const MAX_NAME_BYTES: usize = 48;
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "PC-Mirror";

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub name: String,
    pub language: String,
    /// 0 = 720p/30, 1 = 1080p/60, 2 = 4K/H.265/60
    pub quality: u8,
    pub autostart: bool,
    pub pin_enabled: bool,
    pub pin: String,
    pub software_decoder: bool,
    pub topmost: bool,
    pub muted: bool,
    pub automatic_updates: bool,
    pub ipad_fill: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            name: "PC-Mirror".into(),
            language: "de".into(),
            quality: 1,
            autostart: false,
            pin_enabled: false,
            pin: String::new(),
            software_decoder: false,
            topmost: false,
            muted: false,
            automatic_updates: true,
            ipad_fill: false,
        }
    }
}

/// Fensterposition in physischen Pixeln.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("PC-Mirror")
}

fn ini_path() -> PathBuf {
    data_dir().join("settings.ini")
}

impl Settings {
    pub fn load() -> Self {
        let ini = Ini::read(&ini_path());
        let defaults = Settings::default();
        let mut value = Settings {
            name: ini
                .get("receiver", "name")
                .unwrap_or(&defaults.name)
                .to_owned(),
            language: crate::i18n::normalize(ini.get("ui", "language").unwrap_or("de")).to_owned(),
            quality: ini
                .get("receiver", "quality")
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(1)
                .clamp(0, 2) as u8,
            autostart: autostart_enabled(),
            pin_enabled: ini.bool("receiver", "pinEnabled", false),
            pin: String::new(),
            software_decoder: ini.bool("receiver", "softwareDecoder", false),
            topmost: ini.bool("window", "topmost", false),
            muted: ini.bool("audio", "muted", false),
            automatic_updates: ini.bool("updates", "automatic", true),
            ipad_fill: ini.bool("video", "ipadFill", false),
        };
        // Neue Versionen speichern die PIN per Windows-DPAPI verschlüsselt.
        // Eine Klartext-PIN älterer Versionen wird gelesen und beim nächsten Speichern ersetzt.
        value.pin = match ini.get("receiver", "pinProtected") {
            Some(protected) => secret::unprotect(protected).unwrap_or_default(),
            None => ini.get("receiver", "pin").unwrap_or_default().to_owned(),
        };
        if validate_name(&value.name).is_err() {
            value.name = defaults.name;
        }
        if validate_pin(&value.pin).is_err() {
            value.pin.clear();
        }
        value
    }

    /// Speichert alle Werte. Der Fehler ist ein deutscher Quelltext für `i18n::tr`.
    pub fn save(&self) -> Result<(), &'static str> {
        const FAILED: &str =
            "Die Einstellungen konnten nicht gespeichert werden. Bitte Schreibrechte prüfen.";
        let path = ini_path();
        let mut ini = Ini::read(&path);
        ini.set("receiver", "name", &self.name);
        ini.set("ui", "language", crate::i18n::normalize(&self.language));
        ini.set("receiver", "quality", &self.quality.to_string());
        ini.set("receiver", "pinEnabled", bool_text(self.pin_enabled));
        ini.remove("receiver", "pin");
        if self.pin.is_empty() {
            ini.remove("receiver", "pinProtected");
        } else {
            ini.set(
                "receiver",
                "pinProtected",
                &secret::protect(&self.pin).map_err(|_| FAILED)?,
            );
        }
        ini.set(
            "receiver",
            "softwareDecoder",
            bool_text(self.software_decoder),
        );
        ini.set("window", "topmost", bool_text(self.topmost));
        ini.set("audio", "muted", bool_text(self.muted));
        ini.set("updates", "automatic", bool_text(self.automatic_updates));
        ini.set("video", "ipadFill", bool_text(self.ipad_fill));
        ini.write(&path).map_err(|_| FAILED)?;
        set_autostart(self.autostart)
            .map_err(|_| "Der Windows-Autostart konnte nicht geändert werden.")
    }

    /// Nur Änderungen, die der Empfängerprozess wirklich braucht, erzwingen einen Neustart.
    /// (Behebt: Autostart umschalten trennte früher die laufende Spiegelung.)
    pub fn receiver_differs(&self, other: &Settings) -> bool {
        self.name != other.name
            || self.quality != other.quality
            || self.pin_enabled != other.pin_enabled
            || self.pin != other.pin
            || self.software_decoder != other.software_decoder
    }

    pub fn receiver_arguments(&self, config_file: &Path) -> Vec<OsString> {
        const RESOLUTIONS: [&str; 3] = ["1280x720@30", "1920x1080@60", "3840x2160@60"];
        let quality = usize::from(self.quality.min(2));
        let mut args: Vec<OsString> = vec![
            "-rc".into(),
            config_file.into(),
            "-n".into(),
            self.name.clone().into(),
            "-nh".into(),
            "-p".into(),
            "48000".into(),
            "-s".into(),
            RESOLUTIONS[quality].into(),
            "-fps".into(),
            (if quality == 0 { "30" } else { "60" }).into(),
            "-vs".into(),
            "d3d11videosink force-aspect-ratio=true".into(),
            "-as".into(),
            "wasapisink".into(),
            "-key".into(),
            data_dir().join("receiver.pem").into(),
        ];
        if quality == 2 {
            args.push("-h265".into());
        }
        if self.software_decoder {
            args.push("-avdec".into());
        }
        if self.pin_enabled {
            args.push("-pin".into());
            if !self.pin.is_empty() {
                args.push(self.pin.clone().into());
            }
        }
        args
    }
}

pub fn validate_name(name: &str) -> Result<(), &'static str> {
    let invalid = name.is_empty()
        || name.len() > MAX_NAME_BYTES
        || name != name.trim()
        || name
            .chars()
            .any(|c| c.is_control() || matches!(c, '.' | '\\' | '/' | '|'));
    if invalid {
        Err("Bitte einen Gerätenamen ohne Punkte, Schrägstriche oder Steuerzeichen eingeben (maximal 48 UTF-8-Bytes).")
    } else {
        Ok(())
    }
}

pub fn validate_pin(pin: &str) -> Result<(), &'static str> {
    let ok = pin.is_empty()
        || (pin.len() == 4 && pin.bytes().all(|b| b.is_ascii_digit()) && pin != "0000");
    if ok {
        Ok(())
    } else {
        Err("PIN: vier Ziffern von 0001 bis 9999, oder für einen zufälligen Code leer lassen.")
    }
}

pub fn load_geometry() -> Option<Geometry> {
    let ini = Ini::read(&ini_path());
    let number = |key| ini.get("window", key).and_then(|v| v.parse::<i64>().ok());
    let (x, y, width, height) = (
        number("x")?,
        number("y")?,
        number("width")?,
        number("height")?,
    );
    if !(320..=20000).contains(&width) || !(240..=20000).contains(&height) {
        return None;
    }
    Some(Geometry {
        x: x.clamp(-40000, 40000) as i32,
        y: y.clamp(-40000, 40000) as i32,
        width: width as u32,
        height: height as u32,
        maximized: ini.bool("window", "maximized", false),
    })
}

pub fn save_geometry(geometry: Geometry) {
    let path = ini_path();
    let mut ini = Ini::read(&path);
    ini.set("window", "x", &geometry.x.to_string());
    ini.set("window", "y", &geometry.y.to_string());
    ini.set("window", "width", &geometry.width.to_string());
    ini.set("window", "height", &geometry.height.to_string());
    ini.set("window", "maximized", bool_text(geometry.maximized));
    ini.remove("window", "geometry"); // Qt-Binärformat wird nicht mehr verwendet
    let _ = ini.write(&path);
}

fn bool_text(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn autostart_enabled() -> bool {
    crate::win::registry_get_string(RUN_KEY, RUN_VALUE).is_some()
}

fn set_autostart(enabled: bool) -> Result<(), String> {
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        crate::win::registry_set_string(RUN_KEY, RUN_VALUE, &format!("\"{}\"", exe.display()))
    } else {
        crate::win::registry_delete_value(RUN_KEY, RUN_VALUE)
    }
}

/// Minimaler INI-Leser/-Schreiber, der unbekannte Einträge erhält und
/// Escapes der Qt-Implementierung (`\xNNNN`, Anführungszeichen) versteht.
#[derive(Default, Debug)]
struct Ini {
    sections: BTreeMap<String, BTreeMap<String, String>>,
}

impl Ini {
    fn read(path: &Path) -> Self {
        let mut ini = Ini::default();
        let Ok(text) = fs::read_to_string(path) else {
            return ini;
        };
        let mut section = "General".to_owned();
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].to_owned();
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                ini.sections
                    .entry(section.clone())
                    .or_default()
                    .insert(key.trim().to_owned(), unescape(value.trim()));
            }
        }
        ini
    }

    fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections.get(section)?.get(key).map(String::as_str)
    }

    fn bool(&self, section: &str, key: &str, default: bool) -> bool {
        match self.get(section, key) {
            Some("true") | Some("1") => true,
            Some("false") | Some("0") => false,
            _ => default,
        }
    }

    fn set(&mut self, section: &str, key: &str, value: &str) {
        self.sections
            .entry(section.to_owned())
            .or_default()
            .insert(key.to_owned(), value.to_owned());
    }

    fn remove(&mut self, section: &str, key: &str) {
        if let Some(values) = self.sections.get_mut(section) {
            values.remove(key);
        }
    }

    /// Schreibt atomar über eine temporäre Datei.
    fn write(&self, path: &Path) -> std::io::Result<()> {
        let mut text = String::new();
        for (section, values) in &self.sections {
            if values.is_empty() {
                continue;
            }
            text.push_str(&format!("[{section}]\n"));
            for (key, value) in values {
                text.push_str(&format!("{key}={}\n", escape(value)));
            }
            text.push('\n');
        }
        fs::create_dir_all(path.parent().unwrap())?;
        let temporary = path.with_extension("ini.tmp");
        fs::write(&temporary, text)?;
        fs::rename(&temporary, path)
    }
}

fn unescape(value: &str) -> String {
    let quoted = value.len() >= 2 && value.starts_with('"') && value.ends_with('"');
    let body = if quoted {
        &value[1..value.len() - 1]
    } else {
        value
    };
    let mut out = String::new();
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some('x') => {
                let mut hex = String::new();
                while hex.len() < 4 && chars.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                    hex.push(chars.next().unwrap());
                }
                if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(c);
                }
            }
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

fn escape(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\x{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    let needs_quotes = out != out.trim() || out.contains([';', ',', '=', '#']);
    if needs_quotes {
        format!("\"{out}\"")
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_qt_escapes_and_round_trips() {
        assert_eq!(unescape(r"B\xfcro"), "Büro");
        assert_eq!(unescape(r#""Wohnzimmer, oben""#), "Wohnzimmer, oben");
        for value in ["Büro", "a,b", " Leerzeichen", "Zeile\nzwei", r"C:\Pfad"] {
            assert_eq!(unescape(&escape(value)), value);
        }
    }

    #[test]
    fn validates_name_and_pin() {
        assert!(validate_name("Wohnzimmer-PC").is_ok());
        assert!(validate_name("Büro äöü").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("a.b").is_err());
        assert!(validate_name("a/b").is_err());
        assert!(validate_name(&"ä".repeat(25)).is_err());
        assert!(validate_pin("").is_ok());
        assert!(validate_pin("0001").is_ok());
        assert!(validate_pin("0000").is_err());
        assert!(validate_pin("123").is_err());
        assert!(validate_pin("12a4").is_err());
    }

    #[test]
    fn autostart_does_not_restart_receiver() {
        let a = Settings::default();
        let mut b = a.clone();
        b.autostart = true;
        b.topmost = true;
        b.ipad_fill = true;
        b.language = "en".into();
        assert!(!a.receiver_differs(&b));
        b.quality = 0;
        assert!(a.receiver_differs(&b));
    }

    #[test]
    fn receiver_arguments_match_quality_and_pin() {
        let mut s = Settings {
            quality: 2,
            pin_enabled: true,
            pin: "1234".into(),
            software_decoder: true,
            ..Default::default()
        };
        let args: Vec<String> = s
            .receiver_arguments(Path::new("r.conf"))
            .into_iter()
            .map(|a| a.into_string().unwrap())
            .collect();
        assert!(args.windows(2).any(|w| w == ["-s", "3840x2160@60"]));
        assert!(args.contains(&"-h265".to_owned()) && args.contains(&"-avdec".to_owned()));
        assert!(args.ends_with(&["-pin".to_owned(), "1234".to_owned()]));
        s.pin.clear();
        let args = s.receiver_arguments(Path::new("r.conf"));
        assert_eq!(args.last().unwrap(), "-pin");
    }
}
