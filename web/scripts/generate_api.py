#!/usr/bin/env python3
"""Generate the API reference as MDX, from the package's own docstrings.

mkdocstrings did this by importing the package and rendering it. There is no
equivalent in the JavaScript ecosystem, so this does the same job: import
`ligase`, walk everything it exports, and write MDX.

Two things it does that mkdocstrings did not.

It is grouped by what a reader is trying to do rather than by module, and the
grouping is declared here rather than inferred -- the same grouping the old
api.md used, kept deliberately.

And it treats the grouping as a contract with `__all__`, in both directions. A
name exported and not grouped fails the build, because that is a public API
nobody documented; a name grouped and not exported fails too, because that is a
page describing something that no longer exists. mkdocstrings could not catch
the first case: its member lists were hand-written, so an undocumented export
simply did not appear. That is how `ligase.Shell` and the whole of
`ligase.hivdb` went unlisted while being public.

Nothing is emitted for a name without a docstring. The build fails instead,
because an empty heading looks like documentation and is not.
"""

from __future__ import annotations

import inspect
import re
import shutil
import sys
import textwrap
from dataclasses import dataclass
from pathlib import Path
from typing import Any

OUT = Path(__file__).resolve().parents[1] / "content/reference/api"

# --------------------------------------------------------------------------
# The grouping. This is the contract; see the module docstring.
# --------------------------------------------------------------------------


@dataclass(frozen=True)
class Group:
    """One section of the reference, and the names it covers."""

    slug: str
    title: str
    blurb: str
    names: tuple[str, ...]


