#!/usr/bin/env python3
"""Write the documented samples from the binary, and the flag list from --help.

The first-minute line and the getting-started transcripts used to be typed
by hand, and the reference kept its own flag table. Both drift: a tie is
now ``settled: false`` and the flags in ``--help`` are the ones the binary
actually accepts. This module runs those commands and returns the
transcript. Cog calls it from the markers in the docs and writes the
stdout back, so a later edit to the sample fails ``cog --check --diff``.

Usage, from the repository root, after ``pip install cogapp`` (the ``cog``
distribution). ``jq`` is required for the one sample whose command pipes
through ``jq -c``.

    python3 scripts/cog_samples.py           # rewrite the samples in place
    python3 scripts/cog_samples.py --check   # fail if a sample would change
"""

from __future__ import annotations

import os
import re
import shlex
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# The name a reader types. The process is this checkout's release binary,
# which is the one ``cargo binstall`` installs and the one the getting-started
# page tells a reader to run.
BIN_NAME = "ljos-consensus"

# Alice ships, Bob holds: the first-minute pair. Two equal votes, so the
# settle is a tie.
README_BALLOTS = '[{"agent":"alice","choice":"ship"},{"agent":"bob","choice":"hold"}]'

# Two ship, one hold: the panel the getting-started page walks through.
TUTORIAL_BALLOTS = (
    '[{"agent":"alice","choice":"ship"},{"agent":"bob","choice":"ship"},'
    '{"agent":"carol","choice":"hold"}]'
)

# Alice and Bob each name Carol at 1. Carol names herself at 4 and Alice
# at 1. Those are the rows that page already uses.
TUTORIAL_TRUST = (
    '[["alice","carol",1.0],["bob","carol",1.0],'
    '["carol","carol",4.0],["carol","alice",1.0]]'
)

# The anchor the getting-started page uses: below 1, each voter keeps
# part of the ballot they cast. One half is that page's example.
TUTORIAL_ANCHOR = "0.5"

# A shell token with nothing for a shell to expand. Anything else is
# single-quoted the way the samples have always written JSON.
_BARE = re.compile(r"[A-Za-z0-9_./:=@+-]+")

# Cog treats a line containing one of these as a marker. A sample that
# printed one would swallow the rest of the file on the next run.
_MARKERS = ("[[[cog", "[[[end]]]")


@dataclass(frozen=True)
class Sample:
    """One documented command.

    ``compact`` pipes stdout through ``jq -c``, and the transcript shows
    that pipe. The first-minute block has always been that one line.
    """

    argv: tuple[str, ...]
    compact: bool = False


SAMPLES: dict[str, Sample] = {
    "readme_first": Sample(("settle", "--ballots", README_BALLOTS), compact=True),
    "getting_started_equal": Sample(("settle", "--ballots", TUTORIAL_BALLOTS)),
    "getting_started_trust": Sample(
        ("settle", "--ballots", TUTORIAL_BALLOTS, "--trust", TUTORIAL_TRUST)
    ),
    "getting_started_anchor": Sample(
        (
            "settle",
            "--ballots",
            TUTORIAL_BALLOTS,
            "--trust",
            TUTORIAL_TRUST,
            "--susceptibility",
            TUTORIAL_ANCHOR,
        )
    ),
    "reference_settle_help": Sample(("settle", "--help")),
}

_BIN: Path | None = None


def child_env() -> dict[str, str]:
    """The environment a sample runs in.

    Clap colors a terminal and writes plain text to a pipe. The committed
    sample has to be the plain text, whichever way cog was started.
    """
    env = os.environ.copy()
    env["NO_COLOR"] = "1"
    env["CLICOLOR"] = "0"
    env["CLICOLOR_FORCE"] = "0"
    env["CARGO_TERM_COLOR"] = "never"
    return env


