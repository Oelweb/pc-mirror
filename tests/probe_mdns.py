"""Send a real mDNS PTR query and verify AirPlay + RAOP advertisements.

This checks local discovery only, not iOS pairing, decoding or media playback.
"""
import argparse
import socket
import struct
import time


def name(data, offset, depth=0):
    if depth > 20:
        raise ValueError("DNS pointer cycle")
    labels = []
    while True:
        size = data[offset]
        offset += 1
        if size == 0:
            return ".".join(labels), offset
        if size & 0xC0 == 0xC0:
            pointer = ((size & 0x3F) << 8) | data[offset]
            value, _ = name(data, pointer, depth + 1)
            labels.append(value)
            return ".".join(labels), offset + 1
        if size > 63 or offset + size > len(data):
            raise ValueError("Invalid DNS label")
        labels.append(data[offset:offset + size].decode("utf-8", "replace"))
        offset += size


def records(data):
    _, flags, questions, answers, authority, additional = struct.unpack_from("!6H", data)
    if not flags & 0x8000:
        return []
    offset = 12
    result = []
    for _ in range(questions):
        _, offset = name(data, offset)
        offset += 4
    for _ in range(answers + authority + additional):
        owner, offset = name(data, offset)
        kind, _, ttl, length = struct.unpack_from("!HHIH", data, offset)
        offset += 10
        if kind == 12:
            value, _ = name(data, offset)
            result.append((owner, value, ttl))
        offset += length
    return result


def query(interface, device, timeout):
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind(("", 5353))
    sock.setsockopt(socket.IPPROTO_IP, socket.IP_ADD_MEMBERSHIP,
                    socket.inet_aton("224.0.0.251") + socket.inet_aton(interface))
    sock.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_IF, socket.inet_aton(interface))
    sock.settimeout(0.5)
    services = ("_airplay._tcp.local", "_raop._tcp.local")
    found = set()
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline and len(found) != 2:
        for service in services:
            qname = b"".join(bytes([len(part)]) + part.encode() for part in service.split(".")) + b"\0"
            packet = struct.pack("!6H", 0, 0, 1, 0, 0, 0) + qname + struct.pack("!HH", 12, 1)
            sock.sendto(packet, ("224.0.0.251", 5353))
        try:
            data, peer = sock.recvfrom(65535)
        except socket.timeout:
            continue
        for owner, value, ttl in records(data):
            if owner in services and device in value and ttl:
                if owner not in found:
                    print(f"{peer[0]}: {owner} -> {value} (TTL {ttl})")
                found.add(owner)
    sock.close()
    if len(found) != 2:
        raise SystemExit(f"FAIL: only {len(found)}/2 services found; firewall/multicast or receiver needs diagnosis")
    print("PASS: real mDNS AirPlay and RAOP records received. iPhone test still required.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--interface", required=True, help="Local adapter IPv4")
    parser.add_argument("--name", default="PC-Mirror")
    parser.add_argument("--timeout", type=float, default=8)
    args = parser.parse_args()
    query(args.interface, args.name, args.timeout)
