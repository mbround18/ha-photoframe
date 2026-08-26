"""An S3-backed media source, in the shape one actually produced.

Identifiers from a real deployment look like

    media-source://s3_media/01M0XQH8023TBCX4ED1RTD73MT|Taiwan 2026/

with a bar separating the bucket from the path, spaces in folder names, and a
trailing slash. Photos there listed fine in Home Assistant's own media browser
while the frame showed nothing, so this pins down the walk against that shape.
"""

from __future__ import annotations

from unittest.mock import patch

import pytest

from custom_components.photoframe_bridge.providers import Selection
from custom_components.photoframe_bridge.providers.media_source import MediaSourceProvider

_BROWSE = (
    "custom_components.photoframe_bridge.providers.media_source"
    ".media_source.async_browse_media"
)

ROOT = "media-source://"
S3 = "media-source://s3_media/"
BUCKET = "media-source://s3_media/01M0XQH8023TBCX4ED1RTD73MT|"
ALBUM = "media-source://s3_media/01M0XQH8023TBCX4ED1RTD73MT|Taiwan 2026/"


class _Node:
    def __init__(self, media_content_id, title, can_expand, children=None, media_class=None):
        self.media_content_id = media_content_id
        self.title = title
        self.can_expand = can_expand
        self.children = children or []
        if media_class is not None:
            self.media_class = media_class


#: Photos deliberately carry no media_class: a hand-rolled media source is
#: under no obligation to set one, and requiring it silently emptied the pool.
_TREE = {
    ROOT: _Node(ROOT, "root", True, [_Node(S3, "S3 Media", True)]),
    S3: _Node(S3, "S3 Media", True, [_Node(BUCKET, "my-bucket", True)]),
    BUCKET: _Node(BUCKET, "my-bucket", True, [_Node(ALBUM, "Taiwan 2026", True)]),
    ALBUM: _Node(ALBUM, "Taiwan 2026", True, [
        _Node(f"{ALBUM}IMG_0001.jpg", "IMG_0001.jpg", False),
        _Node(f"{ALBUM}IMG_0002.JPEG", "IMG_0002.JPEG", False),
        _Node(f"{ALBUM}clip.mp4", "clip.mp4", False),
    ]),
}


async def _fake_browse(hass, identifier):  # noqa: ARG001
    return _TREE[identifier]


@pytest.mark.asyncio
async def test_photos_are_found_in_a_deep_album_with_a_bar_and_spaces() -> None:
    selection = Selection(source_id="media_source", collection_ids=(ALBUM,))

    with patch(_BROWSE, side_effect=_fake_browse):
        provider = MediaSourceProvider(hass=object())
        found = [ref.item_id async for ref in provider.async_list_items(selection)]

    assert found == [f"{ALBUM}IMG_0001.jpg", f"{ALBUM}IMG_0002.JPEG"]


@pytest.mark.asyncio
async def test_selecting_the_bucket_reaches_photos_nested_below_it() -> None:
    """Picking a parent must include everything underneath, not just its files."""
    selection = Selection(source_id="media_source", collection_ids=(BUCKET,))

    with patch(_BROWSE, side_effect=_fake_browse):
        provider = MediaSourceProvider(hass=object())
        found = [ref.item_id async for ref in provider.async_list_items(selection)]

    assert len(found) == 2


@pytest.mark.asyncio
async def test_walking_down_reaches_the_album(hass=None) -> None:
    with patch(_BROWSE, side_effect=_fake_browse):
        provider = MediaSourceProvider(hass=object())
        level = await provider.async_browse(BUCKET)

    assert [c.title for c in level.children] == ["Taiwan 2026"]
    assert level.can_select is True


@pytest.mark.asyncio
async def test_only_jpg_jpeg_and_png_are_taken_from_a_source_that_sets_no_media_class() -> None:
    """Matches what the frame's own decoder is built with.

    A format the frame cannot read would work over the network and fail from
    the SD card, so the two have to agree.
    """
    mixed = "media-source://s3_media/mixed/"
    tree = {
        mixed: _Node(mixed, "mixed", True, [
            _Node(f"{mixed}a.jpg", "a.jpg", False),
            _Node(f"{mixed}b.JPEG", "b.JPEG", False),
            _Node(f"{mixed}c.png", "c.png", False),
            _Node(f"{mixed}d.gif", "d.gif", False),
            _Node(f"{mixed}e.webp", "e.webp", False),
            _Node(f"{mixed}f.bmp", "f.bmp", False),
            _Node(f"{mixed}g.heic", "g.heic", False),
            _Node(f"{mixed}h.mp4", "h.mp4", False),
        ]),
    }

    async def browse(hass, identifier):  # noqa: ARG001
        return tree[identifier]

    selection = Selection(source_id="media_source", collection_ids=(mixed,))
    with patch(_BROWSE, side_effect=browse):
        provider = MediaSourceProvider(hass=object())
        found = [ref.item_id.rsplit("/", 1)[-1] async for ref in provider.async_list_items(selection)]

    assert found == ["a.jpg", "b.JPEG", "c.png"]
