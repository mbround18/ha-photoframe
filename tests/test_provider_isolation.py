"""Constitution Principle III, mechanically enforced.

Adding a photo source must touch `providers/` and nothing else (FR-016,
SC-013). That is easy to assert once and easy to erode later, so assert it on
every run instead: no module outside `providers/` may name a concrete provider.

This is what makes "pluggable" a property of the code rather than an intention
in a document.
"""

from __future__ import annotations

from pathlib import Path

from custom_components.photoframe_bridge.providers import available_providers

COMPONENT = Path(__file__).resolve().parents[1] / "custom_components" / "photoframe_bridge"
PROVIDERS_DIR = COMPONENT / "providers"

# `__init__.py` legitimately imports the built-ins to populate the registry, and
# the config flow needs a starting default before the owner has chosen a source.
_ALLOWED_DEFAULT_MENTIONS = {"__init__.py", "config_flow.py"}


def _modules_outside_providers() -> list[Path]:
    return [
        path
        for path in COMPONENT.rglob("*.py")
        if PROVIDERS_DIR not in path.parents and path.parent != PROVIDERS_DIR
    ]


def test_no_module_outside_providers_names_a_provider_class() -> None:
    """Provider class names must not leak into the rest of the integration."""
    class_names = {cls.__name__ for cls in available_providers().values()}
    assert class_names, "no providers registered - the scan would pass vacuously"

    violations: list[str] = []
    for path in _modules_outside_providers():
        text = path.read_text()
        for name in class_names:
            if name in text:
                violations.append(f"{path.relative_to(COMPONENT)} references {name}")

    assert not violations, (
        "Principle III violation: a provider class is named outside providers/.\n"
        + "\n".join(violations)
    )


def test_provider_keys_do_not_leak_into_the_coordinator_or_delivery_path() -> None:
    """The coordinator must branch on capabilities, never on provider identity."""
    keys = set(available_providers())
    hot_path = ["coordinator.py", "control_server.py", "http_view.py", "renderer.py",
                "photo_store.py"]

    violations: list[str] = []
    for name in hot_path:
        path = COMPONENT / name
        if not path.is_file():
            continue
        text = path.read_text()
        for key in keys:
            if f'"{key}"' in text or f"'{key}'" in text:
                violations.append(f"{name} hard-codes provider key {key!r}")

    assert not violations, (
        "Principle III violation: the delivery path must not know which "
        "provider it is talking to.\n" + "\n".join(violations)
    )


def test_default_provider_mentions_are_confined_to_setup() -> None:
    """Only entry setup and the config flow may name the fallback source."""
    violations: list[str] = []
    for path in _modules_outside_providers():
        if path.name in _ALLOWED_DEFAULT_MENTIONS:
            continue
        if '"sample"' in path.read_text():
            violations.append(str(path.relative_to(COMPONENT)))

    assert not violations, (
        "the bundled sample source is named outside setup: " + ", ".join(violations)
    )
