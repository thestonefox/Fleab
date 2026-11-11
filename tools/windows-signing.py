"""Prepare/recover one SignPath request and unpack only its two executables."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import tempfile
from urllib.parse import urlsplit
import uuid
import zipfile

FILES = ("windows-x64/fleab.exe", "windows-arm64/fleab.exe")
MAX_FILE = 64 * 1024 * 1024


def version(value):
    if not re.fullmatch(r"v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", value):
        raise ValueError("Expected a stable vX.Y.Z release tag")
    return value


def hashes(directory):
    result = {}
    for name in FILES:
        file = Path(directory) / name
        if file.is_symlink() or not file.is_file() or not 0 < file.stat().st_size <= MAX_FILE:
            raise ValueError(f"Invalid signing input: {name}")
        result[name] = hashlib.sha256(file.read_bytes()).hexdigest()
    return result


def approval_url(value, request_id):
    parsed = urlsplit(value)
    if (parsed.scheme != "https" or parsed.hostname != "app.signpath.io"
            or parsed.port not in (None, 443) or parsed.username or parsed.password
            or parsed.fragment or request_id.lower() not in value.lower()
            or any(ord(c) < 33 or c in '<>"\\`' for c in value)):
        raise ValueError("Invalid SignPath approval link")
    return value


def record(tag, organization, request_id, url, directory):
    tag = version(tag)
    organization = str(uuid.UUID(organization))
    request_id = str(uuid.UUID(request_id))
    return dict(schema_version=1, tag=tag, organization=organization,
                request_id=request_id, url=approval_url(url, request_id),
                files=hashes(directory))


def resume(path, tag, organization, directory):
    path = Path(path)
    if path.is_symlink() or path.stat().st_size > 65_536:
        raise ValueError("Invalid saved signing request")
    value = json.loads(path.read_text(encoding="utf-8"))
    expected = record(tag, organization, value["request_id"], value["url"], directory)
    if value != expected:
        raise ValueError("Saved signing request does not match this release and its binaries")
    return expected


def summary(value, output, summary_path):
    with Path(output).open("a", encoding="utf-8") as stream:
        stream.write(f"request-id={value['request_id']}\napproval-url={value['url']}\n")
    with Path(summary_path).open("a", encoding="utf-8") as stream:
        stream.write(f"## Approve Windows signing for Fleab {value['tag']}\n\n"
                     f"[**Open SignPath and approve this release**](<{value['url']}>)\n\n"
                     "1. Sign in to SignPath.\n"
                     "2. Check the Fleab version and GitHub build, then click **Approve**.\n"
                     "3. GitHub verifies both signed Windows binaries and publishes the release.\n\n"
                     "This one request covers **Windows x64 and ARM64**. "
                     "No further commit or tag is needed.\n\n"
                     "The run waits up to five hours. If it times out, approve the same "
                     "request, then choose **Re-run failed jobs** in GitHub. "
                     "The saved request is reused. A rejected request stops publication.\n")


def extract(archive, output):
    archive, output = Path(archive), Path(output)
    if output.exists() or output.is_symlink():
        raise ValueError("Signed output already exists")
    if archive.stat().st_size > MAX_FILE * 2:
        raise ValueError("Oversized signed archive")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="fleab-signed-", dir=output.parent) as temp:
        with zipfile.ZipFile(archive) as bundle:
            entries = bundle.infolist()
            seen = set()
            if len(entries) > 4:
                raise ValueError("Unexpected signed archive entries")
            for entry in entries:
                name = entry.filename
                mode = stat.S_IFMT(entry.external_attr >> 16)
                if name in seen or entry.flag_bits & 1:
                    raise ValueError("Duplicate or encrypted signed entry")
                seen.add(name)
                if name in ("windows-x64/", "windows-arm64/"):
                    if not entry.is_dir() or entry.file_size != 0 or mode not in (0, stat.S_IFDIR):
                        raise ValueError("Invalid signed directory entry")
                    continue
                if (name not in FILES or not 0 < entry.file_size <= MAX_FILE
                        or mode not in (0, stat.S_IFREG)):
                    raise ValueError("Unexpected signed file or size")
                path = Path(temp) / name
                path.parent.mkdir(exist_ok=True)
                with bundle.open(entry) as source, path.open("xb") as destination:
                    copied = 0
                    while chunk := source.read(65_536):
                        copied += len(chunk)
                        if copied > entry.file_size:
                            raise ValueError("Signed entry exceeds its size")
                        destination.write(chunk)
                if copied != entry.file_size:
                    raise ValueError("Truncated signed entry")
            if not set(FILES).issubset(seen):
                raise ValueError("Missing signed Windows target")
        Path(temp).rename(output)
        # TemporaryDirectory cleanup tolerates the moved, now-missing path.


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prepare = commands.add_parser("prepare")
    prepare.add_argument("--stages", required=True)
    prepare.add_argument("--output", required=True)
    for name in ("record", "resume"):
        command = commands.add_parser(name)
        command.add_argument("--tag", required=True)
        command.add_argument("--organization", required=True)
        command.add_argument("--input", required=True)
        command.add_argument("--request", required=True)
        if name == "record":
            command.add_argument("--request-id", required=True)
            command.add_argument("--url", required=True)
    unpack = commands.add_parser("extract")
    unpack.add_argument("--archive", required=True)
    unpack.add_argument("--output", required=True)
    args = parser.parse_args()
    if args.command == "prepare":
        output = Path(args.output)
        output.mkdir(parents=True, exist_ok=False)
        for name in FILES:
            destination = output / name
            destination.parent.mkdir()
            shutil.copyfile(Path(args.stages) / name, destination)
        hashes(output)
    elif args.command == "extract":
        extract(args.archive, args.output)
    else:
        if args.command == "record":
            value = record(args.tag, args.organization, args.request_id, args.url, args.input)
            with Path(args.request).open("x", encoding="utf-8") as stream:
                json.dump(value, stream)
        else:
            value = resume(args.request, args.tag, args.organization, args.input)
        summary(value, os.environ["GITHUB_OUTPUT"], os.environ["GITHUB_STEP_SUMMARY"])


if __name__ == "__main__":
    main()
