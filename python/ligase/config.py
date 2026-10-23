"""Where the store URL comes from, and the one thing it must never carry.

Two sources, in this order: the `LIGASE_STORE` environment variable, then a
`[store]` table in a `ligase.toml` beside the pipeline or above it. The
environment wins, because that is what makes a one-off run against another
bucket a prefix on a command rather than an edit to a file under version
control.

`tomllib` is in the standard library from Python 3.11, which is this
package's floor, so reading the file costs no dependency (D6).

**No credentials.** A `[store]` table may hold a `url` and nothing else. A
key that looks like a secret is an error naming the provider's own
credential chain, because `ligase.toml` is a file people commit, and the
one place a user would try to put a secret is the one place that has to
refuse it (D43).
"""

from __future__ import annotations

import os
import tomllib
from dataclasses import dataclass
from pathlib import Path

CONFIG_FILE = "ligase.toml"
STORE_VARIABLE = "LIGASE_STORE"

#: Keys under `[store]` that are refused by name rather than ignored.
#:
#: Ignoring one would be worse than refusing it: the run would work, the
#: secret would sit in version control, and nothing would have said so.
_SECRETS = (
    "access_key",
    "access_key_id",
    "account_key",
    "credentials",
    "password",
    "secret",
    "secret_access_key",
    "secret_key",
    "session_token",
    "token",
)


class ConfigError(Exception):
    """A `ligase.toml` that cannot be used as it is written."""


@dataclass(frozen=True)
class Store:
    """A store URL, and where it was read from."""

    url: str
    #: `LIGASE_STORE`, or the path of the `ligase.toml` that named it.
    source: str


def store(start: Path | None = None) -> Store | None:
    """The store this project is configured to use, if any.

    `start` is where to begin looking for `ligase.toml`: the pipeline's own
    directory, normally. The search walks upwards, so one file at the root
    of a repository serves every pipeline in it.
    """
    found = _from_file(start or Path.cwd())

    environment = os.environ.get(STORE_VARIABLE, "").strip()
    if environment:
        return Store(url=environment, source=STORE_VARIABLE)
    return found


def _from_file(start: Path) -> Store | None:
    """The `[store] url` of the nearest `ligase.toml`, searching upwards."""
    for directory in (start.resolve(), *start.resolve().parents):
        path = directory / CONFIG_FILE
        if not path.is_file():
            continue
        return _read(path)
    return None


def _read(path: Path) -> Store | None:
    try:
        document = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        raise ConfigError(f"{path}: {error}") from error

    table = document.get("store")
    if table is None:
        return None
    if not isinstance(table, dict):
        raise ConfigError(f"{path}: `store` should be a table, as `[store]`")

    for key in table:
        if key.lower() in _SECRETS or "secret" in key.lower():
            raise ConfigError(
                f"{path}: `[store]` may not hold `{key}`. Credentials come "
                f"from your provider's own chain -- the environment, a "
                f"profile, or an instance role -- and never from a Ligase "
                f"config file, which is a file people commit. If `aws s3 ls` "
                f"works, so does this."
            )

    unknown = sorted(set(table) - {"url"})
    if unknown:
        raise ConfigError(
            f"{path}: `[store]` takes `url` and nothing else; it has "
            f"{', '.join(f'`{key}`' for key in unknown)}"
        )

    url = table.get("url")
    if url is None:
        return None
    if not isinstance(url, str):
        raise ConfigError(f"{path}: `[store] url` should be a string")
    return Store(url=url, source=str(path))
