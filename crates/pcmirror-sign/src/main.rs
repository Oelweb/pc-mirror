// SPDX-License-Identifier: GPL-3.0-or-later
//! pcmirror-sign keygen <privat.key> <öffentlich.pub>
//! pcmirror-sign sign   <privat.key> <releases.win.json>   → schreibt <datei>.sig
//! pcmirror-sign verify <öffentlich.pub> <releases.win.json>
use std::{fs, path::Path, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["keygen", private, public] => keygen(Path::new(private), Path::new(public)),
        ["sign", private, feed] => sign(Path::new(private), Path::new(feed)),
        ["verify", public, feed] => verify(Path::new(public), Path::new(feed)),
        _ => Err("Aufruf: pcmirror-sign keygen|sign|verify <schlüssel> <datei>".into()),
    };
    match result {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("FEHLER: {message}");
            ExitCode::FAILURE
        }
    }
}

fn keygen(private: &Path, public: &Path) -> Result<String, String> {
    if private.exists() || public.exists() {
        return Err("Schlüsseldatei existiert bereits; nichts überschrieben".into());
    }
    if let Some(parent) = private.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let (secret, verifying) = pcmirror_sign::generate_keypair();
    fs::write(private, secret + "\n").map_err(|e| e.to_string())?;
    fs::write(public, verifying + "\n").map_err(|e| e.to_string())?;
    Ok(format!(
        "Schlüsselpaar erstellt. Privaten Schlüssel {} geheim halten.",
        private.display()
    ))
}

fn sign(private: &Path, feed: &Path) -> Result<String, String> {
    let key = fs::read_to_string(private).map_err(|e| format!("{}: {e}", private.display()))?;
    let data = fs::read(feed).map_err(|e| format!("{}: {e}", feed.display()))?;
    let signature = pcmirror_sign::sign(&data, &key)?;
    let target = signature_path(feed);
    fs::write(&target, signature + "\n").map_err(|e| e.to_string())?;
    Ok(format!("Signiert: {}", target.display()))
}

fn verify(public: &Path, feed: &Path) -> Result<String, String> {
    let key = fs::read_to_string(public).map_err(|e| format!("{}: {e}", public.display()))?;
    let data = fs::read(feed).map_err(|e| format!("{}: {e}", feed.display()))?;
    let signature = fs::read_to_string(signature_path(feed)).map_err(|e| e.to_string())?;
    pcmirror_sign::verify(&data, &signature, &key)?;
    Ok("PASS Signatur gültig".into())
}

fn signature_path(feed: &Path) -> std::path::PathBuf {
    let mut name = feed.as_os_str().to_owned();
    name.push(".sig");
    name.into()
}
