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
    CloseHandle, GetLastError, ERROR_CANCELLED, HANDLE, S_OK, VARIANT_FALSE, VARIANT_TRUE,
    WAIT_OBJECT_0,
};
use windows::Win32::NetworkManagement::WindowsFirewall::{
    INetFwPolicy2, INetFwRule, NetFwPolicy2, NetFwRule, NET_FW_ACTION_ALLOW, NET_FW_ACTION_BLOCK,
    NET_FW_PROFILE2_DOMAIN, NET_FW_PROFILE2_PRIVATE, NET_FW_PROFILE2_PUBLIC, NET_FW_RULE_DIR_IN,
};
use windows::Win32::Networking::NetworkListManager::{
    INetworkListManager, NetworkListManager, NLM_ENUM_NETWORK_CONNECTED,
    NLM_NETWORK_CATEGORY_PRIVATE, NLM_NETWORK_CATEGORY_PUBLIC,
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
/// Zusatz: verbundene öffentliche Netzwerke als privat einstufen.
pub const PRIVATE_ARG: &str = "--make-private";

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

/// Alle Regeln, deren Programm genau diese uxplay.exe ist (beliebiger Name; auch
/// Regeln aus dem Windows-Dialog „Zugriff zulassen“). Die Regelsammlung bleibt
/// während der Auswertung bestehen.
unsafe fn program_rules(
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
        let app = rule.ApplicationName().map(|n| n.to_string()).unwrap_or_default();
        if same_path(&app, program) {
            rules.push(rule);
        }
    }
    drop(collection);
    Ok(rules)
}

/// Nur die von PC-Mirror selbst angelegten Regeln.
unsafe fn own_rules(
    policy: &INetFwPolicy2,
    program: &str,
) -> windows::core::Result<Vec<INetFwRule>> {
    Ok(program_rules(policy, program)?
        .into_iter()
        .filter(|rule| {
            rule.Name()
                .map(|n| n.to_string().starts_with(RULE_PREFIX))
                .unwrap_or(false)
        })
        .collect())
}

/// Eine eingehende, aktive Regel für das Programm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleInfo {
    pub allow: bool,
    pub profiles: i32,
}

/// Ein gerade aktives Firewall-Profil (Domäne, Privat oder Öffentlich).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileInfo {
    pub profile: i32,
    pub firewall_on: bool,
    pub block_all: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Eingehende Verbindungen zum Empfänger sind in allen aktiven Profilen erlaubt.
    Open,
    /// Eine Block-Regel oder „Alle eingehenden blockieren“ verhindert Verbindungen.
    Blocked,
    /// Keine passende Erlauben-Regel für ein aktives Profil.
    Missing,
}

/// Bewertet wie Windows: Ist die Firewall im Profil aus, ist alles offen; „Alle
/// blockieren“ und Block-Regeln gehen vor; sonst braucht es eine Erlauben-Regel.
pub fn evaluate(profiles: &[ProfileInfo], rules: &[RuleInfo]) -> Verdict {
    let mut verdict = Verdict::Open;
    for profile in profiles.iter().filter(|p| p.firewall_on) {
        let applies = |rule: &&RuleInfo| rule.profiles & profile.profile != 0;
        if profile.block_all || rules.iter().filter(applies).any(|r| !r.allow) {
            return Verdict::Blocked;
        }
        if !rules.iter().filter(applies).any(|r| r.allow) {
            verdict = Verdict::Missing;
        }
    }
    verdict
}

/// Liest aktive Profile und die Regeln des Empfängers aus der Windows-Firewall.
unsafe fn read_firewall(program: &str) -> windows::core::Result<(Vec<ProfileInfo>, Vec<RuleInfo>)> {
    let policy: INetFwPolicy2 = CoCreateInstance(&NetFwPolicy2, None, CLSCTX_INPROC_SERVER)?;
    let current = policy.CurrentProfileTypes()?;
    let mut profiles = Vec::new();
    for profile in [NET_FW_PROFILE2_DOMAIN, NET_FW_PROFILE2_PRIVATE, NET_FW_PROFILE2_PUBLIC] {
        if current & profile.0 == 0 {
            continue;
        }
        profiles.push(ProfileInfo {
            profile: profile.0,
            firewall_on: policy.get_FirewallEnabled(profile)? == VARIANT_TRUE,
            block_all: policy.get_BlockAllInboundTraffic(profile).ok() == Some(VARIANT_TRUE),
        });
    }
    let rules = program_rules(&policy, program)?
        .into_iter()
        .filter(|rule| {
            rule.Enabled().ok() == Some(VARIANT_TRUE)
                && rule.Direction().ok() == Some(NET_FW_RULE_DIR_IN)
        })
        .map(|rule| RuleInfo {
            allow: rule.Action().ok() == Some(NET_FW_ACTION_ALLOW),
            profiles: rule.Profiles().unwrap_or(0),
        })
        .collect();
    Ok((profiles, rules))
}

