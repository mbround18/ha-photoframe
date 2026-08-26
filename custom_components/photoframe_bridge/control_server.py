"""The frame's control channel, hosted on Home Assistant's own web server.

Frames dial out to Home Assistant rather than the other way round, so a frame
needs no open port, no port forward, and no stable address. It reconnects on
its own after a Home Assistant restart or a network outage (FR-026).

Running on Home Assistant's existing aiohttp server rather than a second
listener means one port, one TLS story, and no extra Python dependency -- the
previous implementation pulled in `websockets`, whose 14 release removed the
legacy server API it was written against.
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field
import json
import logging
from typing import Any

from aiohttp import WSMsgType, web

from homeassistant.components.http import HomeAssistantView

from .const import DOMAIN

_LOGGER = logging.getLogger(__name__)

WS_URL = f"/api/{DOMAIN}/ws"

# A frame that connects and then says nothing is not a frame.
HELLO_TIMEOUT_SECONDS = 10.0


@dataclass(slots=True)
class FrameSession:
    """One connected frame."""

    frame_id: str
    device_name: str
    firmware_version: str | None = None
    panel: tuple[int, int] | None = None
    protocol_version: int = 1
    #: State the frame reports about itself.
    storage: str | None = None
    buffered_photos: int | None = None
    photo_source: str | None = None
    #: How many photos the frame has on its SD card.
    cached_photos: int | None = None
    #: Whether this connection has been sent a photo yet.
    #:
    #: Per-connection, not per-frame: a frame that reboots comes back with an
    #: empty buffer and needs a photo again, however many it was sent before.
    photo_pushed: bool = False
    _socket: web.WebSocketResponse | None = field(default=None, repr=False)

    @property
    def connected(self) -> bool:
        return self._socket is not None and not self._socket.closed

    async def send(self, payload: dict[str, Any]) -> bool:
        """Send one control message. False if the frame has gone away."""
        if self._socket is None or self._socket.closed:
            return False
        try:
            await self._socket.send_str(json.dumps(payload))
            return True
        except (ConnectionResetError, RuntimeError) as err:
            _LOGGER.debug("send to %s failed: %s", self.frame_id, err)
            return False


class ControlServer:
    """Tracks connected frames and routes control messages to them."""

    def __init__(self) -> None:
        self._sessions: dict[str, FrameSession] = {}
        self._listeners: list[Callable[[str], None]] = []
        #: Called when a frame asks for another photo, e.g. because someone
        #: tapped the screen faster than the rotation timer supplies them.
        self._photo_requested: list[Callable[[str, int], None]] = []

    # -- session registry -------------------------------------------------

    def session(self, frame_id: str) -> FrameSession | None:
        return self._sessions.get(frame_id)

    def connected_frames(self) -> list[str]:
        return [fid for fid, s in self._sessions.items() if s.connected]

    def add_listener(self, callback: Callable[[str], None]) -> Callable[[], None]:
        """Subscribe to connect/disconnect/status changes for any frame."""
        self._listeners.append(callback)

        def _remove() -> None:
            if callback in self._listeners:
                self._listeners.remove(callback)

        return _remove

    def add_photo_request_listener(
        self, callback: Callable[[str, int], None]
    ) -> Callable[[], None]:
        """Register interest in a frame asking for more photos.

        The callback receives the frame id and how many photos it wants, so a
        frame with an empty SD cache can be filled in one go rather than one
        photo per request.
        """
        self._photo_requested.append(callback)

        def unsubscribe() -> None:
            if callback in self._photo_requested:
                self._photo_requested.remove(callback)

        return unsubscribe

    def _notify(self, frame_id: str) -> None:
        for callback in list(self._listeners):
            try:
                callback(frame_id)
            except Exception:  # a bad listener must not kill the socket
                _LOGGER.exception("control-server listener raised")

    # -- outbound ---------------------------------------------------------

    async def send_render(
        self,
        frame_id: str,
        media_url: str,
        *,
        transition_type: str | None = None,
        brightness: int | None = None,
        correlation_id: str | None = None,
        queue: bool = False,
    ) -> bool:
        """Send one photo to a frame.

        `queue` holds it in reserve instead of showing it, so a tap on the
        frame has something ready and never waits on a download.
        """
        session = self._sessions.get(frame_id)
        if session is None:
            _LOGGER.warning("no session for frame %s", frame_id)
            return False

        payload: dict[str, Any] = {"media_url": media_url}
        if queue:
            payload["queue"] = True
        if transition_type is not None:
            payload["transition_type"] = transition_type
        if brightness is not None:
            payload["brightness"] = brightness
        if correlation_id is not None:
            payload["correlation_id"] = correlation_id
        return await session.send(payload)

    async def send_command(
        self, frame_id: str, command: str, *, correlation_id: str | None = None
    ) -> bool:
        session = self._sessions.get(frame_id)
        if session is None:
            return False
        payload: dict[str, Any] = {"cmd": command}
        if correlation_id is not None:
            payload["correlation_id"] = correlation_id
        return await session.send(payload)

    # -- inbound ----------------------------------------------------------

    def handle_status(self, frame_id: str, message: dict[str, Any]) -> None:
        """Apply one status message from a frame."""
        session = self._sessions.get(frame_id)
        if session is None:
            return

        kind = message.get("type")
        if kind == "connected":
            session.device_name = message.get("device_name") or session.device_name
        elif kind == "health":
            health = message.get("health") or message
            storage = health.get("storage")
            if isinstance(storage, str):
                session.storage = storage
            buffered = health.get("buffered_photos")
            if isinstance(buffered, int):
                session.buffered_photos = buffered
            photo_source = health.get("photo_source")
            if isinstance(photo_source, str):
                session.photo_source = photo_source
        elif kind == "photo_requested":
            # The frame is asking us to refill its cache. Advisory: if nothing
            # is listening it simply keeps showing what it already holds.
            wanted = message.get("wanted")
            session.cached_photos = message.get("cached")
            for callback in list(self._photo_requested):
                callback(frame_id, wanted if isinstance(wanted, int) else 1)
        elif kind == "hello":
            panel = message.get("panel") or {}
            width, height = panel.get("width"), panel.get("height")
            if isinstance(width, int) and isinstance(height, int):
                session.panel = (width, height)
            session.firmware_version = message.get("firmware_version")
            session.protocol_version = message.get("protocol_version", 1)

        self._notify(frame_id)

    def register(self, session: FrameSession) -> None:
        existing = self._sessions.get(session.frame_id)
        if existing is not None and existing.connected:
            _LOGGER.info("frame %s reconnected; replacing previous session", session.frame_id)
        self._sessions[session.frame_id] = session
        self._notify(session.frame_id)

    def unregister(self, frame_id: str) -> None:
        self._sessions.pop(frame_id, None)
        self._notify(frame_id)


class FrameControlView(HomeAssistantView):
    """WebSocket endpoint frames connect to."""

    url = WS_URL
    name = f"api:{DOMAIN}:ws"
    # Frames are devices, not Home Assistant users. They identify themselves in
    # their first message; see the note in http_view.py.
    requires_auth = False

    def __init__(self, server: ControlServer, tokens=None) -> None:
        self._server = server
        # Optional so existing callers and tests keep working; without it the
        # frame is still claimed, just not told its token.
        self._tokens = tokens

    async def get(self, request: web.Request) -> web.WebSocketResponse:
        socket = web.WebSocketResponse(heartbeat=30)
        await socket.prepare(request)

        session: FrameSession | None = None

        try:
            async for msg in socket:
                if msg.type is not WSMsgType.TEXT:
                    continue

                try:
                    payload = json.loads(msg.data)
                except json.JSONDecodeError:
                    _LOGGER.debug("ignoring non-JSON frame message")
                    continue
                if not isinstance(payload, dict):
                    continue

                if session is None:
                    session = self._session_from_first_message(payload, socket)
                    if session is None:
                        _LOGGER.warning("frame connected without identifying itself; closing")
                        break
                    self._server.register(session)
                    _LOGGER.info(
                        "frame %s connected (%s)", session.frame_id, session.device_name
                    )
                    await self._async_claim(session)

                self._server.handle_status(session.frame_id, payload)
        finally:
            if session is not None:
                _LOGGER.info("frame %s disconnected", session.frame_id)
                self._server.unregister(session.frame_id)

        return socket

    async def _async_claim(self, session: FrameSession) -> None:
        """Tell the frame it is ours, and give it the token photos need.

        Sent on every connection rather than only the first: the token lives in
        the frame's memory, so a reboot or a reconnect has to be told again.
        Without this the frame connects happily and then gets 401 on every
        photo, which looks like a frame that is working and simply never shows
        anything.
        """
        token = self._tokens.token_for(session.frame_id) if self._tokens else None
        if token is None:
            _LOGGER.warning(
                "no token registered for frame %s; it will not be able to "
                "download photos",
                session.frame_id,
            )
            return

        await session.send(
            {
                "type": "claim",
                "registration": {
                    "claimed": True,
                    "display_name": session.device_name,
                    "frame_token": token,
                },
            }
        )

    @staticmethod
    def _session_from_first_message(
        payload: dict[str, Any], socket: web.WebSocketResponse
    ) -> FrameSession | None:
        """Identify the frame from its first message.

        Accepts both the `hello` of the newer protocol and the `connected`
        status the current firmware sends, so a frame and a controller on
        different versions still associate (control-protocol.md rule 4).
        """
        frame_id = payload.get("frame_id") or payload.get("device_id")
        if not isinstance(frame_id, str) or not frame_id:
            return None

        return FrameSession(
            frame_id=frame_id,
            device_name=payload.get("device_name") or frame_id,
            firmware_version=payload.get("firmware_version"),
            protocol_version=payload.get("protocol_version", 1),
            _socket=socket,
        )
