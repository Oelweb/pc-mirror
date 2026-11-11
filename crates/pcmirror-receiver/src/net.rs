// SPDX-License-Identifier: GPL-3.0-or-later
//! Netzwerkadapter: IPv4-Schnittstellen für mDNS und die Hardware-Adresse als Geräte-ID.
use std::net::Ipv4Addr;
use windows::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GAA_FLAG_INCLUDE_GATEWAYS, IF_TYPE_ETHERNET_CSMACD, IF_TYPE_IEEE80211,
    IF_TYPE_PPP, IF_TYPE_SOFTWARE_LOOPBACK, IF_TYPE_TUNNEL, IP_ADAPTER_ADDRESSES_LH,
    IP_ADAPTER_NO_MULTICAST,
};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Networking::WinSock::{AF_INET, AF_UNSPEC, SOCKADDR_IN};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interface {
    pub address: Ipv4Addr,
    pub mask: Ipv4Addr,
}

impl Interface {
    pub fn contains(&self, other: Ipv4Addr) -> bool {
        let mask = u32::from(self.mask);
        mask != 0 && (u32::from(self.address) & mask) == (u32::from(other) & mask)
    }
}

struct Adapter {
    description: String,
    friendly: String,
    if_type: u32,
    up: bool,
    multicast: bool,
    gateway: bool,
    ipv4: Option<(Ipv4Addr, u8)>,
    mac: Option<[u8; 6]>,
}

/// VPN- und virtuelle Adapter (Radmin, Hamachi, ZeroTier, Tailscale, WireGuard, Hyper-V …).
pub fn is_virtual(description: &str, friendly: &str, if_type: u32) -> bool {
    const WORDS: [&str; 20] = [
        "Hyper-V", "Virtual", "virtual", "WSL", "vEthernet", "VPN", "TAP-", "Tunnel", "WireGuard", "Wintun",
        "Tailscale", "ZeroTier", "Hamachi", "Radmin", "OpenVPN", "Npcap", "Loopback", "Bluetooth", "VMware",
        "VirtualBox",
    ];
    if_type == IF_TYPE_SOFTWARE_LOOPBACK
        || if_type == IF_TYPE_TUNNEL
        || if_type == IF_TYPE_PPP
        || WORDS.iter().any(|w| description.contains(w) || friendly.contains(w))
}

fn adapters() -> Vec<Adapter> {
    let mut out = Vec::new();
    unsafe {
        let mut size: u32 = 32 * 1024;
        let mut buffer: Vec<u64> = Vec::new();
        for _ in 0..3 {
            buffer = vec![0u64; size as usize / 8 + 1];
            let result = GetAdaptersAddresses(
                AF_UNSPEC.0 as u32,
                GAA_FLAG_INCLUDE_GATEWAYS,
                None,
                Some(buffer.as_mut_ptr().cast()),
                &mut size,
            );
            if result == 0 {
                break;
            }
            if result != 111 {
                // ERROR_BUFFER_OVERFLOW = 111
                return out;
            }
        }
        let mut current = buffer.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
        while !current.is_null() {
            let a = &*current;
            let description = a.Description.to_string().unwrap_or_default();
            let friendly = a.FriendlyName.to_string().unwrap_or_default();
            let mut ipv4 = None;
            let mut unicast = a.FirstUnicastAddress;
            while !unicast.is_null() {
                let u = &*unicast;
                let sockaddr = u.Address.lpSockaddr;
                if !sockaddr.is_null() && (*sockaddr).sa_family == AF_INET {
                    let sin = &*(sockaddr as *const SOCKADDR_IN);
                    let ip = Ipv4Addr::from(u32::from_be(sin.sin_addr.S_un.S_addr));
                    if !ip.is_unspecified() {
                        ipv4 = Some((ip, u.OnLinkPrefixLength));
                        break;
                    }
                }
                unicast = u.Next;
            }
            let mac = (a.PhysicalAddressLength == 6).then(|| {
                let mut m = [0u8; 6];
                m.copy_from_slice(&a.PhysicalAddress[..6]);
                m
            });
            out.push(Adapter {
                description,
                friendly,
                if_type: a.IfType,
                up: a.OperStatus == IfOperStatusUp,
                multicast: a.Anonymous2.Flags & IP_ADAPTER_NO_MULTICAST == 0,
                gateway: !a.FirstGatewayAddress.is_null(),
                ipv4,
                mac,
            });
            current = a.Next;
        }
    }
    out
}

