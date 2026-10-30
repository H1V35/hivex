"""Report the residue classes a project declares and, with --trash, move the unneeded items of its
deterministic classes to the Trash.

Usage: python3 residue.py [--classes FILE] [--trash]

Run it from the project root. FILE defaults to docs/procedures/workspace-hygiene.json, the
declaration the workspace hygiene procedure describes. Each item's check runs through the shell
with RESIDUE set to the item's path: exit 0 means no live work needs it, 1 that something still
does, anything else that it is uncertain. Only `unneeded` items of `deterministic` classes that
contain no other declared item move, after their check passes again, through HIVEX_TRASH when set,
else `trash` or `gio trash`; nothing is ever deleted permanently and nothing empties the Trash. The
result is JSON on standard output.
"""

import argparse
import glob
import json
import os
import shlex
import shutil
import subprocess
import sys

REMOVALS = ("deterministic", "review")
CHECK_SECONDS = 60


def fail(message):
    print(json.dumps({"error": message}), file=sys.stderr)
    sys.exit(2)


def load(path):
    try:
        with open(path, encoding="utf-8") as file:
            classes = json.load(file)["classes"]
    except (OSError, ValueError, KeyError, TypeError) as error:
        fail(f"{path}: {error}")
    for entry in classes:
        valid = (
            isinstance(entry, dict)
            and isinstance(entry.get("name"), str)
            and isinstance(entry.get("paths"), list)
            and all(isinstance(pattern, str) for pattern in entry["paths"])
            and isinstance(entry.get("unneeded"), str)
            and entry["unneeded"].strip()
            and entry.get("removal") in REMOVALS
        )
        if not valid:
            fail(f"{path}: each class needs name, paths, a nonempty unneeded and a removal")
    return classes


def matches(patterns):
    found = set()
    for pattern in patterns:
        found.update(glob.glob(os.path.expandvars(os.path.expanduser(pattern)), recursive=True))
    # Resolve the parent folders so that one item has one path, without following the item itself.
    return sorted(
        os.path.join(os.path.realpath(os.path.dirname(path)), os.path.basename(path))
        for path in map(os.path.abspath, found)
    )


def identity(path):
    try:
        status = os.lstat(path)
    except OSError:
        return None
    return (status.st_dev, status.st_ino)


def chain(path):
    """The identities of the path and of every folder above it."""
    path = os.path.join(os.path.realpath(os.path.dirname(path)), os.path.basename(path))
    found = set()
    while True:
        found.add(identity(path))
        parent = os.path.dirname(path)
        if parent == path:
            return found - {None}
        path = parent


def within(path, folder):
    """Whether `path` is `folder` or lies inside it, by filesystem identity so that aliases such as
    another letter case on a case-insensitive volume count as the same item."""
    folder = identity(folder)
    return folder is not None and folder in chain(path)


def protected(path):
    """The filesystem root, the home folder, the project or any folder containing them."""
    keep = (os.sep, os.path.expanduser("~"), os.getcwd())
    return any(within(os.path.realpath(other), path) for other in keep)


def size(path):
    """Bytes the item occupies on disk, without following symbolic links."""

    def used(status):
        blocks = getattr(status, "st_blocks", None)
        return blocks * 512 if blocks is not None else status.st_size

    try:
        total = used(os.lstat(path))
    except OSError:
        return 0
    if os.path.isdir(path) and not os.path.islink(path):
        for parent, folders, files in os.walk(path):
            for name in folders + files:
                try:
                    total += used(os.lstat(os.path.join(parent, name)))
                except OSError:
                    pass
    return total


def status(path, check):
    if protected(path):
        return "protected"
    try:
        result = subprocess.run(
            check,
            shell=True,
            env={**os.environ, "RESIDUE": path},
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=CHECK_SECONDS,
        )
    except (OSError, subprocess.TimeoutExpired):
        return "uncertain"
    return {0: "unneeded", 1: "needed"}.get(result.returncode, "uncertain")


def trash_command():
    if "HIVEX_TRASH" in os.environ:
        return shlex.split(os.environ["HIVEX_TRASH"]) or None
    if shutil.which("trash"):
        return ["trash"]
    if shutil.which("gio"):
        return ["gio", "trash"]
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--classes", default="docs/procedures/workspace-hygiene.json")
    parser.add_argument("--trash", action="store_true", help="move unneeded deterministic items")
    options = parser.parse_args()

    classes = load(options.classes)
    found = [
        [
            {"path": path, "bytes": size(path), "status": status(path, entry["unneeded"])}
            for path in matches(entry["paths"])
        ]
        for entry in classes
    ]
    everything = [item for items in found for item in items]
    movable = []
    for entry, items in zip(classes, found):
        for item in items:
            if entry["removal"] != "deterministic" or item["status"] != "unneeded":
                continue
            # Moving a folder moves every other declared item inside it, whatever its class.
            if any(other is not item and within(other["path"], item["path"]) for other in everything):
                item["status"] = "overlaps"
            else:
                movable.append((item, entry["unneeded"]))
    report = [
        {
            "name": entry["name"],
            "removal": entry["removal"],
            "items": items,
            "bytes": sum(item["bytes"] for item in items),
            "unneededBytes": sum(i["bytes"] for i in items if i["status"] == "unneeded"),
        }
        for entry, items in zip(classes, found)
    ]

    moved, failed = [], []
    if options.trash and movable:
        command = trash_command()
        if command is None:
            fail("no recoverable trash command found: install `trash` or set HIVEX_TRASH")
        for item, check in movable:
            # Live work may have claimed the item while the rest of the sweep ran.
            item["status"] = status(item["path"], check)
            if item["status"] != "unneeded":
                continue
            result = subprocess.run([*command, item["path"]], stdin=subprocess.DEVNULL)
            gone = result.returncode == 0 and not os.path.lexists(item["path"])
            (moved if gone else failed).append(item)
            item["status"] = "trashed" if gone else "failed"

    print(
        json.dumps(
            {
                "mode": "trash" if options.trash else "report",
                "classes": report,
                "moved": {"items": len(moved), "bytes": sum(item["bytes"] for item in moved)},
                "failed": [item["path"] for item in failed],
            },
            indent=2,
        )
    )
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
