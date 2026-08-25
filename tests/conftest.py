"""Shared pytest fixtures for the photoframe_bridge integration tests."""

from __future__ import annotations

import pytest


@pytest.fixture(autouse=True)
def auto_enable_custom_integrations(enable_custom_integrations):
    """Load `custom_components/photoframe_bridge` in every test.

    `pytest-homeassistant-custom-component` refuses to load custom integrations
    unless a test opts in, so opt in globally.
    """
    yield
