//! Frame-side domain models.
//!
//! The Google Photos types that used to live here (`GoogleUser`,
//! `PhotoMetadata`, `AlbumMetadata`, `MediaMetadata`) were removed with the
//! Home Assistant-managed redesign. Albums, provider identities, and provider
//! media URLs are Home Assistant's concern; the frame only ever sees an opaque
//! `photo_id` and a path on its own controller (FR-043, Principle II/IV).
//!
//! The wire types the frame does use live in [`crate::control`].
