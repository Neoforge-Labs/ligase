#!/usr/bin/env python3
"""Audit a declared stochastic step and its undeclared twin, and record both.

The stochastic-steps pages make one claim that no other page makes and that
D40 rests on: a step that declares itself stochastic is reported
*reproducible* and never *deterministic*, even when its two runs write
byte-identical output -- while the same body, undeclared, is reported
deterministic. A page cannot assert that and be believed, so this proves it,
by building two real pipelines in a temporary directory and recording what the
real CLI printed when each was audited.

A third pipeline declares a seed and draws on system randomness instead, so
that the page's worst-case transcript is also real. It shuffles eight rows, so
two draws agree once in 40320 runs; the build retries a handful of times
before giving up, which costs nothing and cannot flake.

That makes this a test as well as a generator. If a declared stochastic step
ever starts being called deterministic, or an undeclared one stops, the docs
build fails rather than shipping a page that promises the opposite of what the
engine does.

Nothing here touches the repository: it all happens in a temp directory that is
thrown away.
"""

from __future__ import annotations

import io
import json
import os
import sys
import tempfile
from contextlib import redirect_stdout
from pathlib import Path

OUT = Path(__file__).resolve().parents[1] / "lib/stochastic.generated.ts"

#: Eight ranked genes. Enough rows that a system-random shuffle agreeing with
#: itself twice is a one-in-40320 event rather than a flaky build.
RANKED = (
    "id\tscore\n"
    "TP53\t12.4\n"
    "BRCA1\t9.1\n"
    "EGFR\t7.7\n"
    "NKX2-1\t3.2\n"
    "KRAS\t2.9\n"
    "PTEN\t2.1\n"
    "MYC\t1.8\n"
    "RB1\t1.1\n"
)

#: The body every pipeline below shares: a seeded permutation of a ranking.
#: A `RankedList` is the right output because its canonical form keeps row
#: order, so the seed's effect is visible in the engine's own comparison.
BODY = """import random
from pathlib import Path

import ligase as lg

ranking = lg.ranked_list("results/ranked.tsv", organism="Homo sapiens")


def shuffled(source: str, out: Path, seed: int) -> None:
    header, *rows = Path(source).read_text().splitlines()
    random.Random(seed).shuffle(rows)
    out.write_text("\\n".join([header, *rows]) + "\\n")


def unseeded(source: str, out: Path) -> None:
    header, *rows = Path(source).read_text().splitlines()
    random.SystemRandom().shuffle(rows)
    out.write_text("\\n".join([header, *rows]) + "\\n")
"""

#: Declared, and seeded by the parameter it names.
DECLARED = (
    BODY
    + '''

@lg.step(stochastic=True, seed="seed")
def permute(ranking: lg.RankedList, seed: int = 20260101) -> lg.RankedList:
    """Reorder a ranking, reproducibly, from its seed."""
    return lg.write(lambda out: shuffled(ranking.file, out, seed))


pipeline = lg.Pipeline(permute(ranking))
'''
)

#: The same body and the same seed, undeclared.
UNDECLARED = (
    BODY
    + '''

@lg.step
def permute(ranking: lg.RankedList, seed: int = 20260101) -> lg.RankedList:
    """The same work, with nothing said about where its randomness is."""
    return lg.write(lambda out: shuffled(ranking.file, out, seed))


pipeline = lg.Pipeline(permute(ranking))
'''
)

#: Declares a seed and draws on something else: a false declaration.
IGNORING = (
    BODY
    + '''

@lg.step(stochastic=True, seed="seed")
def permute(ranking: lg.RankedList, seed: int = 20260101) -> lg.RankedList:
    """Name a seed and ignore it, as a tool with a seed flag it does not
    honour would."""
    return lg.write(lambda out: unseeded(ranking.file, out))


pipeline = lg.Pipeline(permute(ranking))
'''
)


def run_cli(*argv: str) -> str:
    """Run the CLI in-process and return everything it printed."""
    from ligase.cli import main

    printed = io.StringIO()
    with redirect_stdout(printed):
        main(list(argv))
    return printed.getvalue().rstrip("\n")


def flowed(text: str) -> str:
    """The text with its line wrapping undone.

    A methods paragraph is wrapped at 76 columns, so a phrase these checks
    are about can land across two lines. What is being checked is the prose,
    not where it broke.
    """
    return " ".join(text.split())