GROUPS: tuple[Group, ...] = (
    Group(
        slug="loading",
        title="Loading data",
        blurb=(
            "A loader reads what it can from the file and takes the rest as "
            "declarations. What it inferred, and from where, is recorded on the "
            'result: `dataset.dimension("reference").source` says `declared` or '
            "names the header it came from."
        ),
        names=(
            "reference",
            "reads",
            "samples",
            "alignment",
            "variants",
            "intervals",
            "entity_set",
            "ranked_list",
            "knowledge_graph",
            "molecule_set",
            "molecule_ranking",
            "remote",
            "query",
        ),
    ),
    Group(
        slug="steps",
        title="Declaring and running steps",
        blurb=(
            "A step is an ordinary annotated Python function. The decorator records "
            "its signature; the graph is built from those signatures without calling "
            "any of them."
        ),
        names=(
            "step",
            "sh",
            "write",
            "handle",
            "map_identifiers",
            "map_ranked_identifiers",
            "Pipeline",
            "Step",
            "Shell",
        ),
    ),
    Group(
        slug="library",
        title="Steps you do not have to write",
        blurb=(
            "`ligase.steps` is curated rather than convenient: a step is in it only "
            "once it declares its tool's version capture, its citation, the "
            "dimensions of its output and how that output canonicalises "
            "([D42](/project/decisions/)). Four of them, and the clause has already "
            "kept two of the obvious candidates out -- see "
            "[what a curated step is](/concepts/step-library/)."
        ),
        names=(
            "steps.minimap2.align",
            "steps.bwa.mem",
            "steps.samtools.sort",
            "steps.bcftools.mpileup_call",
            "steps.LIBRARY",
            "steps.LibraryStep",
        ),
    ),
    Group(
        slug="checking",
        title="Checking without running",
        blurb=(
            "Both of these read headers only. Nothing is computed, and no step body "
            "is called."
        ),
        names=("check_step", "dimension_errors"),
    ),
    Group(
        slug="types",
        title="Data types",
        blurb=(
            "Every type is a `Dataset` carrying the dimensions inferred from its "
            "file. The subtypes differ in which dimensions apply to them."
        ),
        names=(
            "Dataset",
            "Reference",
            "Reads",
            "Samples",
            "Alignment",
            "Variants",
            "Intervals",
            "EntitySet",
            "RankedList",
            "KnowledgeGraph",
            "MoleculeSet",
            "MoleculeRanking",
            "Contig",
            "DimensionValue",
            "StepOutput",
            "Written",
        ),
    ),
    Group(
        slug="chemistry",
        title="Standardising molecules",
        blurb=(
            "The one part of Ligase that needs a chemistry toolkit, and the only "
            "module that imports one. Install it with "
            "`pip install 'ligase-bio[chem]'`.\n\n"
            "Everything else about molecules -- loading a table, reading its "
            "`representation`, hashing it, refusing to join two that disagree -- "
            "is the engine's work and needs no toolkit "
            "([D49](/project/decisions/)). What needs RDKit is *producing* a set "
            "whose standardisation is known, which is this."
        ),
        names=(
            "chem.standardize",
            "chem.standardize_ranking",
            "chem.toolkit_version",
        ),
    ),
    Group(
        slug="resistance",
        title="Resistance interpretation",
        blurb=(
            "Turning variant calls into HIV-1 drug-resistance calls. The scoring "
            "engine reproduces 3,348 published Stanford expectations exactly; see "
            "[validation](/project/validation/).\n\n"
            "Positions must be in **whole-genome HXB2 K03455 coordinates**. A "
            "reference numbered any other way -- a *pol*-relative one is the "
            "case that matters -- is refused before any compute, as a "
            "`ReferenceFrameError`; declare the frame it is numbered along "
            "with `lg.reference(..., frame=(...))`. This was silent until "
            "[the SAWA evaluation](/project/in-use/sawa/) found it."
        ),
        names=(
            "ResistanceReport",
            "hivdb.interpret",
            "hivdb.fingerprint",
            "hivdb.ALGORITHMS",
            "hivdb.DEFAULT",
            "hivdb.MIXTURE",
            "hivdb.DEPTH",
            "hivdb.GENE_COVERAGE",
        ),
    ),
    Group(
        slug="provenance",
        title="Provenance and auditing",
        blurb=(
            "What a run recorded about itself: the versions every tool actually ran "
            "at, every parameter as it was hashed, and the references by accession "
            "and checksum \u2014 and, the other way round, what a number you "
            "published rests on."
        ),
        names=(
            "Methods",
            "StepMethods",
            "MethodsGap",
            "Claim",
            "ClaimCheck",
            "BoundTo",
            "Bundle",
            "Verification",
            "PinnedReference",
            "AuditReport",
            "StepAudit",
            "Report",
            "RunReport",
            "StepRun",
        ),
    ),
    Group(
        slug="errors",
        title="Errors",
        blurb=(
            "Every failure mode has its own type, so a caller can tell a reference "
            "mismatch from a naming mismatch without reading the message. That is "
            "what makes the planned automatic fixes possible."
        ),
        names=(
            "CheckError",
            "DimensionError",
            "ContigNamingError",
            "ContigOrderError",
            "ReferenceFrameError",
            "UndeclaredDimensionError",
            "StepTypeError",
            "StepDefinitionError",
            "LoadError",
            "RemoteError",
            "RunError",
            "StepFailure",
            "InputCheck",
            "ResultCheck",
        ),
    ),
    Group(
        slug="version",
        title="Version",
        blurb="",
        names=("version", "__version__"),
    ),
)

# `chem`, `hivdb` and `steps` are submodules rather than names: their own
# members are grouped individually above, so the module objects themselves
# are not documented as entries.
MODULE_EXPORTS = ("chem", "hivdb", "steps")


# --------------------------------------------------------------------------
# Docstring parsing
# --------------------------------------------------------------------------

SECTION = re.compile(
    r"^(Args|Arguments|Parameters|Returns|Yields|Raises|Attributes|Example|Examples"
    r"|Note|Notes|Warning|Warnings|See Also):\s*$"
)


@dataclass
class Doc:
    """A docstring, split into its summary, prose and named sections."""

    summary: str
    body: list[str]
    sections: dict[str, list[str]]


def parse_doc(text: str) -> Doc:
    """Split a google-style docstring into its summary, prose and sections."""
    lines = textwrap.dedent(text).strip().split("\n")

    summary_lines: list[str] = []
    i = 0
    while i < len(lines) and lines[i].strip():
        summary_lines.append(lines[i].strip())
        i += 1
    summary = " ".join(summary_lines)

    body: list[str] = []
    sections: dict[str, list[str]] = {}
    current: list[str] | None = None

    for line in lines[i:]:
        heading = SECTION.match(line.strip())
        if heading:
            current = sections.setdefault(heading.group(1), [])
            continue
        (current if current is not None else body).append(line)

    return Doc(summary=summary, body=body, sections=sections)


