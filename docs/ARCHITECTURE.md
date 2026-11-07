# PC-Mirror – Architektur (ab 0.3.0)

## Überblick

```
PC-Mirror.exe (Rust, Slint)                     uxplay.exe (C/C++, UxPlay 1.74)
├─ Hauptfenster + Video-Host-HWND  ──PCMIRROR_HWND──▶ eigenes D3D11-Kindfenster
├─ Werkzeugleiste (Tool-Fenster)                  │
├─ receiver.rs  ── stdin: STOP/MUTE/SNAPSHOT/IPAD_FILL ──▶
│               ◀── stdout: PCMIRROR|READY/FRAME/IDLE/PIN/MUTED/SNAPSHOT
├─ updater.rs + signed_source.rs ── HTTPS ──▶ pcmirror.oel.best (Velopack-Feed + .sig)
└─ diagnostics.rs ── Firewall-/Netzwerk-COM (Prüfung) / UAC (Regeln)
```

Der Empfänger bleibt ein **eigener Prozess**: Ein Absturz im Codec reißt die Oberfläche nicht mit. Ein Windows-Job (`KILL_ON_JOB_CLOSE`) beendet `uxplay.exe` zuverlässig, auch wenn die App abstürzt. Es gibt keine zusätzlichen Netzwerk-Steuerports.

## Rust-App (`crates/pcmirror`)

| Modul | Aufgabe |
|---|---|
| `main.rs` | Velopack-Hooks, Einzelinstanz (benanntes Mutex), Logging, Software-Renderer |
| `app.rs` | Zustand und Ablauf im UI-Thread: Fenster, Status, Dialoge, Werkzeugleiste |
| `events.rs` | Hintergrund-Threads melden Ergebnisse über einen Kanal an den UI-Thread |
| `receiver.rs` | Start/Stopp von uxplay.exe, Protokoll, Zeitlimits, Fehlerbewertung |
| `video_host.rs` | natives Host-Fenster; UxPlay leitet Doppelklick/Maus/Tasten hierher weiter |
| `updater.rs`, `update_policy.rs` | Prüfen, Laden, Installieren nach der Spiegelung |
| `signed_source.rs` | Velopack-Quelle, die nur Ed25519-signierten Feeds vertraut |
| `settings.rs`, `secret.rs` | `settings.ini` (kompatibel zur Qt-Version), PIN per DPAPI, Autostart |
| `diagnostics.rs`, `network.rs` | Adapterübersicht, Firewall-Prüfung und -Einrichtung |
| `snapshot.rs` | PPM-Frame von UxPlay → PNG (außerhalb des UI-Threads) |
| `i18n.rs` | deutsche Quelltexte, englische Übersetzung aus `translations/en.json` |
| `ui/app.slint` | alle Fenster; Texte über `T.t()`, Sprachwechsel ohne Neustart |

Die Oberfläche nutzt den Software-Renderer von Slint: robust neben dem nativen D3D11-Kindfenster und unabhängig von OpenGL-Treibern. Die Werkzeugleiste ist ein eigenes, nicht aktivierbares Tool-Fenster über dem Video; sie wird außerhalb des Bildschirms geparkt statt versteckt, damit Slint sie weiter zeichnet.

Die App linkt die C-Laufzeit statisch und lädt nur Windows-Systembibliotheken (`tests/verify_package.py` prüft das).

## Sicherheit

- Update-Feed: Ed25519-Signatur (`pcmirror-sign`), öffentlicher Schlüssel im Programm, SHA-256 für jedes Paket Pflicht, Paketnamen ohne Pfadbestandteile.
- Firewall: Regeln nur für genau diese `uxplay.exe`, Profil Privat, lokales Subnetz. Prüfung und Einrichtung laufen über die Windows-Firewall-COM-Schnittstelle (`INetFwPolicy2`), ohne PowerShell. Zum Einrichten startet sich `PC-Mirror.exe --configure-firewall` per UAC selbst; der Dialog zeigt dadurch PC-Mirror als Programm. Vorhandene Regeln werden aktualisiert, nicht gelöscht.
- PIN: per DPAPI an das Windows-Benutzerkonto gebunden.

## Empfänger (UxPlay)

UxPlay ist auf Commit `116e39069e40f09350a372233937a55c890a02f8` fixiert (`third_party/uxplay`). GStreamer übernimmt H.264/H.265-Decodierung (D3D11) und WASAPI-Audio. Der integrierte mDNS-Responder ersetzt Bonjour. Gebaut wird er mit MSYS2 UCRT64 (`scripts/build.sh`), `NO_MARCH_NATIVE=ON` verhindert eine Bindung an die Entwickler-CPU.

## Weiterer Weg

Das AirPlay-Protokoll selbst (mDNS, RTSP, Pairing, FairPlay, Stream-Entschlüsselung) lässt sich schrittweise nach Rust portieren; das Ziel wäre ein Empfänger-Crate, das dieselbe stdin/stdout-Schnittstelle bedient und so `uxplay.exe` austauschbar ersetzt. Die Codecs bleiben native GStreamer-/FFmpeg-Bibliotheken.
