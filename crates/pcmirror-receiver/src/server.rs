// SPDX-License-Identifier: GPL-3.0-or-later
//! RTSP-Server des Empfängers: Pairing, FairPlay, SETUP der Ströme und Steuerung.
//! Port von `raop.c` und `raop_handlers.h` (ohne HLS-Videostreaming, das die App
//! nicht nutzt).
use crate::audio::AudioStream;
use crate::config::Config;
use crate::control;
use crate::crypto::sha512;
use crate::fairplay::FairPlay;
use crate::media::Media;
use crate::mirror::MirrorStream;
use crate::ntp::Ntp;
use crate::pairing::{Identity, Session};
use crate::rtsp::{read_request, ReadError, Request, Response};
use crate::window;
use plist::{Dictionary, Value};
use std::collections::HashMap;
use std::io::{BufReader, Write};
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const GLOBAL_VERSION: &str = "220.68";
pub const GLOBAL_MODEL: &str = "AppleTV3,2";
pub const AIRPLAY_PI: &str = "2e388006-13ba-4041-9a67-25dd4a43d536";
/// Erste 32 Feature-Bits (Bit 27 „legacy pairing“ an), wie UxPlay sie setzt.
pub const FEATURES_LOW: u32 = 0x5A7F_FEE6;
/// Bit 42 (SupportsScreenMultiCodec) erlaubt H.265.
pub const FEATURE_H265: u32 = 1 << 10;
const AUDIO_DELAY_MICROS: u64 = 250_000;
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

pub struct Shared {
    pub config: Config,
    pub identity: Identity,
    pub hw_addr: [u8; 6],
    pub media: Arc<Media>,
    random_pin: Mutex<Option<u16>>,
    connections: Mutex<HashMap<u64, (TcpStream, bool)>>,
    next_id: AtomicU64,
    volume_db: Mutex<f32>,
}

impl Shared {
    pub fn new(config: Config, identity: Identity, hw_addr: [u8; 6], media: Arc<Media>) -> Self {
        Self {
            config,
            identity,
            hw_addr,
            media,
            random_pin: Mutex::new(None),
            connections: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            volume_db: Mutex::new(0.0),
        }
    }

    pub fn features(&self) -> u64 {
        let high = if self.config.h265 { FEATURE_H265 } else { 0 };
        (u64::from(high) << 32) | u64::from(FEATURES_LOW)
    }

    pub fn features_text(&self) -> String {
        let f = self.features();
        format!("0x{:X},0x{:X}", f as u32, (f >> 32) as u32)
    }

    pub fn device_id(&self) -> String {
        self.hw_addr.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(":")
    }

    pub fn raop_hw(&self) -> String {
        self.hw_addr.iter().map(|b| format!("{b:02X}")).collect()
    }

