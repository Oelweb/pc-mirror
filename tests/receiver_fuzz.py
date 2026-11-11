"""Robustheitstest: kaputte und zufällige Eingaben aus dem Netz dürfen den
Empfänger nicht beenden. Danach muss er weiter normal antworten."""
import argparse
import os
import random
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from airplay_e2e_window import host_window  # noqa: E402

parser = argparse.ArgumentParser()
parser.add_argument("--package", type=Path, required=True)
parser.add_argument("--receiver", default="uxplay.exe")
parser.add_argument("--rounds", type=int, default=400)
args = parser.parse_args()
package = args.package.resolve()
rng = random.Random(4711)
PORT = 48100
env = os.environ.copy()
env.update({"PATH": str(package) + ";C:/Windows/System32;C:/Windows",
            "GST_PLUGIN_SYSTEM_PATH_1_0": str(package / "lib/gstreamer-1.0"), "PCMIRROR_EMBEDDED": "1",
            "PCMIRROR_HWND": str(host_window())})


def rtsp(method, url, body=b"", headers=None, raw=None):
    data = raw
    if data is None:
        head = f"{method} {url} RTSP/1.0\r\nCSeq: {rng.randint(0, 99)}\r\n"
        for k, v in (headers or {}).items():
            head += f"{k}: {v}\r\n"
        if body:
            head += f"Content-Length: {len(body)}\r\n"
        data = head.encode() + b"\r\n" + body
    try:
        with socket.create_connection(("127.0.0.1", PORT + 1), timeout=2) as s:
            s.sendall(data)
            s.settimeout(1)
            try:
                return s.recv(65536)
            except socket.timeout:
                return b""
    except OSError:
        return b""


def junk(n):
    return bytes(rng.getrandbits(8) for _ in range(n))


with tempfile.TemporaryDirectory() as folder:
    proc = subprocess.Popen([str(package / args.receiver), "-n", "Fuzz", "-nh", "-p", str(PORT), "-vs", "fakesink",
                             "-as", "fakesink", "-key", str(Path(folder) / "k.pem"), "-pin", "1234"],
                            stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=env,
                            cwd=folder, creationflags=subprocess.CREATE_NO_WINDOW)
    try:
        time.sleep(3)
        plist_ct = {"Content-Type": "application/x-apple-binary-plist"}
        octet = {"Content-Type": "application/octet-stream"}
        for i in range(args.rounds):
            choice = i % 12
            if choice == 0:
                rtsp(None, None, raw=junk(rng.randint(1, 3000)))
            elif choice == 1:
                rtsp("POST", "/fp-setup", junk(rng.choice([16, 164, 15, 200])), octet)
            elif choice == 2:
                rtsp("POST", "/pair-verify", bytes([rng.choice([0, 1, 2])]) + junk(rng.choice([67, 3, 100])), octet)
            elif choice == 3:
                rtsp("SETUP", "rtsp://x", b"bplist00" + junk(rng.randint(0, 400)), plist_ct)
            elif choice == 4:
                rtsp("POST", "/pair-setup-pin", b"bplist00" + junk(rng.randint(0, 300)), plist_ct)
            elif choice == 5:
                rtsp("GET", "/info", junk(rng.randint(0, 50)), plist_ct)
            elif choice == 6:
                rtsp("SET_PARAMETER", "x", b"volume: " + junk(10), {"Content-Type": "text/parameters"})
            elif choice == 7:
                rtsp("FLUSH", "x", headers={"RTP-Info": "seq=" + junk(5).hex()})
            elif choice == 8:
                rtsp("TEARDOWN", "x", b"bplist00" + junk(80), plist_ct)
            elif choice == 9:
                rtsp(None, None, raw=b"GET /info RTSP/1.0\r\nContent-Length: 999999999\r\n\r\n")
            elif choice == 10:
                rtsp(None, None, raw=b"GET /info RTSP/1.0\r\n" + b"X: y\r\n" * 50 + b"\r\n")
            else:
                with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as u:
                    for port in (5353, PORT, PORT + 1, PORT + 2):
                        u.sendto(junk(rng.randint(1, 1400)), ("127.0.0.1", port))
            if proc.poll() is not None:
                raise SystemExit(f"FAIL Empfänger beendet nach Runde {i} (Fall {choice}), Code {proc.returncode}")
        # Videoport mit Müll (falls offen)
        try:
            with socket.create_connection(("127.0.0.1", PORT), timeout=1) as s:
                s.sendall(junk(128) + junk(5000))
        except OSError:
            pass
        time.sleep(1)
        assert proc.poll() is None, "Empfänger beendet"
        reply = rtsp("GET", "/info")
        assert reply.startswith(b"RTSP/1.0 200 OK"), reply[:80]
        print(f"PASS {args.rounds} kaputte Eingaben überstanden, Empfänger antwortet weiter ({args.receiver})")
    finally:
        try:
            proc.stdin.write(b"STOP\n")
            proc.stdin.flush()
            proc.wait(timeout=5)
        except Exception:
            proc.kill()
