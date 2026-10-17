#!/usr/bin/env bash
#
# Fetch the pinned test data for the Ligase HIV-1 reference pipeline into a
# gitignored data/ directory.
#
#   HXB2 reference  NCBI K03455.1 (HIV-1, 9719 bp)
#   PRJNA517147     4 smallest runs -- published resistance results
#   PRJNA1218827    4 smallest runs -- Sanger ground truth
#
# Everything is pinned by accession plus checksum, per decision D2/D4: nothing
# here trusts a filename, a timestamp or "whatever the server returns today".
# FASTQs come straight from ENA over HTTPS with ENA's own per-file MD5, so no
# SRA toolkit is required.
#
# The reference is pinned by the SHA-256 of its *canonical sequence* (sequence
# only, uppercased, newlines stripped) rather than of the raw file. That is
# decision D4 applied to an input: NCBI may reflow lines or reword a defline
# without the biology changing, and that must not break the build.
#
# Written for bash 3.2 so it runs on a stock macOS.
#
# Usage:
#   scripts/fetch_test_data.sh                      # everything
#   scripts/fetch_test_data.sh --project PRJNA517147
#   scripts/fetch_test_data.sh --reference-only
#   scripts/fetch_test_data.sh --list               # print the manifest
#   scripts/fetch_test_data.sh --force              # re-download
#
# Override the destination with LIGASE_DATA_DIR.

set -euo pipefail

SCRIPT_NAME="$(basename "${BASH_SOURCE[0]}")"
readonly SCRIPT_NAME
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly REPO_ROOT
readonly DATA_DIR="${LIGASE_DATA_DIR:-${REPO_ROOT}/data}"

# ---------------------------------------------------------------- manifest --

readonly HXB2_ACCESSION="K03455.1"
readonly HXB2_BASES=9719
readonly HXB2_SEQ_SHA256="5139d40722055faad5071f8aac5187512f66e8b19ecafa2c90c9772d01cfb3b0"
readonly HXB2_URL="https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi?db=nuccore&id=${HXB2_ACCESSION}&rettype=fasta&retmode=text"

# "PROJECT RUN MD5_MATE1 MD5_MATE2"; paired Illumina AMPLICON runs, chosen as
# the smallest in each project so the demo stays quick to run.
readonly RUN_MANIFEST="\
PRJNA517147 SRR8525904 16c6736e3d54a27498ede1497f39ac64 5f9490acc04a7761915764671b5ca692
PRJNA517147 SRR8525908 880871c713d11afcd37d66860e31c43e 08dda6316eb1d25155ffc8ef994abbea
PRJNA517147 SRR8525942 2fecc0b0e6f5c21c9f94f01b518eb27a 19c1801c7a64ed6c34c7155065ea52b7
PRJNA517147 SRR8525920 f05fb9871b81c17f968ada1cc4b4891f bfd76fbe5d9d568bd53bef1becfe4e17
PRJNA1218827 SRR32217714 20c181e9fa945a3fbc014b93c49ac891 4e1bd060b4df205b33aa13a16cd5382e
PRJNA1218827 SRR32217716 6f869da0f0605719cad6619084a4a57b 01fe4bb6f680f24e49dd186c364bc340
PRJNA1218827 SRR32217715 4b66a7a918cf616b1dff6956399cbc79 ec7e5a91ff202e3985f9597f6cc55658
PRJNA1218827 SRR32217726 786c08140b38ec7541e0b5f1391071da 6496c720967839849c2429cb8014f042"

# ----------------------------------------------------------------- helpers --

log()  { printf '  %s\n' "$*"; }
step() { printf '\n==> %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die()  { printf '%s: error: %s\n' "$SCRIPT_NAME" "$*" >&2; exit 1; }

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

md5_of() {
  if command -v md5sum >/dev/null 2>&1; then
    md5sum "$1" | cut -d' ' -f1
  else
    md5 -q "$1"
  fi
}

sha256_of_stdin() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum | cut -d' ' -f1
  else
    shasum -a 256 | cut -d' ' -f1
  fi
}

# Download to a temporary file and move into place only once verified, so an
# interrupted run never leaves a half-file that looks finished.
download_to() {
  local url="$1" dest="$2" tmp
  tmp="${dest}.partial.$$"
  if ! curl -fSL --retry 3 --retry-delay 2 --connect-timeout 30 \
            --progress-bar "$url" -o "$tmp"; then
    rm -f "$tmp"
    die "download failed: $url"
  fi
  mv "$tmp" "$dest"
}

# ENA lays FASTQs out at vol1/fastq/<first 6>/<subdir>/<run>/, where subdir is
# the accession's trailing digits zero-padded to three, and is absent for
# 9-character accessions.
ena_run_subdir() {
  local acc="$1"
  case "${#acc}" in
    9)  printf '' ;;
    10) printf '00%s' "${acc: -1}" ;;
    11) printf '0%s'  "${acc: -2}" ;;
    12) printf '%s'   "${acc: -3}" ;;
    *)  die "unexpected accession length for '$acc'" ;;
  esac
}

ena_fastq_url() {
  local acc="$1" mate="$2" subdir
  subdir="$(ena_run_subdir "$acc")"
  if [ -n "$subdir" ]; then
    printf 'https://ftp.sra.ebi.ac.uk/vol1/fastq/%s/%s/%s/%s_%s.fastq.gz' \
      "${acc:0:6}" "$subdir" "$acc" "$acc" "$mate"
  else
    printf 'https://ftp.sra.ebi.ac.uk/vol1/fastq/%s/%s/%s_%s.fastq.gz' \
      "${acc:0:6}" "$acc" "$acc" "$mate"
  fi
}

# ------------------------------------------------------------------ actions --

