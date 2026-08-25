"""Tests for adopting a frame.

The normaliser matters more than it looks: people retype this ID from a screen
across the room, and rejecting "80:F1:B2:D0:B5:66" because of the colons would
be a bad first five minutes with a gift.
"""

from __future__ import annotations

import pytest

from custom_components.photoframe_bridge.config_flow import normalise_frame_id

CANONICAL = "esp32p4-80f1b2d0b566"


@pytest.mark.parametrize(
    "typed",
    [
        "esp32p4-80f1b2d0b566",
        "ESP32P4-80F1B2D0B566",
        "  esp32p4-80f1b2d0b566  ",
        "80f1b2d0b566",
        "80F1B2D0B566",
        "80:f1:b2:d0:b5:66",
        "80-F1-B2-D0-B5-66",
        "80f1.b2d0.b566",
        "esp32p4-80:f1:b2:d0:b5:66",
        "80 f1 b2 d0 b5 66",
    ],
)
def test_reasonable_variations_all_normalise(typed: str) -> None:
    assert normalise_frame_id(typed) == CANONICAL


@pytest.mark.parametrize(
    "typed",
    [
        "",
        "   ",
        "not-a-frame",
        "80f1b2d0b5",        # too short
        "80f1b2d0b56666",    # too long
        "80f1b2d0b56g",      # not hex
        "esp32p4-",
    ],
)
def test_nonsense_is_rejected(typed: str) -> None:
    assert normalise_frame_id(typed) is None


def test_normalisation_is_idempotent() -> None:
    once = normalise_frame_id("80:F1:B2:D0:B5:66")
    assert once is not None
    assert normalise_frame_id(once) == once
