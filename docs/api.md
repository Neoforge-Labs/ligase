# API reference

Generated from the package's own docstrings. The names here are everything
`ligase` exports; anything not listed is an implementation detail and may
change without notice — including `ligase._core`, the compiled module.

## Loading data

A loader reads what it can from the file and takes the rest as declarations.
What it inferred, and from where, is recorded on the result:
`dataset.dimension("reference").source` says `declared` or names the header it
came from.

::: ligase
    options:
      members:
        - reference
        - reads
        - samples
        - alignment
        - variants
        - intervals

## Declaring and running steps

::: ligase
    options:
      members:
        - step
        - sh
        - write
        - handle
        - Pipeline
        - Step

## Checking without running

::: ligase
    options:
      members:
        - check_step
        - dimension_errors

## Data types

::: ligase
    options:
      members:
        - Dataset
        - Reference
        - Reads
        - Samples
        - Alignment
        - Variants
        - Intervals
        - Contig
        - DimensionValue
        - StepOutput
        - Written

## Resistance interpretation

::: ligase
    options:
      members:
        - ResistanceReport

## Provenance and auditing

::: ligase
    options:
      members:
        - Methods
        - StepMethods
        - MethodsGap
        - Bundle
        - Verification
        - PinnedReference
        - AuditReport
        - StepAudit
        - Report
        - RunReport
        - StepRun

## Errors

Every failure mode has its own type, so a caller can tell a reference
mismatch from a naming mismatch without reading the message.

::: ligase
    options:
      members:
        - CheckError
        - DimensionError
        - ContigNamingError
        - ContigOrderError
        - UndeclaredDimensionError
        - StepTypeError
        - StepDefinitionError
        - LoadError
        - RunError
        - StepFailure
        - InputCheck
        - ResultCheck

## Version

::: ligase
    options:
      members:
        - version
