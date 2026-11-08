"""Reserve SemVer tags for merged PRs; original PR commits determine the bump.

Only reads PR commit messages as data. Never checks out or executes PR heads.
Versioned release trees are built with a private Git index, leaving main alone.
"""
from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

TAG = re.compile(r"^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")
METADATA = ".github/fleab-release.json"
NOTES = ".github/release-notes.md"


@dataclass(frozen=True)
class Commit:
    sha: str
    message: str

def version(tag: str) -> tuple[int, int, int]:
    match = TAG.fullmatch(tag)
    if not match:
        raise ValueError(f"Not a stable vX.Y.Z tag: {tag}")
    return tuple(map(int, match.groups()))


def bump_for(commits: list[Commit]) -> str:
    if not commits:
        raise ValueError("A merged PR must have at least one commit")
    return engine({"action": "analyze", "commits": commit_data(commits)})["bump"]


def next_version(previous: str, bump: str) -> str:
    version(previous)
    return engine({"action": "increment", "previous": previous, "bump": bump})["tag"]


def commit_data(commits):
    return [{"hash": commit.sha, "message": commit.message} for commit in commits]


def engine(payload):
    """Use the locked official semantic-release plugins; never shell out text."""
    node = os.environ.get("FLEAB_RELEASE_NODE") or shutil.which("node")
    if not node:
        raise RuntimeError("Install Node 24 and run npm ci --prefix tools/release-engine --ignore-scripts")
    script = Path(__file__).resolve().parent / "release-engine/engine.mjs"
    result = subprocess.run([node, str(script)], input=json.dumps(payload).encode(), capture_output=True)
    if result.returncode:
        raise RuntimeError("Semantic-release plugin failed: " + result.stderr.decode(errors="replace"))
    return json.loads(result.stdout)


def release_notes(tag: str, previous: str, bump: str, pr: dict, commits: list[Commit]) -> str:
    return engine({"action": "notes", "tag": tag, "previous": previous, "bump": bump,
                   "pr": pr, "commits": commit_data(commits)})["notes"]


class GitHub:
    def __init__(self, repository: str, token: str):
        if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
            raise ValueError("Invalid GITHUB_REPOSITORY")
        self.repository = repository
        self.token = token
        self.api = os.environ.get("GITHUB_API_URL", "https://api.github.com").rstrip("/")
        self.graphql = os.environ.get("GITHUB_GRAPHQL_URL", "https://api.github.com/graphql")

    def request(self, url, payload=None, missing=False):
        data = None if payload is None else json.dumps(payload).encode()
        request = urllib.request.Request(url, data=data, headers={
            "Authorization": f"Bearer {self.token}", "Accept": "application/vnd.github+json",
            "Content-Type": "application/json", "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "Fleab-release-automation",
        })
        for attempt in range(4):
            try:
                with urllib.request.urlopen(request, timeout=60) as response:
                    return json.load(response)
            except urllib.error.HTTPError as error:
                if missing and error.code == 404:
                    return None
                if error.code not in (429, 500, 502, 503, 504) or attempt == 3:
                    raise RuntimeError(f"GitHub API request failed: HTTP {error.code}") from None
                time.sleep(2 ** attempt)
        raise RuntimeError("GitHub API request did not complete")

    def merged_prs(self):
        result = []
        page = 1
        while True:
            query = urllib.parse.urlencode({"state": "closed", "base": "main", "per_page": 100, "page": page})
            batch = self.request(f"{self.api}/repos/{self.repository}/pulls?{query}")
            result.extend(pr for pr in batch if pr.get("merged_at"))
            if len(batch) < 100:
                return result
            page += 1

    def commits(self, number: int) -> list[Commit]:
        # GraphQL pagination avoids REST's 250-commit cap on a PR. This also
        # preserves original messages when the PR was squash/rebase merged.
        query = """query($owner:String!,$name:String!,$number:Int!,$after:String) {
          repository(owner:$owner,name:$name) { pullRequest(number:$number) {
            commits(first:100,after:$after) { totalCount
              nodes { commit { oid message } }
              pageInfo { hasNextPage endCursor }
            }
          } }
        }"""
        owner, name = self.repository.split("/")
        variables = {"owner": owner, "name": name, "number": number, "after": None}
        result = []
        while True:
            response = self.request(self.graphql, {"query": query, "variables": variables})
            if response.get("errors"):
                raise RuntimeError(f"Cannot retrieve commits for PR #{number}")
            connection = response["data"]["repository"]["pullRequest"]["commits"]
            result.extend(Commit(node["commit"]["oid"], node["commit"]["message"]) for node in connection["nodes"])
            info = connection["pageInfo"]
            if not info["hasNextPage"]:
                if len(result) != connection["totalCount"]:
                    raise RuntimeError(f"Incomplete commit list for PR #{number}")
                return result
            if not info["endCursor"] or info["endCursor"] == variables["after"]:
                raise RuntimeError("PR pagination did not advance")
            variables["after"] = info["endCursor"]

    def published(self, tag: str) -> bool:
        release = self.request(f"{self.api}/repos/{self.repository}/releases/tags/{tag}", missing=True)
        return bool(release and not release["draft"])


