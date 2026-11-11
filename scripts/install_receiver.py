"""Setzt den Rust-Empfänger ins Paket ein.

Der Rust-Empfänger wird als `uxplay.exe` abgelegt (gleicher Pfad wie bisher, damit
vorhandene Firewall-Freigaben weiter gelten). Der bisherige C-Empfänger aus der
Runtime bleibt als `uxplay-classic.exe` erhalten (Ausweichlösung in den
Einstellungen). Die Prüfsummenliste `runtime-manifest.json` wird angepasst.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import shutil

from pe_imports import imports, is_system


def install(package: Path, receiver: Path) -> None:
    package = package.resolve()
    manifest_path = package / "runtime-manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    classic = package / "uxplay-classic.exe"
    current = package / "uxplay.exe"
    if current.is_file() and not classic.is_file():
        current.replace(classic)
        for entry in manifest:
            if entry["file"] == "uxplay.exe":
                entry["file"] = "uxplay-classic.exe"
    foreign = [name for name in imports(receiver) if not is_system(name)]
    if foreign:
        raise SystemExit(f"Rust-Empfänger braucht fremde DLLs: {foreign}")
    shutil.copy2(receiver, current)
    manifest = [entry for entry in manifest if entry["file"] != "uxplay.exe"]
    manifest.append({"file": "uxplay.exe", "sha256": hashlib.sha256(current.read_bytes()).hexdigest()})
    manifest_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print(f"Rust-Empfänger eingesetzt: uxplay.exe (klassisch: {'ja' if classic.is_file() else 'nein'})")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", type=Path, required=True)
    parser.add_argument("--receiver", type=Path, required=True)
    arguments = parser.parse_args()
    install(arguments.package, arguments.receiver)
