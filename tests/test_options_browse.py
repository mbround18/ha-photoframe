"""Walking down a nested photo source to reach a folder.

The shape this exists for is real: an S3 media source holding
`media / taiwan / Taiwan 2026`. A flat list of the top two levels cannot reach
that album at all, so the owner descends one level at a time and marks the
folders to show.
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
TAIWAN = "media-source://s3_media/media/taiwan/"
TAIWAN_2026 = "media-source://s3_media/media/taiwan/Taiwan 2026/"


class _Node:
    def __init__(self, media_content_id, title, can_expand, children=None):
        self.media_content_id = media_content_id
        self.title = title
        self.can_expand = can_expand
        self.children = children or []


#: An S3 media source three levels deep, plus a sibling source so the root has
#: something to choose between.
_TREE = {
    ROOT: _Node(ROOT, "root", True, [
        _Node(S3, "S3 Media", True),
        _Node("media-source://media_source/local/", "My media", True),
    ]),
    S3: _Node(S3, "S3 Media", True, [_Node(TAIWAN, "taiwan", True)]),
    TAIWAN: _Node(TAIWAN, "taiwan", True, [_Node(TAIWAN_2026, "Taiwan 2026", True)]),
    TAIWAN_2026: _Node(TAIWAN_2026, "Taiwan 2026", True, []),
}


async def _fake_browse(hass, identifier):  # noqa: ARG001
    return _TREE[identifier]


def _entry(hass) -> MockConfigEntry:
    entry = MockConfigEntry(
        domain=DOMAIN,
        title="Living Room Frame",
        data={"frame_id": "esp32p4-80f1b2d0b566", "frame_token": "tok"},
        options={},
    )
    entry.add_to_hass(hass)
    return entry


def _labels(result) -> dict[str, str]:
    """The dropdown, as {value: label}."""
    field = next(k for k in result["data_schema"].schema if str(k) == "choice")
    return dict(result["data_schema"].schema[field].container)


def _value_containing(result, text: str) -> str:
    for value, label in _labels(result).items():
        if text in label:
            return value
    raise AssertionError(f"no option mentioning {text!r} in {list(_labels(result).values())}")


@pytest.mark.asyncio
async def test_the_owner_can_walk_three_levels_down_and_pick_an_album(hass) -> None:
    result = await hass.config_entries.options.async_init(_entry(hass).entry_id)

    with patch(_BROWSE, side_effect=_fake_browse):
        # Settings first, which lands us at the top of the tree.
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], _SETTINGS
        )
        assert result["step_id"] == "browse"
        assert result["description_placeholders"]["location"] == "All photo sources"

        # Down through S3 Media -> taiwan -> Taiwan 2026.
        for expected in ("S3 Media", "taiwan", "Taiwan 2026"):
            result = await hass.config_entries.options.async_configure(
                result["flow_id"], {"choice": _value_containing(result, expected)}
            )
            assert result["step_id"] == "browse"
        assert result["description_placeholders"]["location"] == "Taiwan 2026"

        # Mark it, then finish.
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], {"choice": _value_containing(result, "Show photos from")}
        )
        assert "Taiwan 2026" in result["description_placeholders"]["chosen"]

        result = await hass.config_entries.options.async_configure(
            result["flow_id"], {"choice": _value_containing(result, "Done")}
        )

    assert result["type"] == "create_entry"
    assert result["data"]["collections"] == [TAIWAN_2026]


@pytest.mark.asyncio
async def test_back_climbs_out_of_a_folder(hass) -> None:
    result = await hass.config_entries.options.async_init(_entry(hass).entry_id)

    with patch(_BROWSE, side_effect=_fake_browse):
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], _SETTINGS
        )
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], {"choice": _value_containing(result, "S3 Media")}
        )
        assert result["description_placeholders"]["location"] == "S3 Media"

        result = await hass.config_entries.options.async_configure(
            result["flow_id"], {"choice": _value_containing(result, "Back")}
        )

    assert result["description_placeholders"]["location"] == "All photo sources"


@pytest.mark.asyncio
async def test_more_than_one_folder_can_be_shown(hass) -> None:
    result = await hass.config_entries.options.async_init(_entry(hass).entry_id)

    with patch(_BROWSE, side_effect=_fake_browse):
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], _SETTINGS
        )
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], {"choice": _value_containing(result, "S3 Media")}
        )
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], {"choice": _value_containing(result, "Show photos from")}
        )
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], {"choice": _value_containing(result, "taiwan")}
        )
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], {"choice": _value_containing(result, "Show photos from")}
        )
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], {"choice": _value_containing(result, "Done")}
        )

    assert result["data"]["collections"] == [S3, TAIWAN]


@pytest.mark.asyncio
async def test_picking_a_folder_twice_unpicks_it(hass) -> None:
    """The dropdown is the only control, so it has to be able to undo itself."""
    result = await hass.config_entries.options.async_init(_entry(hass).entry_id)

    with patch(_BROWSE, side_effect=_fake_browse):
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], _SETTINGS
        )
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], {"choice": _value_containing(result, "S3 Media")}
        )
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], {"choice": _value_containing(result, "Show photos from")}
        )
        assert "S3 Media" in result["description_placeholders"]["chosen"]

        result = await hass.config_entries.options.async_configure(
            result["flow_id"], {"choice": _value_containing(result, "Remove")}
        )

    assert result["description_placeholders"]["chosen"] == "nothing yet"


@pytest.mark.asyncio
async def test_selections_can_be_cleared_without_walking_back_to_each_one(hass) -> None:
    """Un-picking folders one at a time means finding each of them again.

    That is unreasonable once a few are chosen, or once the owner has
    forgotten which ones they were -- which is the usual reason for wanting to
    start over.
    """
    result = await hass.config_entries.options.async_init(_entry(hass).entry_id)

    with patch(_BROWSE, side_effect=_fake_browse):
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], _SETTINGS
        )
        # Pick two folders at different depths.
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], {"choice": _value_containing(result, "S3 Media")}
        )
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], {"choice": _value_containing(result, "Show photos from")}
        )
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], {"choice": _value_containing(result, "taiwan")}
        )
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], {"choice": _value_containing(result, "Show photos from")}
        )
        assert "S3 Media" in result["description_placeholders"]["chosen"]

        result = await hass.config_entries.options.async_configure(
            result["flow_id"], {"choice": _value_containing(result, "Clear all")}
        )

    assert result["description_placeholders"]["chosen"] == "nothing yet"
    # And the way out is still offered, rather than stranding them.
    assert any("Back" in label for label in _labels(result).values())


@pytest.mark.asyncio
async def test_clearing_is_not_offered_when_nothing_is_chosen(hass) -> None:
    result = await hass.config_entries.options.async_init(_entry(hass).entry_id)

    with patch(_BROWSE, side_effect=_fake_browse):
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], _SETTINGS
        )

    assert not any("Clear all" in label for label in _labels(result).values())
