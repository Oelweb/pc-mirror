// SPDX-License-Identifier: GPL-3.0-or-later
//! Netzwerkübersicht und Windows-Firewall: Prüfung ohne, Einrichtung mit UAC.
use crate::events::{self, Event};
use crate::i18n::tr;
use crate::receiver;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use windows::core::HSTRING;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_CANCELLED, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn powershell() -> String {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    format!(r"{root}\System32\WindowsPowerShell\v1.0\powershell.exe")
}

/// Pfad als PowerShell-Literal in einfachen Anführungszeichen – nie als Code.
fn ps_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// PowerShell erwartet für -EncodedCommand UTF-16LE in Base64.
fn encoded(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    STANDARD.encode(bytes)
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
    // Prüft Netzwerkprofil und Regeln getrennt, damit die Oberfläche beides anzeigen kann.
    let script = format!(
        "$ErrorActionPreference='Stop'; $p={};          $r=@(Get-NetFirewallRule -DisplayName 'PC-Mirror*' -ErrorAction SilentlyContinue | Where-Object {{ $_.Enabled -eq 'True' -and $_.Action -eq 'Allow' }} | Get-NetFirewallApplicationFilter | Where-Object {{ $_.Program -eq $p }});          $private=@(Get-NetConnectionProfile | Where-Object {{ $_.NetworkCategory -eq 'Private' }});          Write-Output ('PRIVATE=' + [int]($private.Count -gt 0)); Write-Output ('RULES=' + $r.Count); exit 0",
        ps_literal(&program)
    );
    std::thread::spawn(move || {
        let child = Command::new(powershell())
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand",
                &encoded(&script),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(error) => {
                events::post(Event::FirewallChecked {
                    ok: false,
                    private: None,
                    rules: None,
                    message: "Die Firewall-Prüfung konnte nicht starten. Bitte Diagnose öffnen.",
                    details: error.to_string(),
                });
                return;
            }
        };
        let deadline = Instant::now() + Duration::from_secs(15);
        while child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill();
        }
        let output = child.wait_with_output();
        let (stdout, stderr) = match output {
            Ok(o) => (
                String::from_utf8_lossy(&o.stdout).into_owned(),
                String::from_utf8_lossy(&o.stderr).into_owned(),
            ),
            Err(e) => (String::new(), e.to_string()),
        };
        let (private, rules) = parse_check(&stdout);
        let (ok, message) = firewall_message(private, rules);
        events::post(Event::FirewallChecked {
            ok,
            private,
            rules,
            message,
            details: stderr.trim().to_owned(),
        });
    });
}

/// Liest `PRIVATE=0|1` und `RULES=n` aus der Ausgabe der Prüfung.
pub fn parse_check(output: &str) -> (Option<bool>, Option<bool>) {
    let mut private = None;
    let mut rules = None;
    for line in output.lines() {
        if let Some(value) = line.trim().strip_prefix("PRIVATE=") {
            private = Some(value.trim() == "1");
        } else if let Some(value) = line.trim().strip_prefix("RULES=") {
            rules = value.trim().parse::<u32>().ok().map(|count| count >= 3);
        }
    }
    (private, rules)
}

/// Wichtigster Hinweis zuerst: fehlende Freigabe, dann öffentliches Netzwerk.
pub fn firewall_message(private: Option<bool>, rules: Option<bool>) -> (bool, &'static str) {
    match (private, rules) {
        (Some(true), Some(true)) => (true, "Firewall-Freigaben vorhanden · privates Netzwerk erkannt"),
        (_, Some(false)) | (_, None) => (false, "Firewall-Freigabe fehlt oder ist nicht prüfbar. Über „Firewall einrichten“ freigeben. Windows fragt nach Administratorrechten."),
        (_, Some(true)) => (false, "Kein privates Netzwerk erkannt. Windows-Einstellungen → Netzwerk → Profil Privat wählen, wenn dies dein vertrauenswürdiges Heimnetz ist."),
    }
}

/// Richtet die Regeln mit UAC ein. Das Skript wird direkt übergeben, statt eine
/// Skriptdatei aus dem (vom Benutzer beschreibbaren) Programmordner erhöht auszuführen.
pub fn configure_firewall() {
    let receiver = receiver::executable().display().to_string();
    let script = firewall_script(&receiver);
    let parameters = format!(
        "-NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {}",
        encoded(&script)
    );
    let file = HSTRING::from(powershell());
    let verb = HSTRING::from("runas");
    let parameters = HSTRING::from(parameters);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: windows::core::PCWSTR(verb.as_ptr()),
        lpFile: windows::core::PCWSTR(file.as_ptr()),
        lpParameters: windows::core::PCWSTR(parameters.as_ptr()),
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
        let handle = windows::Win32::Foundation::HANDLE(process as *mut _);
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

/// Regeln nur für genau diese uxplay.exe, Profil Privat, lokales Subnetz.
pub fn firewall_script(receiver: &str) -> String {
    format!(
        r#"$ErrorActionPreference = 'Stop'
$resolved = (Resolve-Path -LiteralPath {path}).Path
if ([IO.Path]::GetFileName($resolved) -ne 'uxplay.exe') {{ exit 3 }}
$prefix = 'PC-Mirror-' + ([Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($resolved.ToLowerInvariant())) -replace '[^a-zA-Z0-9]','')
foreach ($suffix in @('TCP','UDP','mDNS')) {{ Get-NetFirewallRule -Name "$prefix-$suffix" -ErrorAction SilentlyContinue | Remove-NetFirewallRule }}
New-NetFirewallRule -Name "$prefix-TCP" -DisplayName 'PC-Mirror AirPlay TCP' -Direction Inbound -Action Allow -Profile Private -Program $resolved -Protocol TCP -LocalPort '48000-48002' -RemoteAddress LocalSubnet | Out-Null
New-NetFirewallRule -Name "$prefix-UDP" -DisplayName 'PC-Mirror AirPlay UDP' -Direction Inbound -Action Allow -Profile Private -Program $resolved -Protocol UDP -LocalPort '48000-48002' -RemoteAddress LocalSubnet | Out-Null
New-NetFirewallRule -Name "$prefix-mDNS" -DisplayName 'PC-Mirror mDNS' -Direction Inbound -Action Allow -Profile Private -Program $resolved -Protocol UDP -LocalPort 5353 -RemoteAddress LocalSubnet | Out-Null
exit 0"#,
        path = ps_literal(receiver)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_literals_not_code() {
        assert_eq!(ps_literal(r"C:\a'b\uxplay.exe"), r"'C:\a''b\uxplay.exe'");
        let script = firewall_script(r"C:\Users\O'Neil\uxplay.exe");
        assert!(script.contains(r"-LiteralPath 'C:\Users\O''Neil\uxplay.exe'"));
    }

    #[test]
    fn check_output_is_parsed() {
        let both = ["PRIVATE=1", "RULES=3", ""].join("\r\n");
        assert_eq!(parse_check(&both), (Some(true), Some(true)));
        let neither = ["PRIVATE=0", "RULES=0"].join("\n");
        assert_eq!(parse_check(&neither), (Some(false), Some(false)));
        assert_eq!(parse_check(""), (None, None));
        assert!(firewall_message(Some(true), Some(true)).0);
        assert!(firewall_message(Some(false), Some(true))
            .1
            .contains("privates Netzwerk"));
        assert!(firewall_message(Some(true), Some(false))
            .1
            .contains("fehlt"));
    }

    #[test]
    fn encoded_command_is_utf16le_base64() {
        assert_eq!(encoded("ab"), "YQBiAA==");
    }
}
