#!/usr/bin/env python3
"""Exercise Kendr's native self-update and rollback paths on a disposable copy."""

from __future__ import annotations

import argparse
import hashlib
import http.server
import json
import os
import platform
import shutil
import subprocess
import tempfile
import threading
import time
import urllib.parse
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import cli_release


REPOSITORY = "Kendr-AI/Kendr-Optimizer"
REPOSITORY_ID = 1_328_565_025
RELEASE_ID = 9_100_001
ARCHIVE_ASSET_ID = 9_100_002
CHECKSUM_ASSET_ID = 9_100_003
INSTALL_RECEIPT = ".kendr-opt-install.json"
UPDATE_LOCK = ".kendr-opt-update.lock"
API_VERSION = "2026-03-10"
TRAILING_MARKER = b"\nKENDR_SELF_UPDATE_SMOKE_ORIGINAL\x00\xff\n"


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def expected_receipt(target: str, version: str) -> dict[str, str]:
    return {
        "schema_version": "kendr.install/v1",
        "repository": REPOSITORY,
        "install_method": "github-release",
        "target": target,
        "version": version,
        "channel": "preview",
    }


def write_receipt(path: Path, target: str, version: str) -> None:
    path.write_text(
        json.dumps(expected_receipt(target, version), separators=(",", ":")) + "\n",
        encoding="utf-8",
        newline="\n",
    )


