# PC-Mirror

Native Qt-6-Widgets-Oberfläche (C++17), UxPlay als separater nativer Prozess. Kein Browser, keine HTML-Oberfläche und kein lokaler Webserver zur Bedienung. Die AirPlay-Protokollports des Empfängers sind erforderlich.

UxPlay ist auf Commit `116e39069e40f09350a372233937a55c890a02f8` fixiert. Der Quellcode liegt unter `third_party/uxplay`. Version 1.74 ist upstream als experimentell gekennzeichnet. Windows-Build entsprechend Upstream mit MSYS2 UCRT64; `NO_MARCH_NATIVE=ON` verhindert eine Bindung an die Entwickler-CPU.

Der integrierte mDNS-Responder kündigt `_airplay._tcp` und `_raop._tcp` an. Dadurch braucht diese Konfiguration keinen Bonjour-Dienst. UDP 5353 und fest konfigurierte TCP/UDP-Ports 48000–48002 müssen im privaten lokalen Netzwerk erreichbar sein. Eine alternative Bonjour-Konfiguration ist nicht mit diesem Build vermischt.

GStreamer übernimmt H.264-Decodierung und WASAPI-Audio. D3D11 wird gegenüber D3D12 als konservative Windows-Ausgabe bevorzugt. H.265 ist eine spätere optionale Qualitätsstufe; H.264 ist der Kompatibilitätsstandard. AirPlay 2 Multiroom und Android gehören nicht zur ersten Version.

Der separate Prozess begrenzt die Auswirkungen eines Codec-Absturzes. Das abstrakte `ReceiverBackend`-Interface erlaubt spätere andere Empfänger. Ein Windows-Job beendet den Backend-Prozess beim Ende der GUI. Die eigentliche iOS-Kompatibilität wurde durch Benutzerabnahmen von Prototyp, Qt-App und installierter Version bestätigt; Modell-/iOS-übergreifende Tests bleiben separat erforderlich.

Die GUI stellt ein natives Host-HWND bereit. UxPlay erstellt darin ein eigenes natives Kindfenster, dessen Windows-Thread es selbst bedient. GStreamer kann dadurch seinen D3D11-Renderer innerhalb desselben Prozesses betreiben. Die ursprüngliche direkte Übergabe eines prozessfremden Qt-HWND wurde nach dem Einbettungstest ersetzt.

Steuerung über die geerbte anonyme stdin-Pipe (Stop, Mute, Screenshot); Statusereignisse über stdout, keine zusätzlichen Netzwerk-Steuerports. AirPlay-Handshake und Transport bleiben UxPlay-Code. Ein Pad-Probe referenziert einen dekodierten Frame, die Screenshot-Konvertierung läuft außerhalb des Streaming-Threads. Die Datei wird als PNG durch Qt gespeichert.

Einstellungen und Logs liegen pro Benutzer unter `%LOCALAPPDATA%/PC-Mirror`. Autostart verwendet HKCU/Run. Firewall-Diagnose läuft ohne Erhöhung; der eng begrenzte Regel-Helfer fordert UAC an. Das Profil wird nicht automatisch geändert. Die App meldet Betriebsfehler in der UI und schreibt technische Details in die rotierende Logdatei.

Lizenz der eigenen App: GPL-3.0-or-later. UxPlay und Änderungen bleiben offen. Bei Weitergabe müssen vollständiger korrespondierender Quellcode, Buildskripte sowie Lizenz- und Quellcodepflichten aller gebündelten Bibliotheken beachtet werden.

## Version 0.2.0

Die Spiegelungs- und Codec-Implementierung bleibt unverändert. Die UI verwendet neutrale Qt-Widgets und eine native schwebende Werkzeugleiste über dem D3D-Kindfenster, ohne die Videofläche bei Mausbewegung zu skalieren. Vordergrund- und Diagnosefunktionen bleiben im Einstellungsfenster erreichbar.

Velopack 1.2.161 übernimmt Installation und Updates. Die C++-App nutzt dessen native DLL; kein .NET zur Laufzeit. Der separate Update-Helfer hält blockierende GitHub-/Downloadaufrufe aus dem GUI-Prozess fern und endet mit seinem überprüften Eigentümer. Befehle/Ereignisse laufen über geerbte Prozess-Pipes. Keine neue Netzwerk-Steuerschnittstelle.

Automatische Checks beim Start und alle sechs Stunden, standardmäßig an. Ein Download verlangt den expliziten Installationswunsch; die geteilte UpdatePolicy verhindert den Apply-Schritt bei aktiver Spiegelung. Nach einer Ruhefrist wird zuerst UxPlay gestoppt. Velopack wartet anschließend ausdrücklich auf die GUI-Prozess-ID und startet die aktualisierte App neu. Ein aktueller SHA-256 im Feed ist Pflicht, Prüfung und Dateiaustausch bleiben Velopack-Aufgaben. Startup-Auto-Apply des SDK ist deaktiviert.

Das neue flache Velopack-Paket liegt unter `%LOCALAPPDATA%/PCMirror/current`. Die Pfadauflösung unterstützt weiterhin die alte `bin/`-Entwicklungsstruktur. Nutzereinstellungen liegen getrennt davon unter `%LOCALAPPDATA%/PC-Mirror` und überleben Updates.
