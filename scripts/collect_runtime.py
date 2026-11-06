"""Collect native PE dependencies, selected GStreamer plugins and their licenses.

No MSYS DLLs or compiler are required on the destination PC. Fails on every
unresolved import unless it is supplied by Windows itself.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess

PLUGINS = (
    "coreelements", "app", "typefindfunctions", "playback", "autodetect",
    "videoconvertscale", "videoparsersbad", "libav", "d3d11", "wasapi",
    "audioconvert", "audioresample", "volume", "level", "audioparsers", "rtp", "videotestsrc",
)


def collect(prefix: Path, receiver: Path, destination: Path, app: Path | None = None, flat: bool = False) -> None:
    native_bin = prefix / "bin"
    objdump = native_bin / "objdump.exe"
    system32 = Path("C:/Windows/System32")
    out_bin = destination if flat else destination / "bin"
    out_bin.mkdir(parents=True, exist_ok=True)
    queue: list[tuple[Path, Path]] = [(receiver, out_bin / "uxplay.exe")]
    if app:
        queue.append((app, out_bin / "PC-Mirror.exe"))
        sdk = app.parent / "velopack_libc.dll"
        if sdk.is_file():
            queue.append((sdk, out_bin / sdk.name))
        queue.append((prefix / "share/qt6/plugins/platforms/qwindows.dll",
                      destination / "plugins/platforms/qwindows.dll"))
        (out_bin / "qt.conf").write_text("[Paths]\nPrefix=" + ("." if flat else "..") + "\nPlugins=plugins\n", encoding="utf-8")
    for plugin in PLUGINS:
        source = prefix / "lib/gstreamer-1.0" / f"libgst{plugin}.dll"
        queue.append((source, destination / "lib/gstreamer-1.0" / source.name))
    scanner = prefix / "libexec/gstreamer-1.0/gst-plugin-scanner.exe"
    queue.append((scanner, destination / "libexec/gstreamer-1.0" / scanner.name))
    # Keep tools for reproducible diagnosis and the clean-machine smoke test.
    for tool in ("gst-inspect-1.0.exe", "gst-launch-1.0.exe"):
        queue.append((native_bin / tool, out_bin / tool))
    visited: set[str] = set()
    source_files: list[Path] = []
    manifest = []
    while queue:
        source, target = queue.pop(0)
        if str(target).lower() in visited:
            continue
        if not source.is_file():
            raise FileNotFoundError(source)
        visited.add(str(target).lower())
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
        source_files.append(source)
        manifest.append({"file": target.relative_to(destination).as_posix(),
                         "sha256": hashlib.sha256(target.read_bytes()).hexdigest()})
        imports = subprocess.run([str(objdump), "-p", str(source)], check=True,
                                 capture_output=True, text=True).stdout
        for name in re.findall(r"DLL Name:\s*(\S+)", imports):
            dependency = source.parent / name
            if not dependency.is_file():
                dependency = native_bin / name
            if dependency.is_file():
                queue.append((dependency, out_bin / dependency.name))
            elif name.lower().startswith(("api-ms-win-", "ext-ms-win-")):
                continue
            elif not (system32 / name).is_file():
                raise RuntimeError(f"Unresolved import: {source.name} -> {name}")
    license_dir = prefix / "share/licenses"
    if license_dir.is_dir():
        shutil.copytree(license_dir, destination / "licenses/runtime", dirs_exist_ok=True)
    msys = prefix.parent
    pacman = msys / "usr/bin/pacman.exe"
    # Capture exact binary package versions and upstream URLs. This manifest is
    # also the input for the release/source-compliance review.
    paths = ["/" + source.relative_to(msys).as_posix() for source in source_files
             if source.is_relative_to(msys)]
    owner = subprocess.run([str(pacman), "-Qqo", *paths], capture_output=True,
                           text=True, check=True)
    packages = set(owner.stdout.splitlines())
    info = subprocess.run([str(pacman), "-Qi", *sorted(packages)], capture_output=True,
                          text=True, check=True)
    destination.joinpath("runtime-packages.txt").write_text(info.stdout, encoding="utf-8")
    sources = {}
    local_db = msys / "var/lib/pacman/local"
    for package in sorted(packages):
        candidates = list(local_db.glob(package + "-*/desc"))
        candidates = [path for path in candidates if path.read_text(encoding="utf-8").startswith("%NAME%\n" + package + "\n")]
        if len(candidates) != 1:
            raise RuntimeError(f"Cannot identify exact package metadata: {package}")
        fields = {}
        for section in candidates[0].read_text(encoding="utf-8").split("\n\n"):
            lines = section.splitlines()
            if lines:
                fields[lines[0].strip("%")] = "\n".join(lines[1:])
        base, version = fields["BASE"], fields["VERSION"]
        filename = f"{base}-{version}.src.tar.zst"
        sources[filename] = {"package_base": base, "version": version,
            "url": "https://repo.msys2.org/mingw/sources/" + filename,
            "license": fields.get("LICENSE", "See archive"), "upstream": fields.get("URL", "")}
    destination.joinpath("runtime-sources.json").write_text(json.dumps(list(sources.values()), indent=2), encoding="utf-8")
    destination.joinpath("runtime-manifest.json").write_text(
        json.dumps(manifest, indent=2), encoding="utf-8")
    print(f"Collected {len(manifest)} native files from {len(packages)} packages.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", required=True, type=Path)
    parser.add_argument("--receiver", required=True, type=Path)
    parser.add_argument("--destination", required=True, type=Path)
    parser.add_argument("--app", type=Path)
    parser.add_argument("--flat", action="store_true")
    arguments = parser.parse_args()
    collect(arguments.prefix, arguments.receiver, arguments.destination, arguments.app, arguments.flat)
