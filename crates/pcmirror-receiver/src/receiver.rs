// SPDX-License-Identifier: GPL-3.0-or-later
//! Ablauf des Empfängers: Identität, Ankündigung, Server, Steuerkanal.
use crate::config::Config;
use crate::control::{self, Command};
use crate::mdns::{Responder, Service};
use crate::media::Media;
use crate::pairing::Identity;
use crate::server::{self, Shared};
use crate::{gst, net, window};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn run(config: Config) -> i32 {
    if let Err(error) = gst::api() {
        eprintln!("PC-Mirror receiver: GStreamer konnte nicht geladen werden: {error}");
        return 1;
    }
    let key_file = config.key_file.clone().unwrap_or_else(|| std::path::PathBuf::from("receiver.pem"));
    let (identity, created) = Identity::load_or_create(&key_file);
    if created {
        control::log(&format!("Neue Geräteidentität erstellt: {}", key_file.display()));
    }
    let hw_addr = net::hardware_address();
    let handle = window::create_from_env();
    gst::set_window_handle(handle);

    let media = Arc::new(Media::new(&config.video_sink, &config.audio_sink, config.software_decoder));
    let shared = Arc::new(Shared::new(config.clone(), identity, hw_addr, media.clone()));
    let stop = Arc::new(AtomicBool::new(false));
    let server_thread = match server::start(shared.clone(), stop.clone()) {
        Ok(t) => t,
        Err(error) => {
            eprintln!("Error initializing raop: {error}");
            return 1;
        }
    };
    let services = vec![
        Service {
            instance: config.name.clone(),
            kind: "_airplay._tcp.local",
            port: config.raop_port(),
            txt: shared.airplay_txt(),
        },
        Service {
            instance: format!("{}@{}", shared.raop_hw(), config.name),
            kind: "_raop._tcp.local",
            port: config.raop_port(),
            txt: shared.raop_txt(),
        },
    ];
    let responder = match Responder::start(net::host_name(), services) {
        Ok(r) => r,
        Err(error) => {
            eprintln!("could not initialize dnssd (mDNS): {error}");
            return 1;
        }
    };
    control::log(&format!(
        "AirPlay-Empfänger (Rust) bereit: {} auf Port {}",
        config.name,
        config.raop_port()
    ));
    control::emit("READY");

    let (sender, commands) = std::sync::mpsc::channel();
    control::spawn_reader(sender);
    while let Ok(command) = commands.recv() {
        match command {
            Command::Stop => break,
            Command::Mute(muted) => {
                media.set_mute(muted);
                control::emit(&format!("MUTED|{}", u8::from(muted)));
            }
            Command::IpadFill(fill) => {
                window::set_fill(fill);
                control::emit(&format!("IPAD_FILL|{}", u8::from(fill)));
            }
            Command::Snapshot(path) => {
                let ok = control::write_snapshot(&path);
                control::emit(if ok { "SNAPSHOT|OK" } else { "SNAPSHOT|ERROR" });
            }
        }
    }
    stop.store(true, Ordering::SeqCst);
    drop(responder); // Abmeldung im Netz
    shared.shutdown_all();
    let _ = server_thread.join();
    media.stop_all();
    0
}
