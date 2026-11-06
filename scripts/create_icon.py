"""Create the native Windows icon from the same simple vector geometry as Qt."""
from pathlib import Path
import struct
import zlib

root = Path(__file__).resolve().parent.parent
root.joinpath("assets").mkdir(exist_ok=True)


def rounded(x, y, left, top, width, height, radius):
    if not (left <= x <= left + width and top <= y <= top + height):
        return False
    dx = max(left + radius - x, 0, x - (left + width - radius))
    dy = max(top + radius - y, 0, y - (top + height - radius))
    return dx * dx + dy * dy <= radius * radius


def pixel(x, y):
    if not rounded(x, y, 0, 0, 64, 64, 8):
        return (0, 0, 0, 0)
    blue, white = (47, 123, 255, 255), (255, 255, 255, 255)
    color = blue
    for left, top in ((14, 16), (24, 26)):
        if rounded(x, y, left, top, 26, 22, 4):
            color = white if not rounded(x, y, left + 1.5, top + 1.5, 23, 19, 2.5) else blue
    return color


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


def png(size):
    raw = bytearray()
    for y in range(size):
        raw.append(0)
        for x in range(size):
            samples = [pixel((x + sx) * 64 / size, (y + sy) * 64 / size)
                       for sx in (.25, .75) for sy in (.25, .75)]
            raw.extend(round(sum(sample[channel] for sample in samples) / 4) for channel in range(4))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


images = [(size, png(size)) for size in (32, 64, 256)]
offset = 6 + len(images) * 16
entries = []
for size, data in images:
    entries.append(struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset))
    offset += len(data)
(root / "assets/app.ico").write_bytes(struct.pack("<HHH", 0, 1, len(images)) + b"".join(entries) + b"".join(data for _, data in images))
