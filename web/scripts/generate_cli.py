#!/usr/bin/env python3
"""Generate the CLI reference from the argparse parser itself.

`ligase check`, `run`, `audit` and `methods` are named all over the site and
were documented nowhere: their flags existed only in `--help`. A reader who
wanted to know what `--force` did, or where the cache lives, had no page to
go to.

Written from the parser rather than by hand, for the same reason the API
reference is written from the docstrings: a hand-kept copy of a `--help`
output is wrong the first time somebody adds a flag. The build fails if a
command or an option has no help text, since an undocumented flag in a
reference table is worse than an absent one -- it looks checked.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Any

OUT = Path(__file__).resolve().parents[1] / "content/reference/cli.mdx"

# What each command is for, beyond its one-line argparse help: the sentence a
# reader needs before the table of flags means anything.
PREAMBLE: dict[str, str] = {
    "check": (
        "The one to reach for first, and the one that makes the rest of "
        "Ligase worth having. It imports your pipeline file, builds the "
        "graph from the step signatures, and compares every connection's "
        "dimensions. **No step body runs**, nothing is computed, and no "
        "tool is invoked — it reads file headers only, so it finishes in "
        "milliseconds on a pipeline that would take all night."
    ),
    "run": (
        "Checks the pipeline, then computes whatever is not already known. "
        "A step runs when its code, its parameters, its inputs' contents or "
        "one of its tools' versions has changed, and not otherwise. "
        "Independent steps run at the same time."
    ),
    "audit": (
        "Answers the opposite question to the cache: not *can this be "
        "skipped* but *does this repeat*. It runs one step twice more and "
        "compares what came out, both as bytes and as what those bytes "
        "mean. A step whose bytes move while its result does not is "
        "reproducible — the ordinary case for a tool that writes its own "
        "command line into its output."
    ),
    "methods": (
        "Writes the paragraph you put in a paper: every tool at the version "
        "it actually ran at, every parameter as it was hashed, every "
        "reference by accession and checksum, and the works to cite. "
        "`--bundle` packages it so somebody else can check the results, and "
        "`--verify` is that check."
    ),
}

# A worked invocation per command, so the page shows use and not just syntax.
EXAMPLES: dict[str, list[tuple[str, str]]] = {
    "check": [
        ("ligase check", "check `pipeline.py` in the current directory"),
        ("ligase check analysis/variants.py", "check a pipeline somewhere else"),
    ],
    "run": [
        ("ligase run", "run what is not already cached"),
        ("ligase run -j 8", "run up to eight steps at once"),
        ("ligase run --force", "recompute everything, cache or no cache"),
        (
            "ligase run --cache /scratch/ligase",
            "keep the cache off your home directory",
        ),
    ],
    "audit": [
        ("ligase audit align", "run `align` twice more and compare"),
        ("ligase audit align --sample SRR8525904", "audit one sample's node only"),
    ],
    "methods": [
        ("ligase methods", "print the methods paragraph and the reference list"),
        ("ligase methods --bundle paper/bundle", "write a reproducibility bundle"),
        ("ligase methods --verify paper/bundle", "check a bundle against this machine"),
    ],
}


def options_of(parser: argparse.ArgumentParser) -> tuple[list[Any], list[Any]]:
    """A parser's positional and optional actions, minus `--help`."""
    positionals: list[Any] = []
    optionals: list[Any] = []
    for action in parser._actions:
        if isinstance(action, argparse._HelpAction):
            continue
        if isinstance(action, argparse._SubParsersAction):
            continue
        (positionals if not action.option_strings else optionals).append(action)
    return positionals, optionals


def flags(action: Any) -> str:
    """`-j, --jobs N` — how the option is written at a prompt."""
    if not action.option_strings:
        return f"`{action.dest}`"
    metavar = action.metavar or (action.dest.upper() if action.nargs != 0 else "")
    joined = ", ".join(action.option_strings)
    return f"`{joined}{' ' + metavar if metavar else ''}`"


def default_of(action: Any) -> str:
    """The trailing default note, unless the help text already carries one."""
    if action.default in (None, False) or isinstance(action, argparse._StoreTrueAction):
        return ""
    if "default" in (action.help or "").lower():
        return ""
    return f" Default: `{action.default}`."


