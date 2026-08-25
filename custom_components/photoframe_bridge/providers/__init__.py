"""The pluggable photo-source seam (Constitution Principle III).

Adding a photo source means adding one module in this package and registering
it. Nothing outside `providers/` may reference a provider by name -- that rule
is enforced by tests/test_provider_isolation.py, which is what keeps
"pluggable" a property rather than an intention.

Providers differ in ways the coordinator must respond to, so they declare those
differences as capabilities rather than the coordinator branching on identity.
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from collections.abc import AsyncIterator
from dataclasses import dataclass, field
from typing import ClassVar


@dataclass(frozen=True, slots=True)
class Capabilities:
    """What a provider can and cannot do."""

    supports_collections: bool = False
    supports_individual_selection: bool = False
    #: Re-resolving a selection can surface photos added since it was made.
    #: False for sources that freeze a selection at pick time, such as the
    #: Google Photos Picker (research.md R2).
    supports_live_collections: bool = False
    #: Selections have a deadline and eventually need re-picking.
    selection_expires: bool = False
    requires_auth: bool = False


@dataclass(frozen=True, slots=True)
class Collection:
    """A named grouping inside a source: album, bucket, folder."""

    collection_id: str
    title: str
    item_count: int | None = None


@dataclass(frozen=True, slots=True)
class PhotoRef:
    """A photo that exists in a source, not yet fetched."""

    item_id: str
    source_id: str
    mime_type: str = "image/jpeg"
    created_at: str | None = None
    width: int | None = None
    height: int | None = None


@dataclass(frozen=True, slots=True)
class Selection:
    """What the owner chose for one frame."""

    source_id: str
    collection_ids: tuple[str, ...] = field(default_factory=tuple)
    item_ids: tuple[str, ...] = field(default_factory=tuple)


# -- errors ---------------------------------------------------------------
#
# One hierarchy so the coordinator handles every provider the same way. A
# provider must never let a bare exception cross the seam.


class ProviderError(Exception):
    """Base for everything raised across the provider seam."""


class ItemUnavailable(ProviderError):
    """This one photo is gone or unreadable. Skip it, keep going (FR-029)."""


class ItemUnsupported(ProviderError):
    """Not a displayable image, e.g. a video. Drop from the pool (FR-018)."""


class SourceUnavailable(ProviderError):
    """The source is down. Retry with backoff; the frame is unaffected."""


class NeedsReauth(ProviderError):
    """The credential expired or was revoked. Start a repair flow."""


class SelectionExpired(ProviderError):
    """The selection lapsed and must be re-made (research.md R2)."""


class PhotoProvider(ABC):
    """One place photos come from."""

    key: ClassVar[str]
    capabilities: ClassVar[Capabilities]

    @abstractmethod
    async def async_list_collections(self) -> list[Collection]:
        """Return the collections available, or [] if this source has none."""

    @abstractmethod
    def async_list_items(self, selection: Selection) -> AsyncIterator[PhotoRef]:
        """Yield photo refs lazily.

        Must not materialise a 20,000-photo source in memory (SC-011).
        """

    @abstractmethod
    async def async_fetch_bytes(self, ref: PhotoRef, *, want: tuple[int, int]) -> bytes:
        """Return original bytes for one photo, at or above `want` if the
        source can size server-side."""


PROVIDERS: dict[str, type[PhotoProvider]] = {}


def register_provider(cls: type[PhotoProvider]) -> type[PhotoProvider]:
    """Class decorator that makes a provider selectable."""
    if not getattr(cls, "key", None):
        raise ValueError(f"{cls.__name__} must define a non-empty `key`")
    if cls.key in PROVIDERS and PROVIDERS[cls.key] is not cls:
        raise ValueError(f"provider key {cls.key!r} is already registered")
    PROVIDERS[cls.key] = cls
    return cls


def available_providers() -> dict[str, type[PhotoProvider]]:
    """Import the built-in providers and return the registry."""
    from . import media_source, sample  # noqa: F401  (import registers them)

    return dict(PROVIDERS)
