// SPDX-License-Identifier: GPL-3.0-or-later
//! Befehlszeile wie bei der C-Fassung (`uxplay`), soweit die App sie nutzt.
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub name: String,
    /// Neue Verbindung ersetzt eine bestehende (`-nh`).
    pub nohold: bool,
    /// TCP: Video (Basis), RAOP (+1). UDP: Zeit (Basis), Steuerung (+1), Daten (+2).
    pub base_port: u16,
    pub width: u16,
    pub height: u16,
    pub refresh_rate: u8,
    pub max_fps: u8,
    pub video_sink: String,
    pub audio_sink: String,
    pub key_file: Option<PathBuf>,
    pub h265: bool,
    pub software_decoder: bool,
    /// `-pin` ohne Zahl: zufällige PIN je Kopplung; mit Zahl: feste PIN.
    pub pin: Option<Option<u16>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            name: "PC-Mirror".into(),
            nohold: false,
            base_port: 48000,
            width: 1920,
            height: 1080,
            refresh_rate: 60,
            max_fps: 30,
            video_sink: "d3d11videosink force-aspect-ratio=true".into(),
            audio_sink: "wasapisink".into(),
            key_file: None,
            h265: false,
            software_decoder: false,
            pin: None,
        }
    }
}

impl Config {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut config = Self::default();
        let mut i = 0;
        let value = |i: &mut usize| -> Result<String, String> {
            *i += 1;
            args.get(*i).cloned().ok_or_else(|| format!("Wert fehlt nach {}", args[*i - 1]))
        };
        while i < args.len() {
            match args[i].as_str() {
                "-rc" => {
                    value(&mut i)?;
                }
                "-n" => config.name = sanitize_name(&value(&mut i)?),
                "-nh" => config.nohold = true,
                "-p" => {
                    config.base_port = value(&mut i)?.parse().map_err(|_| "ungültiger Port".to_string())?;
                    if !(1024..=65530).contains(&config.base_port) {
                        return Err("Port außerhalb des erlaubten Bereichs".into());
                    }
                }
                "-s" => {
                    let spec = value(&mut i)?;
                    let (size, rate) = spec.split_once('@').map_or((spec.as_str(), None), |(s, r)| (s, Some(r)));
                    let (w, h) = size.split_once('x').ok_or("ungültige Auflösung")?;
                    config.width = w.parse().map_err(|_| "ungültige Breite")?;
                    config.height = h.parse().map_err(|_| "ungültige Höhe")?;
                    if let Some(rate) = rate {
                        config.refresh_rate = rate.parse().map_err(|_| "ungültige Bildrate")?;
                    }
                }
                "-fps" => config.max_fps = value(&mut i)?.parse().map_err(|_| "ungültige FPS")?,
                "-vs" => config.video_sink = value(&mut i)?,
                "-as" => config.audio_sink = value(&mut i)?,
                "-key" => config.key_file = Some(PathBuf::from(value(&mut i)?)),
                "-h265" => config.h265 = true,
                "-avdec" => config.software_decoder = true,
                "-pin" => {
                    let fixed = args
                        .get(i + 1)
                        .filter(|v| !v.starts_with('-'))
                        .and_then(|v| v.parse::<u16>().ok())
                        .filter(|v| *v <= 9999);
                    if fixed.is_some() {
                        i += 1;
                    }
                    config.pin = Some(fixed);
                }
                other => return Err(format!("unbekannte Option {other}")),
            }
            i += 1;
        }
        Ok(config)
    }

    pub fn mirror_port(&self) -> u16 {
        self.base_port
    }
    pub fn raop_port(&self) -> u16 {
        self.base_port + 1
    }
    pub fn timing_port(&self) -> u16 {
        self.base_port
    }
    pub fn control_port(&self) -> u16 {
        self.base_port + 1
    }
    pub fn data_port(&self) -> u16 {
        self.base_port + 2
    }
}

/// Gerätename: höchstens 63 Byte (DNS-Label), keine Steuerzeichen.
fn sanitize_name(name: &str) -> String {
    let mut out: String = name.chars().filter(|c| !c.is_control()).collect();
    while out.len() > 63 {
        out.pop();
    }
    if out.trim().is_empty() {
        "PC-Mirror".into()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_app_arguments() {
        let args: Vec<String> = [
            "-rc", "x.conf", "-n", "Wohnzimmer", "-nh", "-p", "48000", "-s", "3840x2160@60", "-fps", "60",
            "-vs", "d3d11videosink force-aspect-ratio=true", "-as", "wasapisink", "-key", "k.pem", "-h265",
            "-avdec", "-pin", "1234",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let c = Config::parse(&args).unwrap();
        assert_eq!(c.name, "Wohnzimmer");
        assert!(c.nohold && c.h265 && c.software_decoder);
        assert_eq!((c.width, c.height, c.refresh_rate, c.max_fps), (3840, 2160, 60, 60));
        assert_eq!(c.pin, Some(Some(1234)));
        assert_eq!((c.mirror_port(), c.raop_port(), c.data_port()), (48000, 48001, 48002));
        let c = Config::parse(&["-pin".to_string()]).unwrap();
        assert_eq!(c.pin, Some(None));
        assert!(Config::parse(&["-x".to_string()]).is_err());
    }
}
