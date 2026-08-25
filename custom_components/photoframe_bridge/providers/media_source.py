"""Photos from any Home Assistant media source.

This is the general path: local media folders, Immich, Nextcloud, DLNA, Samba,
and anything else that registers a media source. Home Assistant already knows
how to browse and resolve those, so the provider is mostly translation.

Note this is *not* how Google Photos works. Home Assistant's built-in
google_photos integration exposes only what Home Assistant itself uploaded, not
the user's library, so a Google album cannot be reached this way -- that needs
the Picker-based provider (research.md R1).
"""

from __future__ import annotations

from collections.abc import AsyncIterator
import logging
from typing import ClassVar

from homeassistant.components import media_source
from homeassistant.components.media_player import MediaClass
from homeassistant.core import HomeAssistant
from homeassistant.helpers.aiohttp_client import async_get_clientsession
from homeassistant.helpers.network import NoURLAvailableError, get_url

from . import (
    Capabilities,
    Collection,
    ItemUnavailable,
    PhotoProvider,
    PhotoRef,
    Selection,
    SourceUnavailable,
    register_provider,
)

_LOGGER = logging.getLogger(__name__)

try:  # import shape differs across Home Assistant versions
    from homeassistant.components.media_source.error import Unresolvable as Unresolvable_
except ImportError:  # pragma: no cover

    class Unresolvable_(Exception):
        """Fallback when Home Assistant does not expose Unresolvable."""


#: Browsing every folder in a large library is slow and the owner only needs to
#: recognise their album, so stop after this many.
MAX_COLLECTIONS = 200

#: Guards against a pathological library. SC-011 asks for 20,000 photos.
MAX_ITEMS = 20_000

ROOT = "media-source://"


@register_provider
class MediaSourceProvider(PhotoProvider):
    key: ClassVar[str] = "media_source"
    capabilities: ClassVar[Capabilities] = Capabilities(
        supports_collections=True,
        supports_individual_selection=False,
        # Re-browsing a folder picks up files added since, so a refresh really
        # does surface new photos (FR-014).
        supports_live_collections=True,
        selection_expires=False,
        requires_auth=False,
    )

    def __init__(self, hass: HomeAssistant | None = None, source_id: str = "media_source") -> None:
        self._hass = hass
        self._source_id = source_id

    def _require_hass(self) -> HomeAssistant:
        if self._hass is None:
            raise SourceUnavailable("media source provider needs a Home Assistant instance")
        return self._hass

    async def _browse(self, identifier: str):
        hass = self._require_hass()
        try:
            return await media_source.async_browse_media(hass, identifier)
        except (media_source.BrowseError, Unresolvable_) as err:
            raise SourceUnavailable(f"could not browse {identifier}: {err}") from err
        except Exception as err:  # never let a bare error cross the seam
            raise SourceUnavailable(f"could not browse {identifier}: {err}") from err

    async def async_list_collections(self) -> list[Collection]:
        """Every browsable folder, flattened one level deep.

        Media sources nest arbitrarily; presenting the whole tree in a config
        flow is unusable, so this offers the top level plus its immediate
        children, which is where people actually keep albums.
        """
        root = await self._browse(ROOT)
        collections: list[Collection] = []

        for child in root.children or []:
            if not child.can_expand:
                continue
            collections.append(
                Collection(collection_id=child.media_content_id, title=child.title)
            )

            try:
                sub = await self._browse(child.media_content_id)
            except SourceUnavailable:
                continue

            for grandchild in sub.children or []:
                if not grandchild.can_expand:
                    continue
                collections.append(
                    Collection(
                        collection_id=grandchild.media_content_id,
                        title=f"{child.title} / {grandchild.title}",
                    )
                )
                if len(collections) >= MAX_COLLECTIONS:
                    _LOGGER.debug("stopping collection scan at %d entries", MAX_COLLECTIONS)
                    return collections

        return collections

    async def async_list_items(self, selection: Selection) -> AsyncIterator[PhotoRef]:
        """Walk the selected folders, yielding images as they are found.

        Yields lazily and bounded: a large library must not be materialised in
        memory (SC-011).
        """
        pending = list(selection.collection_ids) or [ROOT]
        seen: set[str] = set()
        yielded = 0

        while pending and yielded < MAX_ITEMS:
            identifier = pending.pop(0)
            if identifier in seen:
                continue
            seen.add(identifier)

            try:
                node = await self._browse(identifier)
            except SourceUnavailable as err:
                _LOGGER.debug("skipping %s: %s", identifier, err)
                continue

            for child in node.children or []:
                if child.can_expand:
                    pending.append(child.media_content_id)
                    continue
                if child.media_class != MediaClass.IMAGE:
                    continue  # video and audio never enter the pool (FR-018)

                yielded += 1
                yield PhotoRef(
                    item_id=child.media_content_id,
                    source_id=self._source_id,
                    mime_type="image/jpeg",
                )
                if yielded >= MAX_ITEMS:
                    _LOGGER.info("stopped at %d photos for one frame", MAX_ITEMS)
                    return

    async def async_fetch_bytes(self, ref: PhotoRef, *, want: tuple[int, int]) -> bytes:
        hass = self._require_hass()

        try:
            resolved = await media_source.async_resolve_media(hass, ref.item_id, None)
        except Exception as err:
            raise ItemUnavailable(f"could not resolve {ref.item_id}: {err}") from err

        url = resolved.url
        if url.startswith("/"):
            # Media sources hand back a Home Assistant-relative path.
            try:
                base = get_url(hass, prefer_external=False, allow_ip=True)
            except NoURLAvailableError as err:
                raise SourceUnavailable("Home Assistant has no reachable URL") from err
            url = f"{base.rstrip('/')}{url}"

        session = async_get_clientsession(hass)
        try:
            async with session.get(url) as response:
                if response.status != 200:
                    raise ItemUnavailable(f"{ref.item_id} returned HTTP {response.status}")
                return await response.read()
        except ItemUnavailable:
            raise
        except Exception as err:
            raise SourceUnavailable(f"fetching {ref.item_id} failed: {err}") from err
