# Feature Specification: Home Assistant-Managed Digital Photo Frame

**Feature Branch**: `001-ha-managed-photo-frame`

**Created**: 2026-08-25

**Status**: Draft

**Input**: User description: "Create a HACS-compatible Home Assistant integration for Google Photos. In Home Assistant I want to adopt a photo frame onto the network much like ha-kiosk, but the goal of this device is not to display a portal — it communicates with our Home Assistant integration, and the integration pushes new images from Google Photos. The user selects an album or a series of photos. Build a pluggable provider model so we can add more sources later, like S3. There is a 64GB SD card in the device."

## Overview

A physical photo frame that a non-technical person can set up, hand to someone as a gift, and then
forget about. Home Assistant does all the thinking: it holds the photo-service credentials, decides
which photo shows next, prepares each photo for the frame's exact screen, and hands it over. The
frame does one job — show photos beautifully, and keep showing them even when Home Assistant is
not there.

The frame's owner manages everything from Home Assistant: which photos, how often they change,
how bright the screen is, and when it sleeps. The frame itself has no settings screen, no portal,
and no account of its own.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Getting the frame onto the network and into Home Assistant (Priority: P1)

Someone unboxes a frame, plugs it in, and gets it onto their home Wi-Fi and adopted into Home
Assistant without installing a companion app, typing an IP address, or reading a manual.

**Why this priority**: Nothing else in the product exists until the frame is on the network and
Home Assistant knows about it. Every other story depends on this one, and it is the single point
where a confusing experience loses a non-technical user permanently.

**Independent Test**: Power on a factory-fresh frame, complete Wi-Fi provisioning from a phone or
laptop with no app install, and confirm Home Assistant surfaces the frame as a newly discovered
device that can be adopted in a few clicks.

**Acceptance Scenarios**:

1. **Given** a factory-fresh frame with no Wi-Fi configured, **When** it powers on, **Then** it
   becomes discoverable for wireless setup and shows a calm, plain-language welcome on screen that
   tells the user what to do next — no IP addresses, no error codes, no technical status.
2. **Given** a user running the wireless setup, **When** they choose their home network and enter
   its password, **Then** the frame joins the network and reports success or failure back to the
   setup surface in plain language.
3. **Given** a wrong Wi-Fi password was entered, **When** the attempt fails, **Then** the user is
   told plainly that it failed and can immediately try again without a factory reset or a power
   cycle.
4. **Given** the frame has joined the home network, **When** Home Assistant is running on that
   network, **Then** Home Assistant automatically surfaces the frame as a discovered device without
   the user entering any address.
5. **Given** the discovery card in Home Assistant, **When** the user adopts the frame and names it,
   **Then** the frame is bound to that Home Assistant instance, appears as a device with entities,
   and its screen transitions out of setup into a waiting-for-photos state.
6. **Given** a frame already adopted by one Home Assistant instance, **When** a second instance
   attempts to adopt it, **Then** the second attempt is refused and the user is told the frame is
   already adopted and must be reset first.

---

### User Story 2 - Choosing which photos appear (Priority: P1)

The frame's owner picks the photos. They open the frame's options in Home Assistant, choose where
photos come from, pick an album or a specific set of photos, and the frame starts showing them.

**Why this priority**: A frame that is adopted but has no photos is not yet a product. Together
with Story 1 this is the minimum viable gift. It is equal in priority because "adopted but blank"
is just as useless to the recipient as "not adopted".

**Independent Test**: With an already-adopted frame, configure a photo source and a selection, and
confirm the correct photos — and only those photos — begin appearing on the frame.

**Acceptance Scenarios**:

1. **Given** an adopted frame, **When** the owner opens its configuration in Home Assistant,
   **Then** they are shown the available photo sources and can choose one.
2. **Given** a chosen photo source that requires an account, **When** the owner has not yet
   connected that account, **Then** they are guided through connecting it from within Home
   Assistant, and the resulting credential is stored only in Home Assistant.
3. **Given** a connected photo source, **When** the owner browses it, **Then** they see the
   collections available to them (albums, buckets, folders, or the equivalent for that source) and
   can select one or more.
4. **Given** a source that lets the user hand-pick individual photos rather than a whole album,
   **When** the owner picks a set of photos, **Then** exactly that set becomes the frame's photo
   pool.