canonical_sequence_sha256() {
  # Strip the defline, drop line breaks and carriage returns, uppercase.
  tail -n +2 "$1" | tr -d '\n\r' | tr '[:lower:]' '[:upper:]' | sha256_of_stdin
}

sequence_length() {
  tail -n +2 "$1" | tr -d '\n\r' | wc -c | tr -d '[:space:]'
}

verify_reference() {
  local path="$1" got_sha got_len
  got_len="$(sequence_length "$path")"
  [ "$got_len" = "$HXB2_BASES" ] || {
    warn "expected ${HXB2_BASES} bases, got ${got_len}"
    return 1
  }
  got_sha="$(canonical_sequence_sha256 "$path")"
  [ "$got_sha" = "$HXB2_SEQ_SHA256" ] || {
    warn "canonical sequence SHA-256 mismatch"
    warn "  expected ${HXB2_SEQ_SHA256}"
    warn "  got      ${got_sha}"
    return 1
  }
  return 0
}

fetch_reference() {
  local dest_dir="${DATA_DIR}/refs" dest="${DATA_DIR}/refs/HXB2.fasta"
  step "Reference: HXB2 (${HXB2_ACCESSION})"
  mkdir -p "$dest_dir"

  if [ -f "$dest" ] && [ "$FORCE" != "yes" ] && verify_reference "$dest" 2>/dev/null; then
    log "already present and verified: ${dest#"$REPO_ROOT"/}"
    return 0
  fi

  log "downloading from NCBI"
  download_to "$HXB2_URL" "$dest"

  if ! verify_reference "$dest"; then
    die "reference failed verification. The sequence itself differs from the
  pinned HXB2, so this is not a cosmetic reformat -- do not update the pin
  without checking what changed at NCBI."
  fi
  log "verified: ${HXB2_BASES} bp, canonical SHA-256 matches"
}

# Download one mate unless a verified copy is already on disk.
fetch_mate() {
  local acc="$1" mate="$2" want_md5="$3" dest_dir="$4"
  local dest="${dest_dir}/${acc}_${mate}.fastq.gz" got_md5

  if [ -f "$dest" ] && [ "$FORCE" != "yes" ]; then
    got_md5="$(md5_of "$dest")"
    if [ "$got_md5" = "$want_md5" ]; then
      log "${acc}_${mate}.fastq.gz already present and verified"
      return 0
    fi
    warn "${acc}_${mate}.fastq.gz checksum mismatch; re-downloading"
  fi

  download_to "$(ena_fastq_url "$acc" "$mate")" "$dest"

  got_md5="$(md5_of "$dest")"
  [ "$got_md5" = "$want_md5" ] || die "MD5 mismatch for ${acc}_${mate}.fastq.gz
  expected ${want_md5}
  got      ${got_md5}"
  log "${acc}_${mate}.fastq.gz verified"
}

fetch_project() {
  local project="$1" dest_dir="${DATA_DIR}/reads/$1" found=0
  local proj acc md5_1 md5_2

  step "Reads: ${project}"
  mkdir -p "$dest_dir"

  while IFS=' ' read -r proj acc md5_1 md5_2; do
    [ "$proj" = "$project" ] || continue
    found=$((found + 1))
    fetch_mate "$acc" 1 "$md5_1" "$dest_dir"
    fetch_mate "$acc" 2 "$md5_2" "$dest_dir"
  done <<EOF
$RUN_MANIFEST
EOF

  [ "$found" -gt 0 ] || die "no runs in the manifest for project '$project'"
  log "${found} runs ready in ${dest_dir#"$REPO_ROOT"/}"
}

list_manifest() {
  printf 'Reference\n'
  printf '  %-14s %s\n' "$HXB2_ACCESSION" "HXB2, ${HXB2_BASES} bp"
  printf '\nRuns\n'
  local proj acc md5_1 md5_2
  while IFS=' ' read -r proj acc md5_1 md5_2; do
    printf '  %-14s %-12s %s %s\n' "$proj" "$acc" "$md5_1" "$md5_2"
  done <<EOF
$RUN_MANIFEST
EOF
}

projects_in_manifest() {
  local proj rest
  while IFS=' ' read -r proj rest; do
    printf '%s\n' "$proj"
  done <<EOF
$RUN_MANIFEST
EOF
}

usage() {
  sed -n '2,/^$/p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

# -------------------------------------------------------------------- main --

main() {
  local only_project="" reference_only="no"
  FORCE="no"

  while [ "$#" -gt 0 ]; do
    case "$1" in
      --project)        only_project="${2:-}"; [ -n "$only_project" ] || die "--project needs a value"; shift 2 ;;
      --project=*)      only_project="${1#*=}"; shift ;;
      --reference-only) reference_only="yes"; shift ;;
      --force)          FORCE="yes"; shift ;;
      --list)           list_manifest; return 0 ;;
      -h|--help)        usage; return 0 ;;
      *)                die "unknown argument: $1 (try --help)" ;;
    esac
  done

  require_cmd curl
  require_cmd tr
  require_cmd cut

  printf 'Fetching Ligase test data into %s\n' "$DATA_DIR"
  mkdir -p "$DATA_DIR"

  fetch_reference

  if [ "$reference_only" = "yes" ]; then
    step "Done (reference only)"
    return 0
  fi

  if [ -n "$only_project" ]; then
    projects_in_manifest | grep -qx "$only_project" \
      || die "unknown project: ${only_project}. Known: $(projects_in_manifest | sort -u | tr '\n' ' ')"
    fetch_project "$only_project"
  else
    local project
    for project in $(projects_in_manifest | sort -u); do
      fetch_project "$project"
    done
  fi

  step "Done"
  log "data/ is gitignored; re-run any time, verified files are not re-downloaded"
}

main "$@"
