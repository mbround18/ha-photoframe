"""Constants for the PhotoFrame Bridge integration."""

from __future__ import annotations

from typing import Final

DOMAIN: Final = "photoframe_bridge"

# Config entry keys
CONF_FRAME_ID: Final = "frame_id"
CONF_FRAME_TOKEN: Final = "frame_token"
CONF_PANEL_WIDTH: Final = "panel_width"
CONF_PANEL_HEIGHT: Final = "panel_height"

# Options keys (PresentationSettings in data-model.md)
CONF_ROTATION_INTERVAL: Final = "rotation_interval_s"
CONF_BRIGHTNESS: Final = "brightness"
CONF_ORDER: Final = "order"
CONF_TRANSITION: Final = "transition"
CONF_SOURCE: Final = "source"

DEFAULT_ROTATION_INTERVAL: Final = 300
DEFAULT_BRIGHTNESS: Final = 80
DEFAULT_ORDER: Final = "shuffle"
DEFAULT_TRANSITION: Final = "fade"

# The frame's panel until it tells us otherwise on connect.
DEFAULT_PANEL_WIDTH: Final = 1280
DEFAULT_PANEL_HEIGHT: Final = 800

# How many prepared photos to keep on disk before evicting the least used.
DEFAULT_MAX_PREPARED_PHOTOS: Final = 2000

# Services
SERVICE_DISPLAY_PHOTO: Final = "display_photo"
SERVICE_SEND_COMMAND: Final = "send_command"

# Signals
SIGNAL_FRAME_UPDATED: Final = f"{DOMAIN}_frame_updated"
