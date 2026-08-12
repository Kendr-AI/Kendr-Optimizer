from __future__ import annotations

import json
import sys
import tempfile
import unittest
from unittest import mock
from pathlib import Path


RUNNERS = Path(__file__).resolve().parents[1]
if str(RUNNERS) not in sys.path:
    sys.path.insert(0, str(RUNNERS))

from public_artifacts import (  # noqa: E402
    PublicAlias,
    PublicArtifactSanitizer,
    default_aliases,
    file_sha256,
    verify_release,
)


class PublicArtifactSanitizerTests(unittest.TestCase):
    def setUp(self) -> None:
        separator = chr(92)
        self.project = separator.join(
            ("C:", "Users", "Alice Smith", "work", "KendrOptimizer")
        )
        self.release = self.project + separator + separator.join(
            ("releases", "v0.1.0-benchmark.5")
        )
        self.sanitizer = PublicArtifactSanitizer(
            [
                PublicAlias("PROJECT_ROOT", self.project),
                PublicAlias("RELEASE_ROOT", self.release),
                PublicAlias("USER_NAME", "Alice Smith"),
            ]
        )

    def test_replaces_windows_json_and_forward_slash_variants(self) -> None:
        value = "\n".join(
            [
                self.release + r"\runs\peer.json",
                self.project.replace("\\", "/") + "/benchmarks/runners/worker.py",
                "file:///"
                + self.project.replace("\\", "/").replace(" ", "%20")
                + "/artifact.whl",
                json.dumps({"cwd": self.project}),
                "owner=Alice Smith",
                "fixture=C:/certs/prod-chain.pem",
            ]
        )
        sanitized, count = self.sanitizer.sanitize_text(value)
        self.assertGreaterEqual(count, 5)
        self.assertIn("<RELEASE_ROOT>", sanitized)
        self.assertIn("<PROJECT_ROOT>", sanitized)
        self.assertIn("<USER_NAME>", sanitized)
        self.assertNotIn("Alice Smith", sanitized)
        self.assertIn("C:/certs/prod-chain.pem", sanitized)

    def test_specific_release_alias_wins_over_project_root(self) -> None:
        sanitized, _ = self.sanitizer.sanitize_text(self.release)
        self.assertEqual(sanitized, "<RELEASE_ROOT>")

    @mock.patch.dict("os.environ", {"USER": "runner", "USERNAME": "runner"})
    @mock.patch("getpass.getuser", return_value="runner")
    def test_hosted_ci_runner_is_not_treated_as_a_private_name(
        self, _getuser: mock.Mock
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            release = root / "release"
            aliases = default_aliases(root, release)

        self.assertNotIn(
            ("USER_NAME", "runner"),
            {(alias.label, alias.private) for alias in aliases},
        )

    def test_sanitize_tree_rewrites_utf8_artifacts_and_checks_afterward(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "logs" / "execution.json"
            artifact.parent.mkdir(parents=True)
            artifact.write_text(
                json.dumps(
                    {
                        "cwd": self.project,
                        "owner": "Alice Smith",
                        "package_source": "file:///"
                        + "C:"
                        + "/actions-runner/build/pip.whl",
                    }
                ),
                encoding="utf-8",
            )
            utf16_artifact = root / "logs" / "installer.log"
            utf16_artifact.write_text(
                "Processing " + self.project.lower() + "\\artifact.whl",
                encoding="utf-16",
            )
            report = self.sanitizer.sanitize_tree(root)
            self.assertEqual(
                report.files_changed,
                ("logs/execution.json", "logs/installer.log"),
            )
            self.assertGreaterEqual(report.replacements, 4)
            self.assertIn("<LOCAL_FILE_URI>", artifact.read_text(encoding="utf-8"))
            self.assertIn(
                "<PROJECT_ROOT>",
                utf16_artifact.read_text(encoding="utf-16"),
            )
            self.sanitizer.assert_tree_safe(root)


class ReleaseVerificationTests(unittest.TestCase):
    def _make_release(self, root: Path, *, successful: bool = True) -> Path:
        release = root / "v0.1.0-benchmark.test"
        (release / "logs").mkdir(parents=True)
        (release / "artifact.txt").write_text("public evidence\n", encoding="utf-8")
        (release / "logs" / "execution.json").write_text(
            json.dumps(
                {
                    "attempts": [
                        {
                            "id": "peer",
                            "exit_code": 0 if successful else 1,
                            "timed_out": False,
                        }
                    ]
                }
            )
            + "\n",
            encoding="utf-8",
        )
        artifacts = []
        for path in sorted(item for item in release.rglob("*") if item.is_file()):
            artifacts.append(
                {
                    "path": path.relative_to(release).as_posix(),
                    "sha256": file_sha256(path),
                    "bytes": path.stat().st_size,
                }
            )
        (release / "manifest.json").write_text(
            json.dumps({"artifacts": artifacts}, indent=2) + "\n",
            encoding="utf-8",
        )
        checksum_paths = sorted(
            item for item in release.rglob("*") if item.is_file()
        )
        (release / "SHA256SUMS").write_text(
            "\n".join(
                f"{file_sha256(path)}  {path.relative_to(release).as_posix()}"
                for path in checksum_paths
            )
            + "\n",
            encoding="utf-8",
        )
        return release

    def test_verifies_complete_manifest_checksums_and_attempts(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            release = self._make_release(Path(directory))
            report = verify_release(release, require_complete_attempts=True)
            self.assertEqual(report.manifest_entries, 2)
            self.assertEqual(report.sha256_entries, 3)
            self.assertEqual(report.complete_attempts, 1)

    def test_rejects_tampering(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            release = self._make_release(Path(directory))
            (release / "artifact.txt").write_text("tampered\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "manifest .* mismatch"):
                verify_release(release)

    def test_rejects_incomplete_attempt(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            release = self._make_release(Path(directory), successful=False)
            with self.assertRaisesRegex(ValueError, "incomplete benchmark command"):
                verify_release(release, require_complete_attempts=True)


if __name__ == "__main__":
    unittest.main()
