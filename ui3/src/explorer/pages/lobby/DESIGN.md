# Reference lobby

The user's `lobby.png` (1920 &#xd7; 1080, September 18, 2026) is the visual starting point.
The dusk plaza background was reconstructed from that image with image generation,
removing its sample UI and character. It is an approximation of the original art,
not an original unobstructed source export. The shipped WebP is in `assets/`.

The user's follow-up asks for a more polished composition beyond the screenshot.
Mode: Experience, with the avatar and plaza leading and the interface supporting
world entry and discovery. The screenshot remains the palette and scene reference.

The desktop layout is a three-column grid with a 360px welcome column, a flexible
avatar stage, and a 320px event column. Outer margins scale from 24 to 64px.
Related controls use 8&#x2013;16px spacing; sections use 28&#x2013;32px. The 23&#x2013;30px welcome
heading establishes hierarchy above quiet 12px section labels. The avatar retains
an elevated 12&#xb0; camera in a taller stage, framing the whole body rather than
cropping it with zoom. Customize avatar appears on hover/focus (always on touch).
The avatar preview and its action occupy separate grid rows, with the action
inside the stage boundary. Neither may extend into the discovery section.
Section headings wrap their actions when space is constrained.

Cards use translucent ink surfaces (#17131cd9), white type, turquoise account names
(#4effea) and pink actions (#ff2859). Destination photographs fill their cards;
labels remain in place on hover. The welcome action spans the card width. Empty
recent history takes a compact column, leaving more width for recommendations.

Below 1000px the avatar leads two columns of discovery; below 600px these become
one scrollable column. Short desktop windows also reflow. Carousel dots retain
small visual marks within 24px targets. Focus, reduced motion, loading and error
states remain explicit. Missing activity is never fabricated.

Validation: `tools/screen-tour/capture-lobby-reference.mts` captures the actual
Bevy engine from 320 to 1920px and exercises search and world entry. Empty and
populated recent history both verify that the avatar action stays within its
stage, clears discovery, and that section titles do not overlap their actions.

September 18 iteration: the continuation action sits beneath the left column on
desktop and beneath the shorter avatar on mobile, with a fixed mobile entry button.
Backpack uses a two-column avatar/wardrobe composition, stacking on narrow screens.
Only Save changes the engine outfit; previews and Cancel remain local. Place details
use an inline side panel, and live event discovery is a single feature plus a short
upcoming list. A soft contact shadow anchors the avatar; the supplied plaza remains
the lighting and background reference. The browser tour also exercises draft cancel,
real wearable save acknowledgement, rotation and Escape from editing.
