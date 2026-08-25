"""Config flow for PhotoFrame Bridge.

Discovery-driven adoption over mDNS lands in T067. Until the firmware
announces itself, a frame is added by name here and associates itself when it
connects to the control channel.
"""

from __future__ import annotations

import secrets
from typing import Any

import voluptuous as vol

from homeassistant.config_entries import (
    ConfigEntry,
    ConfigFlow,
    ConfigFlowResult,
    OptionsFlow,
)
from homeassistant.core import callback
from homeassistant.helpers import config_validation as cv

from .const import (
    CONF_BRIGHTNESS,
    CONF_FRAME_ID,
    CONF_FRAME_TOKEN,
    CONF_ROTATION_INTERVAL,
    CONF_SOURCE,
    CONF_TRANSITION,
    DEFAULT_BRIGHTNESS,
    DEFAULT_ROTATION_INTERVAL,
    DEFAULT_TRANSITION,
    DOMAIN,
)

STEP_USER_SCHEMA = vol.Schema(
    {
        vol.Required(CONF_FRAME_ID): cv.string,
        vol.Optional("name", default="Photo Frame"): cv.string,
    }
)


class PhotoFrameConfigFlow(ConfigFlow, domain=DOMAIN):
    """Adopt a frame."""

    VERSION = 1

    async def async_step_user(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        if user_input is None:
            return self.async_show_form(step_id="user", data_schema=STEP_USER_SCHEMA)

        frame_id = user_input[CONF_FRAME_ID].strip()

        # One entry per frame. Re-adding an existing frame updates it rather
        # than creating a duplicate device (FR-005).
        await self.async_set_unique_id(frame_id)
        self._abort_if_unique_id_configured()

        return self.async_create_entry(
            title=user_input.get("name") or frame_id,
            data={
                CONF_FRAME_ID: frame_id,
                # Minted here, never chosen by the frame. Rotating it is a
                # config-entry reload (discovery.md security property 3).
                CONF_FRAME_TOKEN: secrets.token_urlsafe(32),
            },
            options={
                CONF_SOURCE: "sample",
                CONF_ROTATION_INTERVAL: DEFAULT_ROTATION_INTERVAL,
                CONF_BRIGHTNESS: DEFAULT_BRIGHTNESS,
                CONF_TRANSITION: DEFAULT_TRANSITION,
            },
        )

    @staticmethod
    @callback
    def async_get_options_flow(config_entry: ConfigEntry) -> OptionsFlow:
        return PhotoFrameOptionsFlow()


class PhotoFrameOptionsFlow(OptionsFlow):
    """Presentation settings and photo source."""

    async def async_step_init(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        if user_input is not None:
            return self.async_create_entry(data=user_input)

        # Provider choices come from the registry, so adding a provider makes it
        # selectable without touching this flow (Principle III).
        from .providers import available_providers

        options = self.config_entry.options
        schema = vol.Schema(
            {
                vol.Required(
                    CONF_SOURCE, default=options.get(CONF_SOURCE, "sample")
                ): vol.In(sorted(available_providers())),
                vol.Required(
                    CONF_ROTATION_INTERVAL,
                    default=options.get(CONF_ROTATION_INTERVAL, DEFAULT_ROTATION_INTERVAL),
                ): vol.All(vol.Coerce(int), vol.Range(min=5, max=86400)),
                vol.Required(
                    CONF_BRIGHTNESS,
                    default=options.get(CONF_BRIGHTNESS, DEFAULT_BRIGHTNESS),
                ): vol.All(vol.Coerce(int), vol.Range(min=0, max=100)),
                vol.Required(
                    CONF_TRANSITION,
                    default=options.get(CONF_TRANSITION, DEFAULT_TRANSITION),
                ): vol.In(["cut", "fade", "slide_left", "slide_right"]),
            }
        )
        return self.async_show_form(step_id="init", data_schema=schema)
