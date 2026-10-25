"""Run the public ownership regressions and passive native audit on one build."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ("runtime", "host", "compiler", "output"):
        parser.add_argument("--"+key, type=Path, required=True)
    args = parser.parse_args()
    source = Path(__file__).resolve().parents[1]
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    native = out/"native"
    native.mkdir()
    runtime = json.loads(args.runtime.read_text())
    # The executable and every ordered argument are opaque launcher entries.
    runtime["script"] = str(source/"tests/fixtures/document_ownership_worker.py")
    config = out/"observed-runtime.json"
    config.write_text(json.dumps(runtime, indent=2)+"\n")
    files = [args.host.resolve(), args.compiler.resolve(), args.runtime.resolve(),
             *source.glob("backend/**/*.py"), *source.glob("backend/native/*"),
             *source.glob("src/**/*.rs"), *source.glob("tests/document_ownership*.py"),
             source/"tests/fixtures/document_ownership_worker.py", Path(__file__).resolve()]
    hashes = {str(p): digest(p) for p in files if p.is_file()}
    (out/"executed-inputs.json").write_text(json.dumps(hashes, indent=2)+"\n")
    env = dict(os.environ, OWN_BACKEND=str(source/"backend"), OWN_DIAGNOSTICS=str(native),
               PYTHONDONTWRITEBYTECODE="1")
    env.setdefault("FREECAD_USER_HOME", str(out/"config"))
    Path(env["FREECAD_USER_HOME"]).mkdir(parents=True, exist_ok=True)
    commands = [
        ("public", [sys.executable, "-B", str(source/"tests/document_ownership_workflows.py"),
                    "--runtime", str(config), "--host", str(args.host.resolve()),
                    "--compiler", str(args.compiler.resolve()), "--output", str(out/"public")]),
        ("native-audit", [sys.executable, "-B", str(source/"tests/document_ownership_native.py"),
                          "--diagnostics", str(native), "--output", str(out/"native-audit.json")])]
    for name, command in commands:
        with (out/(name+".log")).open("x", encoding="utf-8") as log:
            result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT,
                                    creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS if os.name == "nt" else 0)
        (out/(name+"-receipt.json")).write_text(json.dumps(dict(command=command, exit_code=result.returncode), indent=2)+"\n")
        print(name, result.returncode, flush=True)
        if result.returncode:
            raise SystemExit((out/(name+".log")).read_text())
    assert all(digest(Path(path)) == sha for path, sha in hashes.items())
    (out/"passed.json").write_text(json.dumps(dict(public_cases=11, native_audit=True, inputs_unchanged=True,
                                                  resource_comparison=False, physical_gui=False), indent=2)+"\n")


if __name__ == "__main__":
    main()
