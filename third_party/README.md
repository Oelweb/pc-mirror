# Herkunft

`uxplay/`: https://github.com/FDH2/UxPlay

Basis-Commit: `116e39069e40f09350a372233937a55c890a02f8`

Version: 1.74, upstream experimentell. Lizenz: GPL-3.0-or-later; Unterkomponenten besitzen teils eigene Lizenzen. Originalhinweise und Lizenzdateien bleiben im Baum erhalten.

PC-Mirror-Änderungen vom 24.10.2026: ungepufferte Live-Logs; natives Video-Kindfenster mit Maus-/Tastaturweiterleitung; lokale stdin-Steuerung für Stop, Mute und Screenshot; strukturierte Ready-/Frame-/Idle-/PIN-Ereignisse; referenzierter dekodierter Frame für Screenshots; gekennzeichneter synthetischer Testmodus. Änderungen in `uxplay.cpp` und Audio-/Videorenderern. Handshake, FairPlay, RTP und mDNS-Protokollimplementierung stammen aus UxPlay.

Die Runtime-Pakete stammen aus MSYS2 UCRT64. Versionen und Dateien stehen im Paket in `runtime-packages.txt` und `runtime-manifest.json`. Die Paketierung kopiert verfügbare Lizenztexte aus `share/licenses`. Das separate `PC-Mirror-0.1.0-runtime-sources.zip` enthält passende vollständige MSYS2-Quellpakete mit Upstream-Quellen, Buildrezepten und Patches; Prüfsummen stehen im enthaltenen Manifest. Dieses Archiv bei Binärweitergabe zugänglich machen.

`velopack/`: https://github.com/velopack/velopack, Version 1.2.161, Commit `92d6a1c91716729d449034df5c50307dcce39493`, MIT. Unveränderte Upstream-Quellen zur nativen Update-Bibliothek.

`velopack-sdk/`: offizielles C/C++-SDK aus `velopack_libc_1.2.161.zip`, SHA-256 `31de8f2cf05a47bd5328a9f98b0c41aec1582156f6075f1058b26941f7f17c20`. Für Windows x64 wird die DLL mit ihrem Importnamen `velopack_libc.dll` gebündelt. Die Pack-CLI 1.2.161 ist ein reines Build-Werkzeug, SHA-256 des NuGet-Archivs `2b56ce117f803fc70c103cb423bd040e395e40370f6ff6e10818e9ff9c26a323`.

Ab Version 0.2.0 sind die entsprechenden App- und Runtime-Quellarchive ebenfalls nach der aktuellen Version benannt. Alle neuen SDK-/Upstream-Lizenzhinweise werden unter `licenses/` kopiert.