def dedent_block(lines: list[str]) -> str:
    """A docstring section, dedented."""
    return textwrap.dedent("\n".join(lines)).strip("\n")


# --------------------------------------------------------------------------
# MDX escaping
# --------------------------------------------------------------------------


def split_code_spans(line: str) -> list[tuple[str, bool]]:
    """Split a line into (text, is_code) runs on backtick boundaries."""
    parts: list[tuple[str, bool]] = []
    for i, chunk in enumerate(re.split(r"(`+[^`]*`+)", line)):
        if chunk:
            parts.append((chunk, i % 2 == 1))
    return parts


def mdx_safe(text: str) -> str:
    """Escape what MDX would read as JSX, leaving code spans alone.

    Docstrings are written for a Markdown renderer, so they contain `{}` in
    format strings and `<` in type unions. MDX would take both for syntax.
    Inside a code span they are already literal.
    """
    out: list[str] = []
    in_fence = False
    for line in text.split("\n"):
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            out.append(line)
            continue
        if in_fence or line.startswith("    ") or line.lstrip().startswith(">>>"):
            out.append(line)
            continue
        rebuilt = []
        for chunk, is_code in split_code_spans(line):
            if is_code:
                rebuilt.append(chunk)
            else:
                rebuilt.append(
                    chunk.replace("{", "&#123;")
                    .replace("}", "&#125;")
                    .replace("<", "&lt;")
                )
        out.append("".join(rebuilt))
    return "\n".join(out)


# --------------------------------------------------------------------------
# Rendering
# --------------------------------------------------------------------------


def signature_of(obj: Any, name: str) -> str | None:
    """An object's signature, where it has one."""
    try:
        sig = inspect.signature(obj)
    except (TypeError, ValueError):
        return None
    if inspect.isclass(obj):
        return f"class {name}{sig}"
    return f"{name}{sig}"


def own_members(klass: type) -> list[tuple[str, Any]]:
    """Public members declared by ligase itself, not inherited from builtins.

    Without this the error classes would each document `add_note`, `args` and
    `with_traceback` off `BaseException`.
    """
    found: list[tuple[str, Any]] = []
    for member_name, member in inspect.getmembers(klass):
        if member_name.startswith("_"):
            continue
        owner = next(
            (k for k in klass.__mro__ if member_name in vars(k)),
            None,
        )
        if owner is None:
            continue
        if not (getattr(owner, "__module__", "") or "").startswith("ligase"):
            continue
        found.append((member_name, member))
    return found


def attribute_names(doc: Doc) -> set[str]:
    """The field names a class docstring's `Attributes:` section documents."""
    section = doc.sections.get("Attributes")
    if not section:
        return set()
    return {name.split(" (")[0].strip("*") for name, _ in parse_named_section(section)}


def init_parameters(klass: type) -> set[str]:
    """Names a class's `__init__` establishes, by parameter or assignment.

    An exception class sets its attributes on the instance, so none of them
    exist at class level. Both the constructor's parameters and anything it
    assigns to `self` count, which is how `CheckError.errors` -- assigned but
    not a parameter -- is recognised as real.
    """
    names: set[str] = set()
    try:
        # `klass` is the class itself, so this is the class's own __init__,
        # not an instance's -- which is what mypy's warning is about.
        signature = inspect.signature(klass)
        names |= set(signature.parameters)
    except (TypeError, ValueError):
        pass
    for base in klass.__mro__:
        if not (getattr(base, "__module__", "") or "").startswith("ligase"):
            continue
        try:
            source = inspect.getsource(base)
        except (OSError, TypeError):
            continue
        names |= set(
            re.findall(r"^\s+self\.(\w+)(?:\s*:[^=]+)?\s*=", source, re.MULTILINE)
        )
    return names


def property_type(klass: type, name: str) -> str:
    """`: str` for a property whose getter is annotated, else nothing."""
    member = getattr(klass, name, None)
    getter = getattr(member, "fget", None)
    if getter is None:
        return ""
    annotation = getattr(getter, "__annotations__", {}).get("return")
    return f": {annotation}" if annotation else ""


