// SPDX-License-Identifier: GPL-3.0-or-later
//! PC-Mirror AirPlay-Empfänger. Wird von der App gestartet und über stdin/stdout gesteuert.


use pcmirror_receiver::control::{self, Command};
use pcmirror_receiver::{config::Config, gst, receiver, window};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = if args.iter().any(|a| a == "-v" || a == "--version") {
        // Version und Ladeprüfung der GStreamer-Bibliotheken (für die Paketprüfung)
        println!("PC-Mirror AirPlay-Empfänger (Rust) {}", env!("CARGO_PKG_VERSION"));
        match gst::api() {
            Ok(_) => 0,
            Err(error) => {
                eprintln!("GStreamer: {error}");
                1
            }
        }
    } else if args.iter().any(|a| a == "--pcmirror-selftest") {
        selftest()
    } else {
        match Config::parse(&args) {
            Ok(config) => receiver::run(config),
            Err(error) => {
                eprintln!("PC-Mirror receiver: {error}");
                2
            }
        }
    };
    std::process::exit(code);
}

/// Synthetisches Testbild (kein AirPlay-Test): prüft Fenster, Videoausgabe,
/// Steuerkanal und Screenshot ohne iPhone.
fn selftest() -> i32 {
    window::set_ipad(std::env::var("PCMIRROR_TEST_MODEL").map(|m| window::is_ipad_model(&m)).unwrap_or(false));
    gst::on_frame_size(|w, h| control::emit(&format!("FRAME|{w}|{h}")));
    let headless = std::env::var_os("PCMIRROR_TEST_HEADLESS").is_some();
    let sink = if headless {
        "fakesink name=pcmirror_test sync=true"
    } else {
        "d3d11videosink name=pcmirror_test force-aspect-ratio=true"
    };
    let handle = window::create_from_env();
    gst::set_window_handle(handle);
    let description = format!(
        "videotestsrc is-live=true pattern=ball ! video/x-raw,width=360,height=640,framerate=30/1 ! videoconvert ! {sink}"
    );
    let pipeline = match gst::Pipeline::launch(&description, None, None) {
        Ok(p) => p,
        Err(error) => {
            eprintln!("PC-Mirror synthetic pipeline failed: {error}");
            return 1;
        }
    };
    if let Some(sink) = pipeline.element("pcmirror_test") {
        sink.watch_frames();
    }
    pipeline.set_state(gst::GST_STATE_PLAYING);
    control::emit("READY");
    control::log("Synthetic test pattern only; this is not an AirPlay test.");

    let (sender, commands) = std::sync::mpsc::channel();
    control::spawn_reader(sender);
    while let Ok(command) = commands.recv() {
        match command {
            Command::Stop => break,
            Command::Mute(muted) => control::emit(&format!("MUTED|{}", u8::from(muted))),
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
    drop(pipeline);
    0
}
