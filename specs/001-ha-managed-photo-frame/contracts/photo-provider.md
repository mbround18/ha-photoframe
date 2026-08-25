# Contract: The `PhotoProvider` Seam

**Feature**: `001-ha-managed-photo-frame` | **Constitution**: Principle III

This is the seam that makes FR-016 and SC-013 true: adding a photo source must touch this file's
implementations and nothing else. No provider-specific type, field, or branch may appear in
`coordinator.py`, `renderer.py`, `config_flow.py`, the entity platforms, the control protocol, or
any firmware code.

---

## Capabilities

Providers differ in ways the coordinator must respond to. It branches on capability, never on
provider identity.

```python
@dataclass(frozen=True, slots=True)
class Capabilities:
    supports_collections: bool          # Can enumerate albums/buckets/folders
    supports_individual_selection: bool  # Can select specific photos
    supports_live_collections: bool      # Re-resolving a collection can yield new photos
    selection_expires: bool              # Selections have a deadline and need re-picking
    requires_auth: bool                  # Needs an OAuth/credential step in the config flow
```

| Provider | collections | individual | live | expires | auth |
|---|---|---|---|---|---|
| `sample` | no | no | no | no | no |
| `media_source` | yes | yes | **yes** | no | no |
| `google_photos_picker` | no¹ | yes | **no**² | **yes** | yes |

¹ Google's Picker returns a flat set of picked items; the integration presents them as one synthetic
collection so the owner sees a consistent UI (FR-009, edge case "source with no collections").
² The picked set is frozen at pick time — see [research.md](../research.md) R2. This is precisely why
the capability exists.

---

## Interface

```python
class PhotoProvider(ABC):
    key: ClassVar[str]              # Stable, used in storage. Never renamed.
    capabilities: ClassVar[Capabilities]

    @abstractmethod
    async def async_list_collections(self) -> list[Collection]: ...

    @abstractmethod
    async def async_list_items(
        self, selection: Selection, *, limit: int | None = None
    ) -> AsyncIterator[PhotoRef]:
        """Yield refs lazily. Must not materialize 20,000 items at once (SC-011)."""

    @abstractmethod
    async def async_fetch_bytes(self, ref: PhotoRef, *, want: tuple[int, int]) -> bytes:
        """Return original-ish bytes for one photo, at or above `want` if the source
        can size server-side. Raises ItemUnavailable / SourceUnavailable / NeedsReauth."""

    async def async_check_health(self) -> Health:
        """Default: OK. Override where the source can be probed cheaply."""
```

### Config-flow contribution

A provider that needs setup contributes its own steps rather than the config flow knowing about it:

```python
    async def async_config_steps(self, flow: ConfigFlow) -> ProviderSetupResult: ...
    async def async_selection_steps(self, flow: OptionsFlow) -> Selection: ...
```

This is the part that keeps `config_flow.py` provider-agnostic. Adding Google's picker-session
wait loop must not add a branch to the shared flow.

---

## Errors

One exception hierarchy, so the coordinator handles failures uniformly (FR-017, FR-029):

| Exception | Meaning | Coordinator response |
|---|---|---|
| `ItemUnavailable` | This one photo is gone or unreadable | Skip it silently, continue (FR-029) |
| `ItemUnsupported` | Not a displayable image (e.g. video) | Drop from the pool (FR-018) |
| `SourceUnavailable` | Service down or network failure | Mark source health, retry with backoff; **the frame is unaffected** (FR-026) |
| `NeedsReauth` | Credential expired or revoked | Raise `ConfigEntryAuthFailed`; start the repair flow (FR-038) |
| `SelectionExpired` | Selection deadline passed | Prompt to re-pick; keep serving already-prepared photos (R2) |

**Rule**: a provider must never raise a bare exception across the seam. Anything unexpected is
wrapped as `SourceUnavailable` at the boundary.

---

## Registration

```python
PROVIDERS: Final[dict[str, type[PhotoProvider]]] = {}

def register_provider(cls: type[PhotoProvider]) -> type[PhotoProvider]: ...
```

Adding a provider means: one module under `providers/`, one `@register_provider`, one entry in
`strings.json`. Nothing else.

---

## Conformance tests

Every provider is run against one shared suite (`tests/providers/test_conformance.py`),
parametrized over the registry, so a new provider inherits the whole suite for free:

1. `key` is stable, non-empty, and matches its module name.
2. `capabilities` is internally consistent — e.g. `supports_live_collections` implies
   `supports_collections`.
3. `async_list_collections` returns `[]` rather than raising when `not supports_collections`.
4. `async_list_items` is lazy: consuming 10 items from a 10,000-item source must not fetch all
   10,000 (asserted with an instrumented fake).
5. Every documented error path raises the contract exception, never a bare one.
6. `async_fetch_bytes` on a known-bad ref raises `ItemUnavailable`, not a network error.

**Architecture test** (`tests/test_provider_isolation.py`), enforcing Principle III mechanically:
grep the non-provider modules for every `provider_key` literal and every provider class name. Any
hit outside `providers/` fails the build. This is what turns "pluggable" from an intention into a
property.
