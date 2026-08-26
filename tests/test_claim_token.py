"""Handing the frame the token its photo downloads need.

The frame cannot invent this token and every photo request without it is
refused with 401. A frame that is never told its token connects happily,
reports itself healthy, and shows nothing forever -- which is exactly how this
went unnoticed.
"""

from __future__ import annotations

import json

import pytest

from custom_components.photoframe_bridge.control_server import (
    ControlServer,
    FrameControlView,
    FrameSession,
)
from custom_components.photoframe_bridge.http_view import PhotoFrameTokenRegistry

FRAME_ID = "esp32p4-80f1b2d0b566"
TOKEN = "tok-abc123"


class _Socket:
    """Captures what the controller sends to the frame."""

    closed = False

    def __init__(self) -> None:
        self.sent: list[dict] = []

    async def send_str(self, data: str) -> None:
        self.sent.append(json.loads(data))


def _session(socket: _Socket) -> FrameSession:
    return FrameSession(frame_id=FRAME_ID, device_name="Living Room Frame", _socket=socket)


def test_a_registered_frame_can_be_told_its_token() -> None:
    tokens = PhotoFrameTokenRegistry()
    tokens.register(FRAME_ID, TOKEN)
    assert tokens.token_for(FRAME_ID) == TOKEN


def test_an_unknown_frame_has_no_token() -> None:
    assert PhotoFrameTokenRegistry().token_for(FRAME_ID) is None


def test_a_revoked_frame_stops_being_told_its_token() -> None:
    tokens = PhotoFrameTokenRegistry()
    tokens.register(FRAME_ID, TOKEN)
    tokens.revoke_frame(FRAME_ID)
    assert tokens.token_for(FRAME_ID) is None


@pytest.mark.asyncio
async def test_connecting_frame_is_claimed_and_given_its_token() -> None:
    tokens = PhotoFrameTokenRegistry()
    tokens.register(FRAME_ID, TOKEN)
    socket = _Socket()

    view = FrameControlView(ControlServer(), tokens)
    await view._async_claim(_session(socket))

    assert len(socket.sent) == 1
    claim = socket.sent[0]
    assert claim["type"] == "claim"
    assert claim["registration"]["claimed"] is True
    assert claim["registration"]["frame_token"] == TOKEN
    assert claim["registration"]["display_name"] == "Living Room Frame"


@pytest.mark.asyncio
async def test_a_frame_with_no_token_is_not_sent_a_useless_claim() -> None:
    """Better to log the problem than to claim a frame it cannot act on."""
    socket = _Socket()
    view = FrameControlView(ControlServer(), PhotoFrameTokenRegistry())
    await view._async_claim(_session(socket))
    assert socket.sent == []


def test_a_fresh_connection_has_not_been_sent_a_photo() -> None:
    """A frame that reboots comes back with an empty buffer.

    The push guard used to be "have we ever sent this frame a photo", which
    stayed true across a reboot and left the panel on "Waiting for photos"
    until the next rotation tick.
    """
    session = FrameSession(frame_id=FRAME_ID, device_name="Living Room Frame")
    assert session.photo_pushed is False


@pytest.mark.asyncio
async def test_a_tap_on_the_frame_asks_for_the_next_photo() -> None:
    """Tapping advances the frame's own buffer; when it runs dry it asks us."""
    server = ControlServer()
    session = _session(_Socket())
    server.register(session)

    asked: list[tuple[str, int]] = []
    server.add_photo_request_listener(lambda fid, wanted: asked.append((fid, wanted)))
    server.handle_status(
        FRAME_ID, {"type": "photo_requested", "wanted": 12, "cached": 3}
    )

    # How many it wants, so an empty card is filled in one batch rather than
    # one photo per round trip.
    assert asked == [(FRAME_ID, 12)]
    assert server.session(FRAME_ID).cached_photos == 3


@pytest.mark.asyncio
async def test_a_photo_request_with_nobody_listening_is_harmless() -> None:
    """Advisory by design: the frame keeps showing what it holds regardless."""
    server = ControlServer()
    server.register(_session(_Socket()))
    server.handle_status(FRAME_ID, {"type": "photo_requested"})


@pytest.mark.asyncio
async def test_unsubscribing_stops_the_callbacks() -> None:
    server = ControlServer()
    server.register(_session(_Socket()))
    asked: list[str] = []
    unsubscribe = server.add_photo_request_listener(
        lambda fid, wanted: asked.append(fid)  # noqa: ARG005
    )
    unsubscribe()
    server.handle_status(FRAME_ID, {"type": "photo_requested"})

    assert asked == []