5. **Given** a selection has been made, **When** the owner confirms, **Then** the frame begins
   showing photos from that selection without needing a restart or a re-adoption.
6. **Given** an existing selection, **When** the owner changes it to a different album or source,
   **Then** the frame switches to the new photos and stops showing the old ones.
7. **Given** a source whose collections update live and an album that has new photos added to it
   later, **When** the frame next refreshes its pool, **Then** the newly added photos are included
   without any user action.
8. **Given** a source that freezes a selection when it is made, **When** the owner makes the
   selection, **Then** they are told plainly that it is fixed and how to revise it later.

---

### User Story 3 - The photos look right on the screen (Priority: P1)

Every photo fills the frame's screen properly: correctly oriented, sensibly cropped, sharp, with a
smooth transition between one photo and the next.

**Why this priority**: The entire product is "photos look nice on a wall". A frame that shows the
right photos badly — sideways, squashed, letterboxed, tearing between images — fails at the one
thing it exists to do. This must ship with Stories 1 and 2.

**Independent Test**: Push a mixed set of landscape, portrait, rotated, very large, and very small
photos to a frame and visually confirm each renders correctly oriented, appropriately framed, and
transitions cleanly.

**Acceptance Scenarios**:

1. **Given** a landscape photo, **When** it is shown, **Then** it fills the screen with the
   subject intact and without distortion of proportions.
2. **Given** a portrait photo on a screen mounted in landscape, **When** it is shown, **Then** it
   is presented in a way that is deliberate and attractive rather than stretched or arbitrarily
   cut through the subject.
3. **Given** a photo that carries rotation information from the camera that took it, **When** it
   is shown, **Then** it appears the right way up.
4. **Given** the frame moves from one photo to the next, **When** the transition plays, **Then** it
   is smooth and free of flicker, tearing, or a visible blank gap.
5. **Given** a photo far larger than the screen, **When** it is shown, **Then** it appears sharp
   and correctly sized, and preparing it does not stall the frame or Home Assistant.
6. **Given** a photo in a format the pipeline cannot handle, **When** it is encountered, **Then**
   it is skipped silently and the slideshow continues with the next photo — the user never sees an
   error on the screen.

---

### User Story 4 - It keeps working when things go wrong (Priority: P2)

Home Assistant restarts, the network drops, or the internet goes out — and the frame on the wall
keeps showing photos as if nothing happened.

**Why this priority**: A gift frame that goes blank whenever the giver reboots their Home Assistant
box becomes a support burden and a source of embarrassment. It is P2 only because the frame must
first work at all, but it is what separates an appliance from a demo.

**Independent Test**: With a frame running a slideshow, restart Home Assistant, then disconnect the
network entirely, then restore both — confirming the slideshow never stops and recovers on its own.

**Acceptance Scenarios**:

1. **Given** a frame showing a slideshow, **When** Home Assistant restarts, **Then** the frame
   continues its slideshow uninterrupted and reconnects automatically once Home Assistant is back.
2. **Given** a frame showing a slideshow, **When** the network becomes unavailable, **Then** the
   frame continues showing photos it already holds and retries reconnecting on its own.
3. **Given** a frame that has been offline long enough to exhaust newly-supplied photos, **When**
   it has nothing new, **Then** it continues cycling the photos it holds rather than going blank.
4. **Given** a frame that reconnects after an outage, **When** the connection is restored, **Then**
   it resumes receiving new photos without the user touching anything.
5. **Given** the photo source's own service is unavailable, **When** Home Assistant cannot fetch
   new photos, **Then** the frame is unaffected and Home Assistant reports the problem in its own
   interface, not on the frame's screen.
6. **Given** a frame that loses power mid-slideshow, **When** it powers back on, **Then** it
   returns to showing photos without any setup steps and without waiting on Home Assistant.

---

### User Story 5 - Controlling the frame from Home Assistant (Priority: P2)

The owner controls the frame from Home Assistant and automates it: pause, skip to the next photo,
change how often photos rotate, adjust brightness, and turn the screen off at night.

**Why this priority**: This turns the frame from a fixed appliance into something that fits a home
— dimming in the evening, sleeping overnight, showing a specific photo on a birthday. It is P2
because the frame is already useful without it.

