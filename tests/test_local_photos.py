"""The frame running from photos on its own SD card.

When the owner copies photos into the card's `media/` folder, the frame shows
those and ignores everything this integration sends. That is intended, so the
integration has to be able to say so -- a frame that looks like it stopped
accepting photos is indistinguishable from a broken one otherwise.
"""

from __future__ import annotations

from types import SimpleNamespace

import pytest

from custom_components.photoframe_bridge.control_server import FrameSession


class _Server:
    """Just enough ControlServer to answer `session()`."""

    def __init__(self, session: FrameSession | None) -> None:
        self._session = session

    def session(self, frame_id: str) -> FrameSession | None:  # noqa: ARG002
        return self._session


def _coordinator(session: FrameSession | None):
    """A bare object carrying the two properties under test.

    The real coordinator needs a HomeAssistant instance and a provider; these
    properties depend on neither, so binding them to a stub keeps the test
    about the behaviour rather than about construction.
    """
    from custom_components.photoframe_bridge.coordinator import FrameCoordinator

    stub = SimpleNamespace(server=_Server(session), frame_id="p4-test")
    stub.running_from_sd_card = FrameCoordinator.running_from_sd_card.fget(stub)
    stub.local_photos_notice = FrameCoordinator.local_photos_notice.fget(stub)
    return stub


def _session(**kwargs) -> FrameSession:
    return FrameSession(frame_id="p4-test", device_name="Test Frame", **kwargs)


def test_a_frame_running_from_its_card_is_recognised() -> None:
    session = _session(photo_source="SD card (12 photo(s) on card)")
    assert _coordinator(session).running_from_sd_card is True


def test_a_frame_taking_photos_from_us_is_not_flagged() -> None:
    session = _session(photo_source="Home Assistant")
    coordinator = _coordinator(session)
    assert coordinator.running_from_sd_card is False
    assert coordinator.local_photos_notice is None


def test_a_frame_that_has_not_reported_yet_is_not_assumed_local() -> None:
    """An old firmware sends no photo_source at all; that is not 'local'."""
    coordinator = _coordinator(_session())
    assert coordinator.running_from_sd_card is False
    assert coordinator.local_photos_notice is None


def test_an_absent_frame_is_not_assumed_local() -> None:
    assert _coordinator(None).running_from_sd_card is False


def test_the_notice_says_how_to_hand_control_back() -> None:
    """The whole point of the notice: it must be actionable."""
    session = _session(photo_source="SD card (12 photo(s) on card)")
    notice = _coordinator(session).local_photos_notice
    assert notice is not None
    assert "media" in notice
    assert "restart" in notice.lower()
    assert "12 photo(s) on card" in notice


@pytest.mark.parametrize(
    "reported",
    ["SD card (3 photo(s) on card)", "SD card (no photos on card)"],
)
def test_any_sd_card_source_counts_as_local(reported: str) -> None:
    assert _coordinator(_session(photo_source=reported)).running_from_sd_card is True
