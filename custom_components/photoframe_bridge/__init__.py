"""PhotoFrame Bridge: Home Assistant drives the photo frame.

Home Assistant owns every credential and does every bit of image work; the
frame downloads already-prepared photos and shows them (Constitution
Principles II and VI).
"""

from __future__ import annotations

import logging
from typing import Any

import voluptuous as vol

from homeassistant.config_entries import ConfigEntry
from homeassistant.core import HomeAssistant, ServiceCall
from homeassistant.helpers import config_validation as cv
from homeassistant.helpers.device_registry import DeviceInfo

from .const import (
    CONF_COLLECTIONS,
    DEFAULT_SOURCE,
    FALLBACK_SOURCE,
    CONF_FRAME_ID,
    CONF_FRAME_TOKEN,
    CONF_SOURCE,
    DEFAULT_MAX_PREPARED_PHOTOS,
    DOMAIN,
    SERVICE_DISPLAY_PHOTO,
    SERVICE_SEND_COMMAND,
)
from .control_server import ControlServer, FrameControlView
from .coordinator import FrameCoordinator
from .http_view import PhotoFrameTokenRegistry, PreparedPhotoView
from .photo_store import PhotoStore
from .providers import Selection, available_providers

_LOGGER = logging.getLogger(__name__)

PLATFORMS: list[str] = []

CONFIG_SCHEMA = cv.config_entry_only_config_schema(DOMAIN)


class RuntimeData:
    """Everything shared between config entries.

    The control server and the photo store are instance-wide: several frames
    share one WebSocket endpoint and one prepared-photo cache.
    """

    def __init__(self, hass: HomeAssistant) -> None:
        self.server = ControlServer()
        self.tokens = PhotoFrameTokenRegistry()
        self.store = PhotoStore(
            hass.config.path(), max_entries=DEFAULT_MAX_PREPARED_PHOTOS
        )
        self.coordinators: dict[str, FrameCoordinator] = {}
        self.views_registered = False


def _runtime(hass: HomeAssistant) -> RuntimeData:
    data = hass.data.setdefault(DOMAIN, {})
    if "runtime" not in data:
        data["runtime"] = RuntimeData(hass)
    return data["runtime"]


async def async_setup_entry(hass: HomeAssistant, entry: ConfigEntry) -> bool:
    """Set up one frame."""
    runtime = _runtime(hass)

    if not runtime.views_registered:
        hass.http.register_view(FrameControlView(runtime.server, runtime.tokens))
        hass.http.register_view(PreparedPhotoView(runtime.store, runtime.tokens))
        runtime.views_registered = True
        _LOGGER.debug("registered control and photo endpoints")

    frame_id = entry.data[CONF_FRAME_ID]
    runtime.tokens.register(frame_id, entry.data[CONF_FRAME_TOKEN])

    provider_key = entry.options.get(CONF_SOURCE, DEFAULT_SOURCE)
    providers = available_providers()
    provider_cls = providers.get(provider_key)
    if provider_cls is None:
        _LOGGER.error(
            "unknown photo source %r for frame %s; falling back to the bundled "
            "sample photos so the frame is not left blank",
            provider_key,
            frame_id,
        )
        provider_cls = providers[FALLBACK_SOURCE]
        provider_key = FALLBACK_SOURCE

    # Providers that browse Home Assistant need it; the rest ignore it.
    try:
        provider = provider_cls(hass)  # type: ignore[call-arg]
    except TypeError:
        provider = provider_cls()

    chosen = entry.options.get(CONF_COLLECTIONS)
    if chosen:
        collection_ids = tuple(chosen)
    else:
        # No explicit choice yet (a frame adopted before the source was
        # configured): fall back to everything the source offers, so the frame
        # shows something rather than nothing.
        try:
            collection_ids = tuple(
                c.collection_id for c in await provider.async_list_collections()
            )
        except Exception as err:  # noqa: BLE001 - never block setup on a source
            _LOGGER.warning("could not list collections for %s: %s", provider_key, err)
            collection_ids = ()

    selection = Selection(source_id=provider_key, collection_ids=collection_ids)

    coordinator = FrameCoordinator(
        hass,
        entry,
        frame_id=frame_id,
        provider=provider,
        selection=selection,
        store=runtime.store,
        server=runtime.server,
    )
    await coordinator.async_start()

    # A frame adopted before its source is configured, or pointed at an album
    # that turns out to be empty, would otherwise sit blank. Show the bundled
    # photos instead so it always looks like a working photo frame, and say so
    # in the log rather than silently substituting.
    if not coordinator.pool.items and provider_key != FALLBACK_SOURCE:
        _LOGGER.info(
            "photo source %r resolved to no photos for frame %s; showing the bundled "
            "sample photos until an album is chosen",
            provider_key,
            frame_id,
        )
        coordinator.provider = providers[FALLBACK_SOURCE]()
        coordinator.selection = Selection(
            source_id=FALLBACK_SOURCE,
            collection_ids=tuple(
                c.collection_id for c in await coordinator.provider.async_list_collections()
            ),
        )
        await coordinator.async_refresh_pool()
    runtime.coordinators[frame_id] = coordinator

    # Get a photo onto the frame as soon as it is reachable.
    #
    # Two paths, because either can happen first: the frame may already be
    # connected when the integration is set up (it retries in a loop, so by the
    # time someone finishes adoption it usually is), or it may connect later.
    # Relying only on the connect event meant a frame that was already waiting
    # sat on "Waiting for photos" until the next rotation tick -- up to five
    # minutes after being adopted.
    def _push_first_photo() -> None:
        session = runtime.server.session(frame_id)
        if session is None or not session.connected:
            return
        # Keyed to the connection rather than to whether we have ever sent this
        # frame a photo. A frame that reboots loses everything it was holding,
        # so "we already sent one" was true and useless -- it left the panel on
        # "Waiting for photos" until the next rotation tick, up to five minutes
        # after every restart.
        if session.photo_pushed:
            return
        session.photo_pushed = True
        entry.async_create_background_task(
            hass, coordinator.async_show_next(), f"{DOMAIN}_first_photo_{frame_id}"
        )

    def _on_frame_event(changed_frame_id: str) -> None:
        if changed_frame_id == frame_id:
            _push_first_photo()

    entry.async_on_unload(runtime.server.add_listener(_on_frame_event))
    # Covers the frame that was already connected before we got here.
    _push_first_photo()
    entry.async_on_unload(entry.add_update_listener(_async_reload_entry))

    _register_services(hass, runtime)

    await hass.config_entries.async_forward_entry_setups(entry, PLATFORMS)
    return True