def render_doc(doc: Doc, heading_level: int) -> list[str]:
    """A parsed docstring, as MDX."""
    out: list[str] = []
    if doc.summary:
        out += [mdx_safe(doc.summary), ""]

    body = dedent_block(doc.body)
    if body.strip():
        out += [mdx_safe(body), ""]

    hashes = "#" * (heading_level + 1)

    for label in ("Args", "Arguments", "Parameters"):
        if label in doc.sections:
            rows = parse_named_section(doc.sections[label])
            if rows:
                out += [f"{hashes} Parameters", ""]
                out += render_named_table(rows, ("Parameter", "Description"))
                out.append("")

    for label, title in (("Returns", "Returns"), ("Yields", "Yields")):
        if label in doc.sections:
            out += [
                f"{hashes} {title}",
                "",
                mdx_safe(dedent_block(doc.sections[label])),
                "",
            ]

    if "Raises" in doc.sections:
        rows = parse_named_section(doc.sections["Raises"])
        out += [f"{hashes} Raises", ""]
        out += (
            render_named_table(rows, ("Raises", "When"))
            if rows
            else [mdx_safe(dedent_block(doc.sections["Raises"]))]
        )
        out.append("")

    if "Attributes" in doc.sections:
        rows = parse_named_section(doc.sections["Attributes"])
        out += [f"{hashes} Attributes", ""]
        out += (
            render_named_table(rows, ("Attribute", "Description"))
            if rows
            else [mdx_safe(dedent_block(doc.sections["Attributes"]))]
        )
        out.append("")

    for label in ("Example", "Examples"):
        if label in doc.sections:
            block = dedent_block(doc.sections[label])
            out += [f"{hashes} Example", ""]
            # Docstring examples are doctests; fence them as a session.
            out += ["```python", block, "```", ""]

    for label in ("Note", "Notes", "Warning", "Warnings"):
        if label in doc.sections:
            kind = "note" if label.startswith("Note") else "warning"
            out += [
                f'<Note kind="{kind}">',
                "",
                mdx_safe(dedent_block(doc.sections[label])),
                "",
                "</Note>",
                "",
            ]

    return out


def parse_named_section(lines: list[str]) -> list[tuple[str, str]]:
    """`name: description` entries, with continuation lines folded in."""
    block = dedent_block(lines)
    rows: list[tuple[str, str]] = []
    for line in block.split("\n"):
        if not line.strip():
            continue
        entry = re.match(r"^(\*{0,2}\w[\w.*]*)\s*(?:\(([^)]*)\))?\s*:\s*(.*)$", line)
        if entry and not line.startswith((" ", "\t")):
            name = entry.group(1)
            if entry.group(2):
                name = f"{name} ({entry.group(2)})"
            rows.append((name, entry.group(3).strip()))
        elif rows:
            rows[-1] = (rows[-1][0], f"{rows[-1][1]} {line.strip()}")
    return rows


def render_named_table(
    rows: list[tuple[str, str]], headers: tuple[str, str]
) -> list[str]:
    """A two-column table of `name: description` entries."""
    out = [f"| {headers[0]} | {headers[1]} |", "|---|---|"]
    for name, description in rows:
        clean = mdx_safe(description).replace("|", "\\|")
        out.append(f"| `{name}` | {clean} |")
    return out


def resolve(dotted: str, lg: Any) -> tuple[Any, str]:
    """Resolve `hivdb.interpret` or `reference` to its object and display name."""
    obj: Any = lg
    for part in dotted.split("."):
        obj = getattr(obj, part)
    return obj, dotted