    pub fn pk_hex(&self) -> String {
        self.identity.public_key().iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn pin_mode(&self) -> bool {
        self.config.pin.is_some()
    }

    pub fn airplay_txt(&self) -> Vec<(String, String)> {
        vec![
            ("deviceid".into(), self.device_id()),
            ("features".into(), self.features_text()),
            ("pw".into(), if self.pin_mode() { "true" } else { "false" }.into()),
            ("flags".into(), "0x4".into()),
            ("model".into(), GLOBAL_MODEL.into()),
            ("pk".into(), self.pk_hex()),
            ("pi".into(), AIRPLAY_PI.into()),
            ("srcvers".into(), GLOBAL_VERSION.into()),
            ("vv".into(), "2".into()),
        ]
    }

    pub fn raop_txt(&self) -> Vec<(String, String)> {
        let (pw, sf) = if self.pin_mode() { ("true", "0x8c") } else { ("false", "0x4") };
        vec![
            ("ch".into(), "2".into()),
            ("cn".into(), "0,1,2,3".into()),
            ("da".into(), "true".into()),
            ("et".into(), "0,3,5".into()),
            ("vv".into(), "2".into()),
            ("ft".into(), self.features_text()),
            ("am".into(), GLOBAL_MODEL.into()),
            ("md".into(), "0,1,2".into()),
            ("rhd".into(), "5.6.0.0".into()),
            ("pw".into(), pw.into()),
            ("sr".into(), "44100".into()),
            ("ss".into(), "16".into()),
            ("sv".into(), "false".into()),
            ("tp".into(), "UDP".into()),
            ("txtvers".into(), "1".into()),
            ("sf".into(), sf.into()),
            ("vs".into(), GLOBAL_VERSION.into()),
            ("vn".into(), "65537".into()),
            ("pk".into(), self.pk_hex()),
        ]
    }

    fn txt_bytes(txt: &[(String, String)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (k, v) in txt {
            let item = format!("{k}={v}");
            out.push(item.len().min(255) as u8);
            out.extend_from_slice(&item.as_bytes()[..item.len().min(255)]);
        }
        out
    }

    /// Schließt alle anderen Verbindungen (Option „nohold“).
    fn close_others(&self, keep: u64) {
        let connections = self.connections.lock().unwrap();
        for (id, (stream, _)) in connections.iter() {
            if *id != keep {
                let _ = stream.shutdown(Shutdown::Both);
            }
        }
    }

    fn raop_connection_active(&self, except: u64) -> bool {
        self.connections.lock().unwrap().iter().any(|(id, (_, raop))| *id != except && *raop)
    }

    fn mark_raop(&self, id: u64) {
        if let Some(entry) = self.connections.lock().unwrap().get_mut(&id) {
            entry.1 = true;
        }
    }

    pub fn shutdown_all(&self) {
        for (_, (stream, _)) in self.connections.lock().unwrap().iter() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Kind {
    Unknown,
    Raop,
}

struct Connection {
    id: u64,
    kind: Kind,
    remote: IpAddr,
    fairplay: FairPlay,
    pairing: Session,
    keys: Option<([u8; 16], [u8; 16])>,
    ntp: Option<Ntp>,
    mirror: Option<MirrorStream>,
    audio: Option<AudioStream>,
}

/// Startet den RTSP-Server; Verbindungen laufen in eigenen Threads.
pub fn start(shared: Arc<Shared>, stop: Arc<AtomicBool>) -> Result<std::thread::JoinHandle<()>, String> {
    let listener = TcpListener::bind(("0.0.0.0", shared.config.raop_port()))
        .map_err(|e| format!("failed to bind RAOP port {}: {e}", shared.config.raop_port()))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    Ok(std::thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, peer)) => {
                    let shared = shared.clone();
                    std::thread::spawn(move || serve(shared, stream, peer));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(50)),
                Err(_) => std::thread::sleep(Duration::from_millis(200)),
            }
        }
    }))
}

fn serve(shared: Arc<Shared>, stream: TcpStream, peer: SocketAddr) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(IDLE_TIMEOUT));
    let id = shared.next_id.fetch_add(1, Ordering::SeqCst);
    let Ok(clone) = stream.try_clone() else { return };
    shared.connections.lock().unwrap().insert(id, (clone, false));
    let mut writer = match stream.try_clone() {
        Ok(w) => w,
        Err(_) => return,
    };
    let mut reader = BufReader::new(stream);
    let mut connection = Connection {
        id,
        kind: Kind::Unknown,
        remote: peer.ip(),
        fairplay: FairPlay::default(),
        pairing: Session::default(),
        keys: None,
        ntp: None,
        mirror: None,
        audio: None,
    };
    loop {
        let request = match read_request(&mut reader) {
            Ok(r) => r,
            Err(ReadError::Closed) | Err(ReadError::Invalid) => break,
            Err(ReadError::TooLarge(protocol)) => {
                control::log("rejecting request with overlong headers");
                let _ = writer.write_all(&Response::new(&protocol, 431, "Request Header Fields Too Large").to_bytes());
                break;
            }
        };
        let Some(response) = connection.handle(&shared, &request) else { continue };
        if writer.write_all(&response.to_bytes()).is_err() || response.disconnect {
            break;
        }
    }
    let was_raop = connection.kind == Kind::Raop;
    let had_streams = connection.mirror.is_some() || connection.audio.is_some();
    drop(connection);
    shared.connections.lock().unwrap().remove(&id);
    if was_raop && had_streams && !shared.raop_connection_active(id) {
        shared.media.stop_all();
        control::log("Verbindung zum Gerät beendet");
    }
}

