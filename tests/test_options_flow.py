"""Opening and submitting a frame's options.

This exists because the whole flow returned HTTP 500 in the field and no
album picker ever appeared. The cause was an exception handler that itself
raised: `except media_source.BrowseError` names an attribute that does not
exist, so the moment a browse actually failed, evaluating the handler threw
AttributeError, which escaped past every `except Exception` below it.

The lesson generalises, hence the last test here: a media source that cannot
be browsed is ordinary -- a local media folder that was never created is
enough -- and must never take the flow down with it.
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


def _entry(hass, options: dict | None = None) -> MockConfigEntry:
    entry = MockConfigEntry(
        domain=DOMAIN,
        title="Living Room Frame",
        data={"frame_id": "esp32p4-80f1b2d0b566", "frame_token": "tok"},
        options=options or {},
    )
    entry.add_to_hass(hass)
    return entry


@pytest.mark.asyncio
async def test_the_options_form_offers_every_registered_source(hass) -> None:
    result = await hass.config_entries.options.async_init(_entry(hass).entry_id)

    assert result["type"] == "form"
    assert result["step_id"] == "init"
    source_field = next(k for k in result["data_schema"].schema if str(k) == "source")
    # `In([...])` renders as the dropdown the owner picks from.
    assert "media_source" in str(result["data_schema"].schema[source_field])


@pytest.mark.asyncio
@pytest.mark.parametrize(
    "stored",
    [
        {},
        {"source": "sample"},
        # An entry created before this provider existed names a source we no
        # longer offer; that must not stop the form opening.
        {"source": "google_photos"},
        {"source": "media_source", "collections": ["media-source://x"]},
    ],
    ids=["fresh", "sample", "source-we-dropped", "already-configured"],
)
async def test_the_form_opens_whatever_an_older_build_left_behind(hass, stored: dict) -> None:
    result = await hass.config_entries.options.async_init(_entry(hass, stored).entry_id)
    assert result["type"] == "form"


@pytest.mark.asyncio
async def test_submitting_settings_leads_to_the_album_picker(hass) -> None:
    """media_source nests, so it goes to the walkable browser.

    See test_options_browse.py for the walk itself.
    """
    result = await hass.config_entries.options.async_init(_entry(hass).entry_id)
    result = await hass.config_entries.options.async_configure(
        result["flow_id"], _SETTINGS
    )

    assert result["type"] == "form"
    assert result["step_id"] == "browse"


@pytest.mark.asyncio
async def test_a_flat_source_still_gets_the_simple_picker(hass) -> None:
    """Providers whose collections do not nest are unaffected by the browser."""
    result = await hass.config_entries.options.async_init(_entry(hass).entry_id)
    result = await hass.config_entries.options.async_configure(
        result["flow_id"], {**_SETTINGS, "source": "sample"}
    )

    assert result["step_id"] == "collections"


@pytest.mark.asyncio
async def test_a_media_source_that_cannot_be_browsed_does_not_break_the_flow(hass) -> None:
    """The regression this file exists for.

    A local media folder that was never created raises BrowseError. Before the
    fix this surfaced as a 500 and the owner saw no picker at all.
    """
    from homeassistant.components.media_player.errors import BrowseError

    async def always_fails(hass, identifier):  # noqa: ARG001
        raise BrowseError("Media directory does not exist.")

    result = await hass.config_entries.options.async_init(_entry(hass).entry_id)
    with patch(_BROWSE, side_effect=always_fails):
        result = await hass.config_entries.options.async_configure(
            result["flow_id"], _SETTINGS
        )

    # Aborting with an explanation is fine; raising is not.
    assert result["type"] in ("form", "abort", "create_entry")
    if result["type"] == "abort":
        assert result["reason"] == "source_unavailable"
