"""Verify delivered binaries, source archives and (if present) runtime source archives."""
import argparse
import hashlib
import json
from pathlib import Path
import sys
from zipfile import ZipFile

root = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(root / "scripts"))
from pe_imports import imports, is_system  # noqa: E402

version = (root / 'version.txt').read_text().strip()
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--package', type=Path, default=root / f'dist/PC-Mirror-{version}')
package = parser.parse_args().package.resolve()

manifest = json.loads((package / "runtime-manifest.json").read_text(encoding="utf-8-sig"))
for entry in manifest:
    file = package / entry["file"]
    assert hashlib.sha256(file.read_bytes()).hexdigest() == entry["sha256"], str(file)
print(f"PASS {len(manifest)} native file hashes")

# Die Rust-App darf nur Windows-Systembibliotheken laden (statische C-Laufzeit, kein Qt).
app_imports = imports(package / "PC-Mirror.exe")
foreign = [name for name in app_imports if not is_system(name)]
assert not foreign, f"PC-Mirror.exe needs non-system DLLs: {foreign}"
assert not list(package.glob("Qt6*.dll")), "Qt files must not be shipped anymore"
print(f"PASS PC-Mirror.exe uses only Windows system DLLs ({len(app_imports)} imports)")

runtime_sources = root / f"dist/PC-Mirror-{version}-runtime-sources.zip"
if runtime_sources.exists():
    expected = json.loads((package / "runtime-sources.json").read_text(encoding="utf-8"))
    with ZipFile(runtime_sources) as bundle:
        sources = json.loads(bundle.read("SOURCE-MANIFEST.json"))
        wanted = {(item["package_base"], item["version"]) for item in expected}
        actual = {(item["package_base"], item["version"]) for item in sources}
        assert wanted == actual, (wanted - actual, actual - wanted)
        for item in sources:
            assert hashlib.sha256(bundle.read(item["file"])).hexdigest() == item["sha256"], item["file"]
        assert bundle.testzip() is None
    print(f"PASS {len(expected)} matching source archives, SHA-256 and ZIP CRC")
else:
    print("INFO kein neues Runtime-Quellarchiv; unveränderte Runtime verweist auf das veröffentlichte Archiv")

with ZipFile(root / f"dist/PC-Mirror-{version}-source.zip") as bundle:
    for name in ("Cargo.toml", "Cargo.lock", "crates/pcmirror/src/app.rs", "crates/pcmirror/ui/app.slint",
                 "crates/pcmirror-sign/src/lib.rs", "third_party/uxplay/uxplay.cpp",
                 "third_party/uxplay/lib/raop.c", "scripts/package.ps1", "LICENSE"):
        assert name in bundle.namelist(), name
    assert bundle.testzip() is None
print("PASS complete application / modified UxPlay source bundle")
