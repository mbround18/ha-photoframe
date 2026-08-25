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

from PIL import Image, ImageEnhance, ImageFilter, ImageOps

# Bumping this invalidates every cached render, because it participates in the
# content-addressed photo_id. Bump it whenever the visual output changes.
PIPELINE_VERSION = 1

JPEG_QUALITY = 85

# How far the source aspect ratio may differ from the panel's before we stop
# cropping and switch to the blurred-backdrop treatment. A 16:9 photo on a 16:10
# panel is a harmless crop; a portrait photo is not.
_ASPECT_TOLERANCE = 0.25

# Backdrop styling for the letterbox treatment.
_BACKDROP_BLUR_RADIUS = 40
_BACKDROP_BRIGHTNESS = 0.45
_BACKDROP_ZOOM = 1.15


class Treatment(StrEnum):
    """How a photo was fitted to the panel."""

    FILL = "fill"
    LETTERBOX_BLUR = "letterbox_blur"


@dataclass(frozen=True, slots=True)
class PreparedImage:
    """A photo encoded for one specific panel geometry."""

    data: bytes
    width: int
    height: int
    treatment: Treatment


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
            # Apply the camera's rotation before anything measures the image,
            # or a portrait photo gets treated as landscape (FR-020).
            oriented = ImageOps.exif_transpose(source)
            oriented = oriented.convert("RGB")
            treatment = _choose_treatment(oriented.size, geometry)
            if treatment is Treatment.FILL:
                canvas = _fill(oriented, geometry)
            else:
                canvas = _letterbox_blur(oriented, geometry)
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
    return Treatment.LETTERBOX_BLUR


def _fill(image: Image.Image, geometry: tuple[int, int]) -> Image.Image:
    """Cover the panel completely, cropping the overhang from the centre."""
    return ImageOps.fit(image, geometry, method=Image.LANCZOS, centering=(0.5, 0.5))


def _letterbox_blur(image: Image.Image, geometry: tuple[int, int]) -> Image.Image:
    """Show the whole photo over a blurred, darkened copy of itself.

    This is the treatment phone galleries and TV screensavers use. It beats
    black bars (dead space on a 10" panel) and beats centre-cropping (which
    decapitates portraits).
    """
    target_w, target_h = geometry

    backdrop = ImageOps.fit(
        image,
        (int(target_w * _BACKDROP_ZOOM), int(target_h * _BACKDROP_ZOOM)),
        method=Image.LANCZOS,
        centering=(0.5, 0.5),
    )
    backdrop = backdrop.filter(ImageFilter.GaussianBlur(_BACKDROP_BLUR_RADIUS))
    backdrop = ImageEnhance.Brightness(backdrop).enhance(_BACKDROP_BRIGHTNESS)
    # The zoom overshoots so the blur has pixels to smear at the edges; crop
    # back to the panel from the centre.
    left = (backdrop.width - target_w) // 2
    top = (backdrop.height - target_h) // 2
    canvas = backdrop.crop((left, top, left + target_w, top + target_h))

    foreground = image.copy()
    foreground.thumbnail(geometry, Image.LANCZOS)
    canvas.paste(
        foreground,
        ((target_w - foreground.width) // 2, (target_h - foreground.height) // 2),
    )
    return canvas
