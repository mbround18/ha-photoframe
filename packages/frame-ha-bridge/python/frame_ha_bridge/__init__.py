from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum
import json
from typing import Final, TypeAlias

from ._native import build_command_payload as _build_command_payload
from ._native import build_render_payload as _build_render_payload
from ._native import parse_control_payload as _parse_control_payload

__all__: Final = [
    "CommandAcknowledgedStatus",
    "ConnectedStatus",
    "DeviceCommand",
    "ErrorStatus",
    "FrameSession",
    "HealthStatus",
    "ParsedControlMessage",
    "ScreenStatus",
    "StatusMessage",
    "TransitionType",
    "build_command_payload",
    "build_render_payload",
    "parse_control_payload",
    "parse_status_payload",
    "RenderCompletedStatus",
    "RenderStartedStatus",
]


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
            if isinstance(status, RenderStartedStatus):
                self.screen_status = ScreenStatus.RENDERING
            else:
                self.screen_status = ScreenStatus.IDLE
        elif isinstance(status, CommandAcknowledgedStatus):
            self.last_correlation_id = status.correlation_id
        elif isinstance(status, HealthStatus):
            self.screen_status = status.screen_status
        elif isinstance(status, ErrorStatus):
            self.last_correlation_id = status.correlation_id
            self.screen_status = ScreenStatus.ERROR

        return status


def parse_control_payload(payload: str) -> ParsedControlMessage:
    native = _parse_control_payload(payload)
    return ParsedControlMessage(
        kind=native.kind,
        media_url=native.media_url,
        command=DeviceCommand(native.command) if native.command is not None else None,
        transition_type=(
            TransitionType(native.transition_type)
            if native.transition_type is not None
            else None
        ),
        brightness=native.brightness,
        correlation_id=native.correlation_id,
    )


def build_render_payload(
    media_url: str,
    *,
    transition_type: TransitionType | str | None = None,
    brightness: int | None = None,
    correlation_id: str | None = None,
) -> str:
    transition_value = _coerce_transition(transition_type)
    return _build_render_payload(media_url, transition_value, brightness, correlation_id)


def build_command_payload(
    command: DeviceCommand | str,
    *,
    correlation_id: str | None = None,
) -> str:
    command_value = _coerce_command(command)
    return _build_command_payload(command_value, correlation_id)


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


def _expect_str(payload: dict[str, object], key: str) -> str:
    value = payload.get(key)
    if not isinstance(value, str) or not value:
        raise ValueError(f"status payload field '{key}' must be a non-empty string")
    return value


def _optional_str(payload: dict[str, object], key: str) -> str | None:
    value = payload.get(key)
    if value is None:
        return None
    if not isinstance(value, str):
        raise ValueError(f"status payload field '{key}' must be a string when provided")
    return value


def _optional_int(payload: dict[str, object], key: str) -> int | None:
    value = payload.get(key)
    if value is None:
        return None
    if not isinstance(value, int):
        raise ValueError(f"status payload field '{key}' must be an integer when provided")
    return value