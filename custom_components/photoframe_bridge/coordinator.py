"""Per-frame orchestration: resolve a pool, prepare photos, push them out."""

from __future__ import annotations

from dataclasses import dataclass, field
import logging
import random
from typing import Any

from homeassistant.config_entries import ConfigEntry
from homeassistant.core import HomeAssistant
from homeassistant.helpers.event import async_track_time_interval
from homeassistant.helpers.network import NoURLAvailableError, get_url
from homeassistant.util import dt as dt_util

from datetime import timedelta

from .const import (
    CONF_BRIGHTNESS,
    CONF_ROTATION_INTERVAL,
    CONF_TRANSITION,
    DEFAULT_BRIGHTNESS,
    DEFAULT_ROTATION_INTERVAL,
    DEFAULT_TRANSITION,
)
from .control_server import ControlServer
from .http_view import photo_path
from .photo_store import PhotoStore, compute_photo_id
from .providers import (
    ItemUnsupported,
    ItemUnavailable,
    PhotoProvider,
    PhotoRef,
    Selection,
    SourceUnavailable,
)
from .renderer import UnsupportedImageError, prepare_image

_LOGGER = logging.getLogger(__name__)


@dataclass(slots=True)
class PhotoPool:
    """The resolved photo list for one frame."""

    items: list[PhotoRef] = field(default_factory=list)
    order: list[int] = field(default_factory=list)
    cursor: int = 0

    def reshuffle(self, seed: int | None = None) -> None:
        """Build a play order as a permutation.

        A permutation rather than a random draw is what makes "no photo repeats
        until the pool is exhausted" true (FR-015).
        """
        self.order = list(range(len(self.items)))
        rng = random.Random(seed)
        rng.shuffle(self.order)
        self.cursor = 0

    def advance(self) -> PhotoRef | None:
        if not self.items:
            return None
        if not self.order or self.cursor >= len(self.order):
            self.reshuffle()
        if not self.order:
            return None
        ref = self.items[self.order[self.cursor]]
        self.cursor += 1
        return ref

    def previous(self) -> PhotoRef | None:
        if not self.items or not self.order:
            return None
        self.cursor = max(0, self.cursor - 2)
        return self.advance()


