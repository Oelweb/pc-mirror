"""Read the DLL import names of a Windows PE file (normal and delay-load imports).

Pure Python, so packaging no longer needs MSYS2's objdump.
"""
from __future__ import annotations
from pathlib import Path
import struct


def imports(path: Path) -> list[str]:
    data = Path(path).read_bytes()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError(f"Not a PE file: {path}")
    sections = struct.unpack_from("<H", data, pe + 6)[0]
    optional_size = struct.unpack_from("<H", data, pe + 20)[0]
    optional = pe + 24
    magic = struct.unpack_from("<H", data, optional)[0]
    directories = optional + (112 if magic == 0x20B else 96)
    import_rva = struct.unpack_from("<I", data, directories + 1 * 8)[0]
    delay_rva = struct.unpack_from("<I", data, directories + 13 * 8)[0]
    table = []
    first_section = optional + optional_size
    for index in range(sections):
        virtual_size, address, raw_size, raw = struct.unpack_from("<IIII", data, first_section + index * 40 + 8)
        table.append((address, max(virtual_size, raw_size), raw))

    def offset(rva: int) -> int:
        for address, size, raw in table:
            if address <= rva < address + size:
                return rva - address + raw
        raise ValueError(f"RVA {rva:#x} outside sections in {path}")

    def text(rva: int) -> str:
        start = offset(rva)
        return data[start:data.index(b"\0", start)].decode("ascii")

    names: list[str] = []
    if import_rva:
        position = offset(import_rva)
        while True:
            name_rva = struct.unpack_from("<5I", data, position)[3]
            if name_rva == 0:
                break
            names.append(text(name_rva))
            position += 20
    if delay_rva:
        position = offset(delay_rva)
        while True:
            name_rva = struct.unpack_from("<8I", data, position)[1]
            if name_rva == 0:
                break
            names.append(text(name_rva))
            position += 32
    return names


def is_system(name: str) -> bool:
    lower = name.lower()
    if lower.startswith(("api-ms-win-", "ext-ms-win-")):
        return True
    return (Path("C:/Windows/System32") / name).is_file()


if __name__ == "__main__":
    import sys
    for file in sys.argv[1:]:
        print(file, imports(Path(file)))