fn data(dict: &Dictionary, key: &str) -> Option<Vec<u8>> {
    dict.get(key).and_then(Value::as_data).map(<[u8]>::to_vec)
}

fn uint(dict: &Dictionary, key: &str) -> Option<u64> {
    dict.get(key).and_then(|v| v.as_unsigned_integer().or_else(|| v.as_signed_integer().map(|i| i as u64)))
}

fn string(dict: &Dictionary, key: &str) -> Option<String> {
    dict.get(key).and_then(Value::as_string).map(str::to_owned)
}

fn parse_plist(body: &[u8]) -> Dictionary {
    plist::from_bytes::<Value>(body).ok().and_then(Value::into_dictionary).unwrap_or_default()
}

impl Connection {
    fn handle(&mut self, shared: &Shared, request: &Request) -> Option<Response> {
        let cseq = match request.header("CSeq") {
            Some(v) => Some(v.trim().parse::<u32>().ok()?), // ungültige CSeq: keine Antwort
            None => None,
        };
        let ble = request.protocol == "RTSP/1.0"
            && cseq.is_none()
            && (request.url.contains("txtAirPlay") || request.url.contains("txtRAOP"));
        if cseq.is_none() && !ble {
            // HTTP-Videostreaming (HLS) wird nicht angeboten
            return None;
        }
        let protocol = request.protocol.as_str();
        if self.kind == Kind::Unknown {
            if shared.raop_connection_active(self.id) {
                if shared.config.nohold {
                    control::log("Neue Verbindung ersetzt die bestehende");
                    shared.close_others(self.id);
                    shared.media.stop_all();
                } else {
                    let mut r = Response::new(protocol, 409, "Conflict: Server is connected to another client");
                    finish(&mut r, cseq);
                    return Some(r);
                }
            }
            self.kind = Kind::Raop;
            shared.mark_raop(self.id);
        }
        let mut response = Response::new(protocol, 200, "OK");
        if cseq.is_some() && request.method != "RECORD" {
            response.header("Audio-Jack-Status", "connected; type=digital");
        }
        if protocol == "RTSP/1.0" {
            match (request.method.as_str(), request.url.as_str()) {
                ("POST", "/feedback") | ("POST", "/audioMode") => {}
                ("POST", "/pair-pin-start") => self.pair_pin_start(shared),
                ("POST", "/pair-setup-pin") => self.pair_setup_pin(shared, request, &mut response),
                ("POST", "/pair-setup") => self.pair_setup(shared, request, &mut response),
                ("POST", "/pair-verify") => self.pair_verify(shared, request, &mut response),
                ("POST", "/fp-setup") => self.fp_setup(request, &mut response),
                ("GET", url) if url.contains("/info") => info(shared, request, cseq.is_some(), &mut response),
                ("OPTIONS", _) => response.header(
                    "Public",
                    "SETUP, RECORD, FLUSH, TEARDOWN, OPTIONS, GET_PARAMETER, SET_PARAMETER",
                ),
                ("SETUP", _) => self.setup(shared, request, &mut response),
                ("GET_PARAMETER", _) => get_parameter(shared, request, &mut response),
                ("SET_PARAMETER", _) => set_parameter(shared, request, &mut response),
                ("RECORD", _) => {
                    let latency = AUDIO_DELAY_MICROS * 44_100 / 1_000_000;
                    response.header("Audio-Latency", &latency.to_string());
                    response.header("Audio-Jack-Status", "connected; type=analog");
                }
                ("FLUSH", _) => {
                    let next = request
                        .header("RTP-Info")
                        .and_then(|v| v.strip_prefix("seq="))
                        .and_then(|v| v.split(';').next())
                        .and_then(|v| v.trim().parse::<u32>().ok())
                        .map(|v| v as u16);
                    if let Some(audio) = &self.audio {
                        audio.flush(next);
                    }
                }
                ("TEARDOWN", _) => self.teardown(shared, request, &mut response),
                ("POST", _) | ("GET", _) => control::log(&format!("Unhandled request {} {}", request.method, request.url)),
                _ => response.set_status(501, "Not Implemented"),
            }
        }
        finish(&mut response, cseq);
        Some(response)
    }