class FrameCoordinator:
    """Decides what a frame shows next, and gets it there."""

    def __init__(
        self,
        hass: HomeAssistant,
        entry: ConfigEntry,
        *,
        frame_id: str,
        provider: PhotoProvider,
        selection: Selection,
        store: PhotoStore,
        server: ControlServer,
    ) -> None:
        self.hass = hass
        self.entry = entry
        self.frame_id = frame_id
        self.provider = provider
        self.selection = selection
        self.store = store
        self.server = server

        self.pool = PhotoPool()
        self.current_photo_id: str | None = None
        self.last_error: str | None = None
        self._unsub_timer = None

    # -- lifecycle --------------------------------------------------------

    async def async_start(self) -> None:
        await self.async_refresh_pool()
        self._schedule()

    async def async_stop(self) -> None:
        if self._unsub_timer is not None:
            self._unsub_timer()
            self._unsub_timer = None

    def _schedule(self) -> None:
        interval = self.entry.options.get(CONF_ROTATION_INTERVAL, DEFAULT_ROTATION_INTERVAL)
        if self._unsub_timer is not None:
            self._unsub_timer()
        self._unsub_timer = async_track_time_interval(
            self.hass, self._on_tick, timedelta(seconds=max(5, int(interval)))
        )

    async def _on_tick(self, _now) -> None:
        await self.async_show_next()

    # -- pool -------------------------------------------------------------

    async def async_refresh_pool(self) -> None:
        """Re-resolve the selection into a concrete list of photos."""
        items: list[PhotoRef] = []
        seen: set[tuple[str, str]] = set()
        try:
            async for ref in self.provider.async_list_items(self.selection):
                if not ref.mime_type.startswith("image/"):
                    continue  # video and friends never enter the pool (FR-018)
                key = (ref.source_id, ref.item_id)
                if key in seen:
                    continue  # the same photo in two albums shows once
                seen.add(key)
                items.append(ref)
        except SourceUnavailable as err:
            # The frame is unaffected: it keeps showing what it holds (FR-026).
            self.last_error = str(err)
            _LOGGER.warning("photo source unavailable for %s: %s", self.frame_id, err)
            return

        self.pool.items = items
        self.pool.reshuffle()
        self.last_error = None
        _LOGGER.info("frame %s pool refreshed: %d photos", self.frame_id, len(items))

    # -- delivery ---------------------------------------------------------

    def _geometry(self) -> tuple[int, int]:
        session = self.server.session(self.frame_id)
        if session is not None and session.panel is not None:
            return session.panel
        from .const import DEFAULT_PANEL_HEIGHT, DEFAULT_PANEL_WIDTH

        return (DEFAULT_PANEL_WIDTH, DEFAULT_PANEL_HEIGHT)

    async def async_prepare(self, ref: PhotoRef) -> str | None:
        """Ensure a prepared render exists for `ref`; return its photo_id."""
        geometry = self._geometry()
        photo_id = compute_photo_id(
            source_id=ref.source_id, item_id=ref.item_id, geometry=geometry
        )

        if await self.hass.async_add_executor_job(self.store.has, photo_id):
            return photo_id

        try:
            # Ask for twice the panel so crops stay sharp.
            raw = await self.provider.async_fetch_bytes(
                ref, want=(geometry[0] * 2, geometry[1] * 2)
            )
        except (ItemUnavailable, ItemUnsupported) as err:
            # Recorded as well as logged: one photo failing is unremarkable, but
            # if every photo fails this is the only account of why, and it is
            # what the summary warning reports.
            self.last_error = f"fetching {ref.item_id} failed: {err}"
            _LOGGER.debug("skipping %s: %s", ref.item_id, err)
            return None
        except SourceUnavailable as err:
            self.last_error = f"source unavailable fetching {ref.item_id}: {err}"
            _LOGGER.debug("skipping %s: %s", ref.item_id, err)
            return None

        try:
            prepared = await self.hass.async_add_executor_job(prepare_image, raw, geometry)
        except UnsupportedImageError as err:
            # Never shown on the panel (FR-029, Principle VIII), but it must be
            # discoverable somewhere or a source of, say, HEIC files looks
            # identical to a source that is simply empty.
            self.last_error = f"preparing {ref.item_id} failed: {err}"
            _LOGGER.debug("could not prepare %s: %s", ref.item_id, err)
            return None

        await self.hass.async_add_executor_job(self.store.put, photo_id, prepared)
        return photo_id

    def _absolute_photo_url(self, photo_id: str) -> str | None:
        """Build the URL the frame should fetch.

        The frame is on the same LAN, so the internal URL is the right one; an
        external URL would send local traffic out and back.
        """
        try:
            base = get_url(self.hass, prefer_external=False, allow_ip=True)
        except NoURLAvailableError:
            _LOGGER.error(
                "Home Assistant has no reachable internal URL, so frames cannot "
                "be told where to fetch photos. Set one under Settings > System "
                "> Network."
            )
            return None
        return f"{base.rstrip('/')}{photo_path(photo_id)}"

    async def async_show(self, ref: PhotoRef) -> bool:
        photo_id = await self.async_prepare(ref)
        if photo_id is None:
            return False

        url = self._absolute_photo_url(photo_id)
        if url is None:
            return False

        sent = await self.server.send_render(
            self.frame_id,
            url,
            transition_type=self.entry.options.get(CONF_TRANSITION, DEFAULT_TRANSITION),
            brightness=self.entry.options.get(CONF_BRIGHTNESS, DEFAULT_BRIGHTNESS),
            correlation_id=photo_id,
        )
        if sent:
            self.current_photo_id = photo_id
            _LOGGER.info("frame %s showing %s", self.frame_id, photo_id)
        return sent

    async def async_show_next(self) -> bool:
        """Advance the slideshow, skipping photos that cannot be prepared."""
        if not self.pool.items:
            return False

        # Bounded so a pool of entirely broken photos cannot spin forever.
        attempts = min(len(self.pool.items), 10)
        for _ in range(attempts):
            ref = self.pool.advance()
            if ref is None:
                return False
            if await self.async_show(ref):
                return True

        # Say why. Without the reason this warning is unactionable: a source
        # whose photos cannot be fetched looks exactly like one whose photos
        # cannot be decoded, or a frame that has gone offline mid-pass.
        connected = bool(self.server.session(self.frame_id))
        _LOGGER.warning(
            "frame %s: none of the %d photo(s) tried could be shown (pool holds %d). "
            "Last failure: %s%s",
            self.frame_id,
            attempts,
            len(self.pool.items),
            self.last_error or "none recorded",
            "" if connected else " (the frame is not currently connected)",
        )
        return False

    async def async_show_previous(self) -> bool:
        ref = self.pool.previous()
        return await self.async_show(ref) if ref is not None else False

    def as_diagnostics(self) -> dict[str, Any]:
        session = self.server.session(self.frame_id)
        return {
            "frame_id": self.frame_id,
            "connected": bool(session and session.connected),
            "panel": self._geometry(),
            "pool_size": len(self.pool.items),
            "cursor": self.pool.cursor,
            "current_photo_id": self.current_photo_id,
            "last_error": self.last_error,
            # Reported by the frame. "no card detected" means it is running from
            # its in-memory buffer only and will lose its photos on reboot.
            "storage": session.storage if session else None,
            "buffered_photos": session.buffered_photos if session else None,
            # "SD card (...)" means the frame is running from photos the owner
            # copied onto the card and is ignoring anything we send.
            "photo_source": session.photo_source if session else None,
            "updated": dt_util.utcnow().isoformat(),
        }

    @property
    def running_from_sd_card(self) -> bool:
        """Whether the frame is showing photos from its own SD card.

        While it is, everything this integration sends is deliberately ignored,
        so anything reporting on photo delivery should say so rather than look
        broken.
        """
        session = self.server.session(self.frame_id)
        return bool(session and session.photo_source and session.photo_source.startswith("SD card"))

    @property
    def local_photos_notice(self) -> str | None:
        """An explanation of why this frame is ignoring us, or None."""
        if not self.running_from_sd_card:
            return None
        session = self.server.session(self.frame_id)
        source = session.photo_source if session else "its SD card"
        return (
            f"This frame is showing photos from {source} and is ignoring the album "
            "chosen here. To hand control back to Home Assistant, remove the photos "
            "from the 'media' folder on its SD card and restart the frame."
        )

    @property
    def storage_warning(self) -> str | None:
        """A problem with the frame's SD cache, in words, or None."""
        session = self.server.session(self.frame_id)
        if session is None or session.storage is None:
            return None
        if session.storage.startswith("ready"):
            return None
        return (
            f"This frame has no usable SD card ({session.storage}). It will keep "
            "showing photos, but only the few it holds in memory, and it will lose "
            "them when it restarts."
        )
