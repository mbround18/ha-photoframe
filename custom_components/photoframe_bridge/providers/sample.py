"""Bundled sample photos.

Serves two purposes. It is what an adopted-but-unconfigured frame shows, so a
frame is never blank while its owner is still deciding where photos come from
(research.md R1). And it is the provider the conformance suite can always run
against, with no credentials and no network.

It also proves the seam: adding this provider touched only `providers/`.
"""

from __future__ import annotations

import asyncio

from collections.abc import AsyncIterator
from pathlib import Path
from typing import ClassVar

from . import (
    Capabilities,
    Collection,
    ItemUnavailable,
    PhotoProvider,
    PhotoRef,
    Selection,
    register_provider,
)

IMAGES_DIR = Path(__file__).parent / "images"
SAMPLE_COLLECTION_ID = "bundled"


@register_provider
class SampleProvider(PhotoProvider):
    key: ClassVar[str] = "sample"
    capabilities: ClassVar[Capabilities] = Capabilities(
        supports_collections=True,
        supports_individual_selection=False,
        # The bundled set never changes, so there is nothing to re-poll for.
        supports_live_collections=False,
        selection_expires=False,
        requires_auth=False,
    )

    def __init__(self, source_id: str = "sample") -> None:
        self._source_id = source_id
        self._cached: list[Path] | None = None

    @staticmethod
    def _scan() -> list[Path]:
        if not IMAGES_DIR.is_dir():
            return []
        return sorted(p for p in IMAGES_DIR.iterdir() if p.suffix.lower() == ".jpg")

    async def _async_files(self) -> list[Path]:
        """The bundled photos, listed off the event loop.

        Touching the filesystem inside the event loop stalls everything else
        Home Assistant is doing, and it warns about it. Cached because these
        photos ship inside the integration and cannot change while it runs.
        """
        if self._cached is None:
            self._cached = await asyncio.to_thread(self._scan)
        return self._cached

    async def async_list_collections(self) -> list[Collection]:
        return [
            Collection(
                collection_id=SAMPLE_COLLECTION_ID,
                title="Sample photos",
                item_count=len(await self._async_files()),
            )
        ]

    async def async_list_items(self, selection: Selection) -> AsyncIterator[PhotoRef]:
        for path in await self._async_files():
            yield PhotoRef(
                item_id=path.stem,
                source_id=self._source_id,
                mime_type="image/jpeg",
            )

    async def async_fetch_bytes(self, ref: PhotoRef, *, want: tuple[int, int]) -> bytes:
        path = IMAGES_DIR / f"{ref.item_id}.jpg"
        try:
            return await asyncio.to_thread(path.read_bytes)
        except OSError as err:
            raise ItemUnavailable(f"sample photo {ref.item_id} unreadable: {err}") from err
