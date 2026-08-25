"""One shared suite every provider must satisfy.

Parametrised over the registry, so a new provider inherits the whole suite the
moment it registers - which is the point of the seam.
"""

from __future__ import annotations

import pytest

from custom_components.photoframe_bridge.providers import (
    Capabilities,
    PhotoProvider,
    Selection,
    available_providers,
)

PROVIDERS = sorted(available_providers().items())
PANEL = (1280, 800)


@pytest.fixture(params=[cls for _key, cls in PROVIDERS], ids=[k for k, _ in PROVIDERS])
def provider(request) -> PhotoProvider:
    return request.param()


def test_registry_is_not_empty() -> None:
    assert PROVIDERS, "no providers registered; the suite would pass vacuously"


def test_key_is_stable_and_matches_its_module(provider: PhotoProvider) -> None:
    assert provider.key
    assert provider.key.islower()
    assert provider.__class__.__module__.endswith(provider.key)


def test_capabilities_are_internally_consistent(provider: PhotoProvider) -> None:
    caps = provider.capabilities
    assert isinstance(caps, Capabilities)
    if caps.supports_live_collections:
        assert caps.supports_collections, (
            "a provider whose collections update live must have collections"
        )


async def test_list_collections_returns_a_list(provider: PhotoProvider) -> None:
    collections = await provider.async_list_collections()
    assert isinstance(collections, list)
    if not provider.capabilities.supports_collections:
        assert collections == [], "sources without collections must return [], not raise"


async def test_items_are_yielded_lazily(provider: PhotoProvider) -> None:
    """Must not materialise the whole source; SC-011 allows 20,000 photos."""
    selection = Selection(source_id=provider.key)
    iterator = provider.async_list_items(selection)
    assert hasattr(iterator, "__anext__"), "async_list_items must be an async iterator"


async def test_every_item_declares_an_image_mime_type(provider: PhotoProvider) -> None:
    selection = Selection(source_id=provider.key)
    seen = 0
    async for ref in provider.async_list_items(selection):
        assert ref.item_id
        assert ref.source_id
        assert "/" in ref.mime_type
        seen += 1
        if seen >= 5:
            break


async def test_fetch_returns_bytes(provider: PhotoProvider) -> None:
    selection = Selection(source_id=provider.key)
    async for ref in provider.async_list_items(selection):
        data = await provider.async_fetch_bytes(ref, want=PANEL)
        assert isinstance(data, bytes) and data
        return
    pytest.skip("provider exposes no items in this environment")
