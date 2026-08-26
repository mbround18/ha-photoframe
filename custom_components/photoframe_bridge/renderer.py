"""Prepare photos for a frame's exact panel geometry.

Home Assistant does all the image work so the frame does none (Constitution
Principle VI). Every photo leaves here already oriented, already cropped, and
already encoded to the frame's advertised size, so the frame only has to decode
and show it.

Two properties are load-bearing and are covered by tests:

* **Baseline JPEG, never progressive.** The ESP32-P4's hardware JPEG decoder
  rejects progressive JPEG. A progressive file would sail through this module
  and then fail silently on hardware, which is the worst possible place to find
  out.
* **Exact geometry.** The frame does no scaling, so "close enough" is wrong.

Everything here is synchronous and CPU-bound. Callers must run it in an
executor -- never on the event loop (Principle IX).
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum
import io

from PIL import Image, ImageOps

try:
    import numpy as np
except ImportError:  # pragma: no cover
    np = None

# Bumping this invalidates every cached render, because it participates in the
# content-addressed photo_id. Bump it whenever the visual output changes.
# Bumped when prepared output changes, so cached photos regenerate rather than
# being served stale. 2: letterboxing changed from a blurred backdrop to black.
PIPELINE_VERSION = 2

JPEG_QUALITY = 85

# How far the source aspect ratio may differ from the panel's before we stop
# cropping and switch to the blurred-backdrop treatment. A 16:9 photo on a 16:10
# panel is a harmless crop; a portrait photo is not.
_ASPECT_TOLERANCE = 0.25


class Treatment(StrEnum):
    """How a photo was fitted to the panel."""

    FILL = "fill"
    LETTERBOX_BLACK = "letterbox_black"


@dataclass(frozen=True, slots=True)
class PreparedImage:
    """A photo encoded for one specific panel geometry."""

    data: bytes
    width: int
    height: int
    treatment: Treatment


#: Formats a photo may arrive in, as Pillow names them.
#:
#: Kept in step with the frame's own decoder (JPEG and PNG) so a photo that
#: Home Assistant accepts is one the frame could also read straight off its SD
#: card.
SUPPORTED_FORMATS: frozenset[str] = frozenset({"JPEG", "PNG"})


class UnsupportedImageError(Exception):
    """The bytes are not an image we can display.

    Raised for video, corrupt files, and formats Pillow cannot open. The
    coordinator drops these from the pool rather than failing the batch
    (FR-018, FR-029).
    """


def prepare_image(data: bytes, geometry: tuple[int, int]) -> PreparedImage:
    """Prepare one photo for `geometry`, given the original bytes.

    Blocking and CPU-bound: call it via `async_add_executor_job`.
    """
    target_w, target_h = geometry
    if target_w <= 0 or target_h <= 0:
        raise ValueError(f"invalid geometry: {geometry!r}")

    try:
        with Image.open(io.BytesIO(data)) as source:
            # The filename is a hint, not a promise. Pillow reads far more than
            # the frame does, so a GIF or a WebP that slipped through by name
            # would prepare here and be undisplayable from the SD card, where
            # the frame decodes for itself.
            if source.format not in SUPPORTED_FORMATS:
                raise UnsupportedImageError(
                    f"{source.format or 'unrecognised'} is not a supported photo "
                    f"format; use {' or '.join(sorted(SUPPORTED_FORMATS))}"
                )
            # Apply the camera's rotation before anything measures the image,
            # or a portrait photo gets treated as landscape (FR-020).
            oriented = ImageOps.exif_transpose(source)
            oriented = oriented.convert("RGB")
            treatment = _choose_treatment(oriented.size, geometry)
            if treatment is Treatment.FILL:
                canvas = _fill(oriented, geometry)
            else:
                canvas = _letterbox_black(oriented, geometry)
    except UnsupportedImageError:
        raise
    except Exception as err:  # Pillow raises a wide and version-dependent set.
        raise UnsupportedImageError(f"could not decode image: {err}") from err

    buffer = io.BytesIO()
    canvas.save(
        buffer,
        format="JPEG",
        quality=JPEG_QUALITY,
        optimize=True,
        # Both of these matter on the device side:
        #   progressive=False -> the P4's hardware decoder only does baseline
        #   exif/icc dropped  -> orientation is already baked in, and metadata
        #                        (including GPS) must not leave Home Assistant
        progressive=False,
        subsampling="4:2:0",
    )

    return PreparedImage(
        data=buffer.getvalue(),
        width=target_w,
        height=target_h,
        treatment=treatment,
    )


def _choose_treatment(size: tuple[int, int], geometry: tuple[int, int]) -> Treatment:
    """Crop when the shapes are close; letterbox when they are not.

    Cropping a portrait photo onto a landscape panel is the specific failure
    FR-022 names -- it cuts through faces.
    """
    src_w, src_h = size
    target_w, target_h = geometry
    if src_h == 0 or target_h == 0:
        raise UnsupportedImageError("image has zero height")

    src_aspect = src_w / src_h
    target_aspect = target_w / target_h

    if abs(src_aspect - target_aspect) / target_aspect <= _ASPECT_TOLERANCE:
        return Treatment.FILL
    return Treatment.LETTERBOX_BLACK


def _fill(image: Image.Image, geometry: tuple[int, int]) -> Image.Image:
    """Cover the panel completely, cropping the overhang from the centre."""
    return ImageOps.fit(image, geometry, method=Image.LANCZOS, centering=(0.5, 0.5))


def _letterbox_black(image: Image.Image, geometry: tuple[int, int]) -> Image.Image:
    """Show the whole photo, centred, with black filling the rest.

    Black rather than a blurred copy of the photo: it is what the frame does
    for photos loaded from its own SD card, and one photo should not look
    different depending on how it reached the panel. On this panel black is
    also close to invisible against the bezel, so a portrait photo reads as
    floating rather than as a photo with decoration around it.
    """
    target_w, target_h = geometry
    fitted = ImageOps.contain(image, geometry, Image.LANCZOS)
    canvas = Image.new("RGB", geometry, (0, 0, 0))
    canvas.paste(
        fitted,
        ((target_w - fitted.width) // 2, (target_h - fitted.height) // 2),
    )
    return canvas


#: Content type for pre-decoded pixels, so the frame can tell at a glance what
#: it has been sent without inspecting the bytes.
RGB565_CONTENT_TYPE = "application/vnd.photoframe.rgb565"


def to_rgb565(data: bytes) -> bytes:
    """Convert a prepared photo into the panel's own pixel format.

    The frame's panel takes 16-bit RGB565, so sending this means it copies
    bytes to the screen and does no image work at all -- no decode, and none of
    the cost or risk that comes with doing one on a 400 MHz core.

    Roughly 2 MB for a 1280x800 panel against about 23 KB for the equivalent
    JPEG. That trade is worth making on a local network for a photo that
    changes every few minutes, and it is only made at the point of delivery:
    what is cached on disk stays a small JPEG.

    Little-endian, which is what the ESP32-P4 reads without swapping.
    """
    with Image.open(io.BytesIO(data)) as source:
        rgb = source.convert("RGB")
        pixels = rgb.tobytes()

    if np is not None:
        flat = np.frombuffer(pixels, dtype=np.uint8).reshape(-1, 3).astype(np.uint16)
        packed = (
            ((flat[:, 0] & 0xF8) << 8) | ((flat[:, 1] & 0xFC) << 3) | (flat[:, 2] >> 3)
        )
        return packed.astype("<u2").tobytes()

    # Home Assistant ships numpy, but the fallback keeps this module usable
    # anywhere -- it is only slower, not different.
    out = bytearray(len(pixels) // 3 * 2)
    for i in range(0, len(pixels), 3):
        value = (
            ((pixels[i] & 0xF8) << 8) | ((pixels[i + 1] & 0xFC) << 3) | (pixels[i + 2] >> 3)
        )
        out[i // 3 * 2] = value & 0xFF
        out[i // 3 * 2 + 1] = value >> 8
    return bytes(out)