fn prefix_mask(prefix: u8) -> Ipv4Addr {
    if prefix == 0 {
        Ipv4Addr::UNSPECIFIED
    } else {
        Ipv4Addr::from(u32::MAX << (32 - prefix.min(32) as u32))
    }
}

/// Alle echten IPv4-Schnittstellen (wie in der C-Fassung ab 0.3.4): erst echte
/// Adapter mit Gateway, sonst echte Adapter, sonst alles außer Loopback.
pub fn ipv4_interfaces() -> Vec<Interface> {
    let adapters = adapters();
    for strict in [2, 1, 0] {
        let mut found: Vec<Interface> = Vec::new();
        for a in &adapters {
            if !a.up || !a.multicast || a.if_type == IF_TYPE_SOFTWARE_LOOPBACK {
                continue;
            }
            if strict >= 1 && is_virtual(&a.description, &a.friendly, a.if_type) {
                continue;
            }
            if strict >= 2 && !a.gateway {
                continue;
            }
            if let Some((ip, prefix)) = a.ipv4 {
                if !found.iter().any(|f| f.address == ip) && found.len() < 8 {
                    found.push(Interface { address: ip, mask: prefix_mask(prefix) });
                }
            }
        }
        if !found.is_empty() {
            return found;
        }
    }
    Vec::new()
}

/// Hardware-Adresse wie `find_mac()` der C-Fassung: erster aktive Ethernet-/WLAN-Adapter.
pub fn hardware_address() -> [u8; 6] {
    adapters()
        .into_iter()
        .find(|a| a.up && (a.if_type == IF_TYPE_ETHERNET_CSMACD || a.if_type == IF_TYPE_IEEE80211) && a.mac.is_some())
        .and_then(|a| a.mac)
        .unwrap_or_else(|| {
            // Zufällige, lokal verwaltete Adresse
            let mut m = crate::crypto::random_bytes::<6>();
            m[0] = (m[0] & 0xfe) | 0x02;
            m
        })
}

/// DNS-Hostname wie `gethostname()` (Schreibweise wie in Windows eingestellt).
pub fn host_name() -> String {
    use windows::Win32::System::SystemInformation::{ComputerNamePhysicalDnsHostname, GetComputerNameExW};
    let raw = unsafe {
        let mut size = 0u32;
        let _ = GetComputerNameExW(ComputerNamePhysicalDnsHostname, None, &mut size);
        let mut buffer = vec![0u16; size as usize + 1];
        if GetComputerNameExW(ComputerNamePhysicalDnsHostname, Some(windows::core::PWSTR(buffer.as_mut_ptr())), &mut size).is_ok() {
            String::from_utf16_lossy(&buffer[..size as usize])
        } else {
            std::env::var("COMPUTERNAME").unwrap_or_else(|_| "UxPlay".into())
        }
    };
    let label: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect();
    format!("{}.local", if label.is_empty() { "UxPlay".into() } else { label })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_vpn_adapters() {
        assert!(is_virtual("Famatech Radmin VPN Ethernet Adapter", "Radmin VPN", 6));
        assert!(is_virtual("Hyper-V Virtual Ethernet Adapter", "vEthernet (WSL)", 6));
        assert!(!is_virtual("Realtek PCIe GbE Family Controller", "Ethernet", 6));
        assert!(!is_virtual("Intel(R) Wi-Fi 6 AX201 160MHz", "WLAN", 71));
    }

    #[test]
    fn finds_local_interfaces() {
        let list = ipv4_interfaces();
        assert!(!list.is_empty());
        assert!(list.iter().all(|i| !i.address.is_loopback()));
        let iface = Interface { address: Ipv4Addr::new(192, 168, 178, 21), mask: prefix_mask(24) };
        assert!(iface.contains(Ipv4Addr::new(192, 168, 178, 99)));
        assert!(!iface.contains(Ipv4Addr::new(26, 0, 97, 122)));
    }
}
