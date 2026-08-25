# PhotoFrame Bridge Home Assistant Component

This directory contains the MVP Home Assistant custom-component payload for the
PhotoFrame bridge.

It is packaged into a tarball by `make package` and is intended to be extracted
directly into Home Assistant's `custom_components/` directory.

Included in this MVP package:

- `manifest.json` with minimal integration metadata
- `protocol.py` with pure-Python payload builders and status parsers
- `const.py` and `__init__.py` for the component package boundary

This is intentionally light-weight so the custom-component archive does not
depend on a separately installed native wheel.