def render_entry(dotted: str, lg: Any, problems: list[str]) -> list[str]:
    """One exported name, as MDX."""
    obj, display = resolve(dotted, lg)

    # `@lg.step` replaces a function with a `Step` *instance*, which is
    # neither a class nor a routine -- so without this every step the package
    # exports fell through to the plain-value branch below and was published
    # as `map_identifiers: Step = <Step map_identifiers -> EntitySet>`, with
    # its whole docstring dropped. The build did not complain, because the
    # branch that requires a docstring was never reached. Document the
    # function the decorator read instead, under the step's own name.
    if isinstance(obj, lg.Step):
        obj = obj.function

    # A plain value, such as `__version__` or `hivdb.DEFAULT`, has no docstring
    # of its own; document it by value and type.
    if not (inspect.isclass(obj) or inspect.isroutine(obj) or inspect.ismodule(obj)):
        return [
            f"## {display}",
            "",
            "```python",
            f"{display}: {type(obj).__name__} = {obj!r}",
            "```",
            "",
        ]

    raw = inspect.getdoc(obj)
    if not raw or not raw.strip():
        problems.append(
            f"{display}: no docstring. An empty heading looks like documentation "
            f"and is not, so this fails the build instead of being emitted."
        )
        return []

    doc = parse_doc(raw)
    out = [f"## {display}", ""]
    signature = signature_of(obj, display.split(".")[-1])
    if signature:
        out += ["```python", mdx_sig(signature), "```", ""]
    out += render_doc(doc, heading_level=2)

    if inspect.isclass(obj):
        documented = attribute_names(doc)
        members = own_members(obj)
        member_names = {name for name, _ in members}

        # A field documented in `Attributes:` that is not a member any more:
        # the docstring describes something that no longer exists. An exception
        # class sets its attributes in `__init__`, where they are invisible at
        # class level, so its constructor's parameters count as members too.
        constructed = init_parameters(obj)
        for stale in sorted(documented - member_names - constructed):
            if stale not in vars(obj) and not hasattr(obj, stale):
                problems.append(
                    f"{display}: `Attributes:` documents `{stale}`, which is not a "
                    f"member. The class docstring describes something that is not "
                    f"there."
                )

        for member_name, member in members:
            is_callable = inspect.isroutine(member) or isinstance(
                member, (property, staticmethod, classmethod)
            )

            if not is_callable:
                # A plain data field. These are documented collectively, in the
                # class docstring's `Attributes:` section, which is where the
                # project writes them -- so that is where they are required.
                if member_name not in documented:
                    problems.append(
                        f"{display}.{member_name}: a public field documented "
                        f"nowhere. Add it to the `Attributes:` section of "
                        f"{display}'s docstring."
                    )
                continue

            member_doc = inspect.getdoc(member)
            if isinstance(member, property):
                member_doc = inspect.getdoc(member.fget) or member_doc
            if not member_doc or not member_doc.strip():
                problems.append(
                    f"{display}.{member_name}: no docstring. A method with an "
                    f"empty heading looks like documentation and is not."
                )
                continue

            out.append(f"### {display}.{member_name}")
            out.append("")
            if isinstance(member, property):
                annotation = property_type(obj, member_name)
                out += [
                    "```python",
                    f"{member_name}{annotation}  # property",
                    "```",
                    "",
                ]
            else:
                member_sig = signature_of(member, member_name)
                if member_sig:
                    out += ["```python", mdx_sig(member_sig), "```", ""]
            out += render_doc(parse_doc(member_doc), heading_level=3)

    return out


def mdx_sig(signature: str) -> str:
    """Signatures go in a fence, so they need no escaping -- only wrapping."""
    if len(signature) <= 76:
        return signature
    head, _, tail = signature.partition("(")
    args, _, ret = tail.rpartition(")")
    wrapped = ",\n    ".join(part.strip() for part in args.split(", "))
    return f"{head}(\n    {wrapped},\n){ret}"


# --------------------------------------------------------------------------
# The coverage contract
# --------------------------------------------------------------------------


def exported_names(
    module: Any, names: Any, submodules: tuple[str, ...], prefix: str = ""
) -> set[str]:
    """Every public name under `module`, dotted, with submodules walked.

    A submodule named in `submodules` is not itself an entry: its own
    `__all__` is, and so on down, because `ligase.steps` is a package of
    packages. Without the recursion `steps.minimap2` would be the leaf and
    `steps.minimap2.align` -- the step a reader actually calls -- would be
    neither grouped nor required to be documented.
    """
    found: set[str] = set()
    for name in names:
        dotted = f"{prefix}{name}"
        member = getattr(module, name)
        if name in submodules or (prefix and inspect.ismodule(member)):
            found |= exported_names(
                member, getattr(member, "__all__", ()), submodules, f"{dotted}."
            )
            continue
        found.add(dotted)
    return found


