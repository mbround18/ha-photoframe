"""WebSocket controller runtime for the PhotoFrame custom component."""

from __future__ import annotations

import asyncio
from dataclasses import dataclass, field
import json
import logging
from typing import Any

from websockets.exceptions import ConnectionClosed
from websockets.legacy.server import WebSocketServerProtocol, serve

from .protocol import (
    ConnectedStatus,
    FrameSession,
    StatusMessage,
    build_command_payload,
    build_render_payload,
)

LOGGER = logging.getLogger(__name__)


@dataclass(slots=True)
class DeviceConnection:
    websocket: WebSocketServerProtocol
    session: FrameSession = field(default_factory=FrameSession)


class PhotoFrameController:
    """Own the HA-side WebSocket server and active frame sessions."""

    def __init__(self, host: str, port: int, path: str) -> None:
        self._host = host
        self._port = port
        self._path = path
        self._server = None
        self._connections_by_socket: dict[int, DeviceConnection] = {}
        self._connections_by_device_id: dict[str, DeviceConnection] = {}
        self._claimed_device_ids: set[str] = set()

    @property
    def endpoint(self) -> str:
        return f"ws://{self._host}:{self._port}{self._path}"

    @property
    def connected_device_ids(self) -> list[str]:
        return sorted(self._connections_by_device_id)

    async def start(self) -> None:
        if self._server is not None:
            return

        self._server = await serve(
            self._handle_connection,
            self._host,
            self._port,
            ping_interval=30,
            ping_timeout=30,
        )
        LOGGER.info("started PhotoFrame controller on %s", self.endpoint)

    async def stop(self) -> None:
        if self._server is None:
            return

        for connection in list(self._connections_by_socket.values()):
            await connection.websocket.close()

        self._server.close()
        await self._server.wait_closed()
        self._server = None
        self._connections_by_socket.clear()
        self._connections_by_device_id.clear()
        LOGGER.info("stopped PhotoFrame controller")

    def session(self, device_id: str) -> FrameSession | None:
        connection = self._connections_by_device_id.get(device_id)
        return None if connection is None else connection.session

    async def send_render(
        self,
        device_id: str,
        media_url: str,
        *,
        brightness: int | None = None,
        transition_type: str | None = None,
        correlation_id: str | None = None,
    ) -> None:
        connection = self._require_connection(device_id)
        payload = build_render_payload(
            media_url,
            brightness=brightness,
            transition_type=transition_type,
            correlation_id=correlation_id,
        )
        await connection.websocket.send(payload)

    async def send_command(
        self,
        device_id: str,
        command: str,
        *,
        correlation_id: str | None = None,
    ) -> None:
        connection = self._require_connection(device_id)
        payload = build_command_payload(command, correlation_id=correlation_id)
        await connection.websocket.send(payload)

    async def claim_device(
        self,
        device_id: str,
        *,
        display_name: str | None = None,
    ) -> None:
        connection = self._require_connection(device_id)
        self._claimed_device_ids.add(device_id)
        await self._send_registration_update(
            connection,
            claimed=True,
            display_name=display_name,
            message="Frame claimed in Home Assistant",
        )

    async def _handle_connection(
        self,
        websocket: WebSocketServerProtocol,
        path: str,
    ) -> None:
        if path != self._path:
            LOGGER.warning("rejecting frame connection on unexpected path %s", path)
            await websocket.close(code=1008, reason="unexpected path")
            return

        connection = DeviceConnection(websocket=websocket)
        socket_id = id(websocket)
        self._connections_by_socket[socket_id] = connection
        LOGGER.info("frame connected from %s", websocket.remote_address)

        try:
            async for payload in websocket:
                status = connection.session.apply_status_payload(payload)
                self._record_status(connection, status)
        except ConnectionClosed:
            LOGGER.info("frame disconnected from %s", websocket.remote_address)
        except Exception:
            LOGGER.exception("frame connection failed")
        finally:
            self._remove_connection(socket_id)

    def _record_status(
        self,
        connection: DeviceConnection,
        status: StatusMessage,
    ) -> None:
        if isinstance(status, ConnectedStatus):
            self._connections_by_device_id[status.device_id] = connection
            LOGGER.info(
                "registered frame %s (%s)",
                status.device_id,
                status.device_name,
            )
            self._create_registration_task(status.device_id, connection)
            return

        LOGGER.debug("received frame status: %s", status)

    def _remove_connection(self, socket_id: int) -> None:
        connection = self._connections_by_socket.pop(socket_id, None)
        if connection is None:
            return

        device_id = connection.session.device_id
        if device_id is not None:
            self._connections_by_device_id.pop(device_id, None)

    def _require_connection(self, device_id: str) -> DeviceConnection:
        connection = self._connections_by_device_id.get(device_id)
        if connection is None:
            raise ValueError(f"frame '{device_id}' is not currently connected")
        return connection

    def _create_registration_task(
        self,
        device_id: str,
        connection: DeviceConnection,
    ) -> None:
        claimed = device_id in self._claimed_device_ids
        message = (
            "Frame already claimed in Home Assistant"
            if claimed
            else "Frame connected. Open Home Assistant to claim it."
        )
        asyncio.create_task(
            self._send_registration_update(
                connection,
                claimed=claimed,
                display_name=connection.session.device_name,
                message=message,
            )
        )

    async def _send_registration_update(
        self,
        connection: DeviceConnection,
        *,
        claimed: bool,
        display_name: str | None,
        message: str,
    ) -> None:
        payload = json.dumps(
            {
                "registration": {
                    "claimed": claimed,
                    "display_name": display_name,
                    "message": message,
                }
            }
        )
        await connection.websocket.send(payload)

    def describe_devices(self) -> list[dict[str, Any]]:
        return [
            {
                "device_id": device_id,
                "device_name": connection.session.device_name,
                "screen_status": (
                    connection.session.screen_status.value
                    if connection.session.screen_status is not None
                    else None
                ),
                "last_media_url": connection.session.last_media_url,
                "last_correlation_id": connection.session.last_correlation_id,
                "claimed": device_id in self._claimed_device_ids,
            }
            for device_id, connection in sorted(self._connections_by_device_id.items())
        ]