def verify_receipt(path: Path, target: str, version: str) -> None:
    if not path.is_file() or path.is_symlink():
        raise ValueError("self-update did not leave a regular install receipt")
    try:
        actual = json.loads(path.read_text(encoding="utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError("self-update receipt is not valid UTF-8 JSON") from error
    expected = expected_receipt(target, version)
    if actual != expected:
        raise ValueError(
            f"self-update receipt mismatch; expected={expected!r}, got={actual!r}"
        )


@dataclass(frozen=True)
class ReleaseFixture:
    version: str
    target: str
    archive_name: str
    archive: bytes
    served_archive: bytes
    checksums: bytes

    @classmethod
    def from_archive(
        cls,
        archive_path: Path,
        target: str,
        version: str,
        *,
        corrupt_download: bool = False,
    ) -> "ReleaseFixture":
        archive = archive_path.read_bytes()
        if not archive:
            raise ValueError("self-update fixture archive is empty")
        served_archive = archive
        if corrupt_download:
            served_archive = bytes([archive[0] ^ 0xFF]) + archive[1:]
        checksums = f"{sha256_bytes(archive)}  {archive_path.name}\n".encode(
            "ascii"
        )
        return cls(
            version=version,
            target=target,
            archive_name=archive_path.name,
            archive=archive,
            served_archive=served_archive,
            checksums=checksums,
        )

    def release(self, base_url: str) -> dict[str, Any]:
        return {
            "id": RELEASE_ID,
            "tag_name": f"v{self.version}",
            "html_url": f"{base_url}/release/v{self.version}",
            "draft": False,
            "prerelease": True,
            "immutable": True,
            "published_at": "2026-08-16T00:00:00Z",
            "assets": [
                {
                    "id": ARCHIVE_ASSET_ID,
                    "name": self.archive_name,
                    "size": len(self.archive),
                    "state": "uploaded",
                    "digest": f"sha256:{sha256_bytes(self.archive)}",
                },
                {
                    "id": CHECKSUM_ASSET_ID,
                    "name": "SHA256SUMS",
                    "size": len(self.checksums),
                    "state": "uploaded",
                    "digest": f"sha256:{sha256_bytes(self.checksums)}",
                },
            ],
        }


class FixtureServer(http.server.ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self) -> None:
        super().__init__(("127.0.0.1", 0), FixtureHandler)
        self.fixture: ReleaseFixture | None = None
        self.state_lock = threading.Lock()
        self.request_counts: dict[str, int] = {}
        self.errors: list[str] = []

    @property
    def base_url(self) -> str:
        return f"http://127.0.0.1:{self.server_port}"

    def set_fixture(self, fixture: ReleaseFixture) -> None:
        with self.state_lock:
            self.fixture = fixture
            self.request_counts = {}
            self.errors = []

    def snapshot(self) -> tuple[dict[str, int], list[str]]:
        with self.state_lock:
            return dict(self.request_counts), list(self.errors)

    def record(self, route: str) -> None:
        with self.state_lock:
            self.request_counts[route] = self.request_counts.get(route, 0) + 1

    def record_error(self, message: str) -> None:
        with self.state_lock:
            self.errors.append(message)


class FixtureHandler(http.server.BaseHTTPRequestHandler):
    server: FixtureServer

    def log_message(self, format: str, *args: object) -> None:
        return

    def send_payload(
        self, status: int, body: bytes, content_type: str, route: str
    ) -> None:
        self.server.record(route)
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(body)

    def reject(self, status: int, message: str) -> None:
        self.server.record_error(message)
        body = json.dumps({"message": message}).encode("utf-8")
        self.send_payload(status, body, "application/json", "rejected")

    def require_headers(self, accept: str) -> bool:
        if self.headers.get("X-GitHub-Api-Version") != API_VERSION:
            self.reject(400, "missing or incorrect GitHub API version header")
            return False
        if self.headers.get("Accept") != accept:
            self.reject(406, f"unexpected Accept header for {self.path}")
            return False
        return True

    def do_GET(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler API
        with self.server.state_lock:
            fixture = self.server.fixture
        if fixture is None:
            self.reject(503, "release fixture is not configured")
            return

        parsed = urllib.parse.urlsplit(self.path)
        path = parsed.path
        repository_path = f"/repos/{REPOSITORY}"
        release_path = f"{repository_path}/releases/{RELEASE_ID}"
        archive_path = (
            f"{repository_path}/releases/assets/{ARCHIVE_ASSET_ID}"
        )
        checksums_path = (
            f"{repository_path}/releases/assets/{CHECKSUM_ASSET_ID}"
        )

        if path in {archive_path, checksums_path}:
            if not self.require_headers("application/octet-stream"):
                return
            if path == archive_path:
                self.send_payload(
                    200,
                    fixture.served_archive,
                    "application/octet-stream",
                    "archive",
                )
            else:
                self.send_payload(
                    200,
                    fixture.checksums,
                    "application/octet-stream",
                    "checksums",
                )
            return

        if not self.require_headers("application/vnd.github+json"):
            return
        if path == repository_path and not parsed.query:
            body = json.dumps(
                {
                    "id": REPOSITORY_ID,
                    "full_name": REPOSITORY,
                    "private": False,
                    "archived": False,
                    "disabled": False,
                }
            ).encode("utf-8")
            self.send_payload(200, body, "application/json", "repository")
        elif path == f"{repository_path}/releases":
            if urllib.parse.parse_qs(parsed.query) != {"per_page": ["100"]}:
                self.reject(400, "unexpected releases query")
                return
            body = json.dumps([fixture.release(self.server.base_url)]).encode("utf-8")
            self.send_payload(200, body, "application/json", "releases")
        elif path == release_path and not parsed.query:
            body = json.dumps(fixture.release(self.server.base_url)).encode("utf-8")
            self.send_payload(200, body, "application/json", "release_by_id")
        else:
            self.reject(404, f"unexpected updater request: {self.path}")


def native_target() -> str:
    system = platform.system().lower()
    machine = platform.machine().lower()
    if machine in {"amd64", "x64"}:
        machine = "x86_64"
    elif machine in {"arm64", "arm64e"}:
        machine = "aarch64"
    targets = {
        ("linux", "x86_64"): "x86_64-unknown-linux-musl",
        ("linux", "aarch64"): "aarch64-unknown-linux-musl",
        ("darwin", "x86_64"): "x86_64-apple-darwin",
        ("darwin", "aarch64"): "aarch64-apple-darwin",
        ("windows", "x86_64"): "x86_64-pc-windows-msvc",
    }
    try:
        return targets[(system, machine)]
    except KeyError as error:
        raise ValueError(f"unsupported native smoke host: {system}/{machine}") from error


def run(
    command: list[str],
    environment: dict[str, str],
    *,
    succeeds: bool,
    timeout: int = 180,
) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        command,
        env=environment,
        capture_output=True,
        text=True,
        errors="replace",
        timeout=timeout,
    )
    if (result.returncode == 0) != succeeds:
        expectation = "success" if succeeds else "failure"
        raise RuntimeError(
            f"command did not produce expected {expectation}: {command!r}; "
            f"returncode={result.returncode}; stdout={result.stdout!r}; "
            f"stderr={result.stderr!r}"
        )
    return result


def assert_request_contract(
    server: FixtureServer, *, expect_release_recheck: bool
) -> None:
    counts, errors = server.snapshot()
    if errors:
        raise ValueError(f"local update API rejected requests: {errors!r}")
    required = {"repository", "releases", "checksums", "archive"}
    if expect_release_recheck:
        required.add("release_by_id")
    missing = sorted(route for route in required if counts.get(route, 0) < 1)
    if missing:
        raise ValueError(
            f"updater did not complete required API requests: {missing}; counts={counts!r}"
        )
    if not expect_release_recheck and counts.get("release_by_id", 0):
        raise ValueError("corrupt download unexpectedly reached release revalidation")


def run_update(
    binary: Path,
    environment: dict[str, str],
    server: FixtureServer,
    *,
    succeeds: bool,
) -> subprocess.CompletedProcess[str]:
    try:
        return run(
            [str(binary), "update", "--reinstall", "--json"],
            environment,
            succeeds=succeeds,
        )
    except Exception as error:
        counts, server_errors = server.snapshot()
        raise RuntimeError(
            f"{error}; local API counts={counts!r}; "
            f"local API errors={server_errors!r}"
        ) from error


def verify_version(binary: Path, version: str, environment: dict[str, str]) -> None:
    result = run([str(binary), "--version"], environment, succeeds=True, timeout=30)
    if result.stdout.strip() != f"kendr-opt {version}":
        raise ValueError(f"native binary version mismatch: {result.stdout.strip()!r}")


def wait_for_exact_entries(directory: Path, expected: set[str]) -> None:
    deadline = time.monotonic() + 5
    while True:
        actual = {path.name for path in directory.iterdir()}
        if actual == expected:
            return
        if time.monotonic() >= deadline:
            raise ValueError(
                "self-update left unexpected staging or rollback files: "
                f"{sorted(actual - expected)}"
            )
        time.sleep(0.05)


def wait_for_empty_process_temp(directory: Path) -> None:
    deadline = time.monotonic() + 5
    while True:
        entries = list(directory.iterdir())
        if not entries:
            return
        if time.monotonic() >= deadline:
            raise ValueError(
                "self-update left helper or relocated-image residue in its isolated "
                f"process temp directory: {[path.name for path in entries]!r}"
            )
        time.sleep(0.05)


def reset_test_updater(
    source: Path,
    installed: Path,
    receipt: Path,
    target: str,
    version: str,
    environment: dict[str, str],
    *,
    trailing_marker: bool = False,
) -> None:
    shutil.copy2(source, installed)
    if trailing_marker:
        with installed.open("ab") as stream:
            stream.write(TRAILING_MARKER)
    if os.name != "nt":
        installed.chmod(0o755)
    write_receipt(receipt, target, version)
    verify_version(installed, version, environment)


def compile_post_install_failure_candidate(
    directory: Path, version: str, windows: bool
) -> Path:
    rustc = shutil.which("rustc")
    if rustc is None:
        raise FileNotFoundError("rustc is required for the post-install rollback smoke")
    source = directory / "post_install_failure.rs"
    output = directory / ("post-install-failure.exe" if windows else "post-install-failure")
    source.write_text(
        r'''
use std::env;
use std::process;

const VERSION: &str = "__VERSION__";

fn main() {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments.as_slice() == ["--version"] {
        println!("kendr-opt {VERSION}");
        return;
    }
    if arguments.as_slice() == ["engines", "--compact"] {
        let receipt_exists = env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|parent| parent.join(".kendr-opt-install.json")))
            .is_some_and(|path| path.exists());
        if receipt_exists {
            eprintln!("fixture intentionally fails only after installation");
            process::exit(86);
        }
        println!("{}", r#"[{"name":"post-install-rollback-fixture"}]"#);
        return;
    }
    process::exit(2);
}
'''.replace("__VERSION__", version).lstrip(),
        encoding="utf-8",
        newline="\n",
    )
    result = subprocess.run(
        [
            rustc,
            "--edition=2021",
            "--crate-name",
            "kendr_update_post_install_fixture",
            "-C",
            "debuginfo=0",
            "-C",
            "opt-level=0",
            "-o",
            str(output),
            str(source),
        ],
        capture_output=True,
        text=True,
        errors="replace",
        timeout=120,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"could not compile rollback fixture; stdout={result.stdout!r}; "
            f"stderr={result.stderr!r}"
        )
    return output


def package_candidate(
    candidate: Path, output: Path, target: str, version: str
) -> None:
    members = cli_release.package_members(candidate, target)
    root = cli_release.archive_root(version, target)
    if output.suffix == ".zip":
        cli_release.write_zip(output, root, members, 315_532_800)
    else:
        cli_release.write_tar_gz(output, root, members, 0)
    cli_release.verify_archive(output, version, target)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--test-updater-binary", type=Path, required=True)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--version", required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    binary = args.binary.resolve()
    test_updater_binary = args.test_updater_binary.resolve()
    archive = args.archive.resolve()
    if not binary.is_file() or binary.is_symlink():
        raise FileNotFoundError(f"native release binary is missing or unsafe: {binary}")
    if not archive.is_file() or archive.is_symlink():
        raise FileNotFoundError(f"native release archive is missing or unsafe: {archive}")
    if not test_updater_binary.is_file() or test_updater_binary.is_symlink():
        raise FileNotFoundError(
            "feature-enabled test updater is missing or unsafe: "
            f"{test_updater_binary}"
        )
    if args.target != native_target():
        raise ValueError(
            f"self-update target {args.target!r} does not match native host "
            f"{native_target()!r}"
        )
    expected_archive_name = cli_release.archive_name(args.target)
    if archive.name != expected_archive_name:
        raise ValueError(
            f"self-update archive must be named {expected_archive_name!r}, "
            f"got {archive.name!r}"
        )
    cli_release.verify_archive(archive, args.version, args.target)

    windows = os.name == "nt"
    with tempfile.TemporaryDirectory(prefix="kendr-update-smoke-") as temporary:
        root = Path(temporary)
        install_dir = root / "installed cli"
        cache_dir = root / "update cache"
        fixture_dir = root / "release fixtures"
        process_temp_dir = root / "process temporary files"
        install_dir.mkdir()
        fixture_dir.mkdir()
        process_temp_dir.mkdir()
        installed = install_dir / ("kendr-opt.exe" if windows else "kendr-opt")
        receipt = install_dir / INSTALL_RECEIPT

        environment = os.environ.copy()
        environment.update(
            {
                "KENDR_ALLOW_INSECURE": "1",
                "KENDR_NO_UPDATE_CHECK": "1",
                "KENDR_UPDATE_CACHE_DIR": str(cache_dir),
                "NO_PROXY": "127.0.0.1",
                "TEMP": str(process_temp_dir),
                "TMP": str(process_temp_dir),
                "no_proxy": "127.0.0.1",
            }
        )
        if not windows:
            environment["TMPDIR"] = str(process_temp_dir)
        verify_version(binary, args.version, environment)
        verify_version(test_updater_binary, args.version, environment)
        clean_digest = sha256_file(binary)
        test_updater_digest = sha256_file(test_updater_binary)
        reset_test_updater(
            test_updater_binary,
            installed,
            receipt,
            args.target,
            args.version,
            environment,
            trailing_marker=True,
        )
        if sha256_file(installed) == test_updater_digest:
            raise ValueError("trailing-marker fixture did not change the copied executable")

        server = FixtureServer()
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        environment["KENDR_UPDATE_API_URL"] = server.base_url
        try:
            clean_fixture = ReleaseFixture.from_archive(
                archive, args.target, args.version
            )
            server.set_fixture(clean_fixture)
            official_result = run(
                [str(binary), "update", "--check", "--json"],
                environment,
                succeeds=False,
            )
            if "does not permit an update api override" not in official_result.stderr.lower():
                raise ValueError(
                    "official release binary did not prove its local update override is "
                    f"compile-time disabled: {official_result.stderr!r}"
                )
            counts, server_errors = server.snapshot()
            if counts or server_errors:
                raise ValueError(
                    "official release binary contacted the test update server despite "
                    f"the disabled override; counts={counts!r}; errors={server_errors!r}"
                )
            wait_for_empty_process_temp(process_temp_dir)
            server.set_fixture(clean_fixture)
            result = run_update(
                installed, environment, server, succeeds=True
            )
            assert_request_contract(server, expect_release_recheck=True)
            wait_for_empty_process_temp(process_temp_dir)
            try:
                report = json.loads(result.stdout)
            except json.JSONDecodeError as error:
                raise ValueError(
                    f"successful update did not return JSON: {result.stdout!r}"
                ) from error
            expected_report = {
                "schema_version": "kendr.update/v1",
                "status": "updated",
                "current_version": args.version,
                "latest_version": args.version,
                "channel": "preview",
                "prerelease": True,
                "release_id": RELEASE_ID,
                "immutable": True,
                "target": args.target,
                "archive_name": archive.name,
                "archive_sha256": sha256_bytes(clean_fixture.archive),
            }
            mismatches = {
                key: (value, report.get(key))
                for key, value in expected_report.items()
                if report.get(key) != value
            }
            if mismatches:
                raise ValueError(f"self-update JSON report mismatch: {mismatches!r}")
            executable = report.get("executable")
            if not isinstance(executable, str) or not Path(executable).samefile(installed):
                raise ValueError(
                    f"self-update report identified the wrong executable: {executable!r}"
                )
            if sha256_file(installed) != clean_digest:
                raise ValueError(
                    "same-version reinstall did not replace the trailing-marker copy "
                    "with the clean release binary"
                )
            if installed.read_bytes().endswith(TRAILING_MARKER):
                raise ValueError("same-version reinstall left the trailing marker in place")
            verify_receipt(receipt, args.target, args.version)
            verify_version(installed, args.version, environment)

            reset_test_updater(
                test_updater_binary,
                installed,
                receipt,
                args.target,
                args.version,
                environment,
            )
            preserved_digest = sha256_file(installed)
            preserved_receipt = receipt.read_bytes()
            corrupt_fixture = ReleaseFixture.from_archive(
                archive,
                args.target,
                args.version,
                corrupt_download=True,
            )
            server.set_fixture(corrupt_fixture)
            result = run_update(
                installed, environment, server, succeeds=False
            )
            assert_request_contract(server, expect_release_recheck=False)
            wait_for_empty_process_temp(process_temp_dir)
            if "digest" not in result.stderr.lower():
                raise ValueError(
                    f"corrupt download did not report digest rejection: {result.stderr!r}"
                )
            if sha256_file(installed) != preserved_digest:
                raise ValueError("corrupt download changed the installed executable")
            if receipt.read_bytes() != preserved_receipt:
                raise ValueError("corrupt download changed the install receipt")

            reset_test_updater(
                test_updater_binary,
                installed,
                receipt,
                args.target,
                args.version,
                environment,
            )
            preserved_digest = sha256_file(installed)
            preserved_receipt = receipt.read_bytes()
            failing_candidate = compile_post_install_failure_candidate(
                fixture_dir, args.version, windows
            )
            verify_version(failing_candidate, args.version, environment)
            failing_archive = fixture_dir / archive.name
            package_candidate(
                failing_candidate,
                failing_archive,
                args.target,
                args.version,
            )
            rollback_fixture = ReleaseFixture.from_archive(
                failing_archive, args.target, args.version
            )
            server.set_fixture(rollback_fixture)
            result = run_update(
                installed, environment, server, succeeds=False
            )
            assert_request_contract(server, expect_release_recheck=True)
            wait_for_empty_process_temp(process_temp_dir)
            failure = result.stderr.lower()
            if "post-install validation" not in failure or "restored" not in failure:
                raise ValueError(
                    "post-install fixture did not exercise confirmed rollback: "
                    f"{result.stderr!r}"
                )
            if sha256_file(installed) != preserved_digest:
                raise ValueError("post-install rollback did not restore the previous binary")
            if receipt.read_bytes() != preserved_receipt:
                raise ValueError("post-install rollback did not preserve the previous receipt")
            verify_receipt(receipt, args.target, args.version)
            verify_version(installed, args.version, environment)

            update_lock = install_dir / UPDATE_LOCK
            if (
                not update_lock.is_file()
                or update_lock.is_symlink()
                or update_lock.stat().st_size != 0
            ):
                raise ValueError("self-update did not leave a safe, empty sibling lock file")
            expected_entries = {installed.name, INSTALL_RECEIPT, UPDATE_LOCK}
            wait_for_exact_entries(install_dir, expected_entries)
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)

    print(f"native self-update smoke passed for {args.target}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