/// Netzwerkkategorie der verbundenen Netze auf „Privat“ setzen (nur im erhöhten Helfer).
unsafe fn make_networks_private() -> windows::core::Result<()> {
    let manager: INetworkListManager = CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL)?;
    let networks = manager.GetNetworks(NLM_ENUM_NETWORK_CONNECTED)?;
    loop {
        let mut item = [None];
        let mut fetched = 0u32;
        if networks.Next(&mut item, Some(&mut fetched)).is_err() || fetched == 0 {
            break;
        }
        if let Some(network) = item[0].take() {
            if network.GetCategory()? == NLM_NETWORK_CATEGORY_PUBLIC {
                network.SetCategory(NLM_NETWORK_CATEGORY_PRIVATE)?;
            }
        }
    }
    Ok(())
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

/// Prüft ohne Administratorrechte, ob iPhone/iPad den Empfänger erreichen dürfen.
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
        let (private, rules, message, details) = match unsafe { read_firewall(&program) } {
            Ok((profiles, rules)) => {
                let private = Some(
                    profiles
                        .iter()
                        .any(|p| p.profile & (NET_FW_PROFILE2_PRIVATE.0 | NET_FW_PROFILE2_DOMAIN.0) != 0),
                );
                let verdict = evaluate(&profiles, &rules);
                let open = verdict == Verdict::Open;
                let message = match verdict {
                    Verdict::Blocked => BLOCKED_MESSAGE,
                    _ => firewall_message(private, Some(open)).1,
                };
                let details = format!(
                    "Profile: {}; Regeln für den Empfänger: {} erlauben, {} blockieren",
                    profiles
                        .iter()
                        .map(|p| format!(
                            "{}{}",
                            profile_name(p.profile),
                            if p.firewall_on { "" } else { " (Firewall aus)" }
                        ))
                        .collect::<Vec<_>>()
                        .join(", "),
                    rules.iter().filter(|r| r.allow).count(),
                    rules.iter().filter(|r| !r.allow).count(),
                );
                (private, Some(open), message, details)
            }
            Err(error) => (None, None, firewall_message(None, None).1, error.to_string()),
        };
        events::post(Event::FirewallChecked {
            ok: rules == Some(true),
            private,
            rules,
            message,
            details,
        });
    });
}

const BLOCKED_MESSAGE: &str = "Die Windows-Firewall blockiert den Empfänger (Block-Regel). Über „Firewall einrichten“ beheben.";

fn profile_name(profile: i32) -> &'static str {
    match profile {
        1 => "Domäne",
        2 => "Privat",
        _ => "Öffentlich",
    }
}

/// `rules`: Empfänger im aktiven Profil erreichbar. `private`: privates oder Domänennetz aktiv.
pub fn firewall_message(private: Option<bool>, rules: Option<bool>) -> (bool, &'static str) {
    match (private, rules) {
        (_, Some(true)) => (true, "Firewall-Freigabe aktiv. iPhone und iPad können verbinden."),
        (Some(false), Some(false)) => (false, "Dein Netzwerk ist als „Öffentlich“ eingestuft. Über „Netzwerk freigeben“ als privates Heimnetz einstufen."),
        (_, Some(false)) => (false, "Firewall-Freigabe fehlt. Über „Firewall einrichten“ freigeben. Windows fragt nach Administratorrechten."),
        (_, None) => (false, "Firewall-Status nicht prüfbar. Falls die Verbindung nicht klappt: „Firewall einrichten“."),
    }
}

