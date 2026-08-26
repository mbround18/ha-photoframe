"""The bundled photos stand in; they do not take over.

A photo source is not always ready when this integration starts -- another
integration may still be setting up, a NAS may still be waking. Falling back
permanently meant the owner saw stock photos until they restarted Home
Assistant, with their own album correctly configured the whole time.
"""

from __future__ import annotations

from collections.abc import AsyncIterator

import pytest

from custom_components.photoframe_bridge.coordinator import FrameCoordinator
from custom_components.photoframe_bridge.providers import (
    Capabilities,
    Collection,
    PhotoProvider,
    PhotoRef,
    Selection,
)


class _FlakySource(PhotoProvider):
    """Empty until `ready` is set, like a source that is still starting."""

    key = "flaky"
    capabilities = Capabilities(supports_collections=True)

    def __init__(self) -> None:
        self.ready = False

    async def async_list_collections(self) -> list[Collection]:
        return [Collection(collection_id="album", title="Album")]

    async def async_list_items(self, selection: Selection) -> AsyncIterator[PhotoRef]:
        if not self.ready:
            return
        for n in range(7):
            yield PhotoRef(item_id=f"photo-{n}", source_id=self.key)

    async def async_fetch_bytes(self, ref: PhotoRef, *, want) -> bytes:
        return b""


def _coordinator(provider: _FlakySource) -> FrameCoordinator:
    """A coordinator with only what refresh_pool touches."""
    coordinator = FrameCoordinator.__new__(FrameCoordinator)
    coordinator.frame_id = "esp32p4-80f1b2d0b566"
    coordinator.provider = provider
    coordinator.selection = Selection(source_id=provider.key, collection_ids=("album",))
    coordinator.last_error = None
    coordinator.using_fallback = False
    coordinator._providers = {provider.key: provider}

    from custom_components.photoframe_bridge.coordinator import PhotoPool

    coordinator.pool = PhotoPool()
    return coordinator


@pytest.mark.asyncio
async def test_a_source_with_nothing_yet_falls_back_to_the_bundled_photos() -> None:
    coordinator = _coordinator(_FlakySource())
    await coordinator.async_refresh_pool()

    assert coordinator.using_fallback is True
    assert coordinator.pool.items, "the panel must not go blank"
    assert all(ref.source_id == "sample" for ref in coordinator.pool.items)


@pytest.mark.asyncio
async def test_the_real_source_takes_over_once_it_is_ready() -> None:
    provider = _FlakySource()
    coordinator = _coordinator(provider)
    await coordinator.async_refresh_pool()
    assert coordinator.using_fallback is True

    provider.ready = True
    await coordinator.async_refresh_pool()

    assert coordinator.using_fallback is False
    assert len(coordinator.pool.items) == 7
    assert all(ref.source_id == "flaky" for ref in coordinator.pool.items)


@pytest.mark.asyncio
async def test_a_photo_is_fetched_from_whoever_produced_it() -> None:
    """While standing in, the pool holds refs from two different sources."""
    provider = _FlakySource()
    coordinator = _coordinator(provider)
    await coordinator.async_refresh_pool()

    sample_ref = coordinator.pool.items[0]
    assert coordinator._provider_for(sample_ref).key == "sample"
    assert coordinator._provider_for(PhotoRef(item_id="x", source_id="flaky")) is provider