def expect(key: str, text: str, *, says: str, not_says: str | None = None) -> str:
    """Fail the build when an output no longer says what the pages say it does."""
    if says not in flowed(text):
        raise SystemExit(
            f"{key}: expected the output to contain {says!r}, got:\n\n{text}\n\n"
            "The stochastic-steps pages claim this. Either the engine changed "
            "or the pages are now wrong; both need a human."
        )
    if not_says is not None and not_says in flowed(text):
        raise SystemExit(
            f"{key}: the output must not contain {not_says!r}, and does:\n\n"
            f"{text}\n\n"
            "D40 says a declared stochastic step is never reported "
            "deterministic. Something has made it one."
        )
    return text


def project(tmp: Path, name: str, pipeline: str) -> Path:
    """One pipeline, with its ranking, in its own directory."""
    root = tmp / name
    (root / "results").mkdir(parents=True)
    (root / "results/ranked.tsv").write_text(RANKED)
    (root / "pipeline.py").write_text(pipeline)
    return root


def audited(root: Path) -> str:
    """Run a pipeline, then audit its one step, from inside its directory."""
    here = Path.cwd()
    os.chdir(root)
    try:
        run_cli("run")
        return run_cli("audit", "permute")
    finally:
        os.chdir(here)


def main() -> int:
    """Audit the three pipelines and write the module the pages import."""
    try:
        import ligase  # noqa: F401
    except ImportError as exc:
        print(
            f"error: cannot import ligase ({exc}). The stochastic-steps pages "
            "are generated by auditing real pipelines, so the docs build needs "
            "the package.",
            file=sys.stderr,
        )
        return 1

    here = Path.cwd()
    with tempfile.TemporaryDirectory(prefix="ligase-stochastic-") as tmp:
        root = Path(tmp)
        try:
            declared = audited(project(root, "declared", DECLARED))
            captured = {
                "declared": expect(
                    "declared",
                    declared,
                    says="reproducible",
                    not_says="deterministic",
                ),
                "undeclared": expect(
                    "undeclared",
                    audited(project(root, "undeclared", UNDECLARED)),
                    says="deterministic",
                ),
                "ignoring": "",
                "methods": "",
            }

            # The same bytes both times, which is what makes `reproducible`
            # rather than `deterministic` the whole of the claim.
            expect("declared", declared, says="the same bytes both times")

            # A false declaration. Retried because a system-random shuffle of
            # eight rows can agree with itself; four attempts put that at one
            # in 2.6e18.
            for attempt in range(4):
                text = audited(project(root, f"ignoring-{attempt}", IGNORING))
                if "not reproducible" in text:
                    captured["ignoring"] = expect(
                        "ignoring", text, says="does not control"
                    )
                    break
            else:
                raise SystemExit(
                    "ignoring: four audits of a system-random shuffle of eight "
                    "rows all agreed, which should not happen. Either the step "
                    "stopped being random or the audit stopped comparing."
                )

            os.chdir(root / "declared")
            captured["methods"] = expect(
                "methods", run_cli("methods"), says="seeded with"
            )
        finally:
            os.chdir(here)

    described = {
        "declared": (
            "`ligase audit` on a step that declares itself stochastic and is "
            "seeded: byte-identical, and reproducible rather than "
            "deterministic."
        ),
        "undeclared": (
            "`ligase audit` on the same body with nothing declared: deterministic."
        ),
        "ignoring": (
            "`ligase audit` on a step that names a seed and draws on something else."
        ),
        "methods": "`ligase methods` for the declared step, naming its seed.",
    }

    lines = [
        "// Generated by web/scripts/capture_stochastic.py. Do not edit.",
        "//",
        "// Produced by auditing three real pipelines in a temporary directory:",
        "// a declared stochastic step, the same body undeclared, and a step",
        "// whose declared seed is not what it draws on. That the first is",
        "// reproducible rather than deterministic is the whole of D40, so the",
        "// build fails if it stops being true.",
        "",
        "export const stochastic = {",
    ]
    for key, text in captured.items():
        lines.append(f"  /** {described[key]} */")
        lines.append(f"  {key}: {json.dumps(text)},")
    lines += ["} as const", ""]

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines), encoding="utf-8")
    print(f"audited three pipelines and captured {len(captured)} transcripts")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
