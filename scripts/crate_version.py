#!/usr/bin/env python3
"""The crate version, written by hand in Cargo.toml.

``docs/source/conf.py`` repeats it for Sphinx inside a cog block, listed in
``cogfiles.txt``.

    cog -r @cogfiles.txt        rewrites the copy
    cog --check @cogfiles.txt   fails when the copy has drifted
"""

from __future__ import annotations

import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "Cargo.toml"


def version() -> str:
    with MANIFEST.open("rb") as handle:
        found = tomllib.load(handle)["package"]["version"]
    if not isinstance(found, str):
        raise TypeError("Cargo.toml package version is not a string")
    return found


def short_version() -> str:
    """Sphinx ``|version|`` is major.minor; ``|release|`` is the full semver.

    That split is Sphinx's own: ``version`` replaces ``|version|`` and
    ``release`` replaces ``|release|``.
    https://www.sphinx-doc.org/en/master/usage/configuration.html#confval-version
    """
    return ".".join(version().split(".")[:2])


def main() -> None:
    print(version())


if __name__ == "__main__":
    main()
