// SPDX-License-Identifier: GPL-3.0-or-later
//! Eingebauter mDNS-Responder (ohne Bonjour): kündigt `_airplay._tcp` und
//! `_raop._tcp` auf allen echten IPv4-Adaptern an, jeweils mit deren Adresse.
use crate::net::{self, Interface};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MDNS_ADDR: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
const MDNS_PORT: u16 = 5353;
const TTL_SERVICE: u32 = 4500;
const TTL_HOST: u32 = 120;
const TYPE_A: u16 = 1;
const TYPE_PTR: u16 = 12;
const TYPE_TXT: u16 = 16;
const TYPE_AAAA: u16 = 28;
const TYPE_SRV: u16 = 33;
const TYPE_ANY: u16 = 255;
const CLASS_IN: u16 = 1;
const CACHE_FLUSH: u16 = 0x8000;
const MAX_PACKET: usize = 1500;
const COMBINED_MAX: usize = 1200;

#[derive(Clone)]
pub struct Service {
    /// Instanzname als einzelnes Label (z. B. „AABB…@Wohnzimmer“).
    pub instance: String,
    /// Diensttyp, z. B. `_raop._tcp.local`.
    pub kind: &'static str,
    pub port: u16,
    pub txt: Vec<(String, String)>,
}

impl Service {
    fn instance_labels(&self) -> Vec<String> {
        let mut labels = vec![self.instance.clone()];
        labels.extend(self.kind.split('.').map(str::to_owned));
        labels
    }

    pub fn txt_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for (key, value) in &self.txt {
            let item = format!("{key}={value}");
            let bytes = &item.as_bytes()[..item.len().min(255)];
            out.push(bytes.len() as u8);
            out.extend_from_slice(bytes);
        }
        out
    }
}

struct Packet(Vec<u8>, u16);

impl Packet {
    fn response() -> Self {
        // ID 0, Flags: Antwort + autoritativ, keine Fragen
        Self(vec![0, 0, 0x84, 0x00, 0, 0, 0, 0, 0, 0, 0, 0], 0)
    }
    fn name(&mut self, labels: &[String]) {
        for label in labels {
            let bytes = &label.as_bytes()[..label.len().min(63)];
            self.0.push(bytes.len() as u8);
            self.0.extend_from_slice(bytes);
        }
        self.0.push(0);
    }
    fn record(&mut self, labels: &[String], kind: u16, class: u16, ttl: u32, data: &[u8]) {
        self.name(labels);
        self.0.extend_from_slice(&kind.to_be_bytes());
        self.0.extend_from_slice(&class.to_be_bytes());
        self.0.extend_from_slice(&ttl.to_be_bytes());
        self.0.extend_from_slice(&(data.len() as u16).to_be_bytes());
        self.0.extend_from_slice(data);
        self.1 += 1;
    }
    fn finish(mut self) -> Vec<u8> {
        self.0[6..8].copy_from_slice(&self.1.to_be_bytes());
        self.0
    }
}

fn labels(name: &str) -> Vec<String> {
    name.split('.').filter(|l| !l.is_empty()).map(str::to_owned).collect()
}

fn encode_name(labels: &[String]) -> Vec<u8> {
    let mut p = Packet(Vec::new(), 0);
    p.name(labels);
    p.0
}

struct Records {
    services: Vec<Service>,
    host: Vec<String>,
}

impl Records {
    fn add_service(&self, packet: &mut Packet, service: &Service, ttl: u32) {
        let instance = service.instance_labels();
        packet.record(&labels("_services._dns-sd._udp.local"), TYPE_PTR, CLASS_IN, ttl, &encode_name(&labels(service.kind)));
        packet.record(&labels(service.kind), TYPE_PTR, CLASS_IN, ttl, &encode_name(&instance));
        let mut srv = vec![0, 0, 0, 0];
        srv.extend_from_slice(&service.port.to_be_bytes());
        srv.extend_from_slice(&encode_name(&self.host));
        packet.record(&instance, TYPE_SRV, CLASS_IN | CACHE_FLUSH, if ttl == 0 { 0 } else { TTL_HOST }, &srv);
        packet.record(&instance, TYPE_TXT, CLASS_IN | CACHE_FLUSH, ttl, &service.txt_bytes());
    }

    fn add_host(&self, packet: &mut Packet, address: Ipv4Addr, ttl: u32) {
        packet.record(&self.host, TYPE_A, CLASS_IN | CACHE_FLUSH, if ttl == 0 { 0 } else { TTL_HOST }, &address.octets());
    }

