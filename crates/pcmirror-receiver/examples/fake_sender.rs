// SPDX-License-Identifier: GPL-3.0-or-later
//! Nachgebauter AirPlay-Sender für Tests ohne iPhone: GET /info, Pair-Setup,
//! Pair-Verify, FairPlay-Setup, SETUP (Schlüssel + Videostrom), dann echtes,
//! verschlüsseltes H.264/H.265 über den Videoport, zum Schluss TEARDOWN.
//!
//! Aufruf: fake_sender <host> <raop-port> <clip.264|clip.265> [mode]
//! Gibt die /info-Antwort als XML-Plist und den Ablauf aus; Exit-Code 0 bei Erfolg.
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use pcmirror_receiver::crypto::{derive16, random_bytes, sha512, CtrStream};
use pcmirror_receiver::fairplay::FairPlay;
use pcmirror_receiver::mirror::stream_keys;
use plist::{Dictionary, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, UdpSocket};
use x25519_dalek::{PublicKey, StaticSecret};

struct Rtsp {
    stream: BufReader<TcpStream>,
    cseq: u32,
}

impl Rtsp {
    fn request(&mut self, method: &str, url: &str, content_type: Option<&str>, body: &[u8]) -> (u16, Vec<(String, String)>, Vec<u8>) {
        self.cseq += 1;
        let mut head = format!("{method} {url} RTSP/1.0\r\nCSeq: {}\r\nUser-Agent: AirPlay/690.7.1\r\n", self.cseq);
        if let Some(ct) = content_type {
            head.push_str(&format!("Content-Type: {ct}\r\n"));
        }
        if !body.is_empty() {
            head.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }
        head.push_str("\r\n");
        let socket = self.stream.get_mut();
        socket.write_all(head.as_bytes()).unwrap();
        socket.write_all(body).unwrap();
        let mut status = String::new();
        self.stream.read_line(&mut status).unwrap();
        let code: u16 = status.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
        let mut headers = Vec::new();
        let mut length = 0;
        loop {
            let mut line = String::new();
            self.stream.read_line(&mut line).unwrap();
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some((k, v)) = line.split_once(": ") {
                if k.eq_ignore_ascii_case("Content-Length") {
                    length = v.parse().unwrap();
                }
                headers.push((k.to_owned(), v.to_owned()));
            }
        }
        let mut body = vec![0; length];
        self.stream.read_exact(&mut body).unwrap();
        (code, headers, body)
    }
}

fn plist_bytes(d: Dictionary) -> Vec<u8> {
    let mut out = Vec::new();
    plist::to_writer_binary(&mut out, &Value::Dictionary(d)).unwrap();
    out
}

