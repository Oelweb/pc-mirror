// SPDX-License-Identifier: GPL-3.0-or-later
//! AirPlay-Empfänger (Bildschirmsynchronisierung und Ton) in Rust.
//! Protokoll nach dem Vorbild von UxPlay; Medienwiedergabe über GStreamer.
pub mod audio;
pub mod config;
pub mod control;
pub mod crypto;
pub mod fairplay;
pub mod gst;
pub mod mdns;
pub mod media;
pub mod mirror;
pub mod net;
pub mod ntp;
pub mod pairing;
pub mod receiver;
pub mod rtsp;
pub mod server;
pub mod srp;
pub mod window;