    fn pair_pin_start(&mut self, shared: &Shared) {
        let pin = match shared.config.pin {
            Some(Some(fixed)) => fixed,
            _ => {
                let pin = crate::crypto::random_pin();
                *shared.random_pin.lock().unwrap() = Some(pin);
                pin
            }
        };
        let text = format!("{pin:04}");
        control::emit(&format!("PIN|{text}"));
        control::log("Gerät fordert eine PIN an");
    }

    fn pair_setup_pin(&mut self, shared: &Shared, request: &Request, response: &mut Response) {
        let is_plist = request.header("Content-Type").is_some_and(|t| t.contains("apple-binary-plist"));
        let dict = if is_plist { parse_plist(&request.body) } else { Dictionary::new() };
        let fail = |response: &mut Response| response.set_status(470, "Client Authentication Failure");
        if let (Some(method), Some(user)) = (string(&dict, "method"), string(&dict, "user")) {
            if method != "pin" {
                return;
            }
            let pin = match shared.config.pin {
                Some(Some(fixed)) => fixed,
                _ => shared.random_pin.lock().unwrap().take().unwrap_or(0),
            };
            match self.pairing.srp_start(&user, &format!("{pin:04}")) {
                Some((salt, pk)) => {
                    let mut d = Dictionary::new();
                    d.insert("pk".into(), Value::Data(pk));
                    d.insert("salt".into(), Value::Data(salt));
                    response.plist(&Value::Dictionary(d));
                }
                None => fail(response),
            }
        } else if let (Some(pk), Some(proof)) = (data(&dict, "pk"), data(&dict, "proof")) {
            match self.pairing.srp_verify(&pk, &proof) {
                Some(server_proof) => {
                    let mut d = Dictionary::new();
                    d.insert("proof".into(), Value::Data(server_proof.to_vec()));
                    response.plist(&Value::Dictionary(d));
                }
                None => {
                    control::log("PIN-Kopplung: falsche PIN");
                    fail(response);
                }
            }
        } else if let (Some(epk), Some(tag)) = (data(&dict, "epk"), data(&dict, "authTag")) {
            let (Ok(epk), Ok(tag)) = (<[u8; 32]>::try_from(epk.as_slice()), <[u8; 16]>::try_from(tag.as_slice())) else {
                fail(response);
                return;
            };
            match self.pairing.srp_confirm(&shared.identity, &epk, &tag) {
                Some((our_epk, our_tag)) => {
                    self.pairing.set_setup();
                    let mut d = Dictionary::new();
                    d.insert("epk".into(), Value::Data(our_epk.to_vec()));
                    d.insert("authTag".into(), Value::Data(our_tag.to_vec()));
                    response.plist(&Value::Dictionary(d));
                    control::log("PIN-Kopplung erfolgreich");
                }
                None => fail(response),
            }
        } else {
            fail(response);
        }
    }

    fn pair_setup(&mut self, shared: &Shared, request: &Request, response: &mut Response) {
        if request.body.len() != 32 {
            control::log("Invalid pair-setup data");
            return;
        }
        self.pairing.set_setup();
        response.octets(shared.identity.public_key().to_vec());
    }

