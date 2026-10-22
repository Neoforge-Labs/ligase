"""Crossing a dimension, in the one way the engine allows.

A dimension check refuses a join. For most dimensions there is nothing past
the refusal: two files about different organisms are about different
organisms. For a few there is a real operation, and this is where those
live.

    ensembl = lg.map_identifiers(
        genes,
        table=lg.entity_set(
            "hgnc_to_ensembl.tsv",
            organism="Homo sapiens",
            identifier_namespace="HGNC symbol",
            release="HGNC 2026-09",
        ),
        column="ensembl_gene_id",
        to="Ensembl gene",
    )

## Why a registry rather than a function

The engine keeps these as a table, not as three functions somebody calls.
Each entry says which axis it crosses, which kinds it crosses it for, and
what the user must supply, so that a search over them can answer "what gets
me from here to there?" — which is what
[D5](https://ligase.neoforgelabs.tech/project/decisions/)'s conversion graph
is for, and what v0.3's automatic fixes will be.

v0.2 registers one conversion. `liftover` (reference build to reference
build) and `rename_contigs` (contig naming to contig naming) are the next
two entries, and the registry is shaped so that they are rows rather than
redesigns: both cross an axis, and both will read their destination from the
same `to=` parameter this one does.

## Why the mapping table is typed

`table=` takes an `EntitySet`, not a path. Two things follow from that, and
both are the point.

Its contents join the cache key, so editing the table reruns the mapping —
which a path would not do, because a plain parameter is hashed by its value
and a path's value does not change when the file behind it does.

And its own `identifier_namespace` is checked against the list being mapped.
A table of Entrez IDs handed to a list of HGNC symbols is a
`DimensionError` before anything runs, rather than a mapping that quietly
matches nothing. The table's identifiers are the *source* side of the
mapping, so they are in the same space as the list; the targets live in an
attribute column, named by `column=`.

## What a conversion is not

It is not a rename. Mapping between identifier spaces is a lookup *with
failures*, and what happens to those failures is the whole argument of
[D38](https://ligase.neoforgelabs.tech/project/decisions/): an identifier
that does not cross is named, never dropped in silence and never carried
over unchanged. Carrying it over would put a name from the old space into a
set that declares the new one, which is the implicit rename the dimension
exists to refuse.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from . import _core
from .graph import step
from .recipes import write
from .types import EntitySet

if TYPE_CHECKING:
    from pathlib import Path

    from .recipes import Noticed

__all__ = ["map_identifiers"]


@step(tool="ligase", conversion="map_identifiers")
def map_identifiers(
    entities: EntitySet,
    table: EntitySet,
    *,
    column: str,
    to: str,
    id_column: str = "id",
    table_id_column: str = "id",
) -> EntitySet:
    """Rewrite a gene list's identifiers through a mapping table.

    The output declares the space named by `to`, and carries every other
    dimension of `entities` — its organism, its subject, its release —
    because a conversion crosses one axis and is not a new list.

    Args:
        entities: The list to convert.
        table: The mapping, as an `EntitySet` whose own identifiers are the
            *source* side. Typed rather than a path so that its contents
            join the cache key and its space is checked against
            `entities`: a table of Entrez IDs handed a list of HGNC symbols
            is refused before anything runs.
        column: The column of `table` holding the identifiers to convert
            *to*. Naming the table's own identifier column is an error — it
            would map every identifier to itself and then declare a space
            the identifiers are not in.
        to: The space the output is in, such as `Ensembl gene`. Declared
            rather than inferred: what the output *is* must not depend on
            which identifiers happened to cross.
        id_column: The column of `entities` holding its identifiers.
        table_id_column: The column of `table` holding the source
            identifiers.

    Returns:
        A recipe, typed as the gene list the step produces.

    Note:
        Two kinds of identifier do not cross, and both are named in the
        run report, provenance, a bundle and the methods section rather
        than being dropped quietly. One the table has no row for is usually
        a symbol renamed or retired since the table was downloaded. One the
        table sends to several targets — ordinary for symbol-to-gene
        mappings — is left out because emitting a row for each would make
        the output larger than the input without saying so, and choosing
        one would be a guess about which the analysis meant.

        So the output is smaller than the input by exactly the identifiers
        the notes name.
    """

    def produce(out: Path) -> list[Noticed]:
        """Map and write, once the steps above have produced.

        Returns every identifier that did not cross, which the engine then
        says. Dropped here, they would be genes missing from the rest of
        the analysis with nothing to show why.
        """
        return _core.map_identifiers(
            entities.file,
            id_column,
            table.file,
            table_id_column,
            column,
            str(out),
        )

    return write(produce)
