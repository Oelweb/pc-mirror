"""Copy the unchanged receiver runtime (uxplay.exe, GStreamer, codecs) from an
existing, previously verified package into a new package directory.

Used when the backend did not change and no MSYS2 build environment is
installed. Only the dependency closure of the receiver is copied; the old Qt
application files are left behind. Licenses and source manifests are kept.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import shutil

from pe_imports import imports, is_system

TOOLS = ("gst-inspect-1.0.exe", "gst-launch-1.0.exe")


def reuse(source: Path, destination: Path) -> None:
    source, destination = source.resolve(), destination.resolve()
    if not (source / "uxplay.exe").is_file():
        raise SystemExit(f"{source} enthält kein flaches Paket mit uxplay.exe")
    destination.mkdir(parents=True, exist_ok=True)
    queue = [source / "uxplay.exe", source / "libexec/gstreamer-1.0/gst-plugin-scanner.exe"]
    queue += [source / tool for tool in TOOLS]
    queue += sorted((source / "lib/gstreamer-1.0").glob("*.dll"))
    seen: set[str] = set()
    manifest = []
    while queue:
        file = queue.pop(0)
        key = str(file).lower()
        if key in seen:
            continue
        seen.add(key)
        if not file.is_file():
            raise SystemExit(f"Fehlt im Quellpaket: {file}")
        target = destination / file.relative_to(source)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(file, target)
        manifest.append({"file": target.relative_to(destination).as_posix(),
                         "sha256": hashlib.sha256(target.read_bytes()).hexdigest()})
        for name in imports(file):
            dependency = source / name
            if dependency.is_file():
                queue.append(dependency)
            elif not is_system(name):
                raise SystemExit(f"Nicht auflösbarer Import: {file.name} -> {name}")
    for item in ("licenses", "runtime-sources.json", "runtime-packages.txt"):
        path = source / item
        if path.is_dir():
            shutil.copytree(path, destination / item, dirs_exist_ok=True)
        elif path.is_file():
            shutil.copy2(path, destination / item)
    (destination / "runtime-manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print(f"Runtime übernommen: {len(manifest)} native Dateien aus {source}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--destination", required=True, type=Path)
    arguments = parser.parse_args()
    reuse(arguments.source, arguments.destination)
