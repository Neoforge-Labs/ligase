#!/usr/bin/env python3
"""Check that the paper's citations and its bibliography agree exactly.

Every `[@key]` in paper.md must have an entry in paper.bib, and every
entry in paper.bib must be cited at least once. Run from the repo root
or from `paper/`; exits non-zero on any mismatch.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent


def cited(md: str) -> set[str]:
    """Every key inside a `[@a; @b]` citation group."""
    keys: set[str] = set()
    for group in re.findall(r"\[([^\]]*@[^\]]*)\]", md):
        keys.update(re.findall(r"@([A-Za-z][\w:.#$%&+?<>~/-]*)", group))
    return keys


def defined(bib: str) -> list[str]:
    """Every entry key in the bibliography, in file order."""
    return re.findall(r"^@\w+\{\s*([^,\s]+)\s*,", bib, flags=re.M)


def main() -> int:
    """Report every mismatch, and exit non-zero if there was one."""
    md = (HERE / "paper.md").read_text(encoding="utf-8")
    bib = (HERE / "paper.bib").read_text(encoding="utf-8")

    keys = defined(bib)
    duplicates = sorted({k for k in keys if keys.count(k) > 1})
    entries = set(keys)
    used = cited(md)

    unresolved = sorted(used - entries)
    uncited = sorted(entries - used)

    for key in unresolved:
        print(f"unresolved: [@{key}] is cited but has no entry in paper.bib")
    for key in uncited:
        print(f"uncited:    {key} is in paper.bib but cited nowhere")
    for key in duplicates:
        print(f"duplicate:  {key} appears more than once in paper.bib")

    if unresolved or uncited or duplicates:
        return 1

    print(f"{len(entries)} entries, {len(used)} cited, all resolved, none unused")
    return 0


if __name__ == "__main__":
    sys.exit(main())
