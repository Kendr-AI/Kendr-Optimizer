#!/usr/bin/env python3
"""Exercise a native Kendr installer against a local release fixture."""

from __future__ import annotations

import argparse
import functools
import hashlib
import http.server
import json
import os
import shutil
import subprocess
import tempfile
import threading
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, format: str, *args: object) -> None:
        return


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command: list[str], environment: dict[str, str], *, succeeds: bool) -> None:
    result = subprocess.run(
        command,
        cwd=ROOT,
        env=environment,
        capture_output=True,
        text=True,
        timeout=60,
    )
    if (result.returncode == 0) != succeeds:
        raise RuntimeError(
            f"installer command returned {result.returncode}; "
            f"stdout={result.stdout!r}; stderr={result.stderr!r}"
        )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--version", required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    archive = args.archive.resolve()
    if not archive.is_file():
        raise FileNotFoundError(f"release archive is missing: {archive}")

    windows = args.target.endswith("-windows-msvc")
    if windows != (os.name == "nt"):
        raise ValueError("installer smoke target does not match the native runner")

    with tempfile.TemporaryDirectory(prefix="kendr-installer-smoke-") as temporary:
        root = Path(temporary)
        server_dir = root / "release assets"
        install_dir = root / "installed cli"
        temp_dir = root / "temporary files"
        server_dir.mkdir()
        temp_dir.mkdir()
        served_archive = server_dir / archive.name
        shutil.copyfile(archive, served_archive)
        (server_dir / "SHA256SUMS").write_text(
            f"{digest(served_archive)}  {served_archive.name}\n",
            encoding="ascii",
            newline="\n",
        )

        handler = functools.partial(QuietHandler, directory=str(server_dir))
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            environment = os.environ.copy()
            environment.update(
                {
                    "KENDR_ALLOW_INSECURE": "1",
                    "KENDR_DOWNLOAD_BASE_URL": (
                        f"http://127.0.0.1:{server.server_port}"
                    ),
                    "KENDR_INSTALL_DIR": str(install_dir),
                    "KENDR_NO_MODIFY_PATH": "1",
                    "KENDR_VERSION": f"v{args.version}",
                    "TMPDIR": str(temp_dir),
                }
            )
            if windows:
                command = [
                    "powershell",
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    str(ROOT / "install" / "kendr-opt-installer.ps1"),
                ]
                installed = install_dir / "kendr-opt.exe"
            else:
                command = ["sh", str(ROOT / "install" / "kendr-opt-installer.sh")]
                installed = install_dir / "kendr-opt"

            run(command, environment, succeeds=True)
            run(command, environment, succeeds=True)
            installed_digest = digest(installed)

            with served_archive.open("ab") as stream:
                stream.write(b"corrupted installer smoke fixture")
            run(command, environment, succeeds=False)
            if digest(installed) != installed_digest:
                raise ValueError("failed upgrade changed the installed binary")

            version = subprocess.run(
                [str(installed), "--version"],
                check=True,
                capture_output=True,
                text=True,
                timeout=30,
            ).stdout.strip()
            if version != f"kendr-opt {args.version}":
                raise ValueError(f"installed version mismatch: {version!r}")
            engines = json.loads(
                subprocess.run(
                    [str(installed), "engines", "--compact"],
                    check=True,
                    capture_output=True,
                    text=True,
                    timeout=30,
                ).stdout
            )
            if not isinstance(engines, list) or not engines:
                raise ValueError("installed CLI returned no engine list")
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)

    print(f"installer smoke passed for {args.target}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
