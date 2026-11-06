# Tatsächlich ausgeführte Prüfungen

Datum: 24.10.2026. Entwicklungsrechner: Windows x64, Ethernet, privates Netzwerkprofil.

| Prüfung | Ergebnis | Nachweis |
|---|---|---|
| UxPlay 1.74 kompilieren, UCRT64 / Release / portable CPU | BESTANDEN | 40 Buildschritte, erfolgreicher Link von `uxplay.exe` |
| PE-Abhängigkeiten rekursiv auflösen | BESTANDEN | 145 native Dateien einschließlich 16 GStreamer-Plugins; keine unaufgelösten Nicht-Windows-Imports |
| Paket ohne MSYS2 im PATH starten | BESTANDEN | `tests/Check-Runtime.ps1` lädt alle 14 erforderlichen Elemente und `uxplay -v` |
| AAC-/ALAC- und H.264-Pipelines initialisieren | BESTANDEN | Reale UxPlay-Ausgabe: AAC-ELD + ALAC via WASAPI, H.264 via D3D11, Renderer in READY |
| AirPlay-Listener | BESTANDEN | IPv4/IPv6 TCP 48001 lauscht; weitere konfigurierte Ports werden bei Bedarf vom Protokoll geöffnet |
| mDNS AirPlay und RAOP | BESTANDEN, nur lokal | `tests/probe_mdns.py` empfängt echte PTR-Antworten über den LAN-Adapter für PC-Mirror, beide TTL 4500 |
| Firewall-Regeln einrichten | BESTANDEN | Drei aktivierte eingehende Regeln, Anwendungspfad des portablen Empfängers, nur Privat + lokales Subnetz |
| iPhone entdeckt PC-Mirror | BESTANDEN laut Benutzer | Erste Abnahme im Chat am 24.10.2026 |
| Prototyp: Pairing, Bild, Ton, Drehung | BESTANDEN laut Benutzer | „Ja: sichtbar, Bild und Ton funktionieren, Drehung klappt.“ |
| Qt-Fenster: Bild, Ton, Drehung und Bedienung | BESTANDEN laut Benutzer | „Alles funktioniert im neuen Fenster, auch die Bedienung.“; Anfrage umfasste Screenshot, Ton aus/ein, Trennen und Wiederverbinden |
| Gemessene Latenz / Dauerlast | OFFEN | Keine Messung, keine Millisekunden-Garantie |
| Sauberer Zweitrechner ohne Buildumgebung | OFFEN | Bereinigter PATH ist kein vollständiger Clean-Machine-Test |
| Qt-Build und eigenständige Runtime | BESTANDEN | Erfolgreich kompiliert, 159 native Dateien aus 99 Runtime-Paketen |
| Sichtbares synthetisches Bild und F11 | BESTANDEN | Native Windows-Fensterprüfung; Computer Use anschließend vom Benutzer mit Escape gestoppt |
| Backend-Pipe, Screenshot-Pixel, Fehlerpfad, STOP, Pipe-Ende | BESTANDEN | `tests/backend_integration.py`, einschließlich Umlaut-/Leerzeichenpfad |
| Installer-Build und lokale Installation | BESTANDEN | Inno Setup 6.7.3, Installer-Exit 0, Installation ohne Neustart im aktuellen Benutzerprofil |
| Installierte Version: Bild und Ton am echten iPhone | BESTANDEN laut Benutzer | Dritte Abnahme: „Ja, die installierte Version funktioniert.“ |
| Installierte Runtime ohne Entwicklungs-PATH | BESTANDEN | 15 GStreamer-Elemente und `uxplay -v` erfolgreich; Firewall-Prüfung erkennt Regeln und privates Profil |
| Binär- und Quellarchive | BESTANDEN | 159 native SHA-256-Prüfsummen; 96 passende Runtime-Quellarchive, SHA-256 und ZIP-CRC; vollständiges App-/UxPlay-Quellarchiv |
| 4K/H.265, PIN, Autostart nach Windows-Anmeldung | OFFEN | Implementiert, keine separate Geräte-/Neuanmeldungsabnahme |

