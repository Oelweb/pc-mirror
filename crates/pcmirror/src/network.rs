// SPDX-License-Identifier: GPL-3.0-or-later
//! Aktive IPv4-Adapter mit Anzeigenamen (GetAdaptersAddresses).
use windows::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST,
    IP_ADAPTER_ADDRESSES_LH,
};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};

const LOOPBACK: u32 = 24; // IF_TYPE_SOFTWARE_LOOPBACK

pub fn ipv4_adapters() -> Vec<(String, String)> {
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    let mut size: u32 = 16 * 1024;
    let mut buffer: Vec<u64> = Vec::new();
    for _ in 0..3 {
        buffer = vec![0u64; (size as usize).div_ceil(8)];
        let result = unsafe {
            GetAdaptersAddresses(
                AF_INET.0 as u32,
                flags,
                None,
                Some(buffer.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH),
                &mut size,
            )
        };
        match result {
            0 => return collect(buffer.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH),
            111 => continue, // ERROR_BUFFER_OVERFLOW: size wurde aktualisiert
            _ => return Vec::new(),
        }
    }
    let _ = buffer;
    Vec::new()
}

fn collect(mut adapter: *const IP_ADAPTER_ADDRESSES_LH) -> Vec<(String, String)> {
    let mut found = Vec::new();
    unsafe {
        while let Some(current) = adapter.as_ref() {
            if current.OperStatus == IfOperStatusUp && current.IfType != LOOPBACK {
                let name = current.FriendlyName.to_string().unwrap_or_default();
                let mut address = current.FirstUnicastAddress;
                while let Some(unicast) = address.as_ref() {
                    let socket = unicast.Address.lpSockaddr;
                    if !socket.is_null() && (*socket).sa_family == AF_INET {
                        let ipv4 = &*(socket as *const SOCKADDR_IN);
                        let octets = ipv4.sin_addr.S_un.S_un_b;
                        found.push((
                            name.clone(),
                            format!(
                                "{}.{}.{}.{}",
                                octets.s_b1, octets.s_b2, octets.s_b3, octets.s_b4
                            ),
                        ));
                    }
                    address = unicast.Next;
                }
            }
            adapter = current.Next;
        }
    }
    found
}
