// SPDX-License-Identifier: GPL-3.0-or-later
//! AirPlay-Empfänger (Bildschirmsynchronisierung und Ton) in Rust.
//! Protokoll nach dem Vorbild von UxPlay; Medienwiedergabe über GStreamer.
pub mod crypto;
pub mod fairplay;
pub mod pairing;
pub mod srp;
