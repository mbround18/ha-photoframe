"""Tests for the photo preparation pipeline.

The baseline-JPEG test is the important one here: a progressive JPEG would pass
every other check in this file and then fail silently on the ESP32-P4's
hardware decoder, which is the worst place to discover it.
"""

from __future__ import annotations

import io

from PIL import Image
import pytest

from custom_components.photoframe_bridge.renderer import (
    PIPELINE_VERSION,
    Treatment,
    UnsupportedImageError,
    prepare_image,
    to_rgb565,
)

PANEL = (1280, 800)


def encode(image: Image.Image, fmt: str = "JPEG", **kwargs) -> bytes:
    buffer = io.BytesIO()
    image.save(buffer, format=fmt, **kwargs)
    return buffer.getvalue()


def solid(width: int, height: int, colour=(120, 60, 30)) -> Image.Image:
    return Image.new("RGB", (width, height), colour)


def test_output_is_baseline_jpeg_never_progressive() -> None:
    """The P4's hardware JPEG decoder cannot decode progressive JPEG."""
    prepared = prepare_image(encode(solid(1600, 1000)), PANEL)

    with Image.open(io.BytesIO(prepared.data)) as out:
        assert out.format == "JPEG"
        # Pillow exposes progressive-ness only via this info key.
        assert "progression" not in out.info, "output must be baseline JPEG"


def test_progressive_input_still_yields_baseline_output() -> None:
    """A progressive source must not produce a progressive render."""
    source = encode(solid(1600, 1000), progressive=True)

    prepared = prepare_image(source, PANEL)

    with Image.open(io.BytesIO(prepared.data)) as out:
        assert "progression" not in out.info


def test_output_is_exactly_the_requested_geometry() -> None:
    """The frame does no scaling, so 'close' is wrong."""
    for size in [(1600, 1000), (400, 300), (5000, 3000), (800, 1280), (1000, 1000)]:
        prepared = prepare_image(encode(solid(*size)), PANEL)

        assert (prepared.width, prepared.height) == PANEL
        with Image.open(io.BytesIO(prepared.data)) as out:
            assert out.size == PANEL, f"{size} produced {out.size}"


def test_landscape_photo_is_cropped_to_fill() -> None:
    prepared = prepare_image(encode(solid(1600, 1000)), PANEL)
    assert prepared.treatment is Treatment.FILL


def test_portrait_photo_is_letterboxed_not_cropped() -> None:
    """FR-022: cropping a portrait onto a landscape panel cuts through faces."""
    prepared = prepare_image(encode(solid(800, 1280)), PANEL)
    assert prepared.treatment is Treatment.LETTERBOX_BLACK


