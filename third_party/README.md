# Herkunft

Seit PC-Mirror 0.4.0 ist der Standard-Empfänger in Rust geschrieben (`crates/pcmirror-receiver`); er überträgt Protokoll, Pairing, FairPlay (`playfair`) und Stromverarbeitung aus UxPlay nach Rust und steht ebenfalls unter GPL-3.0-or-later. UxPlay selbst wird weiter als klassischer Empfänger (`uxplay-classic.exe`) mitgeliefert.

`uxplay/`: https://github.com/FDH2/UxPlay

Basis-Commit: `116e39069e40f09350a372233937a55c890a02f8`

Version: 1.74, upstream experimentell. Lizenz: GPL-3.0-or-later; Unterkomponenten besitzen teils eigene Lizenzen. Originalhinweise und Lizenzdateien bleiben im Baum erhalten.

PC-Mirror-Änderungen vom 24.10.2026: ungepufferte Live-Logs; natives Video-Kindfenster mit Maus-/Tastaturweiterleitung; lokale stdin-Steuerung für Stop, Mute und Screenshot; strukturierte Ready-/Frame-/Idle-/PIN-Ereignisse; referenzierter dekodierter Frame für Screenshots; gekennzeichneter synthetischer Testmodus. Änderungen in `uxplay.cpp` und Audio-/Videorenderern. Handshake, FairPlay, RTP und mDNS-Protokollimplementierung stammen aus UxPlay.

PC-Mirror-Änderung vom 07.11.2026 (`lib/mdnsd/mdnsd.c`): Unter Windows kündigt der eingebaute mDNS-Dienst auf allen echten IPv4-Adaptern an, jeweils mit deren eigener Adresse im A-Record. VPN- und virtuelle Adapter (Radmin, Hamachi, ZeroTier, Tailscale, WireGuard, Hyper-V, VMware …) werden ausgelassen. Bisher wurde nur der erste Adapter mit Gateway verwendet, der bei installiertem VPN falsch sein konnte.

Die Runtime-Pakete stammen aus MSYS2 UCRT64. Versionen und Dateien stehen im Paket in `runtime-packages.txt` und `runtime-manifest.json`. Die Paketierung kopiert verfügbare Lizenztexte aus `share/licenses`. Das separate `PC-Mirror-0.1.0-runtime-sources.zip` enthält passende vollständige MSYS2-Quellpakete mit Upstream-Quellen, Buildrezepten und Patches; Prüfsummen stehen im enthaltenen Manifest. Dieses Archiv bei Binärweitergabe zugänglich machen.

Velopack: ab 0.3.0 nutzt die App die Rust-Crate `velopack` 1.2.161 (MIT) direkt aus crates.io; das frühere C/C++-SDK entfällt. Die Pack-CLI 1.2.161 bleibt reines Build-Werkzeug (SHA-256 des NuGet-Archivs `2b56ce117f803fc70c103cb423bd040e395e40370f6ff6e10818e9ff9c26a323`). Lizenzen aller Rust-Abhängigkeiten werden beim Paketieren nach `licenses/Rust-Crates.txt` geschrieben.
