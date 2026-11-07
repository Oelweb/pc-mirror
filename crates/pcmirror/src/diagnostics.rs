// SPDX-License-Identifier: GPL-3.0-or-later
//! Netzwerkübersicht und Windows-Firewall: Prüfung ohne, Einrichtung mit UAC.
//!
//! Alles läuft über die Firewall- und Netzwerk-COM-Schnittstellen von Windows,
//! ohne PowerShell: Der UAC-Dialog zeigt „PC-Mirror“ und Virenscanner sehen
//! kein verstecktes Skript.
use crate::events::{self, Event};
use crate::i18n::tr;
use crate::receiver;
use std::path::Path;
use windows::core::{Interface, BSTR, HSTRING, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_CANCELLED, HANDLE, S_OK, VARIANT_TRUE, WAIT_OBJECT_0,
};
use windows::Win32::NetworkManagement::WindowsFirewall::{
    INetFwPolicy2, INetFwRule, NetFwPolicy2, NetFwRule, NET_FW_ACTION_ALLOW,
    NET_FW_PROFILE2_PRIVATE, NET_FW_RULE_DIR_IN,
};
use windows::Win32::Networking::NetworkListManager::{
    INetworkListManager, NetworkListManager, NLM_ENUM_NETWORK_CONNECTED,
    NLM_NETWORK_CATEGORY_PRIVATE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL,
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Ole::IEnumVARIANT;
use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows::Win32::System::Variant::{VariantClear, VARIANT, VT_DISPATCH};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

/// Befehlszeilenschalter, mit dem sich PC-Mirror selbst erhöht für die Regeln startet.
pub const CONFIGURE_ARG: &str = "--configure-firewall";

const RULE_PREFIX: &str = "PC-Mirror";
const PROTOCOL_TCP: i32 = 6;
const PROTOCOL_UDP: i32 = 17;

/// Die drei Freigaben: nur eingehend, nur Profil Privat, nur lokales Subnetz.
const RULES: [(&str, i32, &str); 3] = [
    ("PC-Mirror AirPlay TCP", PROTOCOL_TCP, "48000-48002"),
    ("PC-Mirror AirPlay UDP", PROTOCOL_UDP, "48000-48002"),
    ("PC-Mirror mDNS", PROTOCOL_UDP, "5353"),
];

/// COM für die Dauer eines Hintergrundaufrufs.
struct Com;

impl Com {
    fn init() -> Option<Self> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .is_ok()
            .then_some(Com)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

fn same_path(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// Unsere Regeln für genau diese uxplay.exe (auch die von älteren Versionen angelegten).
/// Die Regelsammlung bleibt dabei bestehen: Die Regelobjekte gehören zu ihr.
unsafe fn own_rules(
    policy: &INetFwPolicy2,
    program: &str,
) -> windows::core::Result<Vec<INetFwRule>> {
    let collection = policy.Rules()?;
    let enumerator: IEnumVARIANT = collection._NewEnum()?.cast()?;
    let mut rules = Vec::new();
    loop {
        let mut items = [VARIANT::default()];
        let mut fetched = 0u32;
        if enumerator.Next(&mut items, &mut fetched) != S_OK || fetched == 0 {
            break;
        }
        let item = &mut items[0];
        let rule = if item.Anonymous.Anonymous.vt == VT_DISPATCH {
            let raw = item.Anonymous.Anonymous.Anonymous.pdispVal.as_ref().cloned();
            raw.and_then(|d| d.cast::<INetFwRule>().ok())
        } else {
            None
        };
        let _ = VariantClear(item);
        let Some(rule) = rule else { continue };
        let name = rule.Name().map(|n| n.to_string()).unwrap_or_default();
        if !name.starts_with(RULE_PREFIX) {
            continue;
        }
        let app = rule.ApplicationName().map(|n| n.to_string()).unwrap_or_default();
        if same_path(&app, program) {
            rules.push(rule);
        }
    }
    drop(collection);
    Ok(rules)
}

fn private_network() -> Option<bool> {
    unsafe {
        let manager: INetworkListManager =
            CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL).ok()?;
        let networks = manager.GetNetworks(NLM_ENUM_NETWORK_CONNECTED).ok()?;
        let mut private = false;
        loop {
            let mut item = [None];
            let mut fetched = 0u32;
            if networks.Next(&mut item, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            if let Some(network) = item[0].take() {
                if network.GetCategory().ok() == Some(NLM_NETWORK_CATEGORY_PRIVATE) {
                    private = true;
                }
            }
        }
        Some(private)
    }
}

fn rules_present(program: &str) -> Result<bool, String> {
    unsafe {
        let policy: INetFwPolicy2 = CoCreateInstance(&NetFwPolicy2, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| e.to_string())?;
        let rules = own_rules(&policy, program).map_err(|e| e.to_string())?;
        let active = rules
            .iter()
            .filter(|rule| {
                rule.Enabled().ok() == Some(VARIANT_TRUE)
                    && rule.Action().ok() == Some(NET_FW_ACTION_ALLOW)
            })
            .count();
        Ok(active >= RULES.len())
    }
}

pub fn network_summary() -> String {
    let adapters = crate::network::ipv4_adapters();
    if adapters.is_empty() {
        return tr("Kein aktiver IPv4-Netzwerkadapter. Bitte den PC mit deinem Router verbinden.");
    }
    let mut lines: Vec<String> = adapters
        .into_iter()
        .map(|(name, ip)| format!("{name}: {ip}"))
        .collect();
    lines.push(tr("PC und iPhone/iPad müssen im selben lokalen Netz sein. Ethernet am PC ist erlaubt.\nGastnetz, VPN und AP-Isolation können AirPlay blockieren. Das WLAN des Apple-Geräts kann Windows nicht zuverlässig prüfen."));
    lines.join("\n")
}

/// Nur die aktiven Adapter, ohne Erklärtext (für den Einrichtungsassistenten).
pub fn adapter_summary() -> String {
    let adapters = crate::network::ipv4_adapters();
    if adapters.is_empty() {
        return tr("Kein aktiver IPv4-Netzwerkadapter. Bitte den PC mit deinem Router verbinden.");
    }
    adapters
        .into_iter()
        .map(|(name, ip)| format!("{name}: {ip}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Prüft ohne Administratorrechte, ob die Regeln und ein privates Profil vorhanden sind.
pub fn check_firewall() {
    let program = receiver::executable().display().to_string();
    std::thread::spawn(move || {
        let Some(_com) = Com::init() else {
            events::post(Event::FirewallChecked {
                ok: false,
                private: None,
                rules: None,
                message: "Die Firewall-Prüfung konnte nicht starten. Bitte Diagnose öffnen.",
                details: "COM nicht verfügbar".into(),
            });
            return;
        };
        let private = private_network();
        let (rules, details) = match rules_present(&program) {
            Ok(present) => (Some(present), String::new()),
            Err(error) => (None, error),
        };
        let (ok, message) = firewall_message(private, rules);
        events::post(Event::FirewallChecked {
            ok,
            private,
            rules,
            message,
            details,
        });
    });
}

/// Wichtigster Hinweis zuerst: fehlende Freigabe, dann öffentliches Netzwerk.
pub fn firewall_message(private: Option<bool>, rules: Option<bool>) -> (bool, &'static str) {
    match (private, rules) {
        (Some(true), Some(true)) => (true, "Firewall-Freigaben vorhanden · privates Netzwerk erkannt"),
        (_, Some(false)) | (_, None) => (false, "Firewall-Freigabe fehlt oder ist nicht prüfbar. Über „Firewall einrichten“ freigeben. Windows fragt nach Administratorrechten."),
        (_, Some(true)) => (false, "Kein privates Netzwerk erkannt. Windows-Einstellungen → Netzwerk → Profil Privat wählen, wenn dies dein vertrauenswürdiges Heimnetz ist."),
    }
}

/// Startet PC-Mirror selbst mit Administratorrechten, um die Regeln anzulegen.
/// Windows zeigt dabei Name und Symbol von PC-Mirror statt einer PowerShell.
pub fn configure_firewall() {
    let Ok(exe) = std::env::current_exe() else {
        events::post(Event::FirewallConfigured {
            result: Err("Windows konnte die Firewall-Einrichtung nicht starten."),
        });
        return;
    };
    let file = HSTRING::from(exe.as_os_str());
    let verb = HSTRING::from("runas");
    let parameters = HSTRING::from(CONFIGURE_ARG);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(parameters.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    if unsafe { ShellExecuteExW(&mut info) }.is_err() {
        let message = if unsafe { GetLastError() } == ERROR_CANCELLED {
            "Firewall-Einrichtung wurde im Windows-Dialog abgebrochen."
        } else {
            "Windows konnte die Firewall-Einrichtung nicht starten."
        };
        events::post(Event::FirewallConfigured {
            result: Err(message),
        });
        return;
    }
    let process = info.hProcess.0 as isize;
    std::thread::spawn(move || {
        let handle = HANDLE(process as *mut _);
        let mut code = 1u32;
        unsafe {
            if WaitForSingleObject(handle, INFINITE) == WAIT_OBJECT_0 {
                let _ = GetExitCodeProcess(handle, &mut code);
            }
            let _ = CloseHandle(handle);
        }
        events::post(Event::FirewallConfigured { result: Ok(code) });
    });
}

/// Läuft im erhöhten Hilfsprozess (`PC-Mirror.exe --configure-firewall`).
/// Exit-Code: 0 = in Ordnung, 2 = Firewall-Fehler, 3 = Empfänger fehlt.
pub fn apply_rules() -> i32 {
    let receiver = receiver::executable();
    if receiver.file_name().and_then(|n| n.to_str()) != Some("uxplay.exe") || !receiver.is_file()
    {
        return 3;
    }
    let Some(_com) = Com::init() else { return 2 };
    match unsafe { write_rules(&receiver) } {
        Ok(()) => 0,
        Err(_) => 2,
    }
}

/// Vorhandene Regeln werden aktualisiert statt gelöscht, fehlende neu angelegt.
unsafe fn write_rules(receiver: &Path) -> windows::core::Result<()> {
    let program = receiver.display().to_string();
    let policy: INetFwPolicy2 = CoCreateInstance(&NetFwPolicy2, None, CLSCTX_INPROC_SERVER)?;
    let existing = own_rules(&policy, &program)?;
    for (name, protocol, ports) in RULES {
        let found = existing
            .iter()
            .find(|rule| rule.Name().map(|n| n.to_string()).unwrap_or_default() == name)
            .cloned();
        let (rule, new) = match found {
            Some(rule) => (rule, false),
            None => (
                CoCreateInstance::<_, INetFwRule>(&NetFwRule, None, CLSCTX_INPROC_SERVER)?,
                true,
            ),
        };
        rule.SetName(&BSTR::from(name))?;
        rule.SetDescription(&BSTR::from(
            "Erlaubt iPhone/iPad im Heimnetz, per AirPlay an PC-Mirror zu senden.",
        ))?;
        rule.SetGrouping(&BSTR::from("PC-Mirror"))?;
        rule.SetApplicationName(&BSTR::from(program.as_str()))?;
        rule.SetProtocol(protocol)?;
        rule.SetLocalPorts(&BSTR::from(ports))?;
        rule.SetRemoteAddresses(&BSTR::from("LocalSubnet"))?;
        rule.SetDirection(NET_FW_RULE_DIR_IN)?;
        rule.SetAction(NET_FW_ACTION_ALLOW)?;
        rule.SetProfiles(NET_FW_PROFILE2_PRIVATE.0)?;
        rule.SetEnabled(VARIANT_TRUE)?;
        if new {
            policy.Rules()?.Add(&rule)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_follow_state() {
        assert!(firewall_message(Some(true), Some(true)).0);
        assert!(firewall_message(Some(false), Some(true))
            .1
            .contains("privates Netzwerk"));
        assert!(firewall_message(Some(true), Some(false)).1.contains("fehlt"));
        assert!(!firewall_message(None, None).0);
    }

    #[test]
    fn paths_compare_case_insensitive() {
        assert!(same_path(r"C:\Users\Jörg\uxplay.exe", r"c:\users\JÖRG\UXPLAY.EXE"));
        assert!(!same_path(r"C:\a\uxplay.exe", r"C:\b\uxplay.exe"));
    }

    /// Liest die echten Regeln ohne Administratorrechte (prüft die COM-Anbindung).
    #[test]
    fn reads_firewall_without_admin() {
        let _com = Com::init().expect("COM");
        assert_eq!(rules_present(r"C:\nicht\vorhanden\uxplay.exe"), Ok(false));
        assert!(private_network().is_some());
        // Auf einem PC mit installiertem PC-Mirror: dessen Regeln werden erkannt.
        if let Ok(base) = std::env::var("LOCALAPPDATA") {
            let installed = format!(r"{base}\PCMirror\current\uxplay.exe");
            if Path::new(&installed).is_file() {
                eprintln!("installiert: {installed} -> {:?}", rules_present(&installed));
            }
        }
    }
}
