"""Choosing folders: pick the source, then tick folders from one list.

The shape this is built for is real: an S3 media source holding
`01M0XQH8...|Taiwan 2026/` alongside a pile of background images nobody wants
on a photo frame. So nothing is ticked to begin with, and ticking a folder
takes everything inside it without also dragging in whatever sits loose at the
top of the bucket.
"""

from __future__ import annotations

from unittest.mock import patch

import pytest
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.photoframe_bridge.const import DOMAIN

_BROWSE = (
    "custom_components.photoframe_bridge.providers.media_source"
    ".media_source.async_browse_media"
)

_SETTINGS = {
    "source": "media_source",
    "rotation_interval_s": 300,
    "brightness": 80,
    "transition": "fade",
}

ROOT = "media-source://"
S3 = "media-source://s3_media/"
LOCAL = "media-source://media_source/local/"
BUCKET = "media-source://s3_media/01M0XQH8023TBCX4ED1RTD73MT|"
TAIWAN = "media-source://s3_media/01M0XQH8023TBCX4ED1RTD73MT|Taiwan 2026/"
DAY_ONE = "media-source://s3_media/01M0XQH8023TBCX4ED1RTD73MT|Taiwan 2026/Day 1/"


class _Node:
    def __init__(self, media_content_id, title, can_expand, children=None):
        self.media_content_id = media_content_id
        self.title = title
        self.can_expand = can_expand
        self.children = children or []


_TREE = {
    ROOT: _Node(ROOT, "root", True, [
        _Node(S3, "S3 Media", True),
        _Node(LOCAL, "My media", True),
    ]),
    S3: _Node(S3, "S3 Media", True, [_Node(BUCKET, "my-bucket", True)]),
    BUCKET: _Node(BUCKET, "my-bucket", True, [
        _Node(TAIWAN, "Taiwan 2026", True),
        # A loose photo at the top of the bucket: never a folder, never ticked.
        _Node(f"{BUCKET}wallpaper.jpg", "wallpaper.jpg", False),
    ]),
    TAIWAN: _Node(TAIWAN, "Taiwan 2026", True, [_Node(DAY_ONE, "Day 1", True)]),
    DAY_ONE: _Node(DAY_ONE, "Day 1", True, []),
}


async def _fake_browse(hass, identifier):  # noqa: ARG001
    return _TREE[identifier]


def _entry(hass, options=None) -> MockConfigEntry:
    entry = MockConfigEntry(
        domain=DOMAIN,
        title="Living Room Frame",
        data={"frame_id": "esp32p4-80f1b2d0b566", "frame_token": "tok"},
        options=options or {},
    )
    entry.add_to_hass(hass)
    return entry


def _field(result, name: str):
    return next(k for k in result["data_schema"].schema if str(k) == name)


def _choices(result, name: str) -> dict:
    """The offered values, whether the field is a dropdown or a multi-select."""
    validator = result["data_schema"].schema[_field(result, name)]
    return dict(getattr(validator, "container", None) or validator.options)


async def _to_folders(hass, entry):
    """Settings -> pick the S3 source -> the folder list."""
    result = await hass.config_entries.options.async_init(entry.entry_id)
    result = await hass.config_entries.options.async_configure(result["flow_id"], _SETTINGS)
    assert result["step_id"] == "source"
    return await hass.config_entries.options.async_configure(
        result["flow_id"], {"source_root": S3}
    )


@pytest.mark.asyncio
async def test_the_source_is_chosen_before_any_folders(hass) -> None:
    result = await hass.config_entries.options.async_init(_entry(hass).entry_id)
    with patch(_BROWSE, side_effect=_fake_browse):
        result = await hass.config_entries.options.async_configure(result["flow_id"], _SETTINGS)

    assert result["step_id"] == "source"
    assert _choices(result, "source_root") == {S3: "S3 Media", LOCAL: "My media"}


@pytest.mark.asyncio
async def test_folders_are_offered_as_one_indented_list(hass) -> None:
    with patch(_BROWSE, side_effect=_fake_browse):
        result = await _to_folders(hass, _entry(hass))

    assert result["step_id"] == "folders"
    choices = _choices(result, "collections")
    assert list(choices) == [BUCKET, TAIWAN, DAY_ONE]
    # Depth is legible in the label rather than lost in a flat list.
    assert choices[BUCKET].strip().endswith("my-bucket")
    assert "Taiwan 2026" in choices[TAIWAN]
    assert choices[TAIWAN].startswith(" ")
    assert len(choices[DAY_ONE]) - len(choices[DAY_ONE].lstrip(" ")) > len(
        choices[TAIWAN]
    ) - len(choices[TAIWAN].lstrip(" "))


@pytest.mark.asyncio
async def test_loose_files_are_never_offered_as_folders(hass) -> None:
    """A bucket full of wallpaper must not be tickable as a whole."""
    with patch(_BROWSE, side_effect=_fake_browse):
        result = await _to_folders(hass, _entry(hass))

    assert not any("wallpaper" in label for label in _choices(result, "collections").values())


@pytest.mark.asyncio
async def test_nothing_is_ticked_to_begin_with(hass) -> None:
    with patch(_BROWSE, side_effect=_fake_browse):
        result = await _to_folders(hass, _entry(hass))

    assert _field(result, "collections").default() == []


@pytest.mark.asyncio
async def test_what_was_chosen_before_comes_back_ticked(hass) -> None:
    entry = _entry(hass, {"collections": [TAIWAN]})
    with patch(_BROWSE, side_effect=_fake_browse):
        result = await _to_folders(hass, entry)

    assert _field(result, "collections").default() == [TAIWAN]


@pytest.mark.asyncio
async def test_ticking_a_folder_saves_it(hass) -> None:
    with patch(_BROWSE, side_effect=_fake_browse):
        result = await _to_folders(hass, _entry(hass))
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], {"collections": [TAIWAN]}
        )

    assert result["type"] == "create_entry"
    assert result["data"]["collections"] == [TAIWAN]


@pytest.mark.asyncio
async def test_ticking_a_parent_drops_its_children(hass) -> None:
    """A parent already includes them; keeping both lists the photos twice."""
    with patch(_BROWSE, side_effect=_fake_browse):
        result = await _to_folders(hass, _entry(hass))
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], {"collections": [TAIWAN, DAY_ONE]}
        )

    assert result["data"]["collections"] == [TAIWAN]


@pytest.mark.asyncio
async def test_choosing_nothing_shows_nothing_rather_than_everything(hass) -> None:
    with patch(_BROWSE, side_effect=_fake_browse):
        result = await _to_folders(hass, _entry(hass))
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], {"collections": []}
        )

    assert result["data"]["collections"] == []


@pytest.mark.asyncio
async def test_a_single_source_is_not_presented_as_a_choice(hass) -> None:
    """One option is not a decision; do not make the owner click it."""
    only_s3 = dict(_TREE)
    only_s3[ROOT] = _Node(ROOT, "root", True, [_Node(S3, "S3 Media", True)])

    async def browse(hass, identifier):  # noqa: ARG001
        return only_s3[identifier]

    result = await hass.config_entries.options.async_init(_entry(hass).entry_id)
    with patch(_BROWSE, side_effect=browse):
        result = await hass.config_entries.options.async_configure(result["flow_id"], _SETTINGS)

    assert result["step_id"] == "folders"
