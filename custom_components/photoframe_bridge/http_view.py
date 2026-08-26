"""Serve prepared photos to frames over the local network.

The frame never receives a provider URL. Google Picker `baseUrl`s expire after
60 minutes and require Home Assistant's own OAuth bearer token, and more
fundamentally the frame is not supposed to know where a photo came from
(FR-043, Principle II/IV). So Home Assistant fetches, prepares, and re-serves
every photo from here.

Authentication is the frame token minted at adoption, not Home Assistant's user
auth: the frame is a device, not a user, and it must keep working when nobody
is logged in.
"""

from __future__ import annotations

import logging

from aiohttp import web

from homeassistant.components.http import HomeAssistantView
from homeassistant.core import HomeAssistant

from .const import DOMAIN
from .photo_store import PhotoStore

_LOGGER = logging.getLogger(__name__)

PHOTO_URL_BASE = f"/api/{DOMAIN}/photo"


def photo_path(photo_id: str) -> str:
    """The path a frame is told to fetch. Always relative to Home Assistant."""
    return f"{PHOTO_URL_BASE}/{photo_id}"


class PhotoFrameTokenRegistry:
    """The frame tokens currently valid for this Home Assistant instance.

    Kept separate from the view so config entries can add and remove their own
    frame without the view knowing about entries at all.
    """

    def __init__(self) -> None:
        self._tokens: dict[str, str] = {}  # token -> frame_id

    def register(self, frame_id: str, token: str) -> None:
        self._tokens[token] = frame_id

    def revoke_frame(self, frame_id: str) -> None:
        for token, owner in list(self._tokens.items()):
            if owner == frame_id:
                del self._tokens[token]

    def frame_for(self, token: str) -> str | None:
        return self._tokens.get(token)

    def token_for(self, frame_id: str) -> str | None:
        """The token a frame should present, so it can be told what it is.

        The frame cannot invent this and cannot download a single photo without
        it, so the controller has to hand it over when the frame connects.
        """
        for token, owner in self._tokens.items():
            if owner == frame_id:
                return token
        return None

    def __len__(self) -> int:
        return len(self._tokens)


class PreparedPhotoView(HomeAssistantView):
    """GET /api/photoframe_bridge/photo/{photo_id}"""

    url = PHOTO_URL_BASE + "/{photo_id}"
    name = f"api:{DOMAIN}:photo"
    # Frames authenticate with their own token, not a Home Assistant user
    # session, so Home Assistant's auth middleware must not reject them first.
    requires_auth = False

    def __init__(self, store: PhotoStore, tokens: PhotoFrameTokenRegistry) -> None:
        self._store = store
        self._tokens = tokens

    async def get(self, request: web.Request, photo_id: str) -> web.StreamResponse:
        frame_id = self._authenticate(request)
        if frame_id is None:
            # 401 tells the frame to drop its control connection and re-hello,
            # which is how it recovers from a rotated token.
            return web.Response(status=401, text="invalid frame token")

        if not _is_safe_photo_id(photo_id):
            return web.Response(status=404, text="unknown photo")

        hass: HomeAssistant = request.app["hass"]
        data = await hass.async_add_executor_job(self._store.read, photo_id)

        if data is None:
            # The photo was evicted. The frame drops it from its queue rather
            # than retrying, so this must be distinguishable from a transient
            # failure.
            return web.Response(status=404, text="unknown photo")

        _LOGGER.debug("served %s (%d bytes) to frame %s", photo_id, len(data), frame_id)
        return web.Response(
            body=data,
            content_type="image/jpeg",
            headers={
                # The bytes are immutable: photo_id is a content hash.
                "Cache-Control": "public, max-age=31536000, immutable",
            },
        )

    def _authenticate(self, request: web.Request) -> str | None:
        header = request.headers.get("Authorization", "")
        scheme, _, token = header.partition(" ")
        if scheme.lower() != "bearer" or not token:
            return None
        return self._tokens.frame_for(token.strip())


def _is_safe_photo_id(photo_id: str) -> bool:
    """Reject anything that is not a bare hex id.

    `photo_id` is interpolated into a filename, so path separators and dots
    must never reach the store.
    """
    return (
        bool(photo_id)
        and len(photo_id) <= 64
        and all(c in "0123456789abcdef" for c in photo_id)
    )
