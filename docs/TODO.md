# Product TODO

This file tracks the next staged UX and security work after the current Wi-Fi
and OAuth smoke-test slice.

## Local Ownership And Security

- Treat the on-device device-code flow as the account identity / ownership step,
  with the frame itself showing the verification URL and user code.
- Remove the extra LAN pairing gate from the primary sign-in path unless a
  separate browser-only setup task still needs proof-of-possession.
- After Google authorization, fetch the account email and persist it as the
  frame owner.
- Reject account changes from other users unless the frame is explicitly reset.
- Add a reset/ownership-transfer flow with a clear on-device confirmation step.
- Persist refresh-token and owner metadata safely enough that the frame can
  recover after reboot without forcing a new sign-in.

## Album Selection

- Move Google Photos authorization out of the device-code flow and into a
  browser-capable local setup/controller flow, since Google's limited-input
  device flow does not allow Photos library scopes.
- Implement a second-stage browser authorization path that requests the Google
  Photos scopes needed for album browsing and slideshow access.
- Implement Google Photos album listing in `frame-api` once the browser-granted
  Photos token flow exists.
- Add a dedicated post-login album selection step after the browser Photos
  authorization succeeds instead of overloading the auth screen.
- Prototype the album-selection UI in Slint for host-side iteration, then port
  the approved flow to the embedded UI only where the interaction still makes
  sense on-device.
- Persist the selected album and default slideshow preferences on-device.
- Add re-sync and stale-token recovery behavior.

## Embedded Settings UI

- Add a hidden lower-right tap target that reveals a gear icon.
- Add a compact settings menu with `Next`, `Previous`, and `Sign out` actions.
- Make `Sign out` a two-step confirmation flow.
- Add a visible reset entry point once ownership is implemented.

## Slint Setup UX

- Redesign the Slint host/setup UI so it reads like a consumer onboarding flow
  instead of an internal developer status screen.
- Replace raw status-heavy text blocks with clearer step-based guidance,
  stronger visual hierarchy, and task-specific actions for Wi-Fi, device-code
  sign-in, browser handoff, and album selection.
- Align the Slint experience with the embedded setup flow so both surfaces use
  the same language, state transitions, and user-facing terminology.
- Add separate setup-stage mock screens for Wi-Fi connect, on-device device-code
  sign-in, browser Photos consent handoff, and post-login album selection so
  flows can be swapped and iterated on independently.

## Display And Photo Presentation

- Replace the current text-heavy setup layout with a dedicated landscape-first
  photo presentation layout.
- Add image scaling/cropping rules optimized for landscape photos while still
  handling portrait images gracefully.
- Add album cover / current photo preview states during setup and ready modes.

## Networking And Discovery

- Replace the current placeholder LAN setup experience with a dedicated local
  HTTP controller that serves browser-only Photos consent, album selection, and
  settings actions instead of duplicating the on-device device-code sign-in.
- Add stronger mDNS instance naming and persistent device identity instead of a
  session-scoped hostname.
- Decide whether Wi-Fi scanning should be retried in STA mode, proxied through
  the coprocessor, or omitted entirely in favor of manual SSID entry.

## Concurrency And Runtime Model

- Do not do a repo-wide async rewrite by default; keep the current blocking
  ESP-IDF service integrations unless there is a concrete bottleneck.
- Move OAuth device-code polling off the main setup flow into a background
  worker so the local setup page and on-device UI stay responsive during sign-in.
- Move Google Photos bootstrap and later refresh work into a background worker
  so image fetches do not block UI state updates or slideshow control.
- Introduce a small message-passing layer between firmware orchestration,
  network/auth workers, and UI state updates instead of coupling everything
  through one synchronous control path.
- Revisit async only inside the network/controller layer if worker-thread based
  concurrency becomes too limiting, rather than converting the whole firmware
  stack at once.
