"""Exercise the real subprocess IPC and frame capture without touching the UI."""
import os
import argparse
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import time

root = Path(__file__).resolve().parent.parent
version = (root / 'version.txt').read_text().strip()
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--package',type=Path,default=root / f'dist/PC-Mirror-{version}')
package = parser.parse_args().package.resolve()
native_bin = package if (package / 'uxplay.exe').exists() else package / 'bin'
env = os.environ.copy()
system_root = os.environ.get("SYSTEMROOT", "C:/Windows")
env.update({
    "PATH": str(native_bin) + ";" + system_root + "/System32;" + system_root,
    "GST_PLUGIN_SYSTEM_PATH_1_0": str(package / "lib/gstreamer-1.0"),
    "GST_PLUGIN_PATH_1_0": "",
    "GST_PLUGIN_SCANNER_1_0": str(package / "libexec/gstreamer-1.0/gst-plugin-scanner.exe"),
    "PCMIRROR_EMBEDDED": "1", "PCMIRROR_TEST_HEADLESS": "1",
})
env.pop("PCMIRROR_HWND", None)


def launch():
    process = subprocess.Popen([str(native_bin / "uxplay.exe"), "--pcmirror-selftest"],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        text=True, encoding="utf-8", env=env, creationflags=subprocess.CREATE_NO_WINDOW)
    lines = queue.Queue()
    def reader():
        for line in process.stdout:
            lines.put(line.strip())
    threading.Thread(target=reader, daemon=True).start()
    return process, lines


def wait_for(lines, prefix, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            line = lines.get(timeout=0.25)
        except queue.Empty:
            continue
        if line.startswith(prefix):
            print("PASS " + line)
            return line
    raise AssertionError("Timeout waiting for " + prefix)


def command(process, text):
    process.stdin.write(text + "\n")
    process.stdin.flush()


process, lines = launch()
try:
    wait_for(lines, "PCMIRROR|READY")
    wait_for(lines, "PCMIRROR|FRAME|360|640")
    for fill in (1, 0, 1, 0):
        command(process, f"IPAD_FILL {fill}")
        wait_for(lines, f"PCMIRROR|IPAD_FILL|{fill}")
    for muted in (1, 0):
        command(process, f"MUTE {muted}")
        wait_for(lines, f"PCMIRROR|MUTED|{muted}")
    with tempfile.TemporaryDirectory(prefix="pcmirror test ") as folder:
        frame = Path(folder) / "bild ä.ppm"
        command(process, "SNAPSHOT " + str(frame))
        wait_for(lines, "PCMIRROR|SNAPSHOT|OK")
        with frame.open("rb") as image:
            assert image.readline() == b"P6\n"
            assert image.readline() == b"360 640\n"
            assert image.readline() == b"255\n"
            pixels = image.read()
        assert len(pixels) == 360 * 640 * 3
        assert max(pixels) > 200 and min(pixels) < 20
        print("PASS screenshot contains actual RGB pixels, UTF-8 path with spaces")
        command(process, "SNAPSHOT " + str(Path(folder) / "missing/subdir/fail.ppm"))
        wait_for(lines, "PCMIRROR|SNAPSHOT|ERROR")
    command(process, "STOP")
    assert process.wait(timeout=5) == 0
    print("PASS graceful STOP")
finally:
    if process.poll() is None:
        process.kill()
        process.wait(timeout=5)
process, lines = launch()
try:
    wait_for(lines, "PCMIRROR|READY")
    process.stdin.close()
    assert process.wait(timeout=5) == 0
    print("PASS parent pipe closure stops backend")
finally:
    if process.poll() is None:
        process.kill()
        process.wait(timeout=5)
