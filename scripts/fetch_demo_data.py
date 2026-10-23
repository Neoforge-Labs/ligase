#!/usr/bin/env python3
"""Warm a store with everything the HIV-1 demo and its validation need.

This is not how the demo gets its data. The demo *declares* where its data
comes from — `lg.remote("ncbi:K03455.1", ...)`, `lg.remote("sra:SRR8525904",
...)` — so `ligase run` fetches it, checks every digest before any step
runs, and keeps it in the content-addressed store. Nothing here is required
to run the demo.

What this is for is the test suite. A test that needed the demo's inputs
used to gate on a file being on disk; now it gates on the inputs being
obtainable, and it is deliberately **offline** unless the machine says it
was prepared for the demo. A suite that downloaded a hundred megabytes
because you ran it is a suite people stop running. So the tests skip, and
this is the one command that makes them stop skipping:

    python scripts/fetch_demo_data.py

It replaces `scripts/fetch_test_data.sh`, which was three hundred lines that
knew ENA's directory layout, computed checksums and verified them — all of
it outside anything Ligase promises, with the engine taking the script's
word for what had arrived. The layout and the verification are the engine's
now, and the manifest is the demo pipeline itself.

Usage:
    python scripts/fetch_demo_data.py                      # everything
    python scripts/fetch_demo_data.py --project PRJNA517147
    python scripts/fetch_demo_data.py --reference-only
    python scripts/fetch_demo_data.py --list               # the manifest

Override the store with LIGASE_DEMO_STORE.
"""

from __future__ import annotations

import argparse
import importlib.util
import os
import sys
from pathlib import Path
from typing import TYPE_CHECKING

import ligase as lg

if TYPE_CHECKING:
    from collections.abc import Sequence
    from types import ModuleType

REPO = Path(__file__).resolve().parents[1]
DEMO = REPO / "examples" / "hiv-resistance" / "pipeline.py"

#: Where the inputs go. The same default the tests read, so one warm-up
#: serves both.
STORE = Path(os.environ.get("LIGASE_DEMO_STORE", REPO / ".ligase"))


def demo() -> ModuleType:
    """The demo pipeline, loaded for its manifest.

    Loading it also resolves the default project's remotes, which is the
    first half of the work this script does.
    """
    specification = importlib.util.spec_from_file_location("ligase_demo", DEMO)
    if specification is None or specification.loader is None:  # pragma: no cover
        message = f"{DEMO} is not an importable Python file"
        raise ImportError(message)
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def main(argv: Sequence[str] | None = None) -> int:
    """Fetch what the demo declares, and say what was done.

    Args:
        argv: The arguments, excluding the program name.

    Returns:
        0 when everything resolved, 1 when something did not.
    """
    parser = argparse.ArgumentParser(
        prog="fetch_demo_data.py",
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--project", help="only this BioProject's runs")
    parser.add_argument("--reference-only", action="store_true", help="only HXB2")
    parser.add_argument(
        "--list", action="store_true", help="print the manifest and stop"
    )
    arguments = parser.parse_args(argv)

    lg.remotes.configure(cache=STORE, offline=False)

    if arguments.list:
        # Before the module is loaded, so `--list` needs no network. The
        # manifest is read by exec'ing the file, which resolves the default
        # project -- so this prints from the module and accepts that cost
        # only when asked for the list.
        module = demo()
        print(f"{module.HXB2:<16} HXB2, pinned to {module.HXB2_DIGEST}")
        for project, runs in module.RUNS.items():
            print()
            print(project)
            for run, digest in runs.items():
                print(f"  sra:{run:<14} {digest}")
        return 0

    print(f"fetching the demo's inputs into {STORE}")
    try:
        module = demo()
        print(f"  {module.HXB2} ready")
        if not arguments.reference_only:
            wanted = (
                {arguments.project: module.RUNS[arguments.project]}
                if arguments.project
                else module.RUNS
            )
            for project, runs in wanted.items():
                for run in runs:
                    module.run_reads(run, project)
                    print(f"  sra:{run} ready")
    except KeyError:
        known = ", ".join(demo().RUNS)
        print(
            f"fetch_demo_data.py: unknown project: {arguments.project}. Known: {known}",
            file=sys.stderr,
        )
        return 1
    except lg.RemoteError as error:
        # The interesting failure: a digest that does not match means the
        # source moved, and that is a thing to go and look at rather than
        # to retry.
        print(f"fetch_demo_data.py: {error}", file=sys.stderr)
        return 1

    print("done; the store is warm and the tests will stop skipping")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
