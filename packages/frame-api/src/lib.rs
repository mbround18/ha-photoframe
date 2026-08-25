//! HTTP client surface for the frame.
//!
//! Under the Home Assistant-managed design the frame holds no third-party
//! credentials and never talks to a photo provider directly (Constitution
//! Principle II, FR-008, FR-043). Home Assistant fetches and prepares every
//! photo; the frame only downloads the already-prepared bytes from its own
//! adopted controller over the local network.
//!
//! The Google Photos REST client and the on-device OAuth device-code flow that
//! previously lived here were removed with that redesign.
//!
//! The prepared-photo client lands here in task T096; see
//! `specs/001-ha-managed-photo-frame/contracts/control-protocol.md`.