Beim ersten Start des Minimalpakets fehlte das Plugin `level`. Der Starttest hat dies erkannt. Das Plugin wurde in die Paketierung und den Runtime-Test aufgenommen, danach initialisierten die Audio- und Videopipelines erfolgreich. Dieser Fehler ist behoben.

Integrationsänderungen betreffen Prozesssteuerung, Fenster, Mute, Screenshot und Live-Logs. Keine Neuentwicklung des AirPlay-Handshakes. Die direkte Verwendung eines prozessfremden Qt-HWND lieferte zunächst kein Bild; dies wurde durch ein UxPlay-eigenes natives Kindfenster behoben. Testbild und reale iPhone-Abnahme bestätigen die endgültige Anbindung.

Lokale Entwicklungslogs unter `.tools/`; laufender Empfänger unter `%LOCALAPPDATA%\PC-Mirror\logs\prototype.stdout.log` und `prototype.stderr.log`. Beim manuellen Start über `Start.cmd` wird die kombinierte Ausgabe nach `prototype.log` geschrieben.

## Version 0.2.0 – UI und Updates (24.10.2026)

| Prüfung | Ergebnis | Nachweis |
|---|---|---|
| Native neue UI gebaut | BESTANDEN | Qt/C++ mit dunkler nativer Titelleiste, neutrale Flächen und Linien-Icons |
| Wartebildschirm / Einstellungen | VISUELL GEPRÜFT | Echte Fensterscreenshots unter `docs/screenshots`; Computer Use auf Escape des Benutzers beendet |
| Spiegelungsbackend unverändert | BESTANDEN | `uxplay.exe` bytegleich zu Version 0.1.0 der vollständigen Qt-App (SHA-256 `B8B89DDAC2ED0BE4C46D1B4533D731BFEDE5112E032A619B7AC506F013BA1E49`) |
| Neustart-Sperre bei Spiegelung, „Später“, keine ungefragte Installation | BESTANDEN | CTest `update-policy`, verwendet dieselbe Policy wie der echte UpdateService |
| Echte Velopack-SHA-256-Ablehnung | BESTANDEN | Gleich großes Paket mit verändertem Byte wurde vom nativen SDK abgewiesen |
| Echte Update-Installation und Neustart | BESTANDEN | Isolierte Velopack-Testinstallation 0.2.0 → 0.2.1; ersetzte Dateien und Version nach Neustart geprüft, Testinstallation entfernt |
| Produktions-UpdateService und Hilfsprozess | BESTANDEN | Test gegen `Oelweb/pc-mirror` aus dem installierten Paket, Abschluss ohne Banner, Status „Die App ist aktuell.“ |
| Velopack-Installer / installierte Runtime | BESTANDEN | Installer-Exit 0, 15 GStreamer-Elemente ohne Entwicklungs-PATH geladen |
| Audio-/Video- und Screenshot-Regression | BESTANDEN | Echte synthetische Frames, Mute-Bestätigungen, PNG-Vorstufenpixel/UTF-8-Pfade, Fehlerpfad, Stop und Pipe-Ende |
| GitHub-Release mit echter neuer Version / Actions-Job | NOCH OFFEN | Erst nach Übernahme und Veröffentlichung im Benutzer-Repository prüfbar; nichts remote veröffentlicht |

Die native Update-DLL stammt aus Velopack 1.2.161, SDK-Archiv SHA-256 `31de8f2cf05a47bd5328a9f98b0c41aec1582156f6075f1058b26941f7f17c20`. Checks ohne Ergebnis erzeugen ausschließlich Logeinträge. Der Hilfsprozess prüft den Eigentümer über Windows-Dateiidentität statt bloßer Pfadstrings, damit auch Windows-/MSIX-Pfadaliase funktionieren.