async def async_unload_entry(hass: HomeAssistant, entry: ConfigEntry) -> bool:
    """Tear one frame down cleanly (FR-046)."""
    runtime = _runtime(hass)
    frame_id = entry.data[CONF_FRAME_ID]

    coordinator = runtime.coordinators.pop(frame_id, None)
    if coordinator is not None:
        await coordinator.async_stop()

    runtime.tokens.revoke_frame(frame_id)

    if PLATFORMS:
        return await hass.config_entries.async_unload_platforms(entry, PLATFORMS)
    return True


async def async_remove_entry(hass: HomeAssistant, entry: ConfigEntry) -> None:
    """Stop delivering photos and leave nothing behind (FR-039, FR-046)."""
    runtime = _runtime(hass)
    frame_id = entry.data[CONF_FRAME_ID]

    await runtime.server.send_command(frame_id, "reload_ui")

    if not runtime.coordinators:
        removed = await hass.async_add_executor_job(runtime.store.purge)
        _LOGGER.info("removed %d prepared photos for the last frame", removed)


async def _async_reload_entry(hass: HomeAssistant, entry: ConfigEntry) -> None:
    await hass.config_entries.async_reload(entry.entry_id)


def _register_services(hass: HomeAssistant, runtime: RuntimeData) -> None:
    if hass.services.has_service(DOMAIN, SERVICE_DISPLAY_PHOTO):
        return

    async def handle_display_photo(call: ServiceCall) -> None:
        frame_id = call.data.get("device_id")
        targets = (
            [runtime.coordinators[frame_id]]
            if frame_id in runtime.coordinators
            else list(runtime.coordinators.values())
        )
        for coordinator in targets:
            await coordinator.async_show_next()

    async def handle_send_command(call: ServiceCall) -> None:
        command = call.data["command"]
        frame_id = call.data.get("device_id")
        for fid, coordinator in runtime.coordinators.items():
            if frame_id and fid != frame_id:
                continue
            if command == "next":
                await coordinator.async_show_next()
            elif command == "previous":
                await coordinator.async_show_previous()
            elif command == "refresh":
                await coordinator.async_refresh_pool()
                await coordinator.async_show_next()
            else:
                await runtime.server.send_command(fid, command)

    hass.services.async_register(
        DOMAIN,
        SERVICE_DISPLAY_PHOTO,
        handle_display_photo,
        schema=vol.Schema({vol.Optional("device_id"): cv.string}),
    )
    hass.services.async_register(
        DOMAIN,
        SERVICE_SEND_COMMAND,
        handle_send_command,
        schema=vol.Schema(
            {
                vol.Required("command"): cv.string,
                vol.Optional("device_id"): cv.string,
            }
        ),
    )


__all__ = ["DOMAIN", "async_setup_entry", "async_unload_entry", "async_remove_entry"]
