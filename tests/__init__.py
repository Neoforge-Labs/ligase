"""Python-side tests for Ligase.

A package rather than a loose directory so that `tests.conftest` is the one
name its modules are importable under. Without it, the shared fixture paths
are reachable as both `conftest` and `tests.conftest`, which `mypy --strict`
rejects as the same file under two module names.
"""
