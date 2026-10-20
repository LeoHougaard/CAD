"""Fetch the pinned native runtime into this checkout. No installation or FUSE.

Only the final launcher JSON is written to stdout. Extraction/download messages
go to stderr, so callers can consume the result directly.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile
from urllib.request import Request, urlopen


NEXT = Path(__file__).resolve().parents[1]


def platform_key(system=None, machine=None):
    system = (system or platform.system()).lower()
    machine = (machine or platform.machine()).lower()
    arch = {"amd64": "x86_64", "x86_64": "x86_64",
            "arm64": "aarch64", "aarch64": "aarch64"}.get(machine)
    if system == "windows" and arch == "x86_64":
        return "windows-x86_64"
    if system == "linux" and arch in {"x86_64", "aarch64"}:
        return "linux-" + arch
    raise RuntimeError("Unsupported native runtime platform: " + system + "/" + machine)


def checksum(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify(path, expected):
    actual = checksum(path)
    if actual != expected:
        raise RuntimeError(f"SHA256 mismatch for {path}: expected {expected}, got {actual}. "
                           "Remove the invalid cached archive and retry.")


def archive(cache, asset):
    path = cache / asset["filename"]
    if not path.exists():
        print("Downloading " + asset["url"], file=sys.stderr)
        request = Request(asset["url"], headers={"User-Agent": "cad-next-runtime-setup/1"})
        with tempfile.NamedTemporaryFile(dir=cache, prefix=".download-", delete=False) as output:
            temporary = Path(output.name)
            try:
                with urlopen(request, timeout=120) as response:
                    shutil.copyfileobj(response, output, length=1024 * 1024)
                output.flush()
                output.close()
                verify(temporary, asset["sha256"])
                temporary.replace(path)
            finally:
                # This is only the individual temporary file created above.
                temporary.unlink(missing_ok=True)
    verify(path, asset["sha256"])
    return path


def extract(archive_path, destination, asset):
    print("Extracting " + str(archive_path), file=sys.stderr)
    if asset["format"] == "7z":
        # Git Bash puts GNU tar ahead of Windows' libarchive tar. GNU tar treats
        # drive letters as remote hosts and does not read this portable 7z.
        tar = Path(os.environ.get("SystemRoot", r"C:\Windows")) / "System32" / "tar.exe"
        if not tar.is_file():
            raise RuntimeError("Windows tar.exe is required to extract the portable 7z archive")
        command = [str(tar), "-xf", str(archive_path), "-C", str(destination)]
    elif asset["format"] == "appimage":
        # The verified AppImage's built-in extractor does not mount with FUSE.
        archive_path.chmod(archive_path.stat().st_mode | 0o100)
        command = [str(archive_path), "--appimage-extract"]
    else:
        raise RuntimeError("Unsupported runtime archive format")
    subprocess.run(command, cwd=destination, check=True, stdout=subprocess.DEVNULL, stderr=sys.stderr)


def setup(cache, manifest, key):
    asset = manifest["assets"][key]
    cache.mkdir(parents=True, exist_ok=True)
    archive_path = archive(cache, asset)
    # Changing the pinned archive never reuses files from a different runtime.
    target = cache / "extracted" / (manifest["freecad_version"] + "-" + key + "-" + asset["sha256"][:12])
    marker = target / ".runtime.json"
    expected_marker = {"platform": key, "sha256": asset["sha256"]}
    if target.exists():
        if not marker.is_file() or json.loads(marker.read_text(encoding="utf-8")) != expected_marker:
            raise RuntimeError("Incomplete runtime cache at " + str(target) + "; use a fresh --cache-dir")
    else:
        target.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=target.parent, prefix=".extract-") as temporary:
            stage = Path(temporary) / "runtime"
            stage.mkdir()
            extract(archive_path, stage, asset)
            if not (stage / asset["launcher"]).is_file():
                raise RuntimeError("Pinned archive does not contain the expected launcher: " + asset["launcher"])
            (stage / ".runtime.json").write_text(json.dumps(expected_marker), encoding="utf-8")
            stage.rename(target)
    launcher = target / asset["launcher"]
    if not launcher.is_file():
        raise RuntimeError("Runtime launcher is missing from cache: " + str(launcher))
    executable = str(launcher)
    worker_args = asset["args"]
    if key.startswith("linux-"):
        # AppRun prepares its bundled Python, but FreeCAD's extension modules
        # additionally need usr/lib on Python's module search path.
        executable = "/usr/bin/env"
        if not Path(executable).is_file():
            raise RuntimeError("Linux runtime launcher requires /usr/bin/env")
        worker_args = ["PYTHONPATH=" + str(launcher.parent / "usr" / "lib"),
                       str(launcher), *worker_args]
    return {"freecad_version": manifest["freecad_version"], "platform": key,
            "sha256": asset["sha256"], "executable": executable,
            "args": worker_args, "script": str(NEXT / "backend" / "freecad_worker.py")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache-dir", type=Path, default=NEXT.parent / ".tools" / "freecad-runtime")
    parser.add_argument("--dry-run", action="store_true", help="print the platform and pinned asset without downloading or extracting")
    parser.add_argument("--output", type=Path, help="also save launcher JSON as UTF-8, including on Windows PowerShell")
    args = parser.parse_args()
    manifest = json.loads((NEXT / "runtime.json").read_text(encoding="utf-8"))
    key = platform_key()
    if args.dry_run:
        result = {"freecad_version": manifest["freecad_version"], "platform": key,
                  "cache_dir": str(args.cache_dir.resolve()), **manifest["assets"][key]}
    else:
        result = setup(args.cache_dir.resolve(), manifest, key)
    encoded = json.dumps(result)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded + "\n", encoding="utf-8")
    print(encoded)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print("runtime setup: " + str(error), file=sys.stderr)
        sys.exit(1)
