"""Tests for the content-addressed prepared-photo store."""

from __future__ import annotations

import hashlib

import pytest

from custom_components.photoframe_bridge.photo_store import (
    PhotoStore,
    compute_photo_id,
)
from custom_components.photoframe_bridge.renderer import PreparedImage, Treatment

GEOMETRY = (1280, 800)


def prepared(payload: bytes = b"jpeg-bytes") -> PreparedImage:
    return PreparedImage(
        data=payload, width=GEOMETRY[0], height=GEOMETRY[1], treatment=Treatment.FILL
    )


def test_photo_id_is_stable_for_identical_inputs() -> None:
    a = compute_photo_id(source_id="s", item_id="i", geometry=GEOMETRY)
    b = compute_photo_id(source_id="s", item_id="i", geometry=GEOMETRY)
    assert a == b


def test_photo_id_changes_with_geometry() -> None:
    """Two frames with different panels need genuinely different renders."""
    a = compute_photo_id(source_id="s", item_id="i", geometry=(1280, 800))
    b = compute_photo_id(source_id="s", item_id="i", geometry=(1024, 600))
    assert a != b


def test_photo_id_changes_with_pipeline_version() -> None:
    """A renderer change must invalidate the cache, not serve stale output."""
    a = compute_photo_id(source_id="s", item_id="i", geometry=GEOMETRY, pipeline_version=1)
    b = compute_photo_id(source_id="s", item_id="i", geometry=GEOMETRY, pipeline_version=2)
    assert a != b


def test_photo_id_separates_fields_unambiguously() -> None:
    """Concatenation without a separator would collide these two."""
    a = compute_photo_id(source_id="ab", item_id="c", geometry=GEOMETRY)
    b = compute_photo_id(source_id="a", item_id="bc", geometry=GEOMETRY)
    assert a != b


def test_put_then_read_round_trips(tmp_path) -> None:
    store = PhotoStore(tmp_path)
    stored = store.put("abc123", prepared(b"hello"))

    assert store.has("abc123")
    assert store.read("abc123") == b"hello"
    assert stored.byte_size == 5
    assert stored.sha256 == hashlib.sha256(b"hello").hexdigest()


def test_read_missing_returns_none(tmp_path) -> None:
    assert PhotoStore(tmp_path).read("deadbeef") is None


def test_put_leaves_no_temp_files(tmp_path) -> None:
    """A half-written JPEG would render as a corrupt image on the frame."""
    store = PhotoStore(tmp_path)
    store.put("abc123", prepared())

    leftovers = list(store.root.glob("*.tmp"))
    assert leftovers == []


def test_put_overwrites_in_place(tmp_path) -> None:
    store = PhotoStore(tmp_path)
    store.put("abc123", prepared(b"first"))
    store.put("abc123", prepared(b"second"))

    assert store.read("abc123") == b"second"
    assert len(store.entries()) == 1


def test_eviction_drops_least_recently_used(tmp_path) -> None:
    store = PhotoStore(tmp_path, max_entries=3)
    for name in ("aa", "bb", "cc"):
        store.put(name, prepared(name.encode()))

    # Re-reading "aa" makes "bb" the least recently used.
    store.read("aa")
    store.put("dd", prepared(b"dd"))

    assert len(store.entries()) == 3
    assert store.has("aa")
    assert store.has("dd")


def test_delete_is_idempotent(tmp_path) -> None:
    store = PhotoStore(tmp_path)
    store.put("abc123", prepared())

    assert store.delete("abc123") is True
    assert store.delete("abc123") is False


def test_purge_clears_everything(tmp_path) -> None:
    """Removing a config entry must leave no prepared photos behind (FR-046)."""
    store = PhotoStore(tmp_path)
    for name in ("aa", "bb", "cc"):
        store.put(name, prepared())

    assert store.purge() == 3
    assert store.entries() == []
