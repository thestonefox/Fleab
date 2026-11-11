"""No signing credentials or external writes: test release recovery and packages."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import unittest
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("windows_signing", ROOT / "tools/windows-signing.py")
signing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(signing)


class Signing(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="fleab-signing-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.input = self.root / "input"
        for name in signing.FILES:
            file = self.input / name
            file.parent.mkdir(parents=True)
            file.write_bytes(b"application bytes: " + name.encode())
        self.organization, self.request_id = str(uuid.uuid4()), str(uuid.uuid4())
        self.url = f"https://app.signpath.io/Web/{self.organization}/SigningRequests/{self.request_id}"

    def record(self):
        return signing.record("v0.4.0", self.organization, self.request_id, self.url, self.input)

    def archive(self, entries):
        archive = self.root / "response.zip"
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as bundle:
            for name, data, mode in entries:
                entry = zipfile.ZipInfo(name)
                entry.external_attr = mode << 16
                bundle.writestr(entry, data)
        return archive

    def test_timeout_reuses_original_request_for_the_exact_same_binaries(self):
        value = self.record()
        record = self.root / "request.json"
        record.write_text(json.dumps(value))
        resumed = signing.resume(record, "v0.4.0", self.organization, self.input)
        self.assertEqual(resumed, value)
        output, summary = self.root / "output", self.root / "summary"
        signing.summary(resumed, output, summary)
        self.assertIn(f"request-id={self.request_id}", output.read_text())
        self.assertIn(self.url, summary.read_text())
        self.assertIn("Windows x64 and ARM64", summary.read_text())
        self.assertIn("Re-run failed jobs", summary.read_text())

    def test_workflow_commands_prepare_record_and_resume_one_request(self):
        script = str(ROOT / "tools/windows-signing.py")
        payload = self.root / "payload"
        subprocess.run([sys.executable, script, "prepare", "--stages", str(self.input),
                        "--output", str(payload)], check=True, capture_output=True)
        self.assertEqual(signing.hashes(payload), signing.hashes(self.input))
        request, output, summary = self.root / "saved.json", self.root / "outputs", self.root / "summary"
        environment = dict(os.environ, GITHUB_OUTPUT=str(output), GITHUB_STEP_SUMMARY=str(summary))
        common = ["--tag", "v0.4.0", "--organization", self.organization,
                  "--input", str(payload), "--request", str(request)]
        subprocess.run([sys.executable, script, "record", *common,
                        "--request-id", self.request_id, "--url", self.url],
                       env=environment, check=True, capture_output=True)
        saved = request.read_bytes()
        subprocess.run([sys.executable, script, "resume", *common],
                       env=environment, check=True, capture_output=True)
        self.assertEqual(request.read_bytes(), saved)
        self.assertEqual(output.read_text().count(f"request-id={self.request_id}"), 2)

    def test_saved_request_cannot_be_reused_for_changed_code_tag_or_account(self):
        record = self.root / "request.json"
        record.write_text(json.dumps(self.record()))
        for tag, organization in [("v0.4.1", self.organization), ("v0.4.0", str(uuid.uuid4()))]:
            with self.assertRaises(ValueError):
                signing.resume(record, tag, organization, self.input)
        (self.input / signing.FILES[0]).write_bytes(b"different application")
        with self.assertRaises(ValueError):
            signing.resume(record, "v0.4.0", self.organization, self.input)

    def test_approval_links_cannot_redirect_or_inject_workflow_outputs(self):
        for url in ["https://evil.example/" + self.request_id,
                    "http://app.signpath.io/" + self.request_id,
                    "https://user@app.signpath.io/" + self.request_id,
                    self.url + "\nrequest-id=bad", self.url + '#fragment']:
            with self.subTest(url=url), self.assertRaises(ValueError):
                signing.record("v0.4.0", self.organization, self.request_id, url, self.input)
        for tag in ["v01.2.3", "v1.2.3-beta.1", "v1.2.3\nmalicious"]:
            with self.assertRaises(ValueError):
                signing.record(tag, self.organization, self.request_id, self.url, self.input)

    def test_signed_response_contains_exactly_the_two_expected_executables(self):
        entries = [(name, b"returned application " + name.encode(), stat.S_IFREG | 0o644)
                   for name in signing.FILES]
        archive = self.archive(entries)
        output = self.root / "signed"
        signing.extract(archive, output)
        for name, data, _ in entries:
            self.assertEqual((output / name).read_bytes(), data)
        with self.assertRaises(ValueError):
            signing.extract(archive, output)

    def test_unsafe_duplicate_link_and_missing_target_responses_leave_no_output(self):
        normal = [(name, b"returned application", stat.S_IFREG | 0o644) for name in signing.FILES]
        cases = [normal[:1], normal + [("../escape", b"bad", stat.S_IFREG)],
                 normal + [("unexpected.exe", b"bad", stat.S_IFREG)],
                 [(signing.FILES[0], b"target", stat.S_IFLNK)] + normal[1:],
                 [(signing.FILES[0], b"", stat.S_IFREG)] + normal[1:]]
        for entries in cases:
            with self.subTest(entries=entries):
                output = self.root / "signed"
                with self.assertRaises(ValueError):
                    signing.extract(self.archive(entries), output)
                self.assertFalse(output.exists())
                self.assertFalse((self.root / "escape").exists())
        with self.assertWarns(UserWarning):
            duplicate = self.archive(normal + [normal[0]])
        with self.assertRaises(ValueError):
            signing.extract(duplicate, self.root / "signed")

    def test_corrupt_zip_crc_is_rejected_before_output_is_installed(self):
        archive = self.archive([(name, b"a distinctive application payload", stat.S_IFREG)
                                for name in signing.FILES])
        data = bytearray(archive.read_bytes())
        index = data.find(b"a distinctive application payload")
        self.assertGreater(index, 0)
        data[index] ^= 1
        archive.write_bytes(data)
        with self.assertRaises(zipfile.BadZipFile):
            signing.extract(archive, self.root / "signed")
        self.assertFalse((self.root / "signed").exists())

    def test_repackaging_preserves_returned_binary_in_both_download_formats(self):
        # Isolated workspace: no toolchain, real signing key or repository edits.
        repo = self.root / "repo"
        (repo / "tools").mkdir(parents=True)
        shutil.copy2(ROOT / "tools/package-release.py", repo / "tools/package-release.py")
        (repo / "Cargo.toml").write_text('[workspace.package]\nversion = "0.4.0"\n')
        stage = self.root / "stage"
        (stage / "licenses").mkdir(parents=True)
        binary = b"original application with returned signature bytes intact"
        (stage / "fleab.exe").write_bytes(binary)
        (stage / "LICENSE").write_text("license text")
        (stage / "THIRD_PARTY_NOTICES.md").write_text("notices")
        (stage / "licenses/dependency.txt").write_text("dependency license")
        dist = self.root / "dist"
        subprocess.run([sys.executable, str(repo / "tools/package-release.py"),
                        "--platform", "windows", "--arch", "x64", "--from-stage", str(stage),
                        "--output-dir", str(dist)], check=True, capture_output=True)
        with zipfile.ZipFile(dist / "Fleab-0.4.0-windows-x64.zip") as portable:
            self.assertEqual(portable.read("Fleab-0.4.0-windows-x64/fleab.exe"), binary)
        update = dist / "Fleab-0.4.0-update-windows-x64.tar.gz"
        with tarfile.open(update) as bundle:
            self.assertEqual(bundle.extractfile("fleab.exe").read(), binary)
            self.assertEqual(bundle.extractfile("licenses/dependency.txt").read(), b"dependency license")
        metadata = json.loads(update.with_suffix(".gz.json").read_text())
        self.assertEqual(metadata["executable_sha256"], signing.hashlib.sha256(binary).hexdigest())
        self.assertEqual(metadata["sha256"], signing.hashlib.sha256(update.read_bytes()).hexdigest())


if __name__ == "__main__":
    unittest.main()