def check_coverage(lg: Any) -> list[str]:
    """Every export grouped, and every grouped name exported."""
    problems: list[str] = []

    exported = exported_names(lg, lg.__all__, MODULE_EXPORTS)

    grouped = {name for group in GROUPS for name in group.names}

    for name in sorted(exported - grouped):
        problems.append(
            f"{name} is in __all__ but is in no group in GROUPS. It is public and "
            f"undocumented: add it to a group, or stop exporting it."
        )
    for name in sorted(grouped - exported):
        problems.append(
            f"{name} is grouped in GROUPS but is not exported. The reference would "
            f"describe something callers cannot reach: remove it, or export it."
        )
    return problems


# --------------------------------------------------------------------------


def main() -> int:
    """Generate the API pages, or fail saying what is undocumented."""
    try:
        import ligase as lg
    except ImportError as exc:
        print(
            f"error: cannot import ligase ({exc}).\n"
            "The API reference is generated from the package's own docstrings, so "
            "the docs build needs the compiled extension. Install it with "
            "`maturin develop` or `pip install -e .`.",
            file=sys.stderr,
        )
        return 1

    problems = check_coverage(lg)

    pages: list[tuple[Group, str]] = []
    for group in GROUPS:
        lines = [
            "---",
            f'title: "{group.title}"',
            f'description: "Ligase API reference: {group.title.lower()}."',
            "---",
            "",
            f"# {group.title}",
            "",
        ]
        if group.blurb:
            lines += [group.blurb, ""]
        for dotted in group.names:
            lines += render_entry(dotted, lg, problems)
        pages.append((group, "\n".join(lines).rstrip() + "\n"))

    if problems:
        print(
            f"error: {len(problems)} problem(s) generating the API reference:\n",
            file=sys.stderr,
        )
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1

    if OUT.exists():
        shutil.rmtree(OUT)
    OUT.mkdir(parents=True)

    for group, text in pages:
        (OUT / f"{group.slug}.mdx").write_text(text, encoding="utf-8")

    meta = [
        "import type { MetaRecord } from 'nextra'",
        "",
        "// Generated by web/scripts/generate_api.py. Do not edit.",
        "const meta: MetaRecord = {",
        "  index: 'Overview',",
    ]
    meta += [f"  {group.slug}: '{group.title}'," for group in GROUPS]
    meta += ["}", "", "export default meta", ""]
    (OUT / "_meta.ts").write_text("\n".join(meta), encoding="utf-8")

    counts = ", ".join(f"{g.title.lower()} {len(g.names)}" for g in GROUPS)
    (OUT / "index.mdx").write_text(overview(lg, counts), encoding="utf-8")

    total = sum(len(g.names) for g in GROUPS)
    print(
        f"generated {len(GROUPS)} API pages covering {total} names "
        f"from ligase {lg.version()}"
    )
    return 0


def overview(lg: Any, counts: str) -> str:
    """The API section index."""
    total = sum(len(g.names) for g in GROUPS)
    rows = "\n".join(
        f"| [{g.title}](/reference/api/{g.slug}/) | {len(g.names)} | {plain(g.blurb)} |"
        for g in GROUPS
    )
    return f"""---
title: "API reference"
description: "Everything ligase exports, grouped by what you are trying to do."
---

# API reference

<Lede>
Everything `ligase` exports — **{total} names**, generated from the package's own
docstrings against version `{lg.version()}`. Anything not listed here is an
implementation detail and may change without notice, including `ligase._core`,
the compiled module.
</Lede>

| Group | Names | |
|---|---|---|
{rows}

## How this page stays true

The reference is generated by
[`web/scripts/generate_api.py`](https://github.com/Neoforge-Labs/ligase/blob/main/web/scripts/generate_api.py)
on every docs build, which imports the package and reads its docstrings. Three
things fail the build rather than producing a page that is quietly wrong:

- a name in `__all__` that no group lists — a public API nobody documented
- a name a group lists that `__all__` does not export — a page describing
  something callers cannot reach
- any exported name, or any method or property on one, with no docstring

The grouping is by what a reader is trying to do rather than by module, and is
declared in that script rather than inferred.
"""


def plain(blurb: str) -> str:
    """A table cell: first sentence, with markup flattened."""
    if not blurb:
        return ""
    first = blurb.split("\n\n")[0]
    first = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", first)
    first = first.replace("`", "").replace("*", "")
    sentence = first.split(". ")[0].rstrip(".")
    return sentence.replace("|", "\\|")


if __name__ == "__main__":
    raise SystemExit(main())