**Independent Test**: From Home Assistant, exercise each control against a running frame and
confirm the frame responds, and confirm each control is usable from an automation.

**Acceptance Scenarios**:

1. **Given** an adopted frame, **When** the owner views it in Home Assistant, **Then** they see its
   current state — online or offline, what it is showing, and whether the screen is on.
2. **Given** a running slideshow, **When** the owner triggers next or previous, **Then** the frame
   changes photo promptly.
3. **Given** a running slideshow, **When** the owner pauses it, **Then** the current photo stays on
   screen until resumed.
4. **Given** a frame, **When** the owner changes the rotation interval or brightness, **Then** the
   change takes effect without a restart and survives a power cycle.
5. **Given** a frame, **When** the owner turns the screen off, **Then** the panel goes dark and the
   frame draws less power, and turning it back on resumes the slideshow.
6. **Given** an automation, **When** it targets the frame's controls on a schedule, **Then** the
   frame responds the same way it does to manual control.
7. **Given** an automation or script, **When** it asks the frame to show one specific photo,
   **Then** that photo is shown and normal rotation resumes afterward.

---

### User Story 6 - Adding a new kind of photo source (Priority: P3)

Later, a new photo source is added — object storage, a self-hosted photo server, a plain network
folder — and it becomes available to every frame without disturbing anything that already works.

**Why this priority**: The photo-service landscape is unstable and the owner explicitly wants to
add sources later. Building the seam now costs little; retrofitting it later means rewriting the
delivery path. It is P3 because no user is blocked on it today.

**Independent Test**: Add a second photo source implementation and confirm it appears as a choice
for the owner, works end to end, and required no change to adoption, delivery, controls, or the
frame's software.

**Acceptance Scenarios**:

1. **Given** a new photo source is added, **When** the owner configures a frame, **Then** the new
   source appears alongside existing ones with no change to how frames are adopted or controlled.
2. **Given** two frames using two different sources, **When** both run, **Then** each behaves
   identically from the frame's point of view and from the owner's control surface.
3. **Given** a source that cannot list collections at all and only offers a flat set of photos,
   **When** the owner configures it, **Then** they are shown a sensible selection experience rather
   than an empty or broken collection list.
4. **Given** a source is misconfigured or its credential expires, **When** it fails, **Then** the
   failure is reported against that source only and other frames and sources are unaffected.

---

### User Story 7 - Handing the frame on or starting over (Priority: P3)

The owner un-adopts a frame, resets it, or gives it to someone else, and can be confident nothing
personal travels with the hardware.

**Why this priority**: Needed for a device that will be gifted, re-gifted, moved between homes, or
sold. P3 because it is not on the first-run path, but it is a correctness and privacy requirement
before anyone else touches the hardware.

**Independent Test**: Adopt a frame, then reset it, and confirm it returns to the factory-fresh
first-run experience with no residual photos, no network credential, and no binding to the previous
Home Assistant instance.

**Acceptance Scenarios**:

1. **Given** an adopted frame, **When** the owner removes it from Home Assistant, **Then** the
   frame stops receiving photos and returns to an unadopted state that can be adopted again.
2. **Given** a frame, **When** a reset is performed directly on the device, **Then** all stored
   photos, the Wi-Fi credential, and the Home Assistant binding are erased and the frame returns to
   the factory-fresh first-run experience.
3. **Given** a reset is available on the device, **When** it is reachable, **Then** it cannot be
   triggered accidentally by a casual touch and requires a deliberate confirmed action.
4. **Given** a frame that has been reset, **When** it is powered on, **Then** it holds no
   credential, no photo, and no reference to any prior Home Assistant instance.

---

### Edge Cases

- **A selected album disappears or is unshared.** The frame keeps showing what it holds; Home
  Assistant reports the selection is no longer available and prompts the owner to choose again.
- **The selected album is empty.** The owner is told the selection has no photos when they make it,
  rather than discovering a blank frame later.
- **The selection contains a single photo.** The frame shows it without cycling artifacts or
  repeated transition flashes.
- **The selection contains tens of thousands of photos.** Configuration and browsing stay
  responsive, and the frame never attempts to hold the whole set at once.
