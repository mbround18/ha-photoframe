"""Pure-Python protocol helpers for the packaged Home Assistant component.

This module mirrors the controller-side JSON contract without requiring the
editable `frame-ha-bridge` package to be installed inside Home Assistant.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum
import json
from typing import TypeAlias


class TransitionType(StrEnum):
    CUT = "cut"
    FADE = "fade"
    SLIDE_LEFT = "slide_left"
    SLIDE_RIGHT = "slide_right"


class DeviceCommand(StrEnum):
    REBOOT = "reboot"
    RELOAD_UI = "reload_ui"


class ScreenStatus(StrEnum):
    IDLE = "idle"
    RENDERING = "rendering"
    ERROR = "error"


@dataclass(frozen=True, slots=True)
class ParsedControlMessage:
    kind: str
    media_url: str | None
    command: DeviceCommand | None
    transition_type: TransitionType | None
    brightness: int | None
    correlation_id: str | None


@dataclass(frozen=True, slots=True)
class ConnectedStatus:
    device_id: str
    device_name: str


@dataclass(frozen=True, slots=True)
class RenderStartedStatus:
    media_url: str
    correlation_id: str | None


@dataclass(frozen=True, slots=True)
class RenderCompletedStatus:
    media_url: str
    correlation_id: str | None


@dataclass(frozen=True, slots=True)
class CommandAcknowledgedStatus:
    command: DeviceCommand
    correlation_id: str | None


@dataclass(frozen=True, slots=True)
class HealthStatus:
    cpu_temp_millidegrees: int | None
    screen_status: ScreenStatus | None


@dataclass(frozen=True, slots=True)
class ErrorStatus:
    message: str
    correlation_id: str | None


StatusMessage: TypeAlias = (
    ConnectedStatus
    | RenderStartedStatus
    | RenderCompletedStatus
    | CommandAcknowledgedStatus
    | HealthStatus
    | ErrorStatus
)


@dataclass(slots=True)
class FrameSession:
    device_id: str | None = None
    device_name: str | None = None
    last_media_url: str | None = None
    last_correlation_id: str | None = None
    screen_status: ScreenStatus | None = None

    def build_render_payload(
        self,
        media_url: str,
        *,
        transition_type: TransitionType | str | None = None,
        brightness: int | None = None,
        correlation_id: str | None = None,
    ) -> str:
        return build_render_payload(
            media_url,
            transition_type=transition_type,
            brightness=brightness,
            correlation_id=correlation_id,
        )

    def build_command_payload(
        self,
        command: DeviceCommand | str,
        *,
        correlation_id: str | None = None,
    ) -> str:
        return build_command_payload(command, correlation_id=correlation_id)

    def apply_status_payload(self, payload: str) -> StatusMessage:
        status = parse_status_payload(payload)

        if isinstance(status, ConnectedStatus):
            self.device_id = status.device_id
            self.device_name = status.device_name
        elif isinstance(status, (RenderStartedStatus, RenderCompletedStatus)):
            self.last_media_url = status.media_url
            self.last_correlation_id = status.correlation_id
            self.screen_status = (
                ScreenStatus.RENDERING
                if isinstance(status, RenderStartedStatus)
                else ScreenStatus.IDLE
            )
        elif isinstance(status, CommandAcknowledgedStatus):
            self.last_correlation_id = status.correlation_id
        elif isinstance(status, HealthStatus):
            self.screen_status = status.screen_status
        elif isinstance(status, ErrorStatus):
            self.last_correlation_id = status.correlation_id
            self.screen_status = ScreenStatus.ERROR

        return status


def build_render_payload(
    media_url: str,
    *,
    transition_type: TransitionType | str | None = None,
    brightness: int | None = None,
    correlation_id: str | None = None,
) -> str:
    transition_value = _coerce_transition(transition_type)
    payload = {
        "media_url": media_url,
        "correlation_id": correlation_id,
        "transition_type": transition_value,
        "brightness": brightness,
        "cmd": None,
    }
    return json.dumps(_compact_dict(payload))


def build_command_payload(
    command: DeviceCommand | str,
    *,
    correlation_id: str | None = None,
) -> str:
    payload = {
        "correlation_id": correlation_id,
        "cmd": _coerce_command(command),
    }
    return json.dumps(_compact_dict(payload))


def parse_control_payload(payload: str) -> ParsedControlMessage:
    message = json.loads(payload)
    media_url = _optional_str(message, "media_url")
    command = _optional_str(message, "cmd")
    brightness = _optional_int(message, "brightness")
    transition_value = _optional_str(message, "transition_type")
    correlation_id = _optional_str(message, "correlation_id")

    if media_url and command:
        raise ValueError("control message cannot include both media_url and cmd")
    if not media_url and not command:
        raise ValueError("control message must include either media_url or cmd")
    if brightness is not None and brightness > 100:
        raise ValueError("control message brightness must be between 0 and 100 inclusive")
    if command and (brightness is not None or transition_value is not None):
        raise ValueError("control message metadata requires a media_url payload")

    return ParsedControlMessage(
        kind="render" if media_url else "command",
        media_url=media_url,
        command=DeviceCommand(command) if command is not None else None,
        transition_type=(
            TransitionType(transition_value) if transition_value is not None else None
        ),
        brightness=brightness,
        correlation_id=correlation_id,
    )


def parse_status_payload(payload: str) -> StatusMessage:
    message = json.loads(payload)
    message_type = message.get("type")

    if message_type == "connected":
        return ConnectedStatus(
            device_id=_expect_str(message, "device_id"),
            device_name=_expect_str(message, "device_name"),
        )
    if message_type == "render_started":
        return RenderStartedStatus(
            media_url=_expect_str(message, "media_url"),
            correlation_id=_optional_str(message, "correlation_id"),
        )
    if message_type == "render_completed":
        return RenderCompletedStatus(
            media_url=_expect_str(message, "media_url"),
            correlation_id=_optional_str(message, "correlation_id"),
        )
    if message_type == "command_acknowledged":
        return CommandAcknowledgedStatus(
            command=DeviceCommand(_expect_str(message, "cmd")),
            correlation_id=_optional_str(message, "correlation_id"),
        )
    if message_type == "health":
        health = message.get("health")
        if not isinstance(health, dict):
            raise ValueError("health status must include a health object")
        screen_status = health.get("screen_status")
        return HealthStatus(
            cpu_temp_millidegrees=_optional_int(health, "cpu_temp_millidegrees"),
            screen_status=ScreenStatus(screen_status) if screen_status is not None else None,
        )
    if message_type == "error":
        return ErrorStatus(
            message=_expect_str(message, "message"),
            correlation_id=_optional_str(message, "correlation_id"),
        )

    raise ValueError(f"unsupported status payload type: {message_type}")


def _coerce_transition(value: TransitionType | str | None) -> str | None:
    if value is None:
        return None
    if isinstance(value, TransitionType):
        return value.value
    return TransitionType(value).value


def _coerce_command(value: DeviceCommand | str) -> str:
    if isinstance(value, DeviceCommand):
        return value.value
    return DeviceCommand(value).value


def _compact_dict(payload: dict[str, object | None]) -> dict[str, object]:
    return {key: value for key, value in payload.items() if value is not None}


def _expect_str(payload: dict[str, object], key: str) -> str:
    value = payload.get(key)
    if not isinstance(value, str) or not value:
        raise ValueError(f"payload field '{key}' must be a non-empty string")
    return value


def _optional_str(payload: dict[str, object], key: str) -> str | None:
    value = payload.get(key)
    if value is None:
        return None
    if not isinstance(value, str):
        raise ValueError(f"payload field '{key}' must be a string when provided")
    return value


def _optional_int(payload: dict[str, object], key: str) -> int | None:
    value = payload.get(key)
    if value is None:
        return None
    if not isinstance(value, int):
        raise ValueError(f"payload field '{key}' must be an integer when provided")
    return value