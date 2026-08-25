"""PhotoFrame Home Assistant custom component."""

from __future__ import annotations

from typing import Any

import voluptuous as vol

from homeassistant.const import EVENT_HOMEASSISTANT_STOP
from homeassistant.core import HomeAssistant, ServiceCall
import homeassistant.helpers.config_validation as cv

from .const import (
	CONF_HOST,
	CONF_PATH,
	CONF_PORT,
	DEFAULT_HOST,
	DEFAULT_PATH,
	DEFAULT_PORT,
	DOMAIN,
	SERVICE_CLAIM_DEVICE,
	SERVICE_DISPLAY_PHOTO,
	SERVICE_SEND_COMMAND,
)
from .controller import PhotoFrameController

CONFIG_SCHEMA = vol.Schema(
	{
		DOMAIN: vol.Schema(
			{
				vol.Optional(CONF_HOST, default=DEFAULT_HOST): cv.string,
				vol.Optional(CONF_PORT, default=DEFAULT_PORT): cv.port,
				vol.Optional(CONF_PATH, default=DEFAULT_PATH): cv.string,
			}
		)
	},
	extra=vol.ALLOW_EXTRA,
)

DISPLAY_PHOTO_SCHEMA = vol.Schema(
	{
		vol.Required("device_id"): cv.string,
		vol.Required("media_url"): cv.string,
		vol.Optional("brightness"): vol.All(vol.Coerce(int), vol.Range(min=0, max=100)),
		vol.Optional("transition_type"): cv.string,
		vol.Optional("correlation_id"): cv.string,
	}
)

SEND_COMMAND_SCHEMA = vol.Schema(
	{
		vol.Required("device_id"): cv.string,
		vol.Required("command"): cv.string,
		vol.Optional("correlation_id"): cv.string,
	}
)

CLAIM_DEVICE_SCHEMA = vol.Schema(
	{
		vol.Required("device_id"): cv.string,
		vol.Optional("display_name"): cv.string,
	}
)


async def async_setup(hass: HomeAssistant, config: dict[str, Any]) -> bool:
	"""Initialize the PhotoFrame controller server and services."""

	integration_config = config.get(DOMAIN, {})
	controller = PhotoFrameController(
		host=integration_config.get(CONF_HOST, DEFAULT_HOST),
		port=integration_config.get(CONF_PORT, DEFAULT_PORT),
		path=integration_config.get(CONF_PATH, DEFAULT_PATH),
	)
	await controller.start()
	hass.data[DOMAIN] = controller

	async def handle_display_photo(call: ServiceCall) -> None:
		await controller.send_render(
			call.data["device_id"],
			call.data["media_url"],
			brightness=call.data.get("brightness"),
			transition_type=call.data.get("transition_type"),
			correlation_id=call.data.get("correlation_id"),
		)

	async def handle_send_command(call: ServiceCall) -> None:
		await controller.send_command(
			call.data["device_id"],
			call.data["command"],
			correlation_id=call.data.get("correlation_id"),
		)

	async def handle_claim_device(call: ServiceCall) -> None:
		await controller.claim_device(
			call.data["device_id"],
			display_name=call.data.get("display_name"),
		)

	hass.services.async_register(
		DOMAIN,
		SERVICE_DISPLAY_PHOTO,
		handle_display_photo,
		schema=DISPLAY_PHOTO_SCHEMA,
	)
	hass.services.async_register(
		DOMAIN,
		SERVICE_SEND_COMMAND,
		handle_send_command,
		schema=SEND_COMMAND_SCHEMA,
	)
	hass.services.async_register(
		DOMAIN,
		SERVICE_CLAIM_DEVICE,
		handle_claim_device,
		schema=CLAIM_DEVICE_SCHEMA,
	)

	async def _handle_stop(_: Any) -> None:
		await controller.stop()

	hass.bus.async_listen_once(EVENT_HOMEASSISTANT_STOP, _handle_stop)
	return True


__all__ = ["DOMAIN", "PhotoFrameController", "async_setup"]