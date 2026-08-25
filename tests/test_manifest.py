"""Guards on the integration manifest.

These are cheap, but they catch the two mistakes that break a HACS install for
every user at once: a manifest that drifts from what the code actually does,
and a declared requirement the code does not use (or worse, a version range
that includes a release the code cannot run against).
"""

from __future__ import annotations

import json
from pathlib import Path

COMPONENT = Path(__file__).resolve().parents[1] / "custom_components" / "photoframe_bridge"


def load_manifest() -> dict:
    return json.loads((COMPONENT / "manifest.json").read_text())


def test_manifest_has_the_keys_hassfest_requires() -> None:
    manifest = load_manifest()

    for key in ("domain", "name", "version", "documentation", "issue_tracker", "codeowners"):
        assert manifest.get(key), f"manifest is missing `{key}`"

    assert manifest["domain"] == "photoframe_bridge"
    assert manifest["domain"] == COMPONENT.name


def test_config_flow_claim_matches_reality() -> None:
    """`config_flow: true` without a config_flow.py fails hassfest."""
    manifest = load_manifest()
    has_module = (COMPONENT / "config_flow.py").exists()

    assert manifest.get("config_flow", False) == has_module, (
        "manifest `config_flow` must match whether config_flow.py exists. "
        "T067 adds the module and flips the flag together."
    )


def test_declared_requirements_are_importable_names() -> None:
    """Every requirement must be pinned away from releases we cannot run.

    `controller.py` imports `websockets.legacy.server`, which websockets 14
    removed, so the range must exclude 14 and above until T026 rewrites the
    control server on aiohttp and drops the dependency entirely.
    """
    manifest = load_manifest()

    for requirement in manifest.get("requirements", []):
        assert any(op in requirement for op in ("==", ">=", "<")), (
            f"requirement `{requirement}` is unpinned"
        )
        if requirement.startswith("websockets"):
            assert "<14" in requirement, (
                "websockets 14 removed `websockets.legacy.server`, which "
                "controller.py still imports"
            )
