# Updates ab Version 0.2.5

Die App verwendet **https://pcmirror.oel.best**. Velopack 1.2.161 übernimmt HTTPS-Feed, Versionsvergleich, Download, SHA-256-Prüfung, Installation und Neustart. Server-Zugangsdaten sind nicht in der App enthalten.

## Neue Version veröffentlichen

1. Änderungen vornehmen und `version.txt` erhöhen. Veröffentlichte Nummern nicht erneut verwenden.
2. Im Projektordner PowerShell öffnen: `scripts/release.ps1`.
3. Das Skript baut inkrementell, führt die Tests aus, sammelt die Runtime und erzeugt Installer, Update-Pakete und App-/Backend-Quellarchiv.
4. Der getrennte Benutzer `pcmirror-deploy` lädt die Dateien über SFTP hoch. Versteckte `.partial`-Dateien werden erst nach vollständigem Upload umbenannt. Der Versionsfeed wird zuletzt atomar ausgetauscht und danach öffentlich über HTTPS geprüft.
5. Nutzer finden die Version beim Start, bei der regelmäßigen Prüfung oder über „Nach Updates suchen“. Weitere GitHub-Releases sind nicht nötig.

Nur bauen: `scripts/release.ps1 -NoPublish`. Bereits gebaute Dateien erneut veröffentlichen:

```powershell
python scripts/publish-server.py --release-directory dist/server-release
```

Die letzte vollständige `.nupkg` und den Feed in `dist/server-release` behalten. Velopack erzeugt daraus mit `BestSpeed` kleine Delta-Pakete. Ohne vorheriges Paket ist der vollständige Release weiterhin gültig. Deltas beschleunigen vor allem Downloads bei bestehenden Nutzern; der vollständige Installer und das vollständige Paket werden weiterhin veröffentlicht.

## Zugang auf dem Entwicklungs-PC

Die lokale Datei `.tools/ssh/publish.json` enthält Host, Port, Benutzer, Zielordner, öffentliche URL und Pfade für Schlüssel und bekannte Hostschlüssel. Sie und private Schlüssel liegen außerhalb der Quellarchive. Normale Veröffentlichungen benötigen kein Root-Passwort. Der Upload-Schlüssel ist auf SFTP beschränkt.

Auf einem neuen Entwicklungs-PC einen eigenen Upload-Schlüssel hinterlegen und die lokale Datei erstellen:

```json
{
  "host": "SERVER-IP",
  "port": 22,
  "user": "pcmirror-deploy",
  "directory": "/srv/pcmirror-updates",
  "url": "https://pcmirror.oel.best",
  "key": "C:/Users/NAME/.ssh/pcmirror-deploy",
  "known_hosts": "C:/Users/NAME/.ssh/known_hosts"
}
```

Die letzte vollständige `.nupkg` und `releases.win.json` von der Update-Adresse nach `dist/server-release` laden, damit Deltas erzeugt werden können.

## Quellen und Lizenzen

Jeder Release enthält `PC-Mirror-VERSION-source.zip` mit App, modifiziertem UxPlay-Backend und Buildskripten. Für unveränderte externe Abhängigkeiten bleibt das [Runtime-Quellarchiv](https://pcmirror.oel.best/PC-Mirror-0.2.1-runtime-sources.zip) verfügbar. Stimmen die Paketversionen/Quell-URLs in `runtime-sources.json` nicht mehr mit der bisherigen Version überein, mit `scripts/fetch_runtime_sources.py` neue Quellen sammeln, das neue Archiv hochladen und die Referenz aktualisieren. Quellen behalten, solange zugehörige Binärversionen verteilt werden.

## Server

Vorlagen: `deployment/server`. Stack: `/opt/stacks/pcmirror-updates`. Persistente Dateien: `/srv/pcmirror-updates`. Der Nginx-Container `pcmirror-updates` liest die Downloads nur und hat keine zusätzliche öffentliche Docker-Portfreigabe. Er hängt am vorhandenen Netzwerk `proxy`. Nginx Proxy Manager leitet `pcmirror.oel.best` an `http://pcmirror-updates:8080` weiter und verwaltet HTTPS über Let's Encrypt.

Der Feed erhält `Cache-Control: no-store`; versionierte Pakete dürfen lange gecacht werden. Teil-Uploads und Verzeichnisauflistungen sind nicht öffentlich. Status: `docker compose -f /opt/stacks/pcmirror-updates/compose.yaml ps`; anschließend Container-Logs und NPM-Hoststatus prüfen. Bei Fehlern den bisherigen Feed und die bisherigen Pakete erhalten.

## Übergang von GitHub

Versionen bis 0.2.4 prüfen GitHub. Ein letztes GitHub-Release **0.2.5** stellt sie auf den eigenen HTTPS-Server um. Alternativ den [aktuellen Installer](https://pcmirror.oel.best/PCMirror-win-Setup.exe) installieren. Einstellungen und Empfängeridentität bleiben erhalten. Der frühere GitHub-Actions-/Tag-Ablauf ist für weitere Server-Veröffentlichungen nicht erforderlich.

## Verhalten in der App

Automatische Prüfung nach dem Start und alle sechs Stunden, manuelle Prüfung in den Einstellungen. Erst „Jetzt installieren“ löst den Download aus. Velopack prüft Größe/SHA-256 vor der Installation; das ist keine zusätzliche Authenticode-Signatur. Während des Spiegelns kein Neustart; die bestehende Ruhefrist und „Später“ bleiben wirksam. Fehlgeschlagene Prüfungen bleiben still, werden protokolliert und blockieren weder Oberfläche noch Empfang.
