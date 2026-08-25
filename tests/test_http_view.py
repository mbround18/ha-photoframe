"""Tests for the prepared-photo HTTP endpoint.

These exercise the parts that do not need a running Home Assistant: token
authentication and photo_id validation. The status-code contract they encode is
the one the firmware branches on (contracts/control-protocol.md).
"""

from __future__ import annotations

import pytest

from custom_components.photoframe_bridge.http_view import (
    PhotoFrameTokenRegistry,
    _is_safe_photo_id,
    photo_path,
)


def test_photo_path_is_relative_to_home_assistant() -> None:
    """The frame must never be handed an absolute or provider URL (FR-043)."""
    path = photo_path("abc123")
    assert path.startswith("/api/photoframe_bridge/photo/")
    assert "://" not in path


def test_token_registry_resolves_the_owning_frame() -> None:
    registry = PhotoFrameTokenRegistry()
    registry.register("frame-1", "tok-1")

    assert registry.frame_for("tok-1") == "frame-1"
    assert registry.frame_for("nope") is None


def test_revoking_a_frame_invalidates_its_tokens() -> None:
    """Removing a config entry must stop that frame fetching photos (FR-039)."""
    registry = PhotoFrameTokenRegistry()
    registry.register("frame-1", "tok-1")
    registry.register("frame-2", "tok-2")

    registry.revoke_frame("frame-1")

    assert registry.frame_for("tok-1") is None
    assert registry.frame_for("tok-2") == "frame-2"


def test_rotating_a_token_does_not_leave_the_old_one_valid() -> None:
    registry = PhotoFrameTokenRegistry()
    registry.register("frame-1", "old")
    registry.revoke_frame("frame-1")
    registry.register("frame-1", "new")

    assert registry.frame_for("old") is None
    assert registry.frame_for("new") == "frame-1"


@pytest.mark.parametrize(
    "photo_id",
    ["../../../etc/passwd", "abc/def", "abc.jpg", "ABC123", "abc-123", "", "x" * 65, "zz"],
)
def test_unsafe_photo_ids_are_rejected(photo_id: str) -> None:
    """photo_id is interpolated into a filename."""
    assert _is_safe_photo_id(photo_id) is False


def test_valid_photo_id_accepted() -> None:
    assert _is_safe_photo_id("0123456789abcdef") is True
