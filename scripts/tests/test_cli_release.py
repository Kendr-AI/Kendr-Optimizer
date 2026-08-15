from __future__ import annotations

import datetime as dt
import json
import re
import sys
import tempfile
import tomllib
import unittest
import warnings
from pathlib import Path, PurePosixPath


ROOT = Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / "scripts"
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

import cli_release as release  # noqa: E402
import smoke_installer  # noqa: E402


def load_toml(path: Path) -> dict:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def metadata_value(text: str, key: str) -> str:
    match = re.search(rf"^{re.escape(key)}:\s*([^\s]+)\s*$", text, re.MULTILINE)
    if match is None:
        raise AssertionError(f"missing {key!r} metadata")
    return match.group(1)


class CliReleaseTests(unittest.TestCase):
    def test_release_version_is_synchronized(self) -> None:
        workspace = load_toml(ROOT / "Cargo.toml")
        version = workspace["workspace"]["package"]["version"]
        release.validate_version(version)

        citation = (ROOT / "CITATION.cff").read_text(encoding="utf-8")
        self.assertEqual(metadata_value(citation, "version"), version)
        release_date = metadata_value(citation, "date-released")
        dt.date.fromisoformat(release_date)

        changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
        self.assertIn(f"## [{version}] - {release_date}", changelog)

        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        self.assertIn(f"pre-alpha (`{version}`)", readme)
        self.assertIn(f"releases/download/v{version}/", readme)
        release.verify_installer_versions(ROOT / "install", version)

        release_notes = ROOT / "docs" / "releases" / f"v{version}.md"
        self.assertTrue(release_notes.is_file(), "versioned release notes are missing")
        self.assertIn(
            f"# Kendr Optimizer v{version}",
            release_notes.read_text(encoding="utf-8"),
        )

        workflow = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        self.assertIn('echo "RELEASE_VERSION=${version}"', workflow)
        self.assertIn('--version "${RELEASE_VERSION}"', workflow)
        self.assertIn('--notes-file "docs/releases/${GITHUB_REF_NAME}.md"', workflow)

        lock = load_toml(ROOT / "Cargo.lock")
        local_packages = {
            package["name"]: package["version"]
            for package in lock["package"]
            if package["name"].startswith("kendr-optimizer-")
        }
        self.assertEqual(
            local_packages,
            {
                "kendr-optimizer-cli": version,
                "kendr-optimizer-contracts": version,
                "kendr-optimizer-core": version,
            },
        )

        core = load_toml(ROOT / "crates/kendr-optimizer-core/Cargo.toml")
        cli = load_toml(ROOT / "crates/kendr-optimizer-cli/Cargo.toml")
        self.assertEqual(
            core["dependencies"]["kendr-optimizer-contracts"]["version"], version
        )
        for dependency in ("kendr-optimizer-contracts", "kendr-optimizer-core"):
            self.assertEqual(cli["dependencies"][dependency]["version"], version)

        for adapter in (
            "claude-channels",
            "claude-code",
            "openclaw",
            "opencode",
            "pi-agent",
        ):
            package = json.loads(
                (ROOT / "integrations" / adapter / "package.json").read_text(
                    encoding="utf-8"
                )
            )
            self.assertEqual(package["version"], version)
        hermes = load_toml(ROOT / "integrations/hermes-agent/pyproject.toml")
        self.assertEqual(hermes["project"]["version"], version)
        marketplace = json.loads(
            (ROOT / ".claude-plugin/marketplace.json").read_text(encoding="utf-8")
        )
        self.assertEqual(marketplace["plugins"][0]["version"], version)

        notices = (ROOT / "THIRD_PARTY_LICENSES.html").read_text(encoding="utf-8")
        for package in sorted(local_packages):
            self.assertIn(f"{package} {version}", notices)

    def test_release_targets_match_license_generation(self) -> None:
        about = load_toml(ROOT / "about.toml")
        self.assertEqual(set(about["targets"]), set(release.SUPPORTED_TARGETS))

        expected_assets = {
            release.archive_name(target) for target in release.SUPPORTED_TARGETS
        }
        expected_assets.update(release.INSTALLER_ASSETS)
        expected_assets.update(release.adapter_assets())
        expected_assets.add("SHA256SUMS")
        self.assertEqual(release.expected_release_assets(), expected_assets)

        self.assertEqual(
            release.binary_filename("x86_64-pc-windows-msvc"), "kendr-opt.exe"
        )
        self.assertEqual(
            release.binary_filename("x86_64-unknown-linux-musl"), "kendr-opt"
        )
        with self.assertRaisesRegex(ValueError, "unsupported release target"):
            release.archive_name("unsupported-target")

    def test_outbound_http_dependency_is_confined_to_the_cli_updater(self) -> None:
        contracts = load_toml(ROOT / "crates/kendr-optimizer-contracts/Cargo.toml")
        core = load_toml(ROOT / "crates/kendr-optimizer-core/Cargo.toml")
        cli = load_toml(ROOT / "crates/kendr-optimizer-cli/Cargo.toml")
        self.assertNotIn("reqwest", contracts.get("dependencies", {}))
        self.assertNotIn("reqwest", core.get("dependencies", {}))
        self.assertIn("reqwest", cli["dependencies"])

        for crate in ("kendr-optimizer-contracts", "kendr-optimizer-core"):
            for source in (ROOT / "crates" / crate / "src").glob("**/*.rs"):
                self.assertNotIn(
                    "reqwest",
                    source.read_text(encoding="utf-8"),
                    f"outbound HTTP client leaked into {source}",
                )

    def test_official_install_receipt_contract_is_verified(self) -> None:
        target = "x86_64-pc-windows-msvc"
        version = release.PROJECT_VERSION
        expected = {
            "schema_version": "kendr.install/v1",
            "repository": "Kendr-AI/Kendr-Optimizer",
            "install_method": "github-release",
            "target": target,
            "version": version,
            "channel": "preview",
        }
        with tempfile.TemporaryDirectory() as directory:
            receipt = Path(directory) / smoke_installer.INSTALL_RECEIPT
            receipt.write_text(json.dumps(expected) + "\n", encoding="utf-8")
            smoke_installer.verify_install_receipt(receipt, target, version)

            receipt.write_text(
                json.dumps({**expected, "unexpected": True}) + "\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(ValueError, "install receipt mismatch"):
                smoke_installer.verify_install_receipt(receipt, target, version)

        for installer in release.INSTALLER_ASSETS.values():
            text = installer.read_text(encoding="utf-8")
            for marker in (
                ".kendr-opt-install.json",
                "schema_version",
                "kendr.install/v1",
                "Kendr-AI/Kendr-Optimizer",
                "github-release",
                "preview",
            ):
                self.assertIn(marker, text, f"{installer.name} omitted {marker}")
            self.assertIn("KENDR_INSTALLER_TEST_MODE", text)
            self.assertIn("127", text)
            self.assertIn("github.com/", text)

    def test_workflow_actions_are_pinned_and_release_notes_exist(self) -> None:
        workflow = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        references = re.findall(r"\buses:\s+[^@\s]+@([^\s#]+)", workflow)
        self.assertTrue(references)
        for reference in references:
            with self.subTest(reference=reference):
                self.assertRegex(reference, r"^[0-9a-f]{40}$")

        workspace = load_toml(ROOT / "Cargo.toml")
        version = workspace["workspace"]["package"]["version"]
        self.assertTrue((ROOT / f"docs/releases/v{version}.md").is_file())

        draft_verification = workflow.split(
            "- name: Verify uploaded asset digests", maxsplit=1
        )[1].split("- name: Publish prerelease", maxsplit=1)[0]
        self.assertIn('gh release view "${GITHUB_REF_NAME}"', draft_verification)
        self.assertNotIn("releases/tags/", draft_verification)
        self.assertIn('release["isDraft"] is True', draft_verification)

        publication = workflow.split("- name: Publish prerelease", maxsplit=1)[1]
        self.assertIn('release["immutable"] is True', publication)
        self.assertIn('gh release delete "${GITHUB_REF_NAME}" --yes', publication)
        self.assertIn("--features update-test-server", workflow)
        self.assertIn("--all-features", workflow)

    def test_version_validation_rejects_unsafe_values(self) -> None:
        for version in ("0.1.1", "1.2.3-rc.1", "1.2.3+build.7"):
            with self.subTest(version=version):
                release.validate_version(version)

        for version in ("", "v0.1.1", "1.2", "1/2/3", "1.2.3\nnext"):
            with self.subTest(version=version):
                with self.assertRaisesRegex(ValueError, "invalid release version"):
                    release.validate_version(version)

    def test_release_archives_are_reproducible_and_fully_verified(self) -> None:
        workspace = load_toml(ROOT / "Cargo.toml")
        version = workspace["workspace"]["package"]["version"]
        epoch = 1_700_000_000
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            first = root / "first"
            second = root / "second"
            self._write_release_directory(first, version, epoch)
            self._write_release_directory(second, version, epoch)

            release.verify_directory(first, version)
            release.verify_directory(second, version)
            self.assertEqual(
                {
                    path.name: release.sha256(path)
                    for path in first.iterdir()
                    if path.is_file()
                },
                {
                    path.name: release.sha256(path)
                    for path in second.iterdir()
                    if path.is_file()
                },
            )

            damaged = first / release.archive_name("x86_64-pc-windows-msvc")
            damaged.write_bytes(damaged.read_bytes() + b"tampered")
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                release.verify_directory(first, version)

    def test_archive_verification_rejects_duplicate_members(self) -> None:
        version = "0.1.1"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "member"
            source.write_bytes(b"payload")
            target = "x86_64-pc-windows-msvc"
            archive = root / release.archive_name(target)
            members = {
                PurePosixPath(name).name: (source, mode)
                for name, mode in release.expected_archive_members(
                    version, target
                ).items()
            }
            release.write_zip(
                archive,
                release.archive_root(version, target),
                members,
                1_700_000_000,
            )
            binary_name = f"{release.archive_root(version, target)}/kendr-opt.exe"
            import zipfile

            with warnings.catch_warnings():
                warnings.simplefilter("ignore", UserWarning)
                with zipfile.ZipFile(archive, "a") as duplicate:
                    duplicate.writestr(binary_name, b"duplicate")
            with self.assertRaisesRegex(ValueError, "unexpected ZIP members"):
                release.verify_archive(archive, version, target)

    def test_checksum_and_remote_digest_parsers_fail_closed(self) -> None:
        digest = "a" * 64
        version = release.PROJECT_VERSION
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            checksums = root / "SHA256SUMS"
            checksums.write_text(
                f"{digest}  first.zip\n{'b' * 64}  second.tar.gz\n",
                encoding="ascii",
                newline="\n",
            )
            self.assertEqual(
                release.read_checksums(checksums),
                {"first.zip": digest, "second.tar.gz": "b" * 64},
            )

            checksums.write_text(
                f"{digest}  first.zip\n{digest}  first.zip\n",
                encoding="ascii",
                newline="\n",
            )
            with self.assertRaisesRegex(ValueError, "duplicate checksum entry"):
                release.read_checksums(checksums)

            for name in release.expected_release_assets() - {
                release.nanoclaw_asset_name(version)
            }:
                (root / name).write_bytes(f"asset:{name}\n".encode())
            release.package_nanoclaw(root, version, 0)
            payload = {
                "assets": [
                    {
                        "name": name,
                        "digest": f"sha256:{release.sha256(root / name)}",
                        "state": "uploaded",
                        "size": (root / name).stat().st_size,
                    }
                    for name in sorted(release.expected_release_assets())
                ]
            }
            release_json = root / "release.json"
            release_json.write_text(json.dumps(payload), encoding="utf-8")
            release.verify_github_assets(root, release_json)

            payload["assets"][0]["digest"] = f"sha256:{'0' * 64}"
            release_json.write_text(json.dumps(payload), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "digest mismatch"):
                release.verify_github_assets(root, release_json)

    def _write_release_directory(
        self, directory: Path, version: str, epoch: int
    ) -> None:
        directory.mkdir(parents=True)
        sources = directory.parent / f"{directory.name}-sources"
        sources.mkdir()

        for target in release.SUPPORTED_TARGETS:
            target_sources = sources / target
            target_sources.mkdir()
            members: dict[str, tuple[Path, int]] = {}
            for member, mode in release.expected_archive_members(
                version, target
            ).items():
                name = PurePosixPath(member).name
                source = target_sources / name
                source.write_bytes(f"payload:{target}:{name}\n".encode())
                members[name] = (source, mode)

            output = directory / release.archive_name(target)
            archive_root = release.archive_root(version, target)
            if output.suffix == ".zip":
                release.write_zip(output, archive_root, members, epoch)
            else:
                release.write_tar_gz(output, archive_root, members, epoch)
            release.verify_archive(output, version, target)

        release.copy_installers(directory)
        for name in release.adapter_assets(version) - {
            release.nanoclaw_asset_name(version)
        }:
            (directory / name).write_bytes(f"asset:{name}\n".encode())
        release.package_nanoclaw(directory, version, epoch)
        release.write_checksums(directory, version)


if __name__ == "__main__":
    unittest.main()
