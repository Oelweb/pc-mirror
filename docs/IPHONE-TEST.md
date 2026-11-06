# Abnahme mit echtem iPhone

Ein erfolgreicher Build oder ein synthetisches Testbild belegt keine AirPlay-Kompatibilität. Jede Stufe wird separat protokolliert. Ohne Rückmeldung vom echten iPhone bleibt sie **OFFEN**.

## Stufe 1 – Empfänger, vor dem UI-Ausbau

1. Windows 10/11 x64, PC per Ethernet oder WLAN am selben Router wie das iPhone. Kein Gast-WLAN, keine Client-Isolation, VPN vorübergehend ausschalten. Ethernet am PC ist zulässig.
2. Windows-Netzwerkprofil muss „Privat“ sein. Das Profil wird nicht automatisch geändert.
3. Firewall-Regeln mit `scripts/Configure-Firewall.ps1 -ReceiverPath <absoluter Pfad zu build/backend/uxplay.exe>` als Administrator einrichten.
4. `prototype/Start-Prototype.ps1` starten. GStreamer/D3D11 und WASAPI müssen ohne Fehler initialisieren, beide mDNS-Dienste müssen registriert werden.
5. iPhone entsperren → Kontrollzentrum → Bildschirmsynchronisierung → **PC-Mirror**. Zeit bis zur Sichtbarkeit und Verbindungsdauer notieren.
6. Home-Bildschirm und eine normale App anzeigen. Fingerbewegung/Scrollen am PC vergleichen. Hoch-/Querformat mehrfach wechseln (Ausrichtungssperre aus). Kein Strecken, kein Absturz.
7. Einen eigenen, ungeschützten Clip mit Ton abspielen. PC-Ausgabe, Synchronität und Verzögerung beurteilen. Geschützte Videoinhalte sind kein geeigneter Funktionstest.
8. Am iPhone Synchronisierung beenden, erneut verbinden. Mindestens zwei Minuten streamen.

| Datum | Windows/GPU | iPhone / iOS | Sichtbar | Bild | Ton | Drehung | Wiederverbinden | Ergebnis |
|---|---|---|---|---|---|---|---|---|
| 24.10.2026 | lokaler Entwicklungs-PC, Windows x64 | Modell/iOS nicht gemeldet | Ja | Ja | Ja | Ja | im Qt-Test bestätigt | Prototyp und Qt-App laut Benutzer bestanden |

## Stufe 2 – eingebettetes natives Fenster

Nach bestandener Stufe 1 dieselbe Prüfung im App-Fenster wiederholen. Größenänderung, Maximieren, F11 und Doppelklick testen. Fünf Formatwechsel, Fenster minimieren/wiederherstellen. Prüfen, dass das Bild im selben Fenster bleibt.

## Stufe 3 – Bedienung und Installation

Screenshot gegen sichtbares Bild vergleichen, Ton aus/ein während Wiedergabe, Trennen/erneut verbinden, immer im Vordergrund, Einstellungsänderung, PIN richtig/falsch, Autostart nach Anmeldung. Installer auf einem sauberen Windows ohne Qt, MSYS2, GStreamer und Bonjour testen. Firewall-UAC abbrechen und verständlichen Fehler prüfen. LAN trennen und wiederherstellen. Log und UI-Fehler bei fehlendem Backend/Codec prüfen.

Erst nach diesen Prüfungen ist eine Veröffentlichung als getesteter AirPlay-Empfänger gerechtfertigt.

## Rückmeldungen vom 24.10.2026

- Stufe 1: Benutzer bestätigt Sichtbarkeit, Bild, Ton und Drehung.
- Stufe 2/Bedienung: Benutzer bestätigt „Alles funktioniert im neuen Fenster, auch die Bedienung.“ Die Anfrage enthielt Screenshot, Mute, Trennen und erneutes Verbinden.
- Installierte Version aus dem Installer: Benutzer bestätigt „Ja, die installierte Version funktioniert.“
- Noch separat offen: sauberer Zweitrechner, weitere iOS-/GPU-Kombinationen, gemessene Dauerlast/Latenz, experimentelles H.265, PIN-Varianten und Autostart nach Neuanmeldung. Dafür gelten die obigen Abläufe; sie werden nicht als bereits bestanden ausgegeben.
