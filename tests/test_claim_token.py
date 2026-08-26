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
