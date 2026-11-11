"""Prüft den automatischen Rückfall der App auf den klassischen Empfänger:
Ein nachgebauter Sender fordert die Spiegelung an, sendet aber kein Bild.
Nach der Frist muss die App auf `uxplay-classic.exe` wechseln.
Die App läuft mit eigenem LOCALAPPDATA (echte Einstellungen bleiben unberührt)."""
import argparse
import os
import subprocess
import tempfile
import time
from pathlib import Path

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument("--package", type=Path, required=True)
parser.add_argument("--sender", type=Path, default=root / "target/release/examples/fake_sender.exe")
args = parser.parse_args()
package = args.package.resolve()

with tempfile.TemporaryDirectory(prefix="pcmirror fallback ", ignore_cleanup_errors=True) as local:
    data = Path(local) / "PC-Mirror"
    data.mkdir()
    (data / "settings.ini").write_text("[ui]\nonboardingDone=true\nconnectionVerified=true\n", encoding="utf-8")
    env = os.environ.copy()
    env["LOCALAPPDATA"] = local
    app = subprocess.Popen([str(package / "PC-Mirror.exe")], env=env)
    log = data / "logs" / "pc-mirror.log"
    try:
        deadline = time.time() + 30
        while time.time() < deadline and not (log.exists() and "(Rust)" in log.read_text(encoding="utf-8", errors="replace")):
            time.sleep(0.5)
        assert log.exists(), "kein Log"
        time.sleep(3)
        sender_env = dict(os.environ, FAKE_SENDER_NO_VIDEO="1")
        clip = root / "crates/pcmirror-receiver/tests/fixtures/clip.264"
        sender = subprocess.Popen([str(args.sender), "127.0.0.1", "48001", str(clip)], env=sender_env,
                                  stdout=subprocess.PIPE, text=True, encoding="utf-8", errors="replace")
        deadline = time.time() + 45
        switched = False
        while time.time() < deadline:
            text = log.read_text(encoding="utf-8", errors="replace")
            if "Wechsel auf den klassischen Empfänger" in text and "klassisch, C" in text:
                switched = True
                break
            time.sleep(1)
        sender.kill()
        text = log.read_text(encoding="utf-8", errors="replace")
        print("\n".join(line for line in text.splitlines()
                        if any(k in line for k in ("Empfänger", "Spiegelung", "Verbindungsanfrage")))[-1500:])
        assert switched, "Kein Rückfall auf den klassischen Empfänger"
        ini = (data / "settings.ini").read_text(encoding="utf-8")
        assert "classic=true" in ini, ini
        print("PASS Rückfall auf den klassischen Empfänger nach fehlendem Bild, Einstellung gespeichert")
    finally:
        app.terminate()
        try:
            app.wait(timeout=10)
        except subprocess.TimeoutExpired:
            app.kill()