/// Startet PC-Mirror selbst mit Administratorrechten, um die Regeln anzulegen.
/// Windows zeigt dabei Name und Symbol von PC-Mirror statt einer PowerShell.
pub fn configure_firewall(make_private: bool) {
    let Ok(exe) = std::env::current_exe() else {
        events::post(Event::FirewallConfigured {
            result: Err("Windows konnte die Firewall-Einrichtung nicht starten."),
        });
        return;
    };
    let file = HSTRING::from(exe.as_os_str());
    let verb = HSTRING::from("runas");
    let parameters = HSTRING::from(if make_private {
        format!("{CONFIGURE_ARG} {PRIVATE_ARG}")
    } else {
        CONFIGURE_ARG.to_owned()
    });
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

/// Läuft im erhöhten Hilfsprozess (`PC-Mirror.exe --configure-firewall [--make-private]`).
/// Exit-Code: 0 = in Ordnung, 2 = Firewall-Fehler, 3 = Empfänger fehlt, 4 = Netzwerkprofil.
pub fn apply_rules(make_private: bool) -> i32 {
    let receiver = receiver::executable();
    if receiver.file_name().and_then(|n| n.to_str()) != Some("uxplay.exe") || !receiver.is_file()
    {
        return 3;
    }
    let Some(_com) = Com::init() else { return 2 };
    if unsafe { write_rules(&receiver) }.is_err() {
        return 2;
    }
    if make_private && unsafe { make_networks_private() }.is_err() {
        return 4;
    }
    0
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
        rule.SetProfiles(NET_FW_PROFILE2_PRIVATE.0 | NET_FW_PROFILE2_DOMAIN.0)?;
        rule.SetEnabled(VARIANT_TRUE)?;
        if new {
            policy.Rules()?.Add(&rule)?;
        }
    }
    // Block-Regeln für den Empfänger (z. B. „Abbrechen“ im Windows-Dialog) gehen jeder
    // Freigabe vor. Sie werden deaktiviert, nicht gelöscht.
    for rule in program_rules(&policy, &program)? {
        if rule.Direction().ok() == Some(NET_FW_RULE_DIR_IN)
            && rule.Action().ok() == Some(NET_FW_ACTION_BLOCK)
            && rule.Enabled().ok() == Some(VARIANT_TRUE)
        {
            rule.SetEnabled(VARIANT_FALSE)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIVATE: i32 = 2;
    const PUBLIC: i32 = 4;
    const ALL: i32 = 0x7fff_ffff;

    fn on(profile: i32) -> ProfileInfo {
        ProfileInfo { profile, firewall_on: true, block_all: false }
    }

    #[test]
    fn verdict_follows_windows_rules() {
        let allow_private = RuleInfo { allow: true, profiles: PRIVATE };
        let allow_all = RuleInfo { allow: true, profiles: ALL };
        let block_public = RuleInfo { allow: false, profiles: PUBLIC };
        // Eigene Regeln im privaten Netz
        assert_eq!(evaluate(&[on(PRIVATE)], &[allow_private]), Verdict::Open);
        // Öffentliches Netz, aber Windows-Dialog hat für alle Profile erlaubt: kein Fehlalarm
        assert_eq!(evaluate(&[on(PUBLIC)], &[allow_private, allow_all]), Verdict::Open);
        // Nur private Regel im öffentlichen Netz
        assert_eq!(evaluate(&[on(PUBLIC)], &[allow_private]), Verdict::Missing);
        // Block-Regel geht vor
        assert_eq!(evaluate(&[on(PUBLIC)], &[allow_all, block_public]), Verdict::Blocked);
        // Block-Regel für ein anderes Profil stört nicht
        assert_eq!(evaluate(&[on(PRIVATE)], &[allow_private, block_public]), Verdict::Open);
        // Firewall im Profil aus: offen
        let off = ProfileInfo { profile: PUBLIC, firewall_on: false, block_all: false };
        assert_eq!(evaluate(&[off], &[]), Verdict::Open);
        // „Alle eingehenden blockieren“
        let shields = ProfileInfo { profile: PRIVATE, firewall_on: true, block_all: true };
        assert_eq!(evaluate(&[shields], &[allow_all]), Verdict::Blocked);
        // Keine Regeln
        assert_eq!(evaluate(&[on(PRIVATE)], &[]), Verdict::Missing);
    }

    #[test]
    fn messages_follow_state() {
        assert!(firewall_message(Some(true), Some(true)).0);
        assert!(firewall_message(Some(false), Some(true)).0);
        assert!(firewall_message(Some(false), Some(false)).1.contains("Öffentlich"));
        assert!(firewall_message(Some(true), Some(false)).1.contains("fehlt"));
        assert!(!firewall_message(None, None).0);
    }

    #[test]
    fn paths_compare_case_insensitive() {
        assert!(same_path(r"C:\Users\Jörg\uxplay.exe", r"c:\users\JÖRG\UXPLAY.EXE"));
        assert!(!same_path(r"C:\a\uxplay.exe", r"C:\b\uxplay.exe"));
    }

    /// Liest die echte Firewall ohne Administratorrechte (prüft die COM-Anbindung).
    #[test]
    fn reads_firewall_without_admin() {
        let _com = Com::init().expect("COM");
        let (profiles, rules) =
            unsafe { read_firewall(r"C:\nicht\vorhanden\uxplay.exe") }.expect("Firewall lesbar");
        assert!(!profiles.is_empty());
        assert!(rules.is_empty());
        if let Ok(base) = std::env::var("LOCALAPPDATA") {
            let installed = format!(r"{base}\PCMirror\current\uxplay.exe");
            if Path::new(&installed).is_file() {
                let (profiles, rules) = unsafe { read_firewall(&installed) }.unwrap();
                eprintln!("installiert: {:?} {:?} -> {:?}", profiles, rules, evaluate(&profiles, &rules));
            }
        }
    }
}
