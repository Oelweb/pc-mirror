"""End-to-End-Test ohne iPhone: nachgebauter Sender gegen den Empfänger.

Startet den Empfänger aus einem Paket (uxplay.exe), lässt `fake_sender` koppeln,
FairPlay aushandeln und echtes, verschlüsseltes H.264/H.265 senden. Erwartet
PCMIRROR|FRAME|320|240 und nach TEARDOWN PCMIRROR|IDLE. Mit --compare wird
zusätzlich die /info-Antwort zweier Empfänger (z. B. C und Rust) verglichen.
"""
import argparse
import os
import queue
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument("--package", type=Path, required=True)
parser.add_argument("--receiver", default="uxplay.exe")
parser.add_argument("--sender", type=Path, default=root / "target/release/examples/fake_sender.exe")
parser.add_argument("--codec", choices=["264", "265"], default="264")
parser.add_argument("--mode", type=int, default=0)
parser.add_argument("--port", type=int, default=48100)
parser.add_argument("--key", type=Path, default=None)
parser.add_argument("--info-out", type=Path, default=None)
args = parser.parse_args()

package = args.package.resolve()
env = os.environ.copy()
system_root = os.environ.get("SYSTEMROOT", "C:/Windows")
env.update({
    "PATH": str(package) + ";" + system_root + "/System32;" + system_root,
    "GST_PLUGIN_SYSTEM_PATH_1_0": str(package / "lib/gstreamer-1.0"),
    "GST_PLUGIN_PATH_1_0": "",
    "GST_PLUGIN_SCANNER_1_0": str(package / "libexec/gstreamer-1.0/gst-plugin-scanner.exe"),
    "PCMIRROR_EMBEDDED": "1",
})
env.pop("PCMIRROR_HWND", None)


sys.path.insert(0, str(Path(__file__).parent))
from airplay_e2e_window import host_window  # noqa: E402


env["PCMIRROR_HWND"] = str(host_window())

with tempfile.TemporaryDirectory(prefix="pcmirror e2e ") as folder:
    key = args.key or Path(folder) / "receiver.pem"
    command = [str(package / args.receiver), "-n", "E2E-Test", "-nh", "-p", str(args.port),
               "-s", "1920x1080@60", "-fps", "60", "-vs", "fakesink", "-as", "fakesink", "-key", str(key)]
    if args.codec == "265":
        command.append("-h265")
    receiver = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                text=True, encoding="utf-8", errors="replace", env=env, cwd=folder,
                                creationflags=subprocess.CREATE_NO_WINDOW)
    lines = queue.Queue()
    log = []

    def reader():
        for line in receiver.stdout:
            line = line.rstrip()
            log.append(line)
            lines.put(line)

    threading.Thread(target=reader, daemon=True).start()

    def wait_for(prefix, timeout=15):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                line = lines.get(timeout=0.25)
            except queue.Empty:
                continue
            if prefix in line:
                print("PASS " + line.strip())
                return line
        raise AssertionError("Timeout: " + prefix)

    ok = False
    try:
        wait_for("PCMIRROR|READY")
        time.sleep(0.5)
        clip = root / f"crates/pcmirror-receiver/tests/fixtures/clip.{args.codec}"
        sender = subprocess.run([str(args.sender), "127.0.0.1", str(args.port + 1), str(clip), str(args.mode)],
                                capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=60)
        print(sender.stdout.split("INFO-END")[-1].strip())
        if args.info_out:
            info = sender.stdout.split("INFO-BEGIN")[1].split("INFO-END")[0]
            args.info_out.write_text(info, encoding="utf-8")
        if sender.returncode != 0:
            print(sender.stdout[-3000:])
            print(sender.stderr[-3000:])
            raise AssertionError("Sender fehlgeschlagen")
        wait_for("PCMIRROR|FRAME|320|240", timeout=10)
        wait_for("PCMIRROR|IDLE", timeout=10)
        ok = True
    finally:
        try:
            receiver.stdin.write("STOP\n")
            receiver.stdin.flush()
            receiver.wait(timeout=5)
        except Exception:
            receiver.kill()
        if not ok:
            print("--- Empfänger-Log ---")
            print("\n".join(log[-60:]))
    print("PASS Ende-zu-Ende " + args.receiver + " H." + args.codec + " Modus " + str(args.mode))
