"""Download exact MSYS2 source packages (upstream sources + build recipes).

Run after package.ps1. TLS downloads are never executed. Archives are cached
outside the installer and combined into a companion source bundle.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import hashlib
import json
from pathlib import Path
import urllib.error
import urllib.request
from zipfile import ZipFile, ZIP_STORED

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
version = (root / "version.txt").read_text().strip()
parser.add_argument("--manifest", type=Path, default=root / f"dist/PC-Mirror-{version}/runtime-sources.json")
args = parser.parse_args()
sources = json.loads(args.manifest.read_text(encoding="utf-8"))
out = root / ".tools/runtime-sources"
out.mkdir(parents=True, exist_ok=True)


def fetch(item):
    url = item["url"]
    for extension in (".zst", ".gz", ".xz"):
        candidate = url.removesuffix(".zst") + extension
        filename = candidate.rsplit("/", 1)[1]
        target = out / filename
        if not target.exists():
            try:
                with urllib.request.urlopen(candidate, timeout=60) as response:
                    data = response.read()
                if not data.startswith((b"\x28\xb5\x2f\xfd", b"\x1f\x8b", b"\xfd7zXZ")):
                    raise RuntimeError("Not a compressed source archive: " + candidate)
                target.write_bytes(data)
            except urllib.error.HTTPError as error:
                if error.code == 404:
                    continue
                raise
        return {**item, "url": candidate, "file": filename,
                "sha256": hashlib.sha256(target.read_bytes()).hexdigest()}
    raise RuntimeError("No source package found: " + url)


results, failures = [], []
with ThreadPoolExecutor(max_workers=6) as executor:
    tasks = {executor.submit(fetch, item): item for item in sources}
    for task in as_completed(tasks):
        try:
            record = task.result()
            results.append(record)
            print("OK " + record["file"], flush=True)
        except Exception as error:
            failures.append(str(error))
            print("FAILED " + str(error), flush=True)
manifest = out / "SOURCE-MANIFEST.json"
manifest.write_text(json.dumps(sorted(results, key=lambda item: item["file"]), indent=2), encoding="utf-8")
if failures:
    raise SystemExit("Source collection incomplete: " + "\n".join(failures))
bundle = root / f"dist/PC-Mirror-{version}-runtime-sources.zip"
with ZipFile(bundle, "w", ZIP_STORED) as archive:
    archive.write(manifest, manifest.name)
    for item in results:
        archive.write(out / item["file"], item["file"])
print(f"Complete: {len(results)} source packages in {bundle}")
