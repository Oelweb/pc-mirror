"""Write license notices for all Rust crates linked into PC-Mirror.exe.

Uses `cargo metadata`; copies each crate's license files from the local
Cargo registry so the shipped package carries the full texts.
"""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parent.parent


def main(output: Path) -> None:
    cargo = shutil.which("cargo") or str(Path(os.environ["USERPROFILE"]) / ".cargo/bin/cargo.exe")
    metadata = json.loads(subprocess.run(
        [cargo, "metadata", "--format-version", "1", "--filter-platform", "x86_64-pc-windows-msvc",
         "--manifest-path", str(root / "Cargo.toml")],
        check=True, capture_output=True, text=True, encoding="utf-8").stdout)
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    start = next(p["id"] for p in metadata["packages"] if p["name"] == "pcmirror")
    # Nur normale Abhängigkeiten (keine Build-/Dev-Abhängigkeiten) gelangen ins Programm.
    pending, linked = [start], set()
    while pending:
        current = pending.pop()
        if current in linked:
            continue
        linked.add(current)
        for dependency in nodes[current]["deps"]:
            if any(kind["kind"] is None for kind in dependency["dep_kinds"]):
                pending.append(dependency["pkg"])
    parts = ["Lizenzen der in PC-Mirror.exe enthaltenen Rust-Bibliotheken\n" + "=" * 60 + "\n"]
    for package in sorted((packages[i] for i in linked), key=lambda p: (p["name"], p["version"])):
        if package["source"] is None:
            continue  # eigene Crates: GPL-3.0-or-later, siehe LICENSE
        folder = Path(package["manifest_path"]).parent
        parts.append(f"\n## {package['name']} {package['version']}\nLizenz: {package.get('license') or 'siehe Text'}\n"
                     f"Quelle: {package.get('repository') or ''}\n")
        for file in sorted(folder.iterdir()):
            if file.is_file() and file.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE")):
                parts.append(f"\n--- {file.name} ---\n{file.read_text(encoding='utf-8', errors='replace')}\n")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text("".join(parts), encoding="utf-8")
    print(f"{len(linked) - 2} Rust-Abhängigkeiten dokumentiert: {output}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    main(parser.parse_args().output)