- **The photo source contains videos or unsupported media.** They are excluded from the pool
  without breaking the rest of it.
- **A photo's source URL expires between selection and display.** The system obtains a fresh one
  rather than showing a gap.
- **Two frames are adopted in the same home.** They are independently named, independently
  configured, and can show different photo sets simultaneously.
- **Two frames share one photo selection.** Neither frame's behaviour interferes with the other's.
- **The storage medium is missing, full, or unreadable.** The frame still shows photos, using
  whatever it can hold in memory, and reports the condition to Home Assistant only.
- **The frame is adopted while Home Assistant has no photo source configured yet.** The frame
  shows a calm waiting state, not an error.
- **The clock is wrong or unset after a power loss.** Scheduled screen-off behaviour degrades
  safely rather than leaving the screen stuck off.
- **The network is present but Home Assistant is unreachable at the address it was adopted on.**
  The frame rediscovers it rather than requiring re-adoption.
- **The owner reboots the frame while a photo is being written to storage.** The partial photo is
  discarded rather than shown corrupted.
- **The same photo appears in two selected albums.** It is not shown twice in a row.

## Requirements *(mandatory)*

### Functional Requirements

#### Adoption and identity

- **FR-001**: A factory-fresh frame MUST be provisionable onto a Wi-Fi network without a companion
  app, without a cloud service, and without the user typing an address.
- **FR-002**: Wi-Fi provisioning MUST report success and failure back to the user in plain language
  and MUST be retryable immediately after a failure without a reset or power cycle.
- **FR-003**: A frame on the network MUST announce itself such that Home Assistant surfaces it
  automatically as a discovered device.
- **FR-004**: The system MUST allow adoption of a discovered frame through a guided flow in Home
  Assistant in which the owner assigns it a human-readable name.
- **FR-005**: Each frame MUST have a stable identity that survives reboots, network changes, and
  Home Assistant restarts, so that re-adoption is never required after ordinary events.
- **FR-006**: A frame MUST refuse adoption by a second controller while it is already adopted, and
  MUST say so clearly to whoever attempts it.
- **FR-007**: The system MUST support multiple frames per Home Assistant instance, each
  independently named, configured, and controlled.
- **FR-008**: The frame MUST store no credential for any photo service. All photo-service
  credentials MUST reside only in Home Assistant.

#### Photo sources and selection

- **FR-009**: The system MUST present photo sources to the owner as interchangeable choices with a
  consistent selection experience regardless of the underlying service.
- **FR-010**: The system MUST support selecting photos by collection (album, bucket, folder, or
  equivalent) where the source offers collections.
- **FR-011**: The system MUST support selecting an explicit set of individual photos where the
  source offers that instead of, or in addition to, collections.
- **FR-012**: The system MUST support selecting more than one collection for a single frame and
  MUST treat the union as one pool.
- **FR-013**: The system MUST allow the owner to change a frame's source or selection at any time,
  taking effect without restart or re-adoption.
- **FR-014**: Where the photo source supports it, the system MUST periodically refresh the photo
  pool so photos added to a selected collection appear on the frame without user action, at an
  interval the owner can adjust. Where a source freezes a selection at the moment it is made, the
  system MUST say so plainly at selection time and MUST offer a way to revise the selection.
- **FR-014a**: Each photo source MUST declare whether its selections update automatically and
  whether they expire, and the system MUST behave according to that declaration rather than
  assuming one behaviour for all sources.
- **FR-014b**: When a selection expires or can no longer be read, the frame MUST keep showing the
  photos already delivered to it, and the owner MUST be prompted to renew the selection.
- **FR-015**: The system MUST let the owner choose the display order — at minimum shuffled and
  chronological — and MUST avoid repeating a photo until the pool has been exhausted.
- **FR-016**: Adding a new kind of photo source MUST NOT require changes to adoption, delivery,
  control, or the frame's own software.
- **FR-017**: A failure in one photo source MUST NOT affect frames or selections using a different
  source.
- **FR-018**: The system MUST exclude media it cannot display (such as video) from the photo pool
  without failing the rest of the pool.

#### Delivery and presentation

- **FR-019**: Photos MUST be prepared for the frame's exact screen geometry before they reach the
  frame; the frame MUST NOT be required to resize, rotate, or reinterpret them.