def describe(action: Any) -> str:
    """An option's help, as a sentence, without a repeated default."""
    help_text = " ".join((action.help or "").split())
    # argparse help conventionally carries "(default: x)"; the table has a
    # column for that, so it is lifted out rather than printed twice.
    head, _, tail = help_text.partition("(default:")
    sentence = head.strip().rstrip(".")
    if tail:
        return f"{sentence.capitalize()}. Default: `{tail.rstrip(').').strip()}`."
    return f"{sentence.capitalize()}.{default_of(action)}"


def render(parser: argparse.ArgumentParser, problems: list[str]) -> list[str]:
    """One command's section."""
    out: list[str] = []
    subparsers = next(
        (a for a in parser._actions if isinstance(a, argparse._SubParsersAction)),
        None,
    )
    if subparsers is None:
        return out

    for name, sub in subparsers.choices.items():
        if not (sub.description or "").strip():
            problems.append(f"`ligase {name}` has no description.")
            continue

        out += [f"## ligase {name}", ""]
        if name in PREAMBLE:
            out += [PREAMBLE[name], ""]
        usage = " ".join(sub.format_usage().replace("usage: ", "").split())
        out += ["```bash", usage.replace("[-h] ", ""), "```", ""]
        # Where a preamble exists it says everything argparse's description
        # says and more, so printing both would just repeat the shorter one.
        if name not in PREAMBLE:
            out += [" ".join((sub.description or "").split()), ""]

        positionals, optionals = options_of(sub)
        if positionals:
            out += ["### Arguments", "", "| Argument | Description |", "|---|---|"]
            for action in positionals:
                if not action.help:
                    problems.append(
                        f"`ligase {name}`: argument `{action.dest}` has no help."
                    )
                    continue
                out.append(f"| {flags(action)} | {describe(action)} |")
            out.append("")
        if optionals:
            out += ["### Options", "", "| Option | Description |", "|---|---|"]
            for action in optionals:
                if not action.help:
                    problems.append(
                        f"`ligase {name}`: option `{action.option_strings[0]}` "
                        f"has no help."
                    )
                    continue
                out.append(f"| {flags(action)} | {describe(action)} |")
            out.append("")

        if name in EXAMPLES:
            out += ["### Examples", "", "```bash"]
            for command, why in EXAMPLES[name]:
                out += [f"# {why}", command, ""]
            out = [*out[:-1], "```", ""]

    return out


def main() -> int:
    """Write the CLI reference, or fail saying what is undocumented."""
    try:
        from ligase.cli import build_parser
    except ImportError as exc:
        print(
            f"error: cannot import ligase.cli ({exc}).\n"
            "The CLI reference is generated from the parser, so the docs build "
            "needs the package installed. `maturin develop` or `pip install -e .`.",
            file=sys.stderr,
        )
        return 1

    parser = build_parser()
    problems: list[str] = []
    body = render(parser, problems)

    if problems:
        print(
            f"error: {len(problems)} undocumented part(s) of the CLI:\n",
            file=sys.stderr,
        )
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1

    page = [
        "---",
        'title: "Command line"',
        'description: "Every ligase command and flag, from the parser itself."',
        "---",
        "",
        "# Command line",
        "",
        "<Lede>",
        "Four commands. `check` type-checks a pipeline without running it, `run`",
        "computes what is not already known, `audit` asks whether a step repeats,",
        "and `methods` says how the results were produced.",
        "</Lede>",
        "",
        "Every command takes the pipeline file as its last argument and defaults to",
        "`pipeline.py` in the current directory. `run`, `audit` and `methods` all",
        "check the pipeline first, so `check` is never a prerequisite — it is the",
        "fast way to ask the question on its own.",
        "",
        '<Note kind="tip" title="This page is generated from the parser">',
        "",
        "So it cannot drift from `ligase --help`. If a flag is missing here, it does",
        "not exist; if one exists and has no help text, the docs build fails rather",
        "than printing an empty row.",
        "",
        "</Note>",
        "",
        *body,
    ]
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(page).rstrip() + "\n", encoding="utf-8")
    print(f"generated the CLI reference: {len(PREAMBLE)} commands")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
