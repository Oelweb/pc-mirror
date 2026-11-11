// SPDX-License-Identifier: GPL-3.0-or-later
//! RTSP/HTTP-Anfragen lesen und Antworten schreiben (Ersatz für llhttp und
//! `http_request.c`/`http_response.c`), mit festen Grenzen gegen Missbrauch.
use std::io::{BufRead, BufReader, Read};

const MAX_HEADER_LINE: usize = 8 * 1024;
const MAX_HEADERS: usize = 20;
const MAX_FIELD_NAME: usize = 64;
const MAX_FIELD_VALUE: usize = 1024;
const MAX_BODY: usize = 1024 * 1024;

#[derive(Debug, Default, Clone)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub protocol: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

pub enum ReadError {
    /// Verbindung sauber beendet.
    Closed,
    /// Header zu groß (Antwort 431, danach schließen).
    TooLarge(String),
    /// Ungültige Anfrage oder E/A-Fehler.
    Invalid,
}

fn read_line<R: BufRead>(reader: &mut R) -> Result<Option<String>, ReadError> {
    let mut line = Vec::new();
    let read = reader
        .by_ref()
        .take(MAX_HEADER_LINE as u64 + 2)
        .read_until(b'\n', &mut line)
        .map_err(|_| ReadError::Invalid)?;
    if read == 0 {
        return Ok(None);
    }
    if !line.ends_with(b"\n") {
        return Err(ReadError::Invalid);
    }
    while matches!(line.last(), Some(b'\n' | b'\r')) {
        line.pop();
    }
    Ok(Some(String::from_utf8_lossy(&line).into_owned()))
}

/// Liest eine vollständige Anfrage.
pub fn read_request<R: Read>(reader: &mut BufReader<R>) -> Result<Request, ReadError> {
    let mut first = None;
    while first.is_none() {
        match read_line(reader)? {
            None => return Err(ReadError::Closed),
            Some(line) if line.is_empty() => continue, // Leerzeilen zwischen Anfragen
            Some(line) => first = Some(line),
        }
    }
    let first = first.unwrap();
    let mut parts = first.splitn(3, ' ');
    let (Some(method), Some(url), Some(protocol)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(ReadError::Invalid);
    };
    let mut request = Request {
        method: method.to_owned(),
        url: url.to_owned(),
        protocol: protocol.trim().to_owned(),
        ..Default::default()
    };
    if !matches!(request.protocol.as_str(), "RTSP/1.0" | "HTTP/1.1" | "HTTP/1.0") {
        return Err(ReadError::Invalid);
    }
    let mut too_large = false;
    loop {
        let Some(line) = read_line(reader)? else { return Err(ReadError::Invalid) };
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else { continue };
        let (name, value) = (name.trim().to_owned(), value.trim().to_owned());
        if request.headers.len() >= MAX_HEADERS || name.len() > MAX_FIELD_NAME || value.len() > MAX_FIELD_VALUE {
            too_large = true;
            continue;
        }
        request.headers.push((name, value));
    }
    if too_large {
        return Err(ReadError::TooLarge(request.protocol));
    }
    let length = match request.header("Content-Length") {
        Some(v) => v.parse::<usize>().map_err(|_| ReadError::Invalid)?,
        None => 0,
    };
    if length > MAX_BODY {
        return Err(ReadError::Invalid);
    }
    request.body = vec![0; length];
    reader.read_exact(&mut request.body).map_err(|_| ReadError::Invalid)?;
    Ok(request)
}

#[derive(Debug, Clone)]
pub struct Response {
    pub protocol: String,
    pub code: u16,
    pub message: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub disconnect: bool,
}

impl Response {
    pub fn new(protocol: &str, code: u16, message: &str) -> Self {
        Self {
            protocol: protocol.to_owned(),
            code,
            message: message.to_owned(),
            headers: Vec::new(),
            body: Vec::new(),
            disconnect: false,
        }
    }

    pub fn set_status(&mut self, code: u16, message: &str) {
        self.code = code;
        self.message = message.to_owned();
        self.headers.clear();
    }

    pub fn header(&mut self, name: &str, value: &str) {
        self.headers.push((name.to_owned(), value.to_owned()));
    }

    pub fn plist(&mut self, value: &plist::Value) {
        let mut body = Vec::new();
        if plist::to_writer_binary(&mut body, value).is_ok() {
            self.header("Content-Type", "application/x-apple-binary-plist");
            self.body = body;
        }
    }

    pub fn octets(&mut self, data: Vec<u8>) {
        self.header("Content-Type", "application/octet-stream");
        self.body = data;
    }

    /// Wie `http_response_finish`: Content-Length nur mit Daten oder Content-Type.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = format!("{} {} {}\r\n", self.protocol, self.code, self.message);
        for (name, value) in &self.headers {
            out.push_str(&format!("{name}: {value}\r\n"));
        }
        let mut bytes;
        if !self.body.is_empty() {
            out.push_str(&format!("Content-Length: {}\r\n\r\n", self.body.len()));
            bytes = out.into_bytes();
            bytes.extend_from_slice(&self.body);
        } else {
            if self.headers.iter().any(|(n, _)| n == "Content-Type") {
                out.push_str("Content-Length: 0\r\n");
            }
            out.push_str("\r\n");
            bytes = out.into_bytes();
        }
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_request_with_body() {
        let raw = b"POST /pair-setup RTSP/1.0\r\nCSeq: 3\r\nContent-Length: 4\r\nContent-Type: application/octet-stream\r\n\r\nabcd";
        let mut reader = BufReader::new(&raw[..]);
        let r = read_request(&mut reader).ok().unwrap();
        assert_eq!((r.method.as_str(), r.url.as_str(), r.protocol.as_str()), ("POST", "/pair-setup", "RTSP/1.0"));
        assert_eq!(r.header("cseq"), Some("3"));
        assert_eq!(r.body, b"abcd");
    }

    #[test]
    fn rejects_oversized_headers_and_bodies() {
        let mut raw = b"GET /info RTSP/1.0\r\n".to_vec();
        for i in 0..25 {
            raw.extend_from_slice(format!("X-{i}: v\r\n").as_bytes());
        }
        raw.extend_from_slice(b"\r\n");
        assert!(matches!(read_request(&mut BufReader::new(&raw[..])), Err(ReadError::TooLarge(_))));
        let raw = b"POST /x RTSP/1.0\r\nContent-Length: 99999999\r\n\r\n";
        assert!(matches!(read_request(&mut BufReader::new(&raw[..])), Err(ReadError::Invalid)));
    }

    #[test]
    fn response_format_matches_c() {
        let mut r = Response::new("RTSP/1.0", 200, "OK");
        r.header("CSeq", "1");
        assert_eq!(r.to_bytes(), b"RTSP/1.0 200 OK\r\nCSeq: 1\r\n\r\n");
        r.octets(vec![1, 2]);
        assert_eq!(
            r.to_bytes(),
            b"RTSP/1.0 200 OK\r\nCSeq: 1\r\nContent-Type: application/octet-stream\r\nContent-Length: 2\r\n\r\n\x01\x02"
        );
    }
}
