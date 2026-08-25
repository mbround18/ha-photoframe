"""Constants for the PhotoFrame Home Assistant custom component."""

DOMAIN = "photoframe_bridge"
PLATFORMS: list[str] = []
INTEGRATION_TITLE = "PhotoFrame Bridge"
CONF_HOST = "host"
CONF_PORT = "port"
CONF_PATH = "path"
DEFAULT_HOST = "0.0.0.0"
DEFAULT_PORT = 8765
DEFAULT_PATH = "/ws"
SERVICE_DISPLAY_PHOTO = "display_photo"
SERVICE_SEND_COMMAND = "send_command"
SERVICE_CLAIM_DEVICE = "claim_device"