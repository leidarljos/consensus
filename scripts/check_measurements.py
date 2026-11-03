#!/usr/bin/env python3
"""Rerun the command recorded on each measurement file and diff the body.

The first line is the command, after ``#``, with an optional git SHA-1
before it (40 hexadecimal digits, the commit the capture was taken at):

    # [<40-hex sha> ]cargo run --release --example <name> -- <args>

Further lines that start with ``#`` are notes. One blank line may follow.
The rest of the file is the product of that command: the bytes it writes
for ``--write`` or ``--write-independent``, otherwise its stdout.

``examples/replay_history.rs`` skips the same header when it reads a panel.
This fails when that product changes.

    python3 scripts/check_measurements.py [file ...]
"""

from __future__ import annotations

import difflib
import shlex
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# A git SHA-1 is 40 hexadecimal digits.
SHA1_HEX_LEN = 40

# The files this check owns. A measurement another change adds is left alone.
OWNED = (
    "measurements/replay_history.txt",
    "measurements/replay_independent.txt",
    "measurements/replay_panel.json",
    "measurements/replay_panel_independent.json",
    "measurements/correlation_history-0-7-4000.txt",
    "measurements/correlation_gate-20000.txt",
    "measurements/settle_budget.txt",
)

WRITE_FLAGS = ("--write-independent", "--write")


class CheckError(Exception):
    pass


def is_sha1(word: str) -> bool:
    return len(word) == SHA1_HEX_LEN and all(
        c in "0123456789abcdefABCDEF" for c in word
    )


def recorded_command(line: str) -> str | None:
    text = line.strip()
    if not text.startswith("#"):
        return None
    text = text[1:].lstrip()
    word, sep, tail = text.partition(" ")
    if sep and is_sha1(word):
        text = tail.lstrip()
    if text.startswith("cargo run --release --example "):
        return text
    return None


def measurement_body(raw: bytes) -> bytes:
    """The product below the header. Same rules as ``measurement_body`` in the example."""
    nl = raw.find(b"\n")
    if nl < 0:
        return raw
    first = raw[:nl].decode("utf-8")
    if recorded_command(first) is None:
        return raw
    rest = raw[nl + 1 :]
    while rest.startswith(b"#"):
        nl = rest.find(b"\n")
        if nl < 0:
            return b""
        rest = rest[nl + 1 :]
    if rest.startswith(b"\n"):
        rest = rest[1:]
    return rest


def product_of(cmd: str, rel: str) -> bytes:
    args = shlex.split(cmd)
    if args[:4] != ["cargo", "run", "--release", "--example"] or len(args) < 5:
        raise CheckError(f"{rel}: the recorded command is not an example run")
    write_at = None
    for flag in WRITE_FLAGS:
        if flag in args:
            i = args.index(flag)
            if i + 1 >= len(args):
                raise CheckError(f"{rel}: {flag} needs a path")
            if Path(args[i + 1]).as_posix() != rel:
                raise CheckError(f"{rel}: {flag} records {args[i + 1]}, not this file")
            write_at = i + 1
            break
    if write_at is None:
        proc = subprocess.run(args, cwd=ROOT, stdout=subprocess.PIPE, check=False)
        if proc.returncode != 0:
            raise CheckError(f"{rel}: command exited {proc.returncode}")
        return proc.stdout
    with tempfile.TemporaryDirectory() as tmp:
        dest = str(Path(tmp) / Path(rel).name)
        args[write_at] = dest
        proc = subprocess.run(args, cwd=ROOT, check=False)
        if proc.returncode != 0:
            raise CheckError(f"{rel}: command exited {proc.returncode}")
        return Path(dest).read_bytes()


def show_diff(rel: str, body: bytes, product: bytes) -> None:
    committed = body.decode("utf-8", "replace").splitlines(keepends=True)
    regenerated = product.decode("utf-8", "replace").splitlines(keepends=True)
    sys.stdout.writelines(
        difflib.unified_diff(
            committed,
            regenerated,
            fromfile=f"{rel} (below the command)",
            tofile="regenerated",
        )
    )


def check(rel: str) -> bool:
    path = ROOT / rel
    if not path.is_file():
        print(f"{rel}: no such measurement file", file=sys.stderr)
        return False
    raw = path.read_bytes()
    nl = raw.find(b"\n")
    if nl < 0:
        print(f"{rel}: no command on the first line", file=sys.stderr)
        return False
    cmd = recorded_command(raw[:nl].decode("utf-8"))
    if cmd is None:
        print(f"{rel}: the first line does not record an example command", file=sys.stderr)
        return False
    print(f"replay {rel}", flush=True)
    try:
        product = product_of(cmd, rel)
    except CheckError as err:
        print(err, file=sys.stderr)
        return False
    body = measurement_body(raw)
    if body == product:
        return True
    print(f"{rel}: the command's output changed", file=sys.stderr)
    show_diff(rel, body, product)
    return False


def main(argv: list[str]) -> int:
    rels = argv or list(OWNED)
    ok = True
    for rel in rels:
        if not check(rel):
            ok = False
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