class Repository:
    def __init__(self, root: Path):
        self.root = root.resolve()

    def git(self, *args, data=None, env=None, check=True):
        result = subprocess.run(["git", *args], cwd=self.root, input=data, capture_output=True, env=env)
        if check and result.returncode:
            raise subprocess.CalledProcessError(result.returncode, result.args, result.stdout, result.stderr)
        return result.stdout

    def read(self, ref: str, path: str) -> bytes:
        return self.git("show", f"{ref}:{path}")

    def tags(self):
        return sorted((tag for tag in self.git("tag", "--list").decode().splitlines() if TAG.fullmatch(tag)), key=version)

    def ancestor(self, ancestor: str, descendant: str) -> bool:
        result = subprocess.run(["git", "merge-base", "--is-ancestor", ancestor, descendant], cwd=self.root,
                                capture_output=True)
        if result.returncode not in (0, 1):
            raise RuntimeError("Cannot determine release ancestry; fetch the full main history")
        return result.returncode == 0

    def metadata(self, tag):
        try:
            raw = self.read(tag, METADATA)
        except subprocess.CalledProcessError:
            return None
        value = json.loads(raw)
        if value.get("schema_version") != 1 or value.get("version") != tag:
            raise ValueError(f"Invalid release metadata at {tag}")
        if not isinstance(value.get("pr_number"), int) or not re.fullmatch(r"[0-9a-f]{40,64}", value.get("merge_sha", "")):
            raise ValueError(f"Invalid release identity at {tag}")
        return value

    def tag_release(self, merge_sha: str, tag: str, previous: str, bump: str, pr: dict, commits: list[Commit], notes=None):
        import tomllib
        new = tag[1:]
        cargo = self.read(merge_sha, "Cargo.toml").decode()
        old = tomllib.loads(cargo)["workspace"]["package"]["version"]
        cargo, count = re.subn(r'(?ms)(^\[workspace\.package\]\s*\n(?:(?!^\[).)*?^version\s*=\s*)"[^"]+"',
                               lambda m: m[1] + json.dumps(new), cargo)
        if count != 1:
            raise ValueError("Cannot locate workspace package version")
        unity = json.loads(self.read(merge_sha, "unity-package/package.json"))
        unity["version"] = new
        lock = self.read(merge_sha, "Cargo.lock").decode()
        lock, count = re.subn(r'(?m)(^name = "fleab"\nversion = )"[^"]+"', lambda m: m[1] + json.dumps(new), lock)
        if count != 1:
            raise ValueError("Cannot locate Fleab's Cargo.lock version")
        metadata = {"schema_version": 1, "version": tag, "previous_version": previous, "bump": bump,
                    "pr_number": int(pr["number"]), "merge_sha": merge_sha}
        updates = {"Cargo.toml": cargo.encode(), "Cargo.lock": lock.encode(),
                   "unity-package/package.json": (json.dumps(unity, indent=2, ensure_ascii=False) + "\n").encode(),
                   METADATA: (json.dumps(metadata, indent=2) + "\n").encode(),
                   NOTES: (notes if notes is not None else release_notes(tag, previous, bump, pr, commits)).encode()}
        # Update existing pinned installation examples in the tagged snapshot.
        for path in ["README.md", "docs/publisher.md"]:
            text = self.read(merge_sha, path).decode()
            updates[path] = text.replace(f"#v{old}", f"#{tag}").encode()
        environment = dict(os.environ, GIT_AUTHOR_NAME="github-actions[bot]", GIT_COMMITTER_NAME="github-actions[bot]",
                           GIT_AUTHOR_EMAIL="41898282+github-actions[bot]@users.noreply.github.com",
                           GIT_COMMITTER_EMAIL="41898282+github-actions[bot]@users.noreply.github.com")
        with tempfile.TemporaryDirectory(prefix="fleab-release-") as temp:
            environment["GIT_INDEX_FILE"] = str(Path(temp) / "index")
            self.git("read-tree", merge_sha, env=environment)
            for path, content in updates.items():
                oid = self.git("hash-object", "-w", "--stdin", data=content).decode().strip()
                self.git("update-index", "--add", "--cacheinfo", f"100644,{oid},{path}", env=environment)
            tree = self.git("write-tree", env=environment).decode().strip()
            message = (f"chore(release): prepare {tag} for PR #{int(pr['number'])}\n\n"
                       f"Apply a {bump} bump based on the original PR commits.\n"
                       "Update both package versions and record grouped release notes.\n")
            commit = self.git("commit-tree", tree, "-p", merge_sha, data=message.encode(), env=environment).decode().strip()
            self.git("tag", "-a", tag, commit, "-m", f"Fleab {tag}", env=environment)
        return commit