/// Annex-B in Zugriffseinheiten (AUs) zerlegen; Parameter-Sätze getrennt.
fn split_nals(data: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            starts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut nals = Vec::new();
    for (n, &s) in starts.iter().enumerate() {
        let mut end = starts.get(n + 1).map_or(data.len(), |&next| next - 3);
        while end > s && data[end - 1] == 0 {
            end -= 1;
        }
        nals.push(&data[s..end]);
    }
    nals
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let host = args.get(1).cloned().unwrap_or_else(|| "127.0.0.1".into());
    let port: u16 = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(48101);
    let clip = std::fs::read(args.get(3).expect("Clip fehlt")).unwrap();
    let mode: u8 = args.get(4).and_then(|m| m.parse().ok()).unwrap_or(0);
    let h265 = args[3].ends_with(".265");

    let socket = TcpStream::connect((host.as_str(), port)).expect("Verbindung");
    let mut rtsp = Rtsp { stream: BufReader::new(socket), cseq: 0 };

    // 1. /info
    let (code, _, body) = rtsp.request("GET", "/info", None, &[]);
    assert_eq!(code, 200, "/info");
    let info: Value = plist::from_bytes(&body).unwrap();
    let mut xml = Vec::new();
    plist::to_writer_xml(&mut xml, &info).unwrap();
    println!("INFO-BEGIN\n{}\nINFO-END", String::from_utf8_lossy(&xml));

    // 2. Pair-Setup
    let client_ed = SigningKey::from_bytes(&random_bytes::<32>());
    let (code, _, server_ed) = rtsp.request("POST", "/pair-setup", Some("application/octet-stream"), &client_ed.verifying_key().to_bytes());
    assert_eq!((code, server_ed.len()), (200, 32), "pair-setup");

    // 3. Pair-Verify
    let client_x = StaticSecret::random_from_rng(rand_core::OsRng);
    let client_x_pub = PublicKey::from(&client_x).to_bytes();
    let mut verify1 = vec![1, 0, 0, 0];
    verify1.extend_from_slice(&client_x_pub);
    verify1.extend_from_slice(&client_ed.verifying_key().to_bytes());
    let (code, _, reply) = rtsp.request("POST", "/pair-verify", Some("application/octet-stream"), &verify1);
    assert_eq!((code, reply.len()), (200, 96), "pair-verify 1");
    let server_x: [u8; 32] = reply[..32].try_into().unwrap();
    let secret = client_x.diffie_hellman(&PublicKey::from(server_x)).to_bytes();
    let mut stream = CtrStream::new(&derive16(b"Pair-Verify-AES-Key", &secret), &derive16(b"Pair-Verify-AES-IV", &secret));
    let mut server_sig: [u8; 64] = reply[32..].try_into().unwrap();
    stream.apply(&mut server_sig);
    let mut message = [0u8; 64];
    message[..32].copy_from_slice(&server_x);
    message[32..].copy_from_slice(&client_x_pub);
    VerifyingKey::from_bytes(&server_ed.try_into().unwrap())
        .unwrap()
        .verify(&message, &Signature::from_bytes(&server_sig))
        .expect("Server-Signatur");
    message[..32].copy_from_slice(&client_x_pub);
    message[32..].copy_from_slice(&server_x);
    let mut client_sig = client_ed.sign(&message).to_bytes();
    stream.apply(&mut client_sig);
    let mut verify2 = vec![0, 0, 0, 0];
    verify2.extend_from_slice(&client_sig);
    let (code, _, _) = rtsp.request("POST", "/pair-verify", Some("application/octet-stream"), &verify2);
    assert_eq!(code, 200, "pair-verify 2");
    println!("PASS Pairing");

    // 4. FairPlay
    let setup1 = [0x46, 0x50, 0x4c, 0x59, 0x03, 0x01, 0x01, 0, 0, 0, 0, 0x04, 0x02, 0x00, mode, 0xbb];
    let (code, _, reply) = rtsp.request("POST", "/fp-setup", Some("application/octet-stream"), &setup1);
    assert_eq!((code, reply.len()), (200, 142), "fp-setup 1");
    let mut keymsg = random_bytes::<164>();
    keymsg[..12].copy_from_slice(&[0x46, 0x50, 0x4c, 0x59, 0x03, 0x01, 0x03, 0, 0, 0, 0, 0x98]);
    keymsg[12] = mode;
    let (code, _, reply) = rtsp.request("POST", "/fp-setup", Some("application/octet-stream"), &keymsg);
    assert_eq!((code, reply.len()), (200, 32), "fp-setup 2");
    assert_eq!(&reply[12..], &keymsg[144..]);
    println!("PASS FairPlay");

    // Der Sender kennt den AES-Schlüssel: gleiche Entschlüsselung wie der Empfänger.
    let ekey = random_bytes::<72>();
    let eiv = random_bytes::<16>();
    let mut fp = FairPlay::default();
    fp.setup(&setup1).unwrap();
    fp.handshake(&keymsg).unwrap();
    let raw_key = fp.decrypt(&ekey).unwrap();
    let aes_key: [u8; 16] = sha512(&[&raw_key, &secret])[..16].try_into().unwrap();

    // 5. SETUP (Schlüssel und Zeitabgleich)
    let timing = UdpSocket::bind("0.0.0.0:0").unwrap();
    let mut d = Dictionary::new();
    d.insert("ekey".into(), Value::Data(ekey.to_vec()));
    d.insert("eiv".into(), Value::Data(eiv.to_vec()));
    d.insert("deviceID".into(), Value::String("AA:BB:CC:DD:EE:FF".into()));
    d.insert("model".into(), Value::String("iPhone15,2".into()));
    d.insert("name".into(), Value::String("Test-iPhone".into()));
    d.insert("timingProtocol".into(), Value::String("NTP".into()));
    d.insert("timingPort".into(), Value::Integer(u64::from(timing.local_addr().unwrap().port()).into()));
    let (code, _, body) = rtsp.request("SETUP", "rtsp://x/1", Some("application/x-apple-binary-plist"), &plist_bytes(d));
    assert_eq!(code, 200, "SETUP 1");
    let reply: Dictionary = plist::from_bytes(&body).unwrap();
    assert!(reply.get("timingPort").is_some(), "timingPort fehlt");

    // 6. SETUP Videostrom
    let stream_id: u64 = 9_876_543_210;
    let mut s = Dictionary::new();
    s.insert("type".into(), Value::Integer(110u64.into()));
    s.insert("streamConnectionID".into(), Value::Integer(stream_id.into()));
    let mut d = Dictionary::new();
    d.insert("streams".into(), Value::Array(vec![Value::Dictionary(s)]));
    let (code, _, body) = rtsp.request("SETUP", "rtsp://x/1", Some("application/x-apple-binary-plist"), &plist_bytes(d));
    assert_eq!(code, 200, "SETUP 2");
    let reply: Dictionary = plist::from_bytes(&body).unwrap();
    let streams = reply.get("streams").and_then(Value::as_array).expect("streams");
    let data_port = streams[0].as_dictionary().unwrap().get("dataPort").unwrap().as_unsigned_integer().unwrap() as u16;
    let (code, _, _) = rtsp.request("RECORD", "rtsp://x/1", None, &[]);
    assert_eq!(code, 200, "RECORD");
    println!("PASS SETUP (Videoport {data_port})");

    // 7. Video senden
    let mut video = TcpStream::connect((host.as_str(), data_port)).expect("Videoport");
    let (key, iv) = stream_keys(&aes_key, stream_id);
    let mut cipher = CtrStream::new(&key, &iv);
    let nals = split_nals(&clip);
    let nal_type = |n: &[u8]| if h265 { (n[0] >> 1) & 0x3f } else { n[0] & 0x1f };
    let is_param = |t: u8| if h265 { matches!(t, 32..=34) } else { matches!(t, 7 | 8) };
    let is_vcl = |t: u8| if h265 { t < 32 } else { matches!(t, 1 | 5) };
    let mut packet = |kind: u8, option: u8, ts: u64, payload: &[u8], idr: bool| {
        let mut header = [0u8; 128];
        header[0..4].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        header[4] = kind;
        header[5] = if idr { 0x10 } else { 0 };
        header[6] = option;
        header[7] = if kind == 1 { 1 } else { 0 };
        header[8..16].copy_from_slice(&ts.to_le_bytes());
        header[16..20].copy_from_slice(&320f32.to_le_bytes());
        header[20..24].copy_from_slice(&240f32.to_le_bytes());
        header[56..60].copy_from_slice(&320f32.to_le_bytes());
        header[60..64].copy_from_slice(&240f32.to_le_bytes());
        video.write_all(&header).unwrap();
        video.write_all(payload).unwrap();
    };
    let params: Vec<&[u8]> = nals.iter().copied().filter(|n| is_param(nal_type(n))).collect();
    let config = if h265 {
        let mut p = vec![0u8; 0x75];
        p[4..8].copy_from_slice(b"hvc1");
        for (marker, unit) in [0xa0u8, 0xa1, 0xa2].iter().zip(params.iter()) {
            p.extend_from_slice(&[*marker, 0x00, 0x01]);
            p.extend_from_slice(&(unit.len() as u16).to_be_bytes());
            p.extend_from_slice(unit);
        }
        p
    } else {
        let (sps, pps) = (params[0], params[1]);
        let mut p = vec![1, sps[1], sps[2], sps[3], 0xff, 0xe1];
        p.extend_from_slice(&(sps.len() as u16).to_be_bytes());
        p.extend_from_slice(sps);
        p.push(1);
        p.extend_from_slice(&(pps.len() as u16).to_be_bytes());
        p.extend_from_slice(pps);
        p
    };
    let mut ts: u64 = 1_000_000;
    packet(1, if h265 { 0x1e } else { 0x16 }, ts, &config, false);
    let mut frames = 0;
    for nal in nals.iter().filter(|n| is_vcl(nal_type(n))) {
        let mut payload = (nal.len() as u32).to_be_bytes().to_vec();
        payload.extend_from_slice(nal);
        cipher.apply(&mut payload);
        let idr = if h265 { matches!(nal_type(nal), 19 | 20) } else { nal_type(nal) == 5 };
        packet(0, 0, ts, &payload, idr);
        ts += 33_333_333;
        frames += 1;
        std::thread::sleep(std::time::Duration::from_millis(33));
    }
    println!("PASS {frames} Bilder gesendet");
    std::thread::sleep(std::time::Duration::from_millis(1500));

    // 8. TEARDOWN
    let mut s = Dictionary::new();
    s.insert("type".into(), Value::Integer(110u64.into()));
    let mut d = Dictionary::new();
    d.insert("streams".into(), Value::Array(vec![Value::Dictionary(s)]));
    let (code, _, _) = rtsp.request("TEARDOWN", "rtsp://x/1", Some("application/x-apple-binary-plist"), &plist_bytes(d));
    assert_eq!(code, 200, "TEARDOWN");
    println!("PASS TEARDOWN");
    std::thread::sleep(std::time::Duration::from_millis(500));
}