    fn pair_verify(&mut self, shared: &Shared, request: &Request, response: &mut Response) {
        if !self.pairing.ready_for_verify() {
            if shared.pin_mode() {
                self.pairing.set_setup();
            } else {
                return;
            }
        }
        let body = &request.body;
        if body.len() < 4 {
            return;
        }
        match body[0] {
            1 if body.len() == 4 + 32 + 32 => {
                let ecdh: [u8; 32] = body[4..36].try_into().unwrap();
                let ed: [u8; 32] = body[36..68].try_into().unwrap();
                if let Some(reply) = self.pairing.verify_start(&shared.identity, &ecdh, &ed) {
                    response.octets(reply);
                }
            }
            0 if body.len() == 4 + 64 => {
                let signature: [u8; 64] = body[4..68].try_into().unwrap();
                if !self.pairing.verify_finish(&signature) {
                    control::log("Incorrect pair-verify signature");
                    response.disconnect = true;
                    return;
                }
                response.header("Content-Type", "application/octet-stream");
            }
            _ => control::log("Invalid pair-verify data"),
        }
    }

    fn fp_setup(&mut self, request: &Request, response: &mut Response) {
        let body = &request.body;
        match body.len() {
            16 => {
                if body[4] != 0x03 {
                    response.set_status(501, "Not Implemented");
                    return;
                }
                if let Some(reply) = self.fairplay.setup(body) {
                    response.octets(reply);
                }
            }
            164 => {
                if let Some(reply) = self.fairplay.handshake(body) {
                    response.octets(reply);
                }
            }
            _ => control::log("Invalid fp-setup data length"),
        }
    }

    fn setup(&mut self, shared: &Shared, request: &Request, response: &mut Response) {
        let dict = parse_plist(&request.body);
        let mut res = Dictionary::new();
        if let (Some(eiv), Some(ekey)) = (data(&dict, "eiv"), data(&dict, "ekey")) {
            let model = string(&dict, "model").unwrap_or_default();
            let name = string(&dict, "name").unwrap_or_default();
            control::log(&format!("Verbindungsanfrage von {name} ({model})"));
            window::set_ipad(window::is_ipad_model(&model));
            let (Ok(iv), Some(mut key)) = (<[u8; 16]>::try_from(eiv.get(..16).unwrap_or(&[])), self.fairplay.decrypt(&ekey)) else {
                control::log("SETUP: FairPlay-Schlüssel ungültig");
                response.set_status(470, "Client Authentication Failure");
                return;
            };
            let old_protocol = request.header("User-Agent").is_some_and(|ua| ua.contains("AirMyPC"));
            if !old_protocol {
                if let Some(secret) = self.pairing.ecdh_secret() {
                    key = sha512(&[&key, &secret])[..16].try_into().unwrap();
                }
            }
            self.keys = Some((key, iv));
            let timing_port = uint(&dict, "timingPort").unwrap_or(0) as u16;
            match Ntp::start(SocketAddr::new(self.remote, timing_port), shared.config.timing_port()) {
                Ok(ntp) => {
                    res.insert("timingPort".into(), Value::Integer(u64::from(ntp.local_port).into()));
                    self.ntp = Some(ntp);
                }
                Err(e) => control::log(&format!("Zeitabgleich: {e}")),
            }
            res.insert("eventPort".into(), Value::Integer(0u64.into()));
        }
        if let Some(streams) = dict.get("streams").and_then(Value::as_array) {
            let mut out = Vec::new();
            for stream in streams {
                let Some(stream) = stream.as_dictionary() else { continue };
                let kind = uint(stream, "type").unwrap_or(0);
                match kind {
                    110 => {
                        let Some((key, _)) = self.keys else {
                            control::log("Mirroring not initialized at SETUP");
                            response.disconnect = true;
                            continue;
                        };
                        let id = uint(stream, "streamConnectionID").unwrap_or(0);
                        self.mirror = None;
                        match MirrorStream::start(shared.config.mirror_port(), key, id, shared.media.clone(), || {}) {
                            Ok((mirror, port)) => {
                                self.mirror = Some(mirror);
                                let mut s = Dictionary::new();
                                s.insert("dataPort".into(), Value::Integer(u64::from(port).into()));
                                s.insert("type".into(), Value::Integer(110u64.into()));
                                out.push(Value::Dictionary(s));
                            }
                            Err(e) => {
                                control::log(&format!("Video-Port: {e}"));
                                response.disconnect = true;
                            }
                        }
                    }
                    96 => {
                        let Some((key, iv)) = self.keys else {
                            control::log("RAOP not initialized at SETUP");
                            response.disconnect = true;
                            continue;
                        };
                        let remote_control = uint(stream, "controlPort").unwrap_or(0) as u16;
                        let ct = uint(stream, "ct").unwrap_or(0) as u8;
                        self.audio = None;
                        match AudioStream::start(
                            self.remote,
                            remote_control,
                            shared.config.data_port(),
                            shared.config.control_port(),
                            key,
                            iv,
                            ct,
                            shared.media.clone(),
                        ) {
                            Ok(audio) => {
                                shared.media.set_airplay_volume(*shared.volume_db.lock().unwrap());
                                let mut s = Dictionary::new();
                                s.insert("dataPort".into(), Value::Integer(u64::from(audio.data_port).into()));
                                s.insert("controlPort".into(), Value::Integer(u64::from(audio.control_port).into()));
                                s.insert("type".into(), Value::Integer(96u64.into()));
                                out.push(Value::Dictionary(s));
                                self.audio = Some(audio);
                            }
                            Err(e) => {
                                control::log(&format!("Ton: {e}"));
                                response.disconnect = true;
                            }
                        }
                    }
                    other => {
                        control::log(&format!("SETUP für unbekannten Stromtyp {other}"));
                        response.disconnect = true;
                    }
                }
            }
            res.insert("streams".into(), Value::Array(out));
        }
        response.plist(&Value::Dictionary(res));
    }

