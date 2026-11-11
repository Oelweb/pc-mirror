# PC-Mirror – Architektur (ab 0.4.0)

## Überblick

```
PC-Mirror.exe (Rust, Slint)                     uxplay.exe (Rust, crates/pcmirror-receiver)
├─ Hauptfenster + Video-Host-HWND  ──PCMIRROR_HWND──▶ eigenes D3D11-Kindfenster (GStreamer)
├─ Werkzeugleiste (Tool-Fenster)                  │
├─ receiver.rs  ── stdin: STOP/MUTE/SNAPSHOT/IPAD_FILL ──▶
│               ◀── stdout: PCMIRROR|READY/FRAME/IDLE/PIN/MUTED/SNAPSHOT
├─ updater.rs + signed_source.rs ── HTTPS ──▶ pcmirror.oel.best (Velopack-Feed + .sig)
└─ diagnostics.rs ── Firewall-/Netzwerk-COM (Prüfung) / UAC (Regeln)
```

App und Empfänger sind in **Rust** geschrieben. Der Empfänger bleibt ein **eigener Prozess**: Ein Absturz im Codec reißt die Oberfläche nicht mit. Ein Windows-Job (`KILL_ON_JOB_CLOSE`) beendet ihn zuverlässig, auch wenn die App abstürzt. Er heißt weiterhin `uxplay.exe`, damit vorhandene Firewall-Freigaben nach dem Update gültig bleiben. Der frühere C-Empfänger (UxPlay) liegt als `uxplay-classic.exe` bei und lässt sich in den Einstellungen als Ausweichlösung wählen.

## Rust-App (`crates/pcmirror`)

| Modul | Aufgabe |
|---|---|
| `main.rs` | Velopack-Hooks, Einzelinstanz (benanntes Mutex), Logging, Software-Renderer, erhöhter Firewall-Helfer |
| `app.rs` | Zustand und Ablauf im UI-Thread: Fenster, Status, Dialoge, Werkzeugleiste |
| `events.rs` | Hintergrund-Threads melden Ergebnisse über einen Kanal an den UI-Thread |
| `receiver.rs` | Start/Stopp des Empfängers (Rust oder klassisch), Protokoll, Zeitlimits, Fehlerbewertung |
| `video_host.rs` | natives Host-Fenster; der Empfänger leitet Doppelklick/Maus/Tasten hierher weiter |
| `updater.rs`, `update_policy.rs` | Prüfen, Laden, Installieren nach der Spiegelung |
| `signed_source.rs` | Velopack-Quelle, die nur Ed25519-signierten Feeds vertraut |
| `settings.rs`, `secret.rs` | `settings.ini` (kompatibel zur Qt-Version), PIN per DPAPI, Autostart |
| `diagnostics.rs`, `network.rs` | Adapterübersicht, Firewall-Prüfung (wie Windows: Profile, Block-Regeln) und -Einrichtung |
| `snapshot.rs` | PPM-Frame des Empfängers → PNG (außerhalb des UI-Threads) |
| `i18n.rs` | deutsche Quelltexte, englische Übersetzung aus `translations/en.json` |
| `ui/app.slint` | alle Fenster; Texte über `T.t()`, Sprachwechsel ohne Neustart |

Die Oberfläche nutzt den Software-Renderer von Slint: robust neben dem nativen D3D11-Kindfenster und unabhängig von OpenGL-Treibern.

## Empfänger (`crates/pcmirror-receiver`)

| Modul | Aufgabe |
|---|---|
| `mdns.rs`, `net.rs` | eingebauter mDNS-Responder (`_airplay._tcp`, `_raop._tcp`) auf allen echten IPv4-Adaptern; VPN-/virtuelle Adapter ausgenommen |
| `rtsp.rs`, `server.rs` | RTSP-Server: /info, Pair-Setup/-Verify, PIN-Kopplung, fp-setup, SETUP/RECORD/FLUSH/TEARDOWN, Lautstärke |
| `pairing.rs`, `srp.rs`, `crypto.rs` | Ed25519/X25519-Pairing, SRP-6a (Apple-Variante), AES-CTR/CBC/GCM, SHA-512 |
| `fairplay/` | FairPlay-Entschlüsselung des Sitzungsschlüssels (Port von UxPlays `playfair`) |
| `mirror.rs` | Videostrom: 128-Byte-Kopf, AES-CTR, AVCC → Annex-B, H.264/H.265-Parameter-Sätze |
| `audio.rs` | Tonstrom: RTP, AES-CBC, Sortierpuffer, Neuanforderung fehlender Pakete |
| `ntp.rs` | Zeitabgleich mit dem Sender (AirPlay-1-NTP) |
| `media.rs`, `gst.rs`, `window.rs` | GStreamer-Pipelines (zur Laufzeit geladen), Video-Kindfenster, iPad-Ausfüllen, Screenshot |
| `control.rs` | Steuerkanal zur App |

Medien: Die Decodierung (H.264/H.265 per D3D11, AAC-ELD/ALAC per libav) und Ausgabe (D3D11, WASAPI) übernimmt die mitgelieferte GStreamer-Runtime. Rust lädt sie zur Laufzeit; die Programme selbst linken nur Windows-Systembibliotheken.

Verglichen mit der C-Fassung prüft der Rust-Empfänger alle Längen- und Moduswerte aus dem Netz, nimmt Video- und Tondaten nur vom verbundenen Gerät an, begrenzt gleichzeitige Verbindungen und ersetzt eine laufende Sitzung erst, wenn ein anderes Gerät wirklich eine Sitzung beginnt.

## Tests ohne iPhone

- `cargo test`: Unit-Tests, darunter **Vergleichswerte aus der C-Fassung** für FairPlay (100 Fälle, alle Modi), SRP und GCM (`crates/pcmirror-receiver/tests/*.c`).
- `tests/airplay_e2e.py`: nachgebauter Sender (`examples/fake_sender.rs`) koppelt sich wie ein iPhone (Pair-Verify oder PIN), handelt FairPlay aus und sendet echtes, verschlüsseltes H.264/H.265 und AAC-ELD. Erwartet Bild und Ton beim Empfänger. Läuft identisch gegen den C-Empfänger.
- `tests/mdns_txt.py`: mDNS-Ankündigung, gegen die C-Fassung verglichen (identisch).
- `tests/receiver_fuzz.py`: kaputte Eingaben aus dem Netz dürfen den Empfänger nicht beenden.
- `tests/backend_integration.py`: Steuerkanal, Bildgröße, Screenshot.

## Sicherheit

- Update-Feed: Ed25519-Signatur (`pcmirror-sign`), öffentlicher Schlüssel im Programm, SHA-256 für jedes Paket Pflicht, Paketnamen ohne Pfadbestandteile.
- Firewall: Regeln nur für die mitgelieferten Empfänger, Profil Privat/Domäne, lokales Subnetz. Prüfung und Einrichtung über die Windows-Firewall-COM-Schnittstelle (`INetFwPolicy2`), ohne PowerShell. Zum Einrichten startet sich `PC-Mirror.exe --configure-firewall` per UAC selbst. Block-Regeln für den Empfänger werden deaktiviert; optional wird das Netzwerk als privat eingestuft.
- PIN: per DPAPI an das Windows-Benutzerkonto gebunden.

## Klassischer Empfänger (UxPlay, C)

`third_party/uxplay` bleibt im Quellcode (Commit `116e39069e40f09350a372233937a55c890a02f8` mit PC-Mirror-Änderungen) und wird als `uxplay-classic.exe` mitgeliefert. Gebaut wird er mit MSYS2 UCRT64 (`scripts/build.sh`).