    /// Antwortpakete: kombiniert, wenn klein genug, sonst je Dienst.
    fn build(&self, wanted: &[usize], host: bool, address: Ipv4Addr, ttl: u32) -> Vec<Vec<u8>> {
        let mut combined = Packet::response();
        for &i in wanted {
            self.add_service(&mut combined, &self.services[i], ttl);
        }
        if host {
            self.add_host(&mut combined, address, ttl);
        }
        if combined.1 == 0 {
            return Vec::new();
        }
        let combined = combined.finish();
        if combined.len() <= COMBINED_MAX || wanted.len() <= 1 {
            return vec![combined];
        }
        wanted
            .iter()
            .map(|&i| {
                let mut p = Packet::response();
                self.add_service(&mut p, &self.services[i], ttl);
                if host {
                    self.add_host(&mut p, address, ttl);
                }
                p.finish()
            })
            .collect()
    }
}

pub struct Responder {
    stop: Arc<AtomicBool>,
    socket: Arc<Socket>,
    records: Arc<Mutex<Records>>,
    interfaces: Vec<Interface>,
    thread: Option<std::thread::JoinHandle<()>>,
}

fn read_name(packet: &[u8], offset: &mut usize) -> Option<String> {
    let mut labels = Vec::new();
    let mut pos = *offset;
    let mut jumped = false;
    let mut jumps = 0;
    loop {
        let len = *packet.get(pos)? as usize;
        pos += 1;
        if len == 0 {
            if !jumped {
                *offset = pos;
            }
            return Some(labels.join("."));
        }
        if len & 0xc0 == 0xc0 {
            let next = *packet.get(pos)? as usize;
            jumps += 1;
            if jumps > 16 {
                return None;
            }
            if !jumped {
                *offset = pos + 1;
            }
            pos = ((len & 0x3f) << 8) | next;
            jumped = true;
            continue;
        }
        if len & 0xc0 != 0 {
            return None;
        }
        let label = packet.get(pos..pos + len)?;
        labels.push(String::from_utf8_lossy(label).into_owned());
        pos += len;
        if labels.len() > 64 {
            return None;
        }
    }
}

fn type_matches(query: u16, record: u16) -> bool {
    query == record || query == TYPE_ANY
}

impl Responder {
    pub fn start(host: String, services: Vec<Service>) -> Result<Self, String> {
        let interfaces = net::ipv4_interfaces();
        for i in &interfaces {
            crate::control::log(&format!("mDNS interface {}", i.address));
        }
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).map_err(|e| e.to_string())?;
        socket.set_reuse_address(true).map_err(|e| e.to_string())?;
        socket.set_multicast_ttl_v4(255).ok();
        socket.set_multicast_loop_v4(true).ok();
        socket
            .bind(&SockAddr::from(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, MDNS_PORT)))
            .map_err(|e| format!("UDP 5353: {e}"))?;
        let mut joined = 0;
        for i in &interfaces {
            if socket.join_multicast_v4(&MDNS_ADDR, &i.address).is_ok() {
                joined += 1;
            }
        }
        if joined == 0 {
            socket.join_multicast_v4(&MDNS_ADDR, &Ipv4Addr::UNSPECIFIED).map_err(|e| e.to_string())?;
        }
        socket.set_read_timeout(Some(Duration::from_millis(250))).ok();
        let socket = Arc::new(socket);
        let records = Arc::new(Mutex::new(Records { services, host: labels(&host) }));
        let stop = Arc::new(AtomicBool::new(false));
        let mut responder = Self { stop: stop.clone(), socket: socket.clone(), records: records.clone(), interfaces: interfaces.clone(), thread: None };

        let thread_interfaces = interfaces.clone();
        responder.thread = Some(std::thread::spawn(move || {
            let mut buffer = [std::mem::MaybeUninit::<u8>::uninit(); MAX_PACKET];
            let announce_at = [0u64, 1, 3];
            let started = std::time::Instant::now();
            let mut announced = 0;
            while !stop.load(Ordering::SeqCst) {
                if announced < announce_at.len() && started.elapsed().as_secs() >= announce_at[announced] {
                    announce(&socket, &records, &thread_interfaces, TTL_SERVICE);
                    announced += 1;
                }
                let Ok((len, from)) = socket.recv_from(&mut buffer) else { continue };
                let data: Vec<u8> = buffer[..len].iter().map(|b| unsafe { b.assume_init() }).collect();
                let Some(from) = from.as_socket_ipv4() else { continue };
                handle_query(&socket, &records, &thread_interfaces, &data, from);
            }
        }));
        Ok(responder)
    }
}