    fn teardown(&mut self, shared: &Shared, request: &Request, response: &mut Response) {
        let dict = parse_plist(&request.body);
        let types: Vec<u64> = dict
            .get("streams")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|s| s.as_dictionary().and_then(|d| uint(d, "type"))).collect())
            .unwrap_or_default();
        response.header("Connection", "close");
        if types.contains(&96) {
            self.audio = None;
            shared.media.stop_audio();
        } else if types.contains(&110) {
            self.mirror = None;
            shared.media.stop_video();
        } else {
            self.audio = None;
            self.mirror = None;
            shared.media.stop_all();
        }
    }
}

fn finish(response: &mut Response, cseq: Option<u32>) {
    response.header("Server", &format!("AirTunes/{GLOBAL_VERSION}"));
    if let Some(cseq) = cseq {
        response.header("CSeq", &cseq.to_string());
    }
}

fn info(shared: &Shared, request: &Request, has_cseq: bool, response: &mut Response) {
    let content_type = request.header("Content-Type");
    let mut add_airplay = false;
    let mut add_raop = false;
    if content_type.is_some_and(|t| t.contains("application/x-apple-binary-plist")) {
        let dict = parse_plist(&request.body);
        if let Some(q) = dict.get("qualifier").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_string) {
            add_airplay = q == "txtAirPlay";
            add_raop = q == "txtRAOP";
        }
    }
    if !has_cseq {
        add_airplay = request.url.contains("txtAirPlay");
        add_raop = request.url.contains("txtRAOP");
    }
    let mut d = Dictionary::new();
    if add_airplay {
        d.insert("txtAirPlay".into(), Value::Data(Shared::txt_bytes(&shared.airplay_txt())));
    }
    if add_raop {
        d.insert("txtRAOP".into(), Value::Data(Shared::txt_bytes(&shared.raop_txt())));
    }
    if content_type.is_none() {
        let uint = |v: u64| Value::Integer(v.into());
        d.insert("deviceID".into(), Value::String(shared.device_id()));
        d.insert("macAddress".into(), Value::String(shared.device_id()));
        d.insert("pk".into(), Value::Data(shared.identity.public_key().to_vec()));
        d.insert("features".into(), uint(shared.features()));
        d.insert("name".into(), Value::String(shared.config.name.clone()));
        d.insert("pi".into(), Value::String(AIRPLAY_PI.into()));
        d.insert("vv".into(), uint(2));
        d.insert("statusFlags".into(), uint(68));
        d.insert("keepAliveLowPower".into(), Value::Boolean(true));
        d.insert("sourceVersion".into(), Value::String(GLOBAL_VERSION.into()));
        d.insert("keepAliveSendStatsAsBody".into(), Value::Boolean(true));
        d.insert("model".into(), Value::String(GLOBAL_MODEL.into()));
        if has_cseq {
            d.insert("initialVolume".into(), Value::Real(f64::from(*shared.volume_db.lock().unwrap())));
            let latency = |kind: u64, input_first: bool| {
                let mut l = Dictionary::new();
                if input_first {
                    l.insert("type".into(), uint(kind));
                    l.insert("inputLatencyMicros".into(), uint(0));
                    l.insert("audioType".into(), Value::String("default".into()));
                } else {
                    l.insert("type".into(), uint(kind));
                    l.insert("audioType".into(), Value::String("default".into()));
                    l.insert("inputLatencyMicros".into(), uint(0));
                }
                l.insert("outputLatencyMicros".into(), Value::Boolean(false));
                Value::Dictionary(l)
            };
            d.insert("audioLatencies".into(), Value::Array(vec![latency(100, true), latency(101, false)]));
            let format = |kind: u64| {
                let mut f = Dictionary::new();
                f.insert("audioOutputFormats".into(), uint(0x3ff_fffc));
                f.insert("type".into(), uint(kind));
                f.insert("audioInputFormats".into(), uint(0x3ff_fffc));
                Value::Dictionary(f)
            };
            d.insert("audioFormats".into(), Value::Array(vec![format(100), format(101)]));
            let c = &shared.config;
            let mut display = Dictionary::new();
            display.insert("uuid".into(), Value::String("e0ff8a27-6738-3d56-8a16-cc53aacee925".into()));
            display.insert("widthPhysical".into(), uint(0));
            display.insert("heightPhysical".into(), uint(0));
            display.insert("width".into(), uint(c.width.into()));
            display.insert("height".into(), uint(c.height.into()));
            display.insert("widthPixels".into(), uint(c.width.into()));
            display.insert("heightPixels".into(), uint(c.height.into()));
            display.insert("rotation".into(), Value::Boolean(false));
            display.insert("refreshRate".into(), Value::Real(1.0 / f64::from(c.refresh_rate.max(1))));
            display.insert("maxFPS".into(), uint(c.max_fps.into()));
            display.insert("overscanned".into(), Value::Boolean(false));
            display.insert("features".into(), uint(14));
            d.insert("displays".into(), Value::Array(vec![Value::Dictionary(display)]));
        }
    }
    response.plist(&Value::Dictionary(d));
}

fn get_parameter(shared: &Shared, request: &Request, response: &mut Response) {
    let Some(content_type) = request.header("Content-Type") else {
        response.set_status(451, "Parameter not understood");
        return;
    };
    if content_type == "text/parameters" && request.body.starts_with(b"volume\r\n") {
        let volume = *shared.volume_db.lock().unwrap();
        response.header("Content-Type", "text/parameters");
        response.body = format!("volume: {volume:9.6}\r\n").into_bytes();
    }
}

fn set_parameter(shared: &Shared, request: &Request, response: &mut Response) {
    let Some(content_type) = request.header("Content-Type") else {
        response.set_status(451, "Parameter not understood");
        return;
    };
    if content_type == "text/parameters" {
        let text = String::from_utf8_lossy(&request.body);
        if let Some(value) = text.strip_prefix("volume: ") {
            if let Ok(db) = value.trim().parse::<f32>() {
                let db = db.clamp(-144.0, 0.0);
                *shared.volume_db.lock().unwrap() = db;
                shared.media.set_airplay_volume(db);
            }
        }
    }
}
