"""Albums a frame can be pointed at, as browsed from Home Assistant's media sources.

This is the path the owner actually uses: pick "media_source" in the frame's
options, then tick the albums to show. It had no coverage until a frame in the
field could not offer a media source at all, so this pins down both halves --
that the provider is registered and therefore selectable, and that browsing
turns real media-source nodes into pickable albums.
"""

from __future__ import annotations

from unittest.mock import patch

import pytest

from custom_components.photoframe_bridge.providers import available_providers
from custom_components.photoframe_bridge.providers.media_source import MediaSourceProvider

_BROWSE = (
    "custom_components.photoframe_bridge.providers.media_source"
    ".media_source.async_browse_media"
)


class _Node:
    """The shape of a media-source node, reduced to what the provider reads."""

    def __init__(self, media_content_id: str, title: str, can_expand: bool, children=None) -> None:
        self.media_content_id = media_content_id
        self.title = title
        self.can_expand = can_expand
        self.children = children or []


def test_media_source_is_registered_and_therefore_selectable() -> None:
    """The options flow builds its dropdown from this registry.

    A frame that cannot offer "media_source" is running an integration build
    from before the provider existed -- which is exactly how this was first
    diagnosed.
    """
    assert "media_source" in available_providers()


@pytest.mark.asyncio
async def test_browsing_turns_media_sources_into_pickable_albums() -> None:
    root = _Node(
        "media-source://",
        "root",
        True,
        [_Node("media-source://media_source/local/", "My media", True)],
    )
    my_media = _Node(
        "media-source://media_source/local/",
        "My media",
        True,
        [
            _Node("media-source://media_source/local/Wedding", "Wedding", True),
            _Node("media-source://media_source/local/Trip", "Trip", True),
        ],
    )

    async def fake_browse(hass, identifier):  # noqa: ARG001
        return root if identifier == "media-source://" else my_media

    with patch(_BROWSE, side_effect=fake_browse):
        collections = await MediaSourceProvider(hass=object()).async_list_collections()

    titles = [c.title for c in collections]
    # The top level and its children both offered: people keep albums at either
    # depth, and the qualified name is what makes two "2024" folders tellable
    # apart in the picker.
    assert titles == ["My media", "My media / Wedding", "My media / Trip"]


@pytest.mark.asyncio
async def test_a_source_with_nothing_browsable_yields_no_albums() -> None:
    """Not an error: the flow skips the album step rather than showing an empty form."""
    empty = _Node("media-source://", "root", True, [])

    async def fake_browse(hass, identifier):  # noqa: ARG001
        return empty

    with patch(_BROWSE, side_effect=fake_browse):
        collections = await MediaSourceProvider(hass=object()).async_list_collections()

    assert collections == []
