"""Explicit optional LLVM 23.1.5 provisioning from the release archive; --plan performs no tool execution or I/O writes."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request

from common import (ARTIFACTS, BASE, RECEIPT, evidence, inside, native_host, pin, prefix_for,
                    require, require_pinned)
from check import inspect

GIB = 1024 ** 3
MIB = 1024 ** 2
# Installed prefix about 0.3 GiB plus the archive, measured by the release workflow runs.
MIN_FREE_GIB = 2
# Cap when a pin has no size; the trimmed archives are about 0.1 GiB.
UNSIZED_ARCHIVE_LIMIT = 512 * MIB


def download(destination, asset, size, digest, base=BASE):
    hashed, count = hashlib.sha256(), 0
    with urllib.request.urlopen(base + asset, timeout=60) as response, destination.open("xb") as out:
        while True:
            block = response.read(1024 * 1024)
            if not block:
                break
            count += len(block)
            require(count <= (size or UNSIZED_ARCHIVE_LIMIT), "archive exceeds compressed size limit")
            hashed.update(block)
            out.write(block)
    require(size is None or count == size, "archive compressed size mismatch")
    require(hashed.hexdigest() == digest, "archive SHA-256 mismatch")


def extract(archive, destination):
    """Stream into a new directory; defer links so none can redirect file writes."""
    destination.mkdir()
    links, total = [], 0
    with tarfile.open(archive, "r|xz") as source:
        for count, member in enumerate(source, 1):
            path = PurePosixPath(member.name)
            require(not path.is_absolute() and ".." not in path.parts, "archive path traversal")
            target = destination / path
            require(inside(target, destination), "archive path escapes destination")
            total += member.size
            require(count <= 500000 and 0 <= member.size and total <= 40 * GIB, "archive extraction limit exceeded")
            if member.isfile():
                require(shutil.disk_usage(destination).free >= member.size + GIB, "insufficient extraction space (1 GiB reserve)")
            if member.issym() or member.islnk():
                require(not PurePosixPath(member.linkname).is_absolute(), "absolute archive link")
                linked = (target.parent if member.issym() else destination) / member.linkname
                require(inside(linked, destination), "archive link escapes destination")
                links.append((member, target, linked))
            elif member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                require(member.isfile(), "unsupported archive entry (device/fifo/etc)")
                target.parent.mkdir(parents=True, exist_ok=True)
                with source.extractfile(member) as data, target.open("xb") as out:
                    shutil.copyfileobj(data, out)
                target.chmod(member.mode & 0o777)
    for member, target, linked in links:
        require(inside(target.parent, destination) and inside(linked, destination), "escaping archive link chain")
        target.parent.mkdir(parents=True, exist_ok=True)
        if member.issym():
            target.symlink_to(member.linkname)
        else:
            require(linked.is_file() and not linked.is_symlink(), "invalid hardlink target")
            os.link(linked, target)
    for _, target, _ in links:
        require(inside(target, destination), "escaping archive link chain")
    roots = list(destination.iterdir())
    require(len(roots) == 1 and roots[0].is_dir() and not roots[0].is_symlink(), "expected one archive root directory")
    return roots[0]


def provision(args, name, prefix):
    require_pinned(name)
    if prefix.exists():
        return inspect(prefix, name)  # Requires matching receipt AND repeats content/version checks.
    prefix.parent.mkdir(parents=True, exist_ok=True)
    # Atomic mkdir lock; no stale lock or partial tree is automatically deleted/reused.
    lock = prefix.with_name(prefix.name + ".lock")
    lock.mkdir()
    try:
        require(shutil.disk_usage(prefix.parent).free >= MIN_FREE_GIB * GIB, f"insufficient free disk ({MIN_FREE_GIB} GiB required)")
        with tempfile.TemporaryDirectory(prefix=".llvm-stage-", dir=prefix.parent) as temp:
            work = Path(temp)
            download(work / "archive.xz", *ARTIFACTS[name])
            tree = extract(work / "archive.xz", work / "unpacked")
            require(not prefix.exists(), "output appeared during provisioning; refusing replacement")
            tree.rename(prefix)
            try:
                facts = inspect(prefix, name, receipt=False)
                (prefix / RECEIPT).write_text(json.dumps(pin(name), indent=2) + "\n")
            except BaseException:
                shutil.rmtree(prefix)  # Only the newly created tree owned by this invocation.
                raise
            return facts
    finally:
        lock.rmdir()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", action="store_true")
    args = parser.parse_args()
    # Plan is pure selection: do not invoke sysctl or any installed LLVM/build tools.
    if args.plan:
        from common import host
        name = host()
        facts = evidence("plan-only", **pin(name), prefix=str(prefix_for(name)),
                         url=BASE + ARTIFACTS[name][0], c_api_smoke="not-run", provenance_signatures="unverified")
    else:
        name = native_host()
        facts = provision(args, name, prefix_for(name))
    print(json.dumps(facts, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.SubprocessError, tarfile.TarError) as error:
        raise SystemExit(f"LLVM setup failed: {error}")
