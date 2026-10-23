#!/usr/bin/env bash
#
# Align one of the demo's HIV-1 runs against HXB2, producing a real BAM for
# the reference demo in a gitignored data/alignments/.
#
# This exists because M1 needs a BAM whose @SQ header was written by a real
# aligner, not by Ligase's own test helpers. The committed fixtures prove the
# parser reads what the SAM specification says; this proves it reads what
# minimap2 actually writes.
#
# The header is rebuilt with `samtools dict` so the @SQ lines carry M5
# checksums. minimap2 does not write them, and without them a reference check
# falls back to contig names and lengths -- which is precisely the weak check
# Ligase exists to replace, since hg19 and hg38 share plenty of both. Real
# pipelines that care about this reheader the same way.
#
# Written for bash 3.2 so it runs on a stock macOS.
#
# The reads and the reference come from the engine, not from disk: the demo
# pipeline declares where they come from and Ligase keeps them in its
# content-addressed store, so this script asks it for paths rather than
# knowing any. Warm the store first if it is cold:
#
#   python scripts/fetch_demo_data.py
#
# Usage:
#   scripts/make_demo_alignment.sh                   # the demo's first run
#   scripts/make_demo_alignment.sh SRR32217714       # a named run
#   scripts/make_demo_alignment.sh --list            # what is available
#   scripts/make_demo_alignment.sh --force           # realign
#
# Override the destination with LIGASE_DATA_DIR and the store with
# LIGASE_DEMO_STORE.

set -euo pipefail

SCRIPT_NAME="$(basename "${BASH_SOURCE[0]}")"
readonly SCRIPT_NAME
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly REPO_ROOT
readonly DATA_DIR="${LIGASE_DATA_DIR:-${REPO_ROOT}/data}"
readonly OUT_DIR="${DATA_DIR}/alignments"
readonly DEMO="${REPO_ROOT}/examples/hiv-resistance/pipeline.py"
readonly PYTHON="${LIGASE_PYTHON:-python3}"

# ----------------------------------------------------------------- helpers --

log()  { printf '  %s\n' "$*"; }
step() { printf '\n%s\n' "$*"; }
die()  { printf '%s: %s\n' "$SCRIPT_NAME" "$*" >&2; exit 1; }

require() {
    command -v "$1" >/dev/null 2>&1 || die "$1 is not installed. $2"
}

# Ask the demo what it declares, and where the engine has put it. One
# invocation rather than three, because loading the module resolves the
# remotes and that is the slow part.
#
# Prints, in order: the reference's path, then one "<project> <run> <mate1>
# <mate2>" line per run in the manifest.
demo_inputs() {
    "$PYTHON" - "$DEMO" <<'PYTHON'
import importlib.util
import os
import pathlib
import sys

import ligase as lg

demo = pathlib.Path(sys.argv[1])
store = os.environ.get("LIGASE_DEMO_STORE", demo.parents[2] / ".ligase")
lg.remotes.configure(cache=store, offline=True)

specification = importlib.util.spec_from_file_location("ligase_demo", demo)
module = importlib.util.module_from_spec(specification)
try:
    specification.loader.exec_module(module)
    print(module.reference().file)
    for project, runs in module.RUNS.items():
        for run in runs:
            print(project, run, *module.run_reads(run, project).files)
except lg.RemoteError as error:
    print(f"error: {error}", file=sys.stderr)
    raise SystemExit(1) from None
PYTHON
}

# Every paired run the demo declares, as "<project>/<run>".
available_runs() {
    printf '%s\n' "$INPUTS" | tail -n +2 | while read -r project run _rest; do
        printf '%s/%s\n' "$project" "$run"
    done
}

usage() {
    cat <<EOF
Usage: ${SCRIPT_NAME} [RUN|--list|--force]

Aligns one of the demo's paired runs against HXB2 into ${OUT_DIR}.
If the store is cold, warm it with: python scripts/fetch_demo_data.py
EOF
}

# -------------------------------------------------------------------- main --

main() {
    local wanted="" force=0 listing=0

    while [ $# -gt 0 ]; do
        case "$1" in
            --list) listing=1 ;;
            --force) force=1 ;;
            -h|--help)
                usage
                return 0
                ;;
            -*) die "unknown option: $1" ;;
            *)  wanted="$1" ;;
        esac
        shift
    done

    require minimap2 "Install it with: brew install minimap2"
    require samtools "Install it with: brew install samtools"

    INPUTS="$(demo_inputs)" || die "the demo's inputs are not in the store.
  Warm it with: python scripts/fetch_demo_data.py"
    readonly INPUTS
    REFERENCE="$(printf '%s\n' "$INPUTS" | head -n 1)"
    readonly REFERENCE

    if [ "$listing" -eq 1 ]; then
        available_runs
        return 0
    fi

    local candidates target=""
    candidates="$(available_runs)"
    [ -n "$candidates" ] || die "the demo declares no paired runs"

    if [ -n "$wanted" ]; then
        target="$(printf '%s\n' "$candidates" | grep -- "/${wanted}\$" || true)"
        [ -n "$target" ] || die "run ${wanted} is not among the fetched reads"
    else
        target="$(printf '%s\n' "$candidates" | head -n 1)"
    fi

    local project run mates mate1 mate2 out
    project="$(dirname "$target")"
    run="$(basename "$target")"
    mates="$(printf '%s\n' "$INPUTS" | awk -v r="$run" '$2 == r {print $3, $4}')"
    mate1="$(printf '%s\n' "$mates" | cut -d' ' -f1)"
    mate2="$(printf '%s\n' "$mates" | cut -d' ' -f2)"
    out="${OUT_DIR}/${run}.bam"

    if [ -f "$out" ] && [ "$force" -eq 0 ]; then
        log "already aligned: ${out}"
        log "pass --force to realign"
        return 0
    fi

    mkdir -p "$OUT_DIR"
    local work
    work="$(mktemp -d)"
    # shellcheck disable=SC2064  # expand $work now, while it is still set
    trap "rm -rf '${work}'" EXIT

    step "Aligning ${run} (${project}) against HXB2"
    log "reads:     ${mate1}"
    log "           ${mate2}"
    log "reference: ${REFERENCE}"

    # -ax sr: short paired reads. The read group is what gives the BAM a
    # sample set for Ligase to infer, and the sample is the run accession
    # rather than the filename, so the identity survives a rename.
    minimap2 -ax sr \
        -R "@RG\tID:${run}\tSM:${run}\tPL:ILLUMINA" \
        "$REFERENCE" \
        "$mate1" \
        "$mate2" \
        2>"${work}/minimap2.log" \
        | samtools sort -o "${work}/raw.bam" -

    step "Rebuilding the header with M5 checksums"
    # samtools dict emits @SQ lines carrying M5; minimap2 does not. The @SQ
    # order must match the original, or reheader would silently invalidate
    # every record's reference id -- it does, because both follow the FASTA.
    {
        printf '@HD\tVN:1.6\tSO:coordinate\n'
        samtools dict "$REFERENCE" | grep '^@SQ'
        printf '@RG\tID:%s\tSM:%s\tPL:ILLUMINA\n' "$run" "$run"
        samtools view -H "${work}/raw.bam" | grep '^@PG' || true
    } >"${work}/header.sam"

    samtools reheader "${work}/header.sam" "${work}/raw.bam" >"$out"
    samtools index "$out"

    step "Done"
    log "wrote ${out}"
    samtools view -H "$out" | grep -E '^@(SQ|RG)' | sed 's/^/  /'
    log ""
    log "mapped reads: $(samtools view -c -F 4 "$out")"
}

main "$@"
