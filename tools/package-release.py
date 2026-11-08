"""Package a native cargo release and the dependency license notices."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import zipfile

p = argparse.ArgumentParser()
p.add_argument("--platform", required=True, choices=["windows", "linux", "macos"])
p.add_argument("--arch", required=True, choices=["x64", "arm64"])
p.add_argument("--target", default="")
args = p.parse_args()
repo = Path(__file__).resolve().parent.parent
meta = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version", "1"], cwd=repo))
package = next(v for v in meta["packages"] if v["name"] == "fleab")
version = package["version"]
name = f"Fleab-{version}-{args.platform}-{args.arch}"
dist = repo / "dist"
dist.mkdir(exist_ok=True)
stage = dist / name
if stage.exists():
    raise SystemExit(f"Package directory already exists: {stage}; move it before repackaging")
stage.mkdir()
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
exe = "fleab.exe" if args.platform == "windows" else "fleab"
source = repo / "target"
if args.target:
    source /= args.target
shutil.copy2(source / "release" / exe, binary_dir / exe)
if args.platform != "windows":
    (binary_dir / exe).chmod(0o755)
for file in ["README.md", "LICENSE"]:
    shutil.copy2(repo / file, stage)
shutil.copytree(repo / "docs", stage / "docs")
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
    subprocess.run(["codesign", "--force", "--deep", "--sign", "-", str(stage / "Fleab.app")], check=True)
    subprocess.run(["codesign", "--verify", "--deep", "--strict", str(stage / "Fleab.app")], check=True)
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
import hashlib
digest = hashlib.sha256(archive.read_bytes()).hexdigest()
archive.with_suffix(archive.suffix + ".sha256").write_text(f"{digest}  {archive.name}\n", encoding="ascii")
print(archive)
