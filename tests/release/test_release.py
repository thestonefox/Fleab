"""No API credentials or remote writes: exercise rules and real Git snapshots."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("fleab_release", ROOT / "tools/prepare-release.py")
release = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = release
spec.loader.exec_module(release)


def commit(message, number=1):
    return release.Commit(f"{number:040x}", message)


class Rules(unittest.TestCase):
    def test_patch_types_and_unscoped_commits(self):
        for kind in ["fix", "refactor", "docs", "test", "ci", "build", "chore", "perf", "style", "revert"]:
            with self.subTest(kind=kind):
                self.assertEqual(release.bump_for([commit(f"{kind}(core): improve behaviour")]), "patch")
        self.assertEqual(release.bump_for([commit("fix: handle missing share")]), "patch")

    def test_any_feature_wins_over_patch_commits(self):
        commits = [commit("docs(app): describe profiles"), commit("feat(app): add runner selection"), commit("fix(core): avoid locks")]
        self.assertEqual(release.bump_for(commits), "minor")

    def test_breaking_syntax_for_any_type_and_body_footer(self):
        for message in ["feat!: change API", "feat(core)!: change API", "refactor(api)!: rename method",
                        "fix(api): change behaviour\n\nBREAKING CHANGE: users must migrate",
                        "docs: document migration\n\nBREAKING-CHANGE: remove legacy support",
                        "Nonconventional heading\n\nBREAKING CHANGE: format changed"]:
            with self.subTest(message=message):
                self.assertEqual(release.bump_for([commit("feat: add feature"), commit(message)]), "major")

    def test_major_is_major_even_before_one(self):
        self.assertEqual(release.next_version("v0.9.8", "major"), "v1.0.0")
        self.assertEqual(release.next_version("v1.9.8", "minor"), "v1.10.0")
        self.assertEqual(release.next_version("v1.9.8", "patch"), "v1.9.9")

    def test_version_order_is_numeric_and_strict(self):
        self.assertEqual(sorted(["v1.9.0", "v1.10.0", "v1.2.0"], key=release.version), ["v1.2.0", "v1.9.0", "v1.10.0"])
        for tag in ["1.2.3", "v01.2.3", "v1.2", "v1.2.3-beta.1", "v1.2.3;evil"]:
            with self.assertRaises(ValueError):
                release.version(tag)

    def test_nonconventional_commit_still_releases_patch(self):
        self.assertEqual(release.bump_for([commit("Improve player deployment")]), "patch")
        with self.assertRaises(ValueError):
            release.bump_for([])

    def test_notes_type_scope_subject_hierarchy_and_links(self):
        pr = {"number": 8, "title": "Improve builds", "html_url": "https://github.com/example/fleab/pull/8"}
        notes = release.release_notes("v1.1.0", "v1.0.0", "minor", pr, [
            commit("feat(publisher): Add managed compressor", 1),
            commit("fix(deployer): Handle offline shares", 2),
            commit("fix(publisher): Preserve output", 3),
            commit("docs: Explain profiles", 4),
        ])
        self.assertIn("- **feat**\n  - **publisher**\n    - Add managed compressor", notes)
        self.assertIn("- **fix**\n  - **deployer**\n    - Handle offline shares", notes)
        self.assertIn("  - **publisher**\n    - Preserve output", notes)
        self.assertIn("- **docs**\n  - **general**\n    - Explain profiles", notes)
        self.assertIn("https://github.com/example/fleab/commit/", notes)

    def test_notes_mark_breaking_and_escape_user_text(self):
        pr = {"number": 1, "title": "<script> @everyone", "html_url": "https://github.com/example/fleab/pull/1"}
        notes = release.release_notes("v1.0.0", "v0.1.0", "major", pr, [commit("feat(ui)!: [click](https://evil.test) <img> @someone")])
        self.assertIn("**BREAKING CHANGE**", notes)
        self.assertNotIn("<script>", notes)
        self.assertNotIn("[click](https://evil.test)", notes)
        self.assertNotIn("@everyone", notes)
        self.assertIn("&lt;img&gt;", notes)


class Pagination(unittest.TestCase):
    def test_original_pr_commits_beyond_rest_250_limit(self):
        api = release.GitHub("example/fleab", "not-a-real-token")
        calls = []
        def request(url, payload):
            calls.append(payload["variables"]["after"])
            page = len(calls) - 1
            count = 100 if page < 2 else 51
            nodes = [{"commit": {"oid": f"{page*100+i+1:040x}", "message": "feat!: break" if page == 2 and i == 50 else "fix(core): patch"}} for i in range(count)]
            return {"data": {"repository": {"pullRequest": {"commits": {
                "totalCount": 251, "nodes": nodes,
                "pageInfo": {"hasNextPage": page < 2, "endCursor": f"page{page}"},
            }}}}}
        with patch.object(api, "request", side_effect=request):
            commits = api.commits(7)
        self.assertEqual(len(commits), 251)
        self.assertEqual(release.bump_for(commits), "major")
        self.assertEqual(calls, [None, "page0", "page1"])

    def test_closed_pr_pagination_ignores_unmerged_prs(self):
        api = release.GitHub("example/fleab", "not-a-real-token")
        first = [{"number": i, "merged_at": "2026-11-08T10:00:00Z" if i % 2 else None} for i in range(100)]
        with patch.object(api, "request", side_effect=[first, [{"number": 101, "merged_at": "2026-11-08T10:00:00Z"}]]):
            self.assertEqual(len(api.merged_prs()), 51)

    def test_api_commit_count_must_be_complete(self):
        api = release.GitHub("example/fleab", "not-a-real-token")
        response = {"data": {"repository": {"pullRequest": {"commits": {
            "totalCount": 2, "nodes": [{"commit": {"oid": "a"*40, "message": "fix: patch"}}],
            "pageInfo": {"hasNextPage": False, "endCursor": None},
        }}}}}
        with patch.object(api, "request", return_value=response), self.assertRaisesRegex(RuntimeError, "Incomplete"):
            api.commits(1)


class FakeGitHub:
    def __init__(self):
        self.prs = []
        self.messages = {}
        self.released = set()

    def merged_prs(self):
        return self.prs

    def commits(self, number):
        return self.messages[number]

    def published(self, tag):
        return tag in self.released


class GitIntegration(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="fleab-semver-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.repo = release.Repository(self.root)
        self.repo.git("init", "-b", "main")
        self.repo.git("config", "user.name", "Fleab tests")
        self.repo.git("config", "user.email", "tests@example.invalid")
        files = {
            "Cargo.toml": '[workspace]\nmembers = ["deployer"]\n\n[workspace.package]\nversion = "0.1.0"\nedition = "2024"\n',
            "Cargo.lock": 'version = 4\n\n[[package]]\nname = "fleab"\nversion = "0.1.0"\n',
            "unity-package/package.json": '{"name":"com.fleab.publisher","version":"0.1.0"}\n',
            "README.md": 'Install ?path=/unity-package#v0.1.0\n',
            "docs/publisher.md": 'Pin #v0.1.0\n',
        }
        for name, value in files.items():
            file = self.root / name
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_text(value, encoding="utf-8")
        self.repo.git("add", ".")
        self.repo.git("commit", "-m", "chore(repo): initial version", "-m", "Set up fixture files.")
        self.initial = self.repo.git("rev-parse", "HEAD").decode().strip()
        self.repo.git("tag", "v0.1.0")
        self.api = FakeGitHub()

    def merge(self, number, messages, date=None):
        # Squash-style merge: main's message intentionally hides the features.
        (self.root / f"change{number}.txt").write_text(f"PR {number}")
        self.repo.git("add", ".")
        self.repo.git("commit", "-m", f"Merge PR #{number}")
        sha = self.repo.git("rev-parse", "HEAD").decode().strip()
        self.api.prs.append({"number": number, "title": f"PR {number}", "html_url": f"https://github.com/example/fleab/pull/{number}",
                             "merged_at": date or f"2026-11-08T10:00:{number:02d}Z", "merge_commit_sha": sha})
        self.api.messages[number] = [commit(message, number*100+i) for i, message in enumerate(messages)]
        return sha

    def prepare(self):
        return release.prepare(self.repo, self.api, push=False)

    def test_tag_updates_both_manifests_and_lock_without_touching_main(self):
        merge_sha = self.merge(1, ["fix(core): patch", "feat(publisher): add packaging"])
        original_index = self.repo.git("write-tree")
        self.assertEqual(self.prepare(), [{"tag": "v0.2.0"}])
        self.assertEqual(self.repo.git("rev-parse", "HEAD").decode().strip(), merge_sha)
        self.assertEqual(self.repo.git("write-tree"), original_index)
        self.assertEqual(self.repo.git("status", "--porcelain"), b"")
        self.assertEqual(tomllib.loads(self.repo.read("v0.2.0", "Cargo.toml").decode())["workspace"]["package"]["version"], "0.2.0")
        self.assertEqual(json.loads(self.repo.read("v0.2.0", "unity-package/package.json"))["version"], "0.2.0")
        self.assertEqual(tomllib.loads(self.repo.read("v0.2.0", "Cargo.lock").decode())["package"][0]["version"], "0.2.0")
        self.assertEqual(self.repo.git("rev-parse", "v0.2.0^1").decode().strip(), merge_sha)
        self.assertIn(b"#v0.2.0", self.repo.read("v0.2.0", "README.md"))
        self.assertEqual(self.repo.metadata("v0.2.0")["pr_number"], 1)
        self.assertIn(b"- **feat**\n  - **publisher**", self.repo.read("v0.2.0", release.NOTES))
        message = self.repo.git("show", "-s", "--format=%B", "v0.2.0^{}").decode().strip()
        self.assertTrue(all(len(line) <= 72 for line in message.splitlines()))

    def test_missed_concurrency_events_are_drained_in_merge_order(self):
        first = self.merge(1, ["docs: explain"])
        second = self.merge(2, ["feat(ui): choose runner"])
        third = self.merge(3, ["refactor(api)!: remove old API"])
        self.api.prs.reverse()  # API ordering must not determine version order.
        self.assertEqual(self.prepare(), [{"tag": "v0.1.1"}, {"tag": "v0.2.0"}, {"tag": "v1.0.0"}])
        for tag, sha in [("v0.1.1", first), ("v0.2.0", second), ("v1.0.0", third)]:
            self.assertEqual(self.repo.metadata(tag)["merge_sha"], sha)
            self.assertEqual(self.repo.git("rev-parse", f"{tag}^1").decode().strip(), sha)
        self.assertNotIn(b"change3.txt", self.repo.git("ls-tree", "--name-only", "v0.1.1"))

    def test_reruns_keep_existing_tags_and_retry_only_unpublished(self):
        self.merge(1, ["fix: patch"])
        self.prepare()
        old_tag = self.repo.git("rev-parse", "v0.1.1")
        self.assertEqual(self.prepare(), [{"tag": "v0.1.1"}])
        self.assertEqual(self.repo.git("rev-parse", "v0.1.1"), old_tag)
        self.api.released.add("v0.1.1")
        self.assertEqual(self.prepare(), [])
        self.merge(2, ["feat: feature"])
        self.assertEqual(self.prepare(), [{"tag": "v0.2.0"}])

    def test_prs_in_initial_tag_are_not_released_again(self):
        self.api.prs.append({"number": 12, "merge_commit_sha": self.initial, "merged_at": "2026-11-08T09:00:00Z"})
        self.assertEqual(self.prepare(), [])

    def test_no_initial_tag_fails_without_making_tags(self):
        self.repo.git("tag", "-d", "v0.1.0")
        self.merge(1, ["feat: feature"])
        with self.assertRaisesRegex(ValueError, "initial stable version tag"):
            self.prepare()
        self.assertEqual(self.repo.tags(), [])

    def test_latest_manual_tag_is_an_explicit_new_baseline(self):
        self.merge(1, ["feat: already released manually"])
        self.repo.git("tag", "v2.0.0")
        self.merge(2, ["fix: new patch"])
        self.assertEqual(self.prepare(), [{"tag": "v2.0.1"}])

    def test_all_tags_are_pushed_atomically_without_pushing_main(self):
        self.merge(1, ["fix: one"])
        self.merge(2, ["feat: two"])
        commands = []
        original = self.repo.git
        def git(*args, **kwargs):
            if args[0] == "push":
                commands.append(args)
                return b""
            return original(*args, **kwargs)
        with patch.object(self.repo, "git", side_effect=git):
            release.prepare(self.repo, self.api)
        self.assertEqual(commands, [("push", "--atomic", "origin", "refs/tags/v0.1.1", "refs/tags/v0.2.0")])

    def test_missing_lock_version_does_not_create_a_tag_or_modify_worktree(self):
        (self.root / "Cargo.lock").write_text('version = 4\n')
        self.repo.git("add", "Cargo.lock")
        self.repo.git("commit", "-m", "chore: remove lock entry in fixture")
        self.merge(1, ["feat: one"])
        with self.assertRaisesRegex(ValueError, "Cargo.lock"):
            self.prepare()
        self.assertEqual(self.repo.tags(), ["v0.1.0"])
        self.assertEqual(self.repo.git("status", "--porcelain"), b"")


if __name__ == "__main__":
    unittest.main()
