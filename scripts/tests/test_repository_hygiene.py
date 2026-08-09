from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path, PurePosixPath


SCRIPTS = Path(__file__).resolve().parents[1]
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

import check_repository_hygiene as hygiene  # noqa: E402


class RepositoryHygieneTests(unittest.TestCase):
    def test_rejects_repository_development_controls(self) -> None:
        for path in (
            PurePosixPath(".codex/settings.json"),
            PurePosixPath("tools/.claude/settings.json"),
            PurePosixPath("AGENTS.md"),
            PurePosixPath("nested/CLAUDE.md"),
            PurePosixPath("nested/CODEX.md"),
            PurePosixPath("GEMINI.md"),
            PurePosixPath(".cursor/rules/project.mdc"),
            PurePosixPath(".agents/local-policy.md"),
            PurePosixPath(".github/copilot-instructions.md"),
            PurePosixPath(".github/prompts/release.prompt.md"),
            PurePosixPath(".windsurfrules"),
        ):
            with self.subTest(path=path):
                self.assertTrue(hygiene.path_violations(path))

    def test_allows_only_audited_target_harness_packages(self) -> None:
        self.assertFalse(
            hygiene.path_violations(
                PurePosixPath(
                    "integrations/claude-code/.claude-plugin/plugin.json"
                )
            )
        )
        self.assertFalse(
            hygiene.path_violations(
                PurePosixPath("integrations/nanoclaw/skill/SKILL.md")
            )
        )
        self.assertTrue(
            hygiene.path_violations(
                PurePosixPath("integrations/other/.claude-plugin/plugin.json")
            )
        )
        self.assertTrue(
            hygiene.path_violations(PurePosixPath("scripts/SKILL.md"))
        )
        self.assertFalse(
            hygiene.missing_distribution_files(
                hygiene.REQUIRED_DISTRIBUTION_FILES
            )
        )
        self.assertEqual(
            hygiene.missing_distribution_files(
                hygiene.REQUIRED_DISTRIBUTION_FILES - {hygiene.NANOCLAW_SKILL}
            ),
            [hygiene.NANOCLAW_SKILL],
        )

    def test_rejects_generated_caches_and_bytecode(self) -> None:
        for path in (
            PurePosixPath("src/__pycache__/module.pyc"),
            PurePosixPath("package/node_modules/dependency/index.js"),
            PurePosixPath("target/debug/kendr-opt"),
            PurePosixPath("tmp/pdfs/page-001.png"),
            PurePosixPath("package/build/lib/module.py"),
            PurePosixPath("package/src/example.egg-info/PKG-INFO"),
            PurePosixPath("frontend/tsconfig.tsbuildinfo"),
            PurePosixPath("frontend/.eslintcache"),
        ):
            with self.subTest(path=path):
                self.assertTrue(hygiene.path_violations(path))

    def test_detects_raw_escaped_encoded_and_utf16_profile_paths(self) -> None:
        separator = chr(92)
        raw_profile = f"C:{separator}Users{separator}Example{separator}file.json"
        samples = (
            raw_profile.encode(),
            raw_profile.replace(separator, separator * 2).encode(),
            raw_profile.replace(separator, "/").encode(),
            b"C%3A%5C" + b"Users" + b"%5CExample%5Cfile.json",
        )
        for sample in samples:
            with self.subTest(sample=sample):
                self.assertIsNotNone(hygiene.WINDOWS_USER_PROFILE.search(sample))

        with tempfile.TemporaryDirectory() as directory:
            artifact = Path(directory) / "artifact.txt"
            artifact.write_text(raw_profile, encoding="utf-16")
            self.assertTrue(hygiene.contains_windows_user_profile(artifact))

    def test_public_release_paths_have_no_legacy_exception(self) -> None:
        relative = PurePosixPath("releases/v0.1.0-benchmark.4/logs/execution.json")
        separator = chr(92)
        content = f"C:{separator}Users{separator}Different{separator}artifact.json"

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root.joinpath(*relative.parts)
            artifact.parent.mkdir(parents=True)
            artifact.write_text(content, encoding="utf-8")

            findings = hygiene.inspect(root, [relative])
            self.assertIn(
                (
                    relative.as_posix(),
                    "absolute Windows user-profile path in public content",
                ),
                findings,
            )


if __name__ == "__main__":
    unittest.main()