- **FR-020**: Preparation MUST honour the photo's embedded orientation so photos appear the right
  way up.
- **FR-021**: Preparation MUST preserve proportions — photos MUST never be stretched or squashed.
- **FR-022**: Portrait-oriented photos on a landscape-oriented screen MUST be presented through a
  deliberate, visually acceptable treatment rather than an arbitrary crop through the subject.
- **FR-023**: The frame MUST retain a local cache of prepared photos large enough to continue the
  slideshow through a controller restart or a network outage.
- **FR-024**: The frame MUST have the next photo ready before it is needed, so transitions never
  wait on a download.
- **FR-025**: Transitions between photos MUST be visually smooth, with no flicker, tearing, or
  blank gap.
- **FR-026**: The frame MUST continue its slideshow from its local cache whenever the controller is
  unreachable, and MUST reconnect and resume automatically without user action.
- **FR-027**: The frame MUST resume showing photos after a power loss without any setup step and
  without waiting for the controller.
- **FR-028**: The frame MUST manage its local storage within a bounded budget, evicting the
  least-useful cached photos rather than filling the medium.
- **FR-029**: A photo that fails to prepare, transfer, or decode MUST be skipped silently, with the
  slideshow continuing.
- **FR-030**: The frame MUST tolerate a missing, full, or unreadable storage medium by degrading to
  in-memory operation rather than stopping.

#### Control and observability

- **FR-031**: The owner MUST be able to see each frame's connection state and what it is currently
  showing, from Home Assistant.
- **FR-032**: The owner MUST be able to pause and resume the slideshow, and skip to the next or
  previous photo.
- **FR-033**: The owner MUST be able to set the rotation interval and the screen brightness, and
  those settings MUST survive a power cycle.
- **FR-034**: The owner MUST be able to turn the frame's screen off and on, with the screen off
  state drawing measurably less power.
- **FR-035**: Every control MUST be usable from a Home Assistant automation, not only manually.
- **FR-036**: The system MUST offer a way to display one specific chosen photo on demand, after
  which normal rotation resumes.
- **FR-037**: Diagnostic detail — addresses, identifiers, error text, version numbers — MUST be
  available to the owner in Home Assistant and on the device's wired diagnostic output, and MUST
  NOT appear on the frame's screen once the frame is adopted.
- **FR-038**: Problems the owner must act on (an unavailable selection, an expired account) MUST be
  surfaced in Home Assistant in language a non-technical person can act on.

#### Reset, removal, and privacy

- **FR-039**: Removing a frame from Home Assistant MUST stop photo delivery and return the frame to
  an adoptable state.
- **FR-040**: The frame MUST offer a deliberate, confirmed on-device reset that erases stored
  photos, the network credential, and the controller binding.
- **FR-041**: The on-device reset MUST NOT be triggerable by casual or accidental contact.
- **FR-042**: A reset frame MUST retain no photo, no credential, and no reference to a prior
  controller.
- **FR-043**: Photos and credentials MUST NOT be transmitted anywhere except between the frame and
  its own adopted Home Assistant instance, and between Home Assistant and the photo source the
  owner chose.

#### Distribution

- **FR-044**: The Home Assistant integration MUST be installable through HACS as a custom
  repository and MUST be configurable entirely through Home Assistant's interface, with no manual
  file editing.
- **FR-045**: All user-visible text in the integration MUST be translatable.
- **FR-046**: The integration MUST be removable cleanly, leaving no orphaned devices, entities, or
  stored credentials behind.

### Key Entities

- **Frame**: A physical device. Has a stable identity, a human-readable name, a screen geometry, a
  connection state, a current photo, and exactly one controller once adopted.
- **Photo Source**: A configured connection to a place photos come from — a Home Assistant media
  source, a directly connected photo service account, or object storage. Owns whatever credential
  that place requires. One source may serve many frames.
- **Collection**: A named grouping within a source — an album, bucket, or folder. Some sources have
  none.
- **Selection**: What the owner chose for a frame: one or more collections, an explicit set of
  photos, or both. Belongs to one frame. Resolves to a photo pool.
- **Photo Pool**: The current resolved set of photos a frame draws from, refreshed periodically
  from the selection.
