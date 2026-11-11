"""Fragt die mDNS-Ankündigung eines laufenden Empfängers ab (wie ein iPhone) und
gibt Dienste, TXT-Einträge, Port und Adressen sortiert aus. Zum Vergleich C/Rust."""
import argparse
import os
import socket
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("--package", type=Path, required=True)
parser.add_argument("--receiver", default="uxplay.exe")
parser.add_argument("--key", type=Path, required=True)
parser.add_argument("--pin", action="store_true")
args = parser.parse_args()
sys.path.insert(0, str(Path(__file__).parent))


def query(name, qtype=12):
    q = struct.pack(">HHHHHH", 0, 0, 1, 0, 0, 0)
    for part in name.split("."):
        q += bytes([len(part)]) + part.encode()
    return q + b"\0" + struct.pack(">HH", qtype, 1)


def read_name(data, offset):
    labels, jumped, end = [], False, offset
    while True:
        length = data[offset]
        if length == 0:
            offset += 1
            break
        if length & 0xC0 == 0xC0:
            if not jumped:
                end = offset + 2
            offset = ((length & 0x3F) << 8) | data[offset + 1]
            jumped = True
            continue
        labels.append(data[offset + 1:offset + 1 + length].decode("utf-8", "replace"))
        offset += 1 + length
    return ".".join(labels), (end if jumped else offset)


def records(packet):
    answers = []
    count = sum(struct.unpack(">HHH", packet[6:12]))
    offset = 12
    for _ in range(struct.unpack(">H", packet[4:6])[0]):
        _, offset = read_name(packet, offset)
        offset += 4
    for _ in range(count):
        name, offset = read_name(packet, offset)
        rtype, _cls, _ttl, length = struct.unpack(">HHIH", packet[offset:offset + 10])
        offset += 10
        data = packet[offset:offset + length]
        if rtype == 16:
            items, i = [], 0
            while i < len(data):
                items.append(data[i + 1:i + 1 + data[i]].decode("utf-8", "replace"))
                i += 1 + data[i]
            answers.append(("TXT", name, tuple(sorted(items))))
        elif rtype == 33:
            port = struct.unpack(">H", data[4:6])[0]
            answers.append(("SRV", name, port))
        elif rtype == 12:
            answers.append(("PTR", name, read_name(packet, offset)[0]))
        elif rtype == 1:
            answers.append(("A", name, socket.inet_ntoa(data)))
        offset += length
    return answers


package = args.package.resolve()
env = os.environ.copy()
env.update({"PATH": str(package) + ";C:/Windows/System32;C:/Windows",
            "GST_PLUGIN_SYSTEM_PATH_1_0": str(package / "lib/gstreamer-1.0"), "PCMIRROR_EMBEDDED": "1"})
from airplay_e2e_window import host_window  # noqa: E402
env["PCMIRROR_HWND"] = str(host_window())
with tempfile.TemporaryDirectory() as folder:
    command = [str(package / args.receiver), "-n", "MDNS-Vergleich", "-nh", "-p", "48100", "-vs", "fakesink",
               "-as", "fakesink", "-key", str(args.key)]
    if args.pin:
        command += ["-pin", "1234"]
    receiver = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                env=env, cwd=folder, creationflags=subprocess.CREATE_NO_WINDOW)
    try:
        time.sleep(3)
        found = set()
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.bind(("0.0.0.0", 0))
        sock.settimeout(0.5)
        for service in ("_airplay._tcp.local", "_raop._tcp.local"):
            sock.sendto(query(service), ("224.0.0.251", 5353))
            end = time.time() + 1.5
            while time.time() < end:
                try:
                    data, _ = sock.recvfrom(4096)
                except socket.timeout:
                    continue
                for record in records(data):
                    if "MDNS-Vergleich" in str(record) or record[0] == "A":
                        found.add(record)
        for record in sorted(found, key=str):
            print(record)
    finally:
        receiver.stdin.write(b"STOP\n")
        receiver.stdin.flush()
        receiver.wait(timeout=5)
