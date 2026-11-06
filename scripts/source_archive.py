"""Archive the complete application and modified UxPlay sources, not build caches."""
from pathlib import Path
from zipfile import ZipFile, ZIP_DEFLATED

root = Path(__file__).resolve().parent.parent
version = (root / "version.txt").read_text().strip()
target = root / f"dist/PC-Mirror-{version}-source.zip"
target.parent.mkdir(parents=True, exist_ok=True)
with ZipFile(target, "w", ZIP_DEFLATED) as archive:
    for item in ("src", "scripts", "tests", "docs", "packaging", "prototype", "third_party", ".github", "assets", "deployment"):
        for file in sorted((root / item).rglob("*")):
            if file.is_file() and not {".git", "__pycache__"}.intersection(file.parts):
                relative = file.relative_to(root).as_posix()
                # Unrelated upstream test applications and unused static SDK
                # binaries are not needed to build PC-Mirror. Keep all sources,
                # licenses and the native SDK used by this application.
                if relative.startswith(("third_party/velopack/test/fixtures/", "third_party/velopack-sdk/lib-static/")):
                    continue
                archive.write(file, file.relative_to(root))
    for item in ("CMakeLists.txt", "version.txt", "README.md", "LICENSE", ".gitignore", ".gitattributes"):
        archive.write(root / item, item)
print(target)
