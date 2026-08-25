"""Content-addressed store for prepared photos.

Preparing a photo is expensive and perfectly deterministic, so the result is
keyed by everything that affects the output. Re-preparing the same photo for
the same frame is then a cache hit rather than a second Pillow pass, and a
change to the pipeline invalidates every render automatically instead of
silently serving stale ones.

The store is also the durability boundary that lets a frame outlive its source:
once a photo is prepared it survives the Google Picker session that produced it
expiring (research.md R2). The frame keeps showing photos; it just stops
gaining new ones until the owner re-picks.

All I/O here is blocking. Callers must use an executor (Principle IX).
"""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
import logging
import os
from pathlib import Path

from .renderer import PIPELINE_VERSION, PreparedImage

_LOGGER = logging.getLogger(__name__)

# Long enough to make collisions a non-issue, short enough to keep URLs and log
# lines readable.
PHOTO_ID_LENGTH = 16

STORE_DIRNAME = "photoframe_bridge"
PHOTOS_DIRNAME = "photos"


def compute_photo_id(
    *,
    source_id: str,
    item_id: str,
    geometry: tuple[int, int],
    pipeline_version: int = PIPELINE_VERSION,
) -> str:
    """Derive the stable id for one photo prepared for one frame geometry.

    Geometry participates because two frames with different panels need
    genuinely different renders. `pipeline_version` participates so changing the
    renderer invalidates the cache rather than serving stale output.
    """
    digest = hashlib.sha256()
    for part in (source_id, item_id, f"{geometry[0]}x{geometry[1]}", str(pipeline_version)):
        digest.update(part.encode("utf-8"))
        digest.update(b"\x00")  # unambiguous separator
    return digest.hexdigest()[:PHOTO_ID_LENGTH]


@dataclass(frozen=True, slots=True)
class StoredPhoto:
    photo_id: str
    path: Path
    byte_size: int
    sha256: str


class PhotoStore:
    """A bounded, content-addressed cache of prepared photos on disk."""

    def __init__(self, root: Path, *, max_entries: int = 2000) -> None:
        self._root = Path(root) / STORE_DIRNAME / PHOTOS_DIRNAME
        self._max_entries = max_entries

    @property
    def root(self) -> Path:
        return self._root

    def _path_for(self, photo_id: str) -> Path:
        return self._root / f"{photo_id}.jpg"

    def has(self, photo_id: str) -> bool:
        return self._path_for(photo_id).is_file()

    def get(self, photo_id: str) -> StoredPhoto | None:
        path = self._path_for(photo_id)
        if not path.is_file():
            return None
        data = path.read_bytes()
        # Touch so eviction sees it as recently used.
        os.utime(path, None)
        return StoredPhoto(
            photo_id=photo_id,
            path=path,
            byte_size=len(data),
            sha256=hashlib.sha256(data).hexdigest(),
        )

    def read(self, photo_id: str) -> bytes | None:
        path = self._path_for(photo_id)
        if not path.is_file():
            return None
        os.utime(path, None)
        return path.read_bytes()

    def put(self, photo_id: str, prepared: PreparedImage) -> StoredPhoto:
        """Write a prepared photo atomically.

        The temp-file-then-rename dance matters: Home Assistant can be killed at
        any moment, and a half-written JPEG served to a frame would display as a
        corrupt image rather than failing cleanly.
        """
        self._root.mkdir(parents=True, exist_ok=True)
        final = self._path_for(photo_id)
        tmp = final.with_suffix(".jpg.tmp")

        tmp.write_bytes(prepared.data)
        os.replace(tmp, final)

        self._evict_if_needed()

        return StoredPhoto(
            photo_id=photo_id,
            path=final,
            byte_size=len(prepared.data),
            sha256=hashlib.sha256(prepared.data).hexdigest(),
        )

    def delete(self, photo_id: str) -> bool:
        path = self._path_for(photo_id)
        try:
            path.unlink()
            return True
        except FileNotFoundError:
            return False

    def entries(self) -> list[Path]:
        if not self._root.is_dir():
            return []
        return [p for p in self._root.iterdir() if p.suffix == ".jpg"]

    def _evict_if_needed(self) -> None:
        """Drop the least-recently-used renders once over budget."""
        entries = self.entries()
        excess = len(entries) - self._max_entries
        if excess <= 0:
            return

        # Oldest access time first.
        entries.sort(key=lambda p: p.stat().st_atime)
        for path in entries[:excess]:
            try:
                path.unlink()
            except OSError as err:  # A concurrent read may have removed it.
                _LOGGER.debug("could not evict %s: %s", path.name, err)

        _LOGGER.debug("evicted %d prepared photos to stay within %d", excess, self._max_entries)

    def purge(self) -> int:
        """Remove everything. Used when a config entry is deleted (FR-046)."""
        removed = 0
        for path in self.entries():
            try:
                path.unlink()
                removed += 1
            except OSError:
                pass
        return removed
