#!/usr/bin/env bash
#
# Align one fetched HIV-1 run against HXB2, producing a real BAM for the
# reference demo in a gitignored data/alignments/.
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
# Usage:
#   scripts/make_demo_alignment.sh                   # the first run found
#   scripts/make_demo_alignment.sh SRR32217714       # a named run
#   scripts/make_demo_alignment.sh --list            # what is available
#   scripts/make_demo_alignment.sh --force           # realign
#
# Override the destination with LIGASE_DATA_DIR.

set -euo pipefail

SCRIPT_NAME="$(basename "${BASH_SOURCE[0]}")"
readonly SCRIPT_NAME
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly REPO_ROOT
readonly DATA_DIR="${LIGASE_DATA_DIR:-${REPO_ROOT}/data}"
readonly REFERENCE="${DATA_DIR}/refs/HXB2.fasta"
readonly OUT_DIR="${DATA_DIR}/alignments"

# ----------------------------------------------------------------- helpers --

log()  { printf '  %s\n' "$*"; }
step() { printf '\n%s\n' "$*"; }
die()  { printf '%s: %s\n' "$SCRIPT_NAME" "$*" >&2; exit 1; }

require() {
    command -v "$1" >/dev/null 2>&1 || die "$1 is not installed. $2"
}

# Every paired run that has been fetched, as "<project>/<run>".
available_runs() {
    local mate1
    for mate1 in "${DATA_DIR}"/reads/*/*_1.fastq.gz; do
        [ -e "$mate1" ] || continue
        [ -e "${mate1%_1.fastq.gz}_2.fastq.gz" ] || continue
        local run project
        run="$(basename "${mate1%_1.fastq.gz}")"
        project="$(basename "$(dirname "$mate1")")"
        printf '%s/%s\n' "$project" "$run"
    done
}

usage() {
    cat <<EOF
Usage: ${SCRIPT_NAME} [RUN|--list|--force]

Aligns a fetched paired run against HXB2 into ${OUT_DIR}.
Run scripts/fetch_test_data.sh first.
EOF
}

# -------------------------------------------------------------------- main --

main() {
    local wanted="" force=0

    while [ $# -gt 0 ]; do
        case "$1" in
            --list)
                available_runs
                return 0
                ;;
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

    [ -f "$REFERENCE" ] || die "no reference at ${REFERENCE}. Run scripts/fetch_test_data.sh"

    local candidates target=""
    candidates="$(available_runs)"
    [ -n "$candidates" ] || die "no paired reads found. Run scripts/fetch_test_data.sh"

    if [ -n "$wanted" ]; then
        target="$(printf '%s\n' "$candidates" | grep -- "/${wanted}\$" || true)"
        [ -n "$target" ] || die "run ${wanted} is not among the fetched reads"
    else
        target="$(printf '%s\n' "$candidates" | head -n 1)"
    fi

    local project run reads_dir out
    project="$(dirname "$target")"
    run="$(basename "$target")"
    reads_dir="${DATA_DIR}/reads/${project}"
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
    log "reads:     ${reads_dir}/${run}_{1,2}.fastq.gz"
    log "reference: ${REFERENCE}"

    # -ax sr: short paired reads. The read group is what gives the BAM a
    # sample set for Ligase to infer, and the sample is the run accession
    # rather than the filename, so the identity survives a rename.
    minimap2 -ax sr \
        -R "@RG\tID:${run}\tSM:${run}\tPL:ILLUMINA" \
        "$REFERENCE" \
        "${reads_dir}/${run}_1.fastq.gz" \
        "${reads_dir}/${run}_2.fastq.gz" \
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
