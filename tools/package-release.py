"""Package a native cargo release and the dependency license notices."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import zipfile
import hashlib
import tomllib

p = argparse.ArgumentParser()
p.add_argument("--platform", required=True, choices=["windows", "linux", "macos"])
p.add_argument("--arch", required=True, choices=["x64", "arm64"])
p.add_argument("--target", default="")
p.add_argument("--output-dir", default="", help="Isolated packaging output directory (default: dist)")
p.add_argument("--stage-only", action="store_true", help="Prepare files without creating archives")
p.add_argument("--from-stage", default="", help="Repackage an existing Windows stage after signing")
args = p.parse_args()
repo = Path(__file__).resolve().parent.parent
if args.from_stage:
    if args.platform != "windows":
        raise SystemExit("Only Windows releases can be repackaged from a signing stage")
    version = tomllib.loads((repo / "Cargo.toml").read_text())["workspace"]["package"]["version"]
else:
    meta = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version", "1"], cwd=repo))
    version = next(v for v in meta["packages"] if v["name"] == "fleab")["version"]
name = f"Fleab-{version}-{args.platform}-{args.arch}"
dist = Path(args.output_dir).resolve() if args.output_dir else repo / "dist"
dist.mkdir(parents=True, exist_ok=True)
stage = dist / name
if stage.exists():
    raise SystemExit(f"Package directory already exists: {stage}; move it before repackaging")
stage.mkdir()

exe = "fleab.exe" if args.platform == "windows" else "fleab"

def prepare_stage():
    if args.from_stage:
        source_stage = Path(args.from_stage).resolve(strict=True)
        for required in ["fleab.exe", "LICENSE", "THIRD_PARTY_NOTICES.md", "licenses"]:
            if not (source_stage / required).exists():
                raise SystemExit(f"Incomplete Windows signing stage: {required}")
        shutil.copytree(source_stage, stage, dirs_exist_ok=True)
        return stage
    if args.platform == "macos":
        bundle = stage / "Fleab.app" / "Contents"
        binary_dir = bundle / "MacOS"
        resources = bundle / "Resources"
        binary_dir.mkdir(parents=True)
        resources.mkdir()
        shutil.copy2(repo / "assets/fleab.icns", resources)
        import plistlib
        with (bundle / "Info.plist").open("wb") as f:
            plistlib.dump({"CFBundleExecutable": "fleab", "CFBundleName": "Fleab", "CFBundleIdentifier": "org.fleab.deployer", "CFBundleVersion": version, "CFBundleShortVersionString": version, "CFBundlePackageType": "APPL", "CFBundleIconFile": "fleab.icns", "LSMinimumSystemVersion": "11.0", "NSHighResolutionCapable": True}, f)
    else:
        binary_dir = stage
    source = repo / "target"
    if args.target:
        source /= args.target
    shutil.copy2(source / "release" / exe, binary_dir / exe)
    if args.platform != "windows":
        (binary_dir / exe).chmod(0o755)
    for file in ["README.md", "LICENSE"]:
        shutil.copy2(repo / file, stage)
    shutil.copytree(repo / "docs", stage / "docs")
    shutil.copytree(repo / "assets", stage / "assets")
    notices = ["# Fleab dependency notices", "", "Includes transitive and target-specific dependencies. See the bundled license texts.", ""]
    for dep in sorted(meta["packages"], key=lambda v: (v["name"], v["version"])):
        if dep["name"] == "fleab":
            continue
        notices.append(f"* {dep['name']} {dep['version']}: {dep.get('license') or 'see source'}; {dep.get('repository') or dep.get('homepage') or 'https://crates.io/crates/' + dep['name']}")
        crate_root = Path(dep["manifest_path"]).parent
        for file in crate_root.rglob("*"):
            if file.is_file() and any(token in file.name.lower() for token in ["license", "licence", "copying", "notice"]) and file.stat().st_size < 1_000_000:
                dest = stage / "licenses" / f"{dep['name']}-{dep['version']}" / file.relative_to(crate_root)
                dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(file, dest)
    (stage / "THIRD_PARTY_NOTICES.md").write_text("\n".join(notices) + "\n", encoding="utf-8")
    if args.platform == "macos":
        shutil.copytree(stage / "licenses", resources / "licenses")
        shutil.copy2(stage / "THIRD_PARTY_NOTICES.md", resources)
        subprocess.run(["codesign", "--force", "--deep", "--sign", "-", str(stage / "Fleab.app")], check=True)
        subprocess.run(["codesign", "--verify", "--deep", "--strict", str(stage / "Fleab.app")], check=True)
    return binary_dir

binary_dir = prepare_stage()
if args.stage_only:
    print(stage)
    raise SystemExit(0)

if args.platform == "windows":
    archive = dist / f"{name}.zip"
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED, strict_timestamps=False) as z:
        for file in stage.rglob("*"):
            if file.is_file():
                z.write(file, file.relative_to(dist))
else:
    archive = dist / f"{name}.tar.gz"
    with tarfile.open(archive, "w:gz") as t:
        t.add(stage, arcname=name)
digest = hashlib.sha256(archive.read_bytes()).hexdigest()
archive.with_suffix(archive.suffix + ".sha256").write_text(f"{digest}  {archive.name}\n", encoding="ascii")

# Updater archives use a deliberately small, link-free USTAR layout. They carry
# their license texts as well as the binary; the updater retains this package.
update = dist / f"Fleab-{version}-update-{args.platform}-{args.arch}.tar.gz"
with tarfile.open(update, "w:gz", format=tarfile.USTAR_FORMAT) as t:
    application = stage / ("Fleab.app" if args.platform == "macos" else exe)
    t.add(application, arcname=application.name)
    for item in ["LICENSE", "THIRD_PARTY_NOTICES.md", "licenses"]:
        t.add(stage / item, arcname=item)
with tarfile.open(update, "r:gz") as t:
    members = t.getmembers()
    if any(not m.isfile() and not m.isdir() for m in members):
        raise SystemExit("Updater archives must not contain links or special entries")
    unpacked = sum(m.size for m in members)
update_metadata = {
    "filename": update.name,
    "size": update.stat().st_size,
    "sha256": hashlib.sha256(update.read_bytes()).hexdigest(),
    "executable_sha256": hashlib.sha256((binary_dir / exe).read_bytes()).hexdigest(),
    "unpacked_size": unpacked,
}
update.with_suffix(update.suffix + ".json").write_text(json.dumps(update_metadata) + "\n", encoding="utf-8")
print(archive)
