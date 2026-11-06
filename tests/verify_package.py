"""Verify delivered binaries and matching complete runtime source archives."""
import hashlib
import argparse
import json
from pathlib import Path
from zipfile import ZipFile

root = Path(__file__).resolve().parent.parent
version = (root / 'version.txt').read_text().strip()
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--package',type=Path,default=root / f'dist/PC-Mirror-{version}')
package = parser.parse_args().package.resolve()
manifest = json.loads((package / "runtime-manifest.json").read_text(encoding="utf-8-sig"))
for entry in manifest:
    file = package / entry["file"]
    assert hashlib.sha256(file.read_bytes()).hexdigest() == entry["sha256"], str(file)
print(f"PASS {len(manifest)} native file hashes")
expected = json.loads((package / "runtime-sources.json").read_text(encoding="utf-8"))
with ZipFile(root / f"dist/PC-Mirror-{version}-runtime-sources.zip") as bundle:
    sources = json.loads(bundle.read("SOURCE-MANIFEST.json"))
    wanted = {(item["package_base"], item["version"]) for item in expected}
    actual = {(item["package_base"], item["version"]) for item in sources}
    assert wanted == actual, (wanted - actual, actual - wanted)
    for item in sources:
        assert hashlib.sha256(bundle.read(item["file"])).hexdigest() == item["sha256"], item["file"]
    assert bundle.testzip() is None
print(f"PASS {len(expected)} matching source archives, SHA-256 and ZIP CRC")
with ZipFile(root / f"dist/PC-Mirror-{version}-source.zip") as bundle:
    for name in ("CMakeLists.txt", "src/mainwindow.cpp", "src/receiver.cpp", "third_party/uxplay/uxplay.cpp",
                 "third_party/uxplay/lib/raop.c", "scripts/build.sh", "scripts/package.ps1", "LICENSE"):
        assert name in bundle.namelist(), name
    assert bundle.testzip() is None
print("PASS complete application / modified UxPlay source bundle")
