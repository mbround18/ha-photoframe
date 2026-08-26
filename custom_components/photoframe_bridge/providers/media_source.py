"""Photos from any Home Assistant media source.

This is the general path: local media folders, Immich, Nextcloud, DLNA, Samba,
and anything else that registers a media source. Home Assistant already knows
how to browse and resolve those, so the provider is mostly translation.

Google Photos works through here too, with a caveat worth knowing before you go
looking for a missing album: Home Assistant's `google_photos` media source is a
real album browser, but the integration requests the
`photoslibrary.readonly.appcreateddata` scope, so Google returns only albums and
media that Home Assistant itself created. Photos that live solely in a personal
Google library are not reachable by any application since Google withdrew
library-wide read access in March 2025.

For a photo frame, Immich is the better source: its media source groups assets by
albums, people and tags, so "everything with these faces in it" is a live album
that maintains itself.
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
    BrowseLevel,
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

# Home Assistant raises BrowseError from `media_player.errors`, not from
# `media_source`, and Unresolvable from `media_source.error`. Both are imported
# by name here rather than reached through the `media_source` module: writing
# `except media_source.BrowseError` looks fine and is not -- the attribute does
# not exist, so evaluating the except clause raises AttributeError, which
# escapes past every handler below it and surfaces as a 500 from the options
# flow. That is exactly how this went wrong, and it only showed up when a
# browse actually failed.
try:  # import shape differs across Home Assistant versions
    from homeassistant.components.media_player.errors import BrowseError
except ImportError:  # pragma: no cover

    class BrowseError(Exception):
        """Fallback when Home Assistant does not expose BrowseError."""


try:  # import shape differs across Home Assistant versions
    from homeassistant.components.media_source.error import Unresolvable
except ImportError:  # pragma: no cover

    class Unresolvable(Exception):
        """Fallback when Home Assistant does not expose Unresolvable."""


#: Browsing every folder in a large library is slow and the owner only needs to
#: recognise their album, so stop after this many.
MAX_COLLECTIONS = 200

#: Guards against a pathological library. SC-011 asks for 20,000 photos.
MAX_ITEMS = 20_000

ROOT = "media-source://"

#: Recognised by name when a media source does not classify its files.
#:
#: Deliberately just these three. They are what the frame's own decoder is
#: built with and what the note on its SD card promises, so accepting more here
#: would put photos in the pool that Home Assistant can prepare and the frame
#: can never show from the card. Animated formats are a poor fit for a photo
#: frame regardless: it shows one still for minutes at a time.
_PHOTO_SUFFIXES = (".jpg", ".jpeg", ".png")


def _is_photo(child) -> bool:
    """Whether a media-source item is a still image.

    `media_class` is the right answer when a source sets it, but a source is
    free not to -- a hand-rolled S3 media source may leave it unset or use
    something generic -- and silently dropping every photo from such a source
    is a miserable failure to diagnose. So fall back to the filename, while
    still refusing anything explicitly classified as something other than an
    image.
    """
    media_class = getattr(child, "media_class", None)
    if media_class == MediaClass.IMAGE:
        return True
    if media_class in (MediaClass.VIDEO, MediaClass.MUSIC, MediaClass.PODCAST):
        return False

    name = (getattr(child, "media_content_id", "") or "").lower()
    title = (getattr(child, "title", "") or "").lower()
    return name.endswith(_PHOTO_SUFFIXES) or title.endswith(_PHOTO_SUFFIXES)


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
        # Media sources nest arbitrarily deep -- an S3 bucket might hold
        # media/taiwan/Taiwan 2026 -- so the owner walks down rather than
        # picking from a flattened list that cannot reach the bottom.
        supports_hierarchical_browsing=True,
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
        except (BrowseError, Unresolvable) as err:
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

    async def async_browse(self, identifier: str | None = None) -> BrowseLevel:
        """One level of the media-source tree.

        Walking one level at a time is what makes an album at any depth
        reachable. `async_list_collections` stays as it was, flattening two
        levels, because the coordinator and any provider-agnostic caller still
        want a plain list.
        """
        target = identifier or ROOT
        node = await self._browse(target)

        children = tuple(
            Collection(collection_id=child.media_content_id, title=child.title)
            for child in (node.children or [])
            if child.can_expand
        )

        at_root = target == ROOT
        return BrowseLevel(
            identifier=None if at_root else target,
            title="" if at_root else node.title,
            children=children,
            parent_identifier=None if at_root else self._parent_of(target),
            # The root is every media source at once, which is essentially
            # never what someone means by "show me this album".
            can_select=not at_root,
        )

    @staticmethod
    def _parent_of(identifier: str) -> str | None:
        """Where 'go back' leads, derived from the identifier's own path.

        Media source identifiers are `media-source://<domain>/<path>`, so the
        parent is the path with its last segment removed. Returning None means
        the next step back is the root, which the flow handles.
        """
        if not identifier.startswith(ROOT):
            return None
        remainder = identifier[len(ROOT):].rstrip("/")
        if "/" not in remainder:
            return None
        parent = remainder.rsplit("/", 1)[0]
        return f"{ROOT}{parent}/" if parent else None

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
                # Warning, not debug: if this is the album the owner picked,
                # this line is the entire explanation for an empty frame.
                _LOGGER.warning("could not read %s, skipping it: %s", identifier, err)
                continue

            for child in node.children or []:
                if child.can_expand:
                    pending.append(child.media_content_id)
                    continue
                if not _is_photo(child):
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