def binary() -> Path:
    """The release binary this checkout builds.

    ``cargo build`` does nothing when that binary is already current, and
    a stale binary would bless a sample for source that is no longer there.
    ``LJOS_CONSENSUS`` overrides the path when a caller already has the
    binary to sample.
    """
    global _BIN
    if _BIN is not None:
        return _BIN
    override = os.environ.get("LJOS_CONSENSUS")
    if override:
        path = Path(override)
        if not path.is_file():
            raise SystemExit(f"LJOS_CONSENSUS={override} is not a file")
        _BIN = path
        return path
    built = ROOT / "target" / "release" / BIN_NAME
    proc = subprocess.run(
        ["cargo", "build", "--release", "--bin", BIN_NAME],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
        env=child_env(),
    )
    if proc.returncode != 0:
        raise SystemExit(proc.stderr or proc.stdout or "cargo build --release failed")
    if not built.is_file():
        raise SystemExit(f"cargo build --release did not write {built}")
    _BIN = built
    return built


def invoke(argv: tuple[str, ...]) -> str:
    proc = subprocess.run(
        [str(binary()), *argv],
        check=False,
        capture_output=True,
        text=True,
        env=child_env(),
    )
    if proc.returncode != 0:
        shown = " ".join(argv)
        raise SystemExit(
            f"{BIN_NAME} {shown} exited {proc.returncode}\n{proc.stderr}"
        )
    return proc.stdout


def jq_compact(text: str) -> str:
    """``jq -c`` with no filter, which is what ``| jq -c`` runs."""
    jq = shutil.which("jq")
    if jq is None:
        raise SystemExit("jq is required for the compact sample; install jq")
    proc = subprocess.run(
        [jq, "-c"],
        input=text,
        check=False,
        capture_output=True,
        text=True,
        env=child_env(),
    )
    if proc.returncode != 0:
        raise SystemExit(f"jq -c failed\n{proc.stderr}")
    return proc.stdout


def shell_token(text: str) -> str:
    if _BARE.fullmatch(text):
        return text
    return shlex.quote(text)


def command_line(spec: Sample) -> str:
    parts = [BIN_NAME, *(shell_token(arg) for arg in spec.argv)]
    shown = " ".join(parts)
    if spec.compact:
        shown += " | jq -c"
    return shown


def transcript(spec: Sample) -> str:
    out = invoke(spec.argv)
    if spec.compact:
        out = jq_compact(out)
    if out and not out.endswith("\n"):
        out += "\n"
    text = f"$ {command_line(spec)}\n{out}"
    for marker in _MARKERS:
        if marker in text:
            raise SystemExit(f"sample output contains cog marker {marker!r}")
    return text


def _spec(name: str) -> Sample:
    try:
        return SAMPLES[name]
    except KeyError:
        known = ", ".join(sorted(SAMPLES))
        raise SystemExit(f"unknown sample {name!r}; known: {known}") from None


def markdown_sample(name: str) -> str:
    return f"```console\n{transcript(_spec(name))}```\n"


def org_sample(name: str) -> str:
    return f"#+begin_src console\n{transcript(_spec(name))}#+end_src\n"


def main() -> int:
    if len(sys.argv) == 2 and sys.argv[1] == "--check":
        check = True
    elif len(sys.argv) == 1:
        check = False
    else:
        raise SystemExit("usage: python3 scripts/cog_samples.py [--check]")
    # -U keeps the rewritten files on LF, which is what the check compares.
    cmd = [sys.executable, "-m", "cogapp", "-U"]
    if check:
        cmd += [
            "--check",
            "--diff",
            "--check-fail-msg=python3 scripts/cog_samples.py",
        ]
    else:
        cmd.append("-r")
    cmd.append("@cogfiles.txt")
    try:
        return subprocess.call(cmd, cwd=ROOT)
    except FileNotFoundError:
        raise SystemExit(
            "cogapp is not installed; pip install cogapp"
        ) from None


if __name__ == "__main__":
    raise SystemExit(main())
