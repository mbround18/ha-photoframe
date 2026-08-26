"""Config flow for PhotoFrame Bridge.

Discovery-driven adoption over mDNS lands in T067. Until the firmware
announces itself, a frame is added by name here and associates itself when it
connects to the control channel.
"""

from __future__ import annotations

import logging

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


_LOGGER = logging.getLogger(__name__)

CONF_SOURCE_ROOT = "source_root"

#: How far below the chosen source to look for folders, and how many to offer.
#: A form is not a file manager; these keep the list honest and the wait short.
_MAX_TREE_DEPTH = 3
_MAX_TREE_FOLDERS = 300


def _drop_redundant_children(chosen: list[str]) -> list[str]:
    """Keep only the outermost of any nested pair.

    Ticking a folder already includes everything inside it, so keeping a child
    alongside its parent would put the same photos in the pool twice.
    """
    return [
        candidate
        for candidate in chosen
        if not any(
            other != candidate and candidate.startswith(other) for other in chosen
        )
    ]


class PhotoFrameOptionsFlow(OptionsFlow):
    """Presentation settings, photo source, and which albums to show."""

    def __init__(self) -> None:
        self._pending: dict[str, Any] = {}
        #: The source folders are being taken from, e.g. an S3 bucket.
        self._source_root: str | None = None

    async def async_step_init(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        if user_input is not None:
            # Choosing the source is only half the job: the owner still has to
            # say *which* albums. Carry the settings forward and ask.
            self._pending = dict(user_input)
            provider = await self._async_provider()
            if provider is not None and provider.capabilities.supports_hierarchical_browsing:
                # Reset the walk each time settings are submitted, so re-opening
                # options does not resume halfway down a tree.
                return await self.async_step_source()
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

    async def _async_provider(self):
        """The provider the owner picked, or None if it has gone away.

        Providers that browse Home Assistant need the `hass` object; the rest
        take no arguments. Deciding that here keeps every provider ignorant of
        config flows (Principle III).
        """
        from .providers import available_providers

        provider_cls = available_providers().get(
            self._pending.get(CONF_SOURCE, DEFAULT_SOURCE)
        )
        if provider_cls is None:
            return None
        try:
            return provider_cls(self.hass)  # type: ignore[call-arg]
        except TypeError:
            return provider_cls()

    async def async_step_source(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        """Pick which photo source to take folders from.

        Sources are chosen before folders because a media source is a place,
        not a collection: nobody means "everything on the NAS and everything in
        the bucket". Narrowing first also keeps the folder list to something a
        dropdown can honestly present.
        """
        from .providers import ProviderError

        provider = await self._async_provider()
        if provider is None:
            return self.async_create_entry(data=self._pending)

        if user_input is not None:
            self._source_root = user_input[CONF_SOURCE_ROOT]
            return await self.async_step_folders()

        try:
            top = await provider.async_browse(None)
        except ProviderError as err:
            return self.async_abort(
                reason="source_unavailable",
                description_placeholders={"error": str(err)},
            )

        if not top.children:
            # Nothing to narrow to; let the source speak for itself.
            self._source_root = None
            return await self.async_step_folders()

        if len(top.children) == 1:
            # One source is not a choice. Skip the click.
            self._source_root = top.children[0].collection_id
            return await self.async_step_folders()

        choices = {c.collection_id: c.title for c in top.children}
        default = self._source_root if self._source_root in choices else None

        return self.async_show_form(
            step_id="source",
            data_schema=vol.Schema(
                {
                    vol.Required(
                        CONF_SOURCE_ROOT, **({"default": default} if default else {})
                    ): vol.In(choices)
                }
            ),
        )

    async def async_step_folders(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        """Tick the folders to show, as one indented list.

        Everything below the chosen source, to a bounded depth, presented at
        once. Ticking a folder includes everything inside it, so a whole trip
        is one tick rather than one per year-folder underneath it.

        Nothing is ticked by default. A frame that quietly showed an entire
        bucket because the owner had not yet chosen would be showing wallpaper,
        screenshots and whatever else lives there.
        """
        from .providers import ProviderError

        provider = await self._async_provider()
        if provider is None:
            return self.async_create_entry(data=self._pending)

        if user_input is not None:
            chosen = list(user_input.get(CONF_COLLECTIONS) or [])
            # Ticking a parent already includes its children, so keeping both
            # would list the same photos twice in the pool.
            self._pending[CONF_COLLECTIONS] = _drop_redundant_children(chosen)
            return self.async_create_entry(data=self._pending)

        try:
            tree = await self._async_collect_tree(provider, self._source_root)
        except ProviderError as err:
            return self.async_abort(
                reason="source_unavailable",
                description_placeholders={"error": str(err)},
            )

        if not tree:
            # No folders at all: offer the source itself rather than an empty
            # form, so a flat bucket is still usable.
            if self._source_root:
                self._pending[CONF_COLLECTIONS] = [self._source_root]
            return self.async_create_entry(data=self._pending)

        choices = {
            collection.collection_id: f"{'\u00a0' * 4 * depth}{'\u21b3 ' if depth else ''}{collection.title}"
            for depth, collection in tree
        }
        previous = [c for c in (self.config_entry.options.get(CONF_COLLECTIONS) or []) if c in choices]

        return self.async_show_form(
            step_id="folders",
            data_schema=vol.Schema(
                {
                    vol.Required(CONF_COLLECTIONS, default=previous): cv.multi_select(
                        choices
                    )
                }
            ),
            description_placeholders={"count": str(len(choices))},
        )

    async def _async_collect_tree(
        self, provider, root: str | None
    ) -> list[tuple[int, Any]]:
        """Flatten the folders under `root` into an indented list.

        Depth- and size-bounded on purpose. A form is not a file manager, and
        an unbounded walk of a large bucket would take long enough that the
        owner would assume it had hung.
        """
        collected: list[tuple[int, Any]] = []

        async def walk(identifier: str | None, depth: int) -> None:
            if depth > _MAX_TREE_DEPTH or len(collected) >= _MAX_TREE_FOLDERS:
                return
            level = await provider.async_browse(identifier)
            for child in level.children:
                if len(collected) >= _MAX_TREE_FOLDERS:
                    _LOGGER.debug(
                        "stopping folder listing at %d entries", _MAX_TREE_FOLDERS
                    )
                    return
                collected.append((depth, child))
                await walk(child.collection_id, depth + 1)

        await walk(root, 0)
        return collected


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