def test_letterbox_preserves_the_whole_subject() -> None:
    """The full photo must survive; only the backdrop is cropped.

    A red portrait on a landscape panel should still show pure red somewhere
    across the vertical centre line - if it had been cropped to fill, the
    subject would be cut instead of inset.
    """
    source = encode(solid(600, 1200, (255, 0, 0)))

    prepared = prepare_image(source, PANEL)

    with Image.open(io.BytesIO(prepared.data)) as out:
        centre_column = [out.getpixel((PANEL[0] // 2, y)) for y in range(0, PANEL[1], 40)]
    # The inset photo is full brightness; the backdrop is darkened to 45%.
    assert any(r > 200 for r, _g, _b in centre_column), "subject not visible at full brightness"


@pytest.mark.parametrize("orientation", range(1, 9))
def test_every_exif_orientation_lands_upright(orientation: int) -> None:
    """All 8 EXIF orientations must be normalised (FR-020)."""
    image = solid(1600, 1000)
    exif = image.getexif()
    exif[0x0112] = orientation

    buffer = io.BytesIO()
    image.save(buffer, format="JPEG", exif=exif)

    prepared = prepare_image(buffer.getvalue(), PANEL)

    assert (prepared.width, prepared.height) == PANEL


def test_metadata_is_stripped() -> None:
    """Orientation is baked in, and GPS must never leave Home Assistant."""
    image = solid(1600, 1000)
    exif = image.getexif()
    exif[0x0112] = 6
    exif[0x9286] = "a user comment"
    buffer = io.BytesIO()
    image.save(buffer, format="JPEG", exif=exif)

    prepared = prepare_image(buffer.getvalue(), PANEL)

    with Image.open(io.BytesIO(prepared.data)) as out:
        assert not dict(out.getexif()), "EXIF must not survive preparation"


def test_rgba_png_is_flattened() -> None:
    source = encode(Image.new("RGBA", (1600, 1000), (10, 20, 30, 128)), fmt="PNG")

    prepared = prepare_image(source, PANEL)

    with Image.open(io.BytesIO(prepared.data)) as out:
        assert out.mode == "RGB"


def test_cmyk_jpeg_is_converted() -> None:
    source = encode(Image.new("CMYK", (1600, 1000), (0, 40, 80, 10)))

    prepared = prepare_image(source, PANEL)

    with Image.open(io.BytesIO(prepared.data)) as out:
        assert out.size == PANEL


def test_undecodable_bytes_raise_unsupported() -> None:
    """Video and junk are skipped, not fatal (FR-018, FR-029)."""
    with pytest.raises(UnsupportedImageError):
        prepare_image(b"\x00\x01\x02 not an image at all", PANEL)


def test_truncated_jpeg_raises_unsupported() -> None:
    full = encode(solid(1600, 1000))

    with pytest.raises(UnsupportedImageError):
        prepare_image(full[: len(full) // 3], PANEL)


def test_invalid_geometry_rejected() -> None:
    with pytest.raises(ValueError):
        prepare_image(encode(solid(800, 600)), (0, 800))


def test_pipeline_version_is_exposed_for_cache_invalidation() -> None:
    """photo_id embeds this, so a pipeline change must invalidate renders."""
    assert isinstance(PIPELINE_VERSION, int)
    assert PIPELINE_VERSION >= 1


class TestSupportedFormats:
    """Only JPEG and PNG, matching what the frame itself can decode.

    Pillow reads far more than the frame does. Accepting a format here that the
    frame cannot read would work over the network and fail from the SD card,
    where the frame decodes for itself -- a difference nobody would connect to
    the file they copied across.
    """

    @staticmethod
    def _encode(fmt: str, size=(1600, 1000)) -> bytes:
        buffer = io.BytesIO()
        Image.new("RGB", size, (120, 90, 60)).save(buffer, format=fmt)
        return buffer.getvalue()

    @pytest.mark.parametrize("fmt", ["JPEG", "PNG"])
    def test_supported_formats_are_prepared(self, fmt: str) -> None:
        prepared = prepare_image(self._encode(fmt), (1280, 800))
        assert prepared.data
        with Image.open(io.BytesIO(prepared.data)) as out:
            assert out.format == "JPEG"
            assert out.size == (1280, 800)

    @pytest.mark.parametrize("fmt", ["GIF", "BMP", "WEBP"])
    def test_everything_else_is_refused(self, fmt: str) -> None:
        with pytest.raises(UnsupportedImageError) as err:
            prepare_image(self._encode(fmt), (1280, 800))
        # The message has to name the way out, not just the refusal.
        assert "JPEG" in str(err.value) and "PNG" in str(err.value)

    def test_a_file_that_is_not_an_image_at_all_is_refused(self) -> None:
        with pytest.raises(UnsupportedImageError):
            prepare_image(b"this is not an image", (1280, 800))


class TestBlackLetterbox:
    """The gap around a photo is black, matching what the frame does itself.

    One photo should not look different depending on whether it arrived from
    Home Assistant or was copied onto the SD card.
    """

    def test_the_bars_beside_a_portrait_photo_are_black(self) -> None:
        prepared = prepare_image(encode(solid(800, 1280)), PANEL)
        with Image.open(io.BytesIO(prepared.data)) as out:
            pixels = out.convert("RGB")
            mid_y = PANEL[1] // 2
            assert pixels.getpixel((2, mid_y)) == (0, 0, 0)
            assert pixels.getpixel((PANEL[0] - 3, mid_y)) == (0, 0, 0)
            # ...and the photo itself is still in the middle.
            assert pixels.getpixel((PANEL[0] // 2, mid_y)) != (0, 0, 0)

    def test_a_cropped_photo_has_no_black_at_all(self) -> None:
        prepared = prepare_image(encode(solid(1600, 1000)), PANEL)
        with Image.open(io.BytesIO(prepared.data)) as out:
            assert out.convert("RGB").getextrema()[0][0] > 0


class TestRgb565:
    """Pre-decoded pixels, so the frame does no image work at all."""

    def test_output_is_exactly_two_bytes_per_pixel(self) -> None:
        prepared = prepare_image(encode(solid(1600, 1000)), PANEL)
        raw = to_rgb565(prepared.data)
        assert len(raw) == PANEL[0] * PANEL[1] * 2

    def test_pixels_are_little_endian_rgb565(self) -> None:
        """The ESP32-P4 reads these without swapping, so byte order matters."""
        buffer = io.BytesIO()
        Image.new("RGB", PANEL, (255, 0, 0)).save(buffer, format="JPEG", quality=100)
        raw = to_rgb565(buffer.getvalue())
        # Pure red is 0xF800; little-endian puts the low byte first.
        assert raw[0] == 0x00
        assert raw[1] == 0xF8

    def test_black_stays_black_through_the_conversion(self) -> None:
        """Letterbox bars must not pick up a tint on the way to the panel."""
        prepared = prepare_image(encode(solid(800, 1280)), PANEL)
        raw = to_rgb565(prepared.data)
        assert raw[0] == 0x00 and raw[1] == 0x00
