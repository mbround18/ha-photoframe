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
    CONF_COLLECTIONS,
    DEFAULT_SOURCE,
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

#: The frame derives its id from the P4's eFuse MAC and prints it on its setup
#: screen. People will retype it from that screen, possibly from across a room,
#: so accept the reasonable variations rather than rejecting a near miss:
#: "esp32p4-80f1b2d0b566", "80f1b2d0b566", "80:F1:B2:D0:B5:66", "80-f1-...".
FRAME_ID_PREFIX = "esp32p4-"


def normalise_frame_id(raw: str) -> str | None:
    """Return the canonical frame id, or None if it cannot be one."""
    candidate = raw.strip().lower().replace(" ", "")
    if candidate.startswith(FRAME_ID_PREFIX):
        candidate = candidate[len(FRAME_ID_PREFIX) :]

    # MAC separators are noise; the id itself is bare hex.
    for separator in (":", "-", "."):
        candidate = candidate.replace(separator, "")

    if len(candidate) != 12 or any(c not in "0123456789abcdef" for c in candidate):
        return None
    return f"{FRAME_ID_PREFIX}{candidate}"


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
        errors: dict[str, str] = {}

        if user_input is not None:
            frame_id = normalise_frame_id(user_input[CONF_FRAME_ID])
            if frame_id is None:
                errors[CONF_FRAME_ID] = "invalid_frame_id"
            else:
                return await self._create(frame_id, user_input)

        return self.async_show_form(
            step_id="user", data_schema=STEP_USER_SCHEMA, errors=errors
        )

    async def _create(
        self, frame_id: str, user_input: dict[str, Any]
    ) -> ConfigFlowResult:

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
                CONF_SOURCE: DEFAULT_SOURCE,
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
    """Presentation settings, photo source, and which albums to show."""

    def __init__(self) -> None:
        self._pending: dict[str, Any] = {}

    async def async_step_init(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        if user_input is not None:
            # Choosing the source is only half the job: the owner still has to
            # say *which* albums. Carry the settings forward and ask.
            self._pending = dict(user_input)
            return await self.async_step_collections()

        # Provider choices come from the registry, so adding a provider makes it
        # selectable without touching this flow (Principle III).
        from .providers import available_providers

        options = self.config_entry.options
        schema = vol.Schema(
            {
                vol.Required(
                    CONF_SOURCE, default=options.get(CONF_SOURCE, DEFAULT_SOURCE)
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

    async def async_step_collections(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        """Pick which albums, folders or buckets this frame shows."""
        from .providers import (
            Capabilities,
            ProviderError,
            available_providers,
        )

        if user_input is not None:
            self._pending[CONF_COLLECTIONS] = user_input.get(CONF_COLLECTIONS, [])
            return self.async_create_entry(data=self._pending)

        provider_key = self._pending.get(CONF_SOURCE, DEFAULT_SOURCE)
        provider_cls = available_providers().get(provider_key)
        if provider_cls is None:
            return self.async_create_entry(data=self._pending)

        # Providers that need Home Assistant to browse take it; the others
        # ignore the argument. Keeping that here rather than in the provider
        # avoids every provider having to know about config flows.
        try:
            provider = provider_cls(self.hass)  # type: ignore[call-arg]
        except TypeError:
            provider = provider_cls()

        try:
            collections = await provider.async_list_collections()
        except ProviderError as err:
            return self.async_abort(
                reason="source_unavailable",
                description_placeholders={"error": str(err)},
            )

        if not collections:
            # Nothing to choose between: don't make the owner click through an
            # empty form.
            self._pending[CONF_COLLECTIONS] = []
            return self.async_create_entry(data=self._pending)

        current = self.config_entry.options.get(CONF_COLLECTIONS) or [
            c.collection_id for c in collections
        ]
        choices = {c.collection_id: c.title for c in collections}

        schema = vol.Schema(
            {
                vol.Required(
                    CONF_COLLECTIONS,
                    default=[c for c in current if c in choices],
                ): cv.multi_select(choices),
            }
        )
        return self.async_show_form(step_id="collections", data_schema=schema)
