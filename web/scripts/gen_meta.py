#!/usr/bin/env python3
"""Generate the per-directory _meta.ts files from mkdocs.yml's nav.

The existing nav order and page titles are considered-over work -- "Running,
and not running", "When a tool names a sample after a file" -- and are better
than anything a migration would invent. So they are read out of mkdocs.yml
rather than retyped, for as long as both sites exist.

A migration aid, like port_docs.py. Once docs/ goes, the _meta files are the
source of truth and this script is finished.
"""

from __future__ import annotations

import re
from collections import OrderedDict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CONTENT = Path(__file__).resolve().parents[1] / "content"
MKDOCS = ROOT / "mkdocs.yml"

# Sections this script does not own: `index` is the handwritten landing page,
# and these are new in the Nextra site.
HANDWRITTEN_TOP = (
    "index",
    "silent-failures",
    "case-studies",
    "notebooks",
    "api",
    "licence",
)

# Pages that left their mkdocs section on the new site. The notebooks are a
# section of their own, rendered from real runs; installation was rewritten
# for someone installing the package; and the rest are written about the
# project rather than for a user of it, so they live under project/.
MOVED = {
    "guide/notebooks.md",
    "guide/installation.md",
    "guide/releasing.md",
    "concepts/project-layout.md",
    "concepts/validation.md",
}


def parse_nav() -> OrderedDict[str, list[tuple[str, str]]]:
    """Return {section: [(title, docs-relative path), ...]} in nav order.

    A deliberately small reader for the one nav shape this file uses, rather
    than a YAML dependency for a script that runs twice.
    """
    lines = MKDOCS.read_text(encoding="utf-8").split("\n")
    start = next(i for i, line in enumerate(lines) if line.rstrip() == "nav:")

    sections: OrderedDict[str, list[tuple[str, str]]] = OrderedDict()
    current = "<root>"
    sections[current] = []

    for line in lines[start + 1 :]:
        if line.strip() and not line.startswith(" "):
            break
        if not line.strip():
            continue
        entry = re.match(r"^(\s+)- (.+?):\s*(.*)$", line)
        if not entry:
            continue
        indent, label, target = (
            len(entry.group(1)),
            entry.group(2),
            entry.group(3).strip(),
        )
        if not target:
            current = label
            sections[current] = []
            continue
        sections["<root>" if indent <= 4 else current].append((label, target))
    return sections


def ts_key(stem: str) -> str:
    """A _meta key: quoted only where it is not a bare identifier."""
    return stem if re.fullmatch(r"[A-Za-z_$][\w$]*", stem) else f"'{stem}'"


def write_meta(directory: Path, entries: list[tuple[str, str]], why: str) -> None:
    """Write one directory's _meta.ts."""
    directory.mkdir(parents=True, exist_ok=True)
    body = [
        "import type { MetaRecord } from 'nextra'",
        "",
        f"// {why}",
        "const meta: MetaRecord = {",
    ]
    for title, path in entries:
        if path in MOVED:
            continue
        body.append(f"  {ts_key(Path(path).stem)}: {title!r},".replace("'", "'", 1))
    body += ["}", "", "export default meta", ""]
    text = "\n".join(body).replace('"', "'")
    (directory / "_meta.ts").write_text(text, encoding="utf-8")


def main() -> int:
    """Generate every section's _meta.ts from the mkdocs nav."""
    sections = parse_nav()
    # Architecture is the last section still ported wholesale from docs/, so
    # it is the last whose order is worth taking from mkdocs.yml. Guides and
    # Concepts are authored as MDX now and keep hand-written _meta files;
    # Reference, Start here and Project are groupings mkdocs never had.
    label_to_dir = {
        "Architecture": "project/architecture",
    }

    for label, folder in label_to_dir.items():
        entries = sections.get(label, [])
        if not entries:
            print(f"warning: no nav entries for {label}")
            continue
        write_meta(
            CONTENT / folder,
            entries,
            f"Order and titles taken from the {label} section of mkdocs.yml.",
        )
        print(f"{folder}/_meta.ts  {len(entries)} pages")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