- **Prepared Photo**: A single photo processed to the frame's exact screen geometry, ready to
  display with no further work. The unit that is delivered and cached.
- **Frame Cache**: The frame's local store of prepared photos, bounded in size, that keeps the
  slideshow running when the controller is unreachable.
- **Presentation Settings**: Per-frame preferences — rotation interval, order, brightness, screen
  schedule, transition style — that persist across power cycles.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A non-technical person, given only the frame and a phone, gets from powered-off to
  photos on screen in under 10 minutes without written instructions or outside help.
- **SC-002**: Adoption in Home Assistant — from discovery card to named, configured frame — takes
  under 2 minutes and fewer than 10 interactions.
- **SC-003**: 100% of photos shown appear correctly oriented and with proportions preserved, across
  a test set spanning landscape, portrait, camera-rotated, oversized, and undersized photos.
- **SC-004**: Transitions between photos complete with no visible flicker, tearing, or blank gap,
  confirmed by recorded video at full frame rate.
- **SC-005**: A frame continues its slideshow with zero visible interruption through a full Home
  Assistant restart and through a 30-minute total network outage.
- **SC-006**: After connectivity is restored, a frame resumes receiving new photos within 60
  seconds with no user action.
- **SC-007**: A power-cycled frame is showing photos again within 30 seconds of power-on, without
  contacting the controller first.
- **SC-008**: Changing a frame's album selection is reflected on the screen within 60 seconds.
- **SC-009**: For a photo source that supports live collections, a photo newly added to a selected
  album appears on the frame within one refresh interval with no user action. For a source with
  frozen selections, the limitation is stated in the interface at selection time and the frame
  continues showing its existing photos when the selection lapses.
- **SC-010**: Every control responds on the frame within 2 seconds of being triggered in Home
  Assistant while the frame is online.
- **SC-011**: Configuration and album browsing remain responsive with a source containing at least
  20,000 photos.
- **SC-012**: No screen of an adopted frame ever displays an address, an identifier, a stack trace,
  an error code, or a version string — verified by reviewing every reachable display state.
- **SC-013**: A second photo source can be added by implementing one well-defined seam, with zero
  changes to adoption, delivery, control, or frame software — demonstrated by actually adding one.
- **SC-014**: A reset frame contains no recoverable photo, credential, or controller reference.
- **SC-015**: The integration installs from HACS and passes Home Assistant's own integration
  validation with no errors.
- **SC-016**: A frame runs for 7 consecutive days without a reboot, a memory-related failure, or a
  visible fault.

## Assumptions

- The frame's screen is 800x1280 physical pixels and is mounted in landscape orientation, giving an
  effective 1280x800 canvas. Both orientations are worth supporting eventually, but landscape is the
  target for this feature.
- Home Assistant runs on the same local network as the frame and is reachable at all times except
  during the outages described in User Story 4.
- The owner is comfortable with Home Assistant. The recipient of the gift is not, and is never
  required to interact with Home Assistant or the frame's configuration at all.
- Home Assistant's built-in Google Photos support handles the Google account connection and photo
  selection where it can. Where the owner wants a direct connection instead, they are willing to
  create their own Google Cloud project — the same requirement Home Assistant's own Google
  integrations impose.
- Google's withdrawal of library-wide album listing in March 2025 means "select an album" for
  Google Photos in practice means "select photos through Google's own picker". Phase 0 research
  confirmed that Home Assistant's built-in Google Photos support cannot list a user's own albums —
  it exposes only what Home Assistant itself uploaded — so a direct picker-based connection is
  required rather than optional. Picker selections are fixed at the moment they are made and expire
  after a period set by Google, which is why FR-014, FR-014a, and FR-014b are worded per-source
  rather than as a universal guarantee.
- Video and non-photo media are out of scope for this feature.
- The frame is mains-powered. Battery operation is out of scope.
- Touch interaction on the frame is out of scope for this feature beyond what the reset in User
  Story 7 requires; all control comes from Home Assistant.
- Audio, the camera connector, and the real-time clock battery are out of scope for this feature.
- The existing on-device Google sign-in built into the frame's setup portal is superseded by this
  feature and is removed as part of it.
- Photos are delivered over the local network only; no photo bytes leave the home except as part of
  fetching them from the owner's chosen source.
