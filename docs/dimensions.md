# Dimension Catalogue

Every biological type carries dimensions. Each dimension is inferred from the file where possible; otherwise the user declares it once and it propagates.

| Dimension | Meaning | FASTA | FASTQ | BAM/SAM | VCF | BED/GFF |
|---|---|---|---|---|---|---|
| organism | e.g. HIV-1, SARS-CoV-2, human | declared | declared | from reference | from header if present | declared |
| reference | build/accession, e.g. HXB2 (K03455), hg38 | sequence names + MD5 of contigs | n/a | @SQ lines (SN, LN, M5) | ##reference, ##contig | declared |
| contig_naming | `chr1` vs `1` style | from names | n/a | @SQ SN | ##contig IDs | from records |
| coordinates | 0-based half-open vs 1-based closed | n/a | n/a | 1-based (SAM text), 0-based (BAM binary) | 1-based | BED 0-based, GFF 1-based |
| sample_set | ordered sample IDs | n/a | from filename/declared | @RG SM | header sample columns | n/a |
| strand/pairing | single/paired, read layout | n/a | from file pairing | flags | n/a | n/a |
| quality_encoding | Phred+33 vs +64 | n/a | inferred from range | n/a | n/a | n/a |

## Rules

- Reference identity is determined by contig names + lengths + MD5 where available, never by filename alone.
- Contig naming mismatches are a distinct error from reference mismatches.
- Sample sets must match by identity and order wherever steps join samples.
- Unknown dimensions are allowed only if no downstream step requires them; otherwise `check()` fails with a request to declare them.