def prepare(repository: Repository, github: GitHub, main="HEAD", push=True):
    tags = repository.tags()
    if not tags:
        raise ValueError("Push the initial stable version tag (for example v0.1.0) before enabling automatic releases")
    records = {tag: repository.metadata(tag) for tag in tags}
    initial = next((tag for tag in reversed(tags) if records[tag] is None), None)
    if initial is None:
        raise ValueError("No initial/manual stable release tag was found")
    if not repository.ancestor(initial, main):
        raise ValueError("Initial version tag must be an ancestor of main")
    known = {record["pr_number"] for record in records.values() if record}
    pending_prs = []
    for pr in github.merged_prs():
        sha = pr.get("merge_commit_sha", "")
        if pr["number"] in known or not re.fullmatch(r"[0-9a-f]{40,64}", sha):
            continue
        if repository.ancestor(sha, initial) or not repository.ancestor(sha, main):
            continue
        pending_prs.append(pr)
    pending_prs.sort(key=lambda pr: (pr["merged_at"], pr["number"]))
    previous = tags[-1]
    created = []
    for pr in pending_prs:
        commits = github.commits(pr["number"])
        plan = engine({"previous": previous, "pr": pr, "commits": commit_data(commits)})
        bump, tag = plan["bump"], plan["tag"]
        repository.tag_release(pr["merge_commit_sha"], tag, previous, bump, pr, commits, notes=plan["notes"])
        print(f"PR #{pr['number']}: {previous} -> {tag} ({bump})")
        created.append(tag)
        records[tag] = repository.metadata(tag)
        previous = tag
    if created and push:
        # Tags are append-only, pushed together or not at all. Never move main.
        repository.git("push", "--atomic", "origin", *[f"refs/tags/{tag}" for tag in created])
    # Reruns recover reserved tags whose build failed or release remains draft.
    # A later queued run drains every merged PR, even if GitHub replaced an
    # older pending concurrency run while several PRs merged in quick succession.
    return [{"tag": tag} for tag in sorted(records, key=version)
            if records[tag] is not None and not github.published(tag)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", default=os.environ.get("GITHUB_REPOSITORY"))
    args = parser.parse_args()
    token = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
    if not token or not args.repository:
        parser.error("GITHUB_REPOSITORY and GH_TOKEN/GITHUB_TOKEN are required")
    repo = Repository(Path.cwd())
    releases = prepare(repo, GitHub(args.repository, token))
    # Each release fans out to six native builds. Bound catch-up batches and
    # explicitly dispatch a continuation after success instead of exceeding
    # Actions matrix limits or leaving a large backlog unreleased.
    encoded = json.dumps(releases[:32], separators=(",", ":"))
    if output := os.environ.get("GITHUB_OUTPUT"):
        with open(output, "a", encoding="utf-8") as stream:
            stream.write(f"releases={encoded}\n")
            stream.write(f"more={'true' if len(releases) > 32 else 'false'}\n")
    print(f"Pending portable releases: {encoded}")


if __name__ == "__main__":
    main()