fn send_multicast(socket: &Socket, interface: Ipv4Addr, packet: &[u8]) {
    let _ = socket.set_multicast_if_v4(&interface);
    let _ = socket.send_to(packet, &SockAddr::from(SocketAddrV4::new(MDNS_ADDR, MDNS_PORT)));
}

fn announce(socket: &Socket, records: &Mutex<Records>, interfaces: &[Interface], ttl: u32) {
    let records = records.lock().unwrap();
    let all: Vec<usize> = (0..records.services.len()).collect();
    for iface in interfaces {
        for packet in records.build(&all, true, iface.address, ttl) {
            send_multicast(socket, iface.address, &packet);
        }
    }
}

fn handle_query(socket: &Socket, records: &Mutex<Records>, interfaces: &[Interface], data: &[u8], from: SocketAddrV4) {
    if data.len() < 12 || data[2] & 0x80 != 0 {
        return; // keine Anfrage
    }
    let questions = u16::from_be_bytes([data[4], data[5]]);
    let records = records.lock().unwrap();
    let host_name = records.host.join(".");
    let mut wanted: Vec<usize> = Vec::new();
    let mut host = false;
    let mut unicast = false;
    let mut offset = 12;
    for _ in 0..questions.min(32) {
        let Some(name) = read_name(data, &mut offset) else { break };
        if offset + 4 > data.len() {
            break;
        }
        let kind = u16::from_be_bytes([data[offset], data[offset + 1]]);
        let class = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);
        offset += 4;
        if class & 0x8000 != 0 {
            unicast = true;
        }
        if name.eq_ignore_ascii_case("_services._dns-sd._udp.local") && type_matches(kind, TYPE_PTR) {
            for i in 0..records.services.len() {
                if !wanted.contains(&i) {
                    wanted.push(i);
                }
            }
            continue;
        }
        for (i, service) in records.services.iter().enumerate() {
            let instance = service.instance_labels().join(".");
            if (name.eq_ignore_ascii_case(service.kind) && type_matches(kind, TYPE_PTR))
                || (name.eq_ignore_ascii_case(&instance) && (type_matches(kind, TYPE_TXT) || type_matches(kind, TYPE_SRV)))
            {
                if !wanted.contains(&i) {
                    wanted.push(i);
                }
                host = true;
            }
        }
        if name.eq_ignore_ascii_case(&host_name) && (type_matches(kind, TYPE_A) || type_matches(kind, TYPE_AAAA)) {
            host = true;
        }
    }
    if wanted.is_empty() && !host {
        return;
    }
    // Multicast auf jeder Schnittstelle mit deren eigener Adresse
    for iface in interfaces {
        for packet in records.build(&wanted, host, iface.address, TTL_SERVICE) {
            send_multicast(socket, iface.address, &packet);
        }
    }
    // Unicast an den Fragenden (QU-Bit oder Altanfrage von anderem Port)
    if unicast || from.port() != MDNS_PORT {
        let address = interfaces
            .iter()
            .find(|i| i.contains(*from.ip()))
            .or(interfaces.first())
            .map_or(Ipv4Addr::UNSPECIFIED, |i| i.address);
        for packet in records.build(&wanted, host, address, TTL_SERVICE) {
            let _ = socket.send_to(&packet, &SockAddr::from(SocketAddr::V4(from)));
        }
    }
}

impl Drop for Responder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        announce(&self.socket, &self.records, &self.interfaces, 0); // Abmeldung
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_parsable_records() {
        let records = Records {
            services: vec![Service {
                instance: "AABBCCDDEEFF@Wohn.zimmer".into(),
                kind: "_raop._tcp.local",
                port: 48001,
                txt: vec![("ch".into(), "2".into())],
            }],
            host: labels("PC.local"),
        };
        let packets = records.build(&[0], true, Ipv4Addr::new(192, 168, 1, 5), TTL_SERVICE);
        assert_eq!(packets.len(), 1);
        let p = &packets[0];
        assert_eq!(u16::from_be_bytes([p[6], p[7]]), 5); // 4 Dienst-Records + A
        // Instanzname mit Punkt bleibt ein Label
        let mut offset = 12;
        let first = read_name(p, &mut offset).unwrap();
        assert_eq!(first, "_services._dns-sd._udp.local");
        assert!(p.windows(4).any(|w| w == [192, 168, 1, 5]));
    }
}
