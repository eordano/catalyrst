# Decentraland lobby

<!-- impeccable:product-schema 1 -->

## Platform

Web, in the React overlay of Bevy Explorer.

## Users and purpose

Players entering or returning to Decentraland need to see their avatar, find
friends, revisit destinations, discover places and events, and enter the world.

## Capabilities and constraints

The user-provided `lobby.png` establishes the visual direction; the follow-up
asks for improved positioning and polish beyond it: account controls at the top,
welcome and friends on the left, the real Bevy avatar in the center, live and
upcoming events on the right, recent and recommended places along the bottom.
Hovering or focusing the avatar reveals an edit button; touch always shows it.
Editing opens an inline Backpack with local drafts and explicit Save/Cancel.
Dragging rotates the avatar; idle animation and an equipped-emote control keep it live; search remains available from the header. Notifications,
profile, event discovery and marketplace controls use existing routes.

All names, balances, places, events and online states come from existing services
or the bridge. Unknown balances show Credits, not a made-up number. Friend Join
requires a known location. World destinations use realm changes; land destinations
use realm-aware parcel navigation. Failed requests have explicit recovery.

Authentication/onboarding and the independent sidebar design remain separate.
The profile picture opens the lobby. Escape returns directly to the scene, including
from inline editing, search, place details and full-screen panels. Unsaved avatar
drafts are discarded when leaving the lobby.

Continue exploring uses the last destination, or Genesis Plaza for new visitors.
Explicit realm/position/preview links keep their scene as the main entry action.
Place cards open information before an explicit Jump in. Hero, recent and
recommended destinations are deduplicated. Friends retain readable locations and
visible Join controls. One live event and two upcoming events replace event
carousels; start times update each minute. Mobile keeps entry near the bottom.
Loading sections reserve space and the cached avatar remains until its first frame.
Background editing, furniture, lobby monetization and mock social activity are
outside this scope.

## Evidence

The user's September 18 screenshot supersedes the earlier concept PDF. The
background is a reconstructed plate; the UI and avatar remain live and interactive.
