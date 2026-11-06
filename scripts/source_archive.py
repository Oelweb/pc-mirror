"""Archive the complete application and modified UxPlay sources, not build caches."""
from pathlib import Path
from zipfile import ZipFile, ZIP_DEFLATED

root = Path(__file__).resolve().parent.parent
version = (root / "version.txt").read_text().strip()
target = root / f"dist/PC-Mirror-{version}-source.zip"
target.parent.mkdir(parents=True, exist_ok=True)
with ZipFile(target, "w", ZIP_DEFLATED) as archive:
    for item in ("crates", "scripts", "tests", "docs", "packaging", "third_party/uxplay", "assets", "deployment", ".cargo"):
        for file in sorted((root / item).rglob("*")):
            if file.is_file() and not {".git", "__pycache__", "target"}.intersection(file.parts):
                archive.write(file, file.relative_to(root))
    for item in ("Cargo.toml", "Cargo.lock", "CMakeLists.txt", "version.txt", "README.md", "LICENSE",
                 ".gitignore", ".gitattributes", "third_party/README.md"):
        archive.write(root / item, item)
print(target)
