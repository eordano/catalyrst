# Bevy Explorer sidebar &#x2014; September 2026

Visual authority: [Figma frame 111:7268, New Navigation Bar](https://www.figma.com/proto/1J6TtmwcxH9jTuxq0XwbJL/Projects-and-Proposals?node-id=111-7268&page-id=1%3A1135&starting-point-node-id=111%3A7268). Scope is the Bevy Explorer in-world sidebar. Its exported vectors are in `sidebar-assets/`; labels remain accessible HTML and identity, location, clock and counts come from live data.

At the source frame&#x2019;s 1920&#xd7;1080 viewport, navigation circles measure 42px with 8px gaps, starting at (10,418). Quick actions end 10px above the bottom. The visible scene card is (11,12), 272&#xd7;133px, with a separate notification bell. Chat is (66,1030), 340&#xd7;40px. Drawers are 274px wide with 10px corners and 90% black backgrounds: Me is 377.48px tall, Discover 182.48px, and System 322.48px. Row icons and button artwork come from normal Figma SVG exports. The same Inter font is already bundled with the app.

Me contains Backpack, Profile, Badges, Sign In/Out and Exit. Discover opens Events, Places and Communities. Marketplace opens the existing shop. System exposes Settings, Mouse / Key Controls, FAQ, Report Bug, Contact Support and Discord. Profile [P] and Badges [B] are contextual Me shortcuts; outside that menu the existing world bindings remain. Badges uses the existing passport destination. Exit uses the supplied lobby callback when available, otherwise the site homepage.

The overflow menu contains the reference&#x2019;s auto-hide switch. Additional size, minimap and connection/performance controls remain accessible through Scene options &#x2192; Sidebar display settings. Escape restores focus, outside clicks dismiss menus, and HUD popups are mutually exclusive. Short or enlarged desktop layouts scroll the dock; narrow windows constrain menus. Touch devices retain the existing mobile HUD.

The scene clock queries the engine `/time` command while visible. Without that capability it displays &#x201c;Time of day&#x201d; and opens the skybox controls. Saving places requires a signed-in account and a matching catalog place; failures are surfaced. Reference portrait, world image and example unread counts are never shipped as live product state.

## Preview

The `2026-09-sidebar-design` flag is enabled by default. Open `/play/?2026-09-sidebar-design=1&2026-09-lobby=0` to preview directly in-world. `=0` selects the existing sidebar. The initial choice survives in-app navigation; reload to change it. URL previews do not change stored preferences.

Settings &#x2192; Feature Flags provides Default, Enabled and Disabled browser overrides, plus Reset all and Reload Explorer. A saved opt-out uses `localStorage.setItem("dcl.feature.2026-09-sidebar-design", "0")`; remove the key to follow the enabled release default. URL overrides take priority until replaced through Settings.

## Verification

`src/test/hud-sidebar-design.test.tsx` covers real routes, contextual shortcuts, popup exclusivity, focus, display settings and the mobile boundary. `src/data/sidebarDesignFlag.test.ts` covers default and override resolution. Storybook entries live under Explorer/Frames/Sidebar September 2026.

With the overlay Vite server running, execute `node tools/screen-tour/capture-bevy-sidebar.mts`. Set `HUD_PREVIEW_URL` and `TOUR_OUT` as needed. It captures the real HUD components with an explicitly staged identity, engine clock and population; external sockets are isolated. Optional `SIDEBAR_REFERENCE_BACKGROUND` (SVG) and `SIDEBAR_REFERENCE_AVATAR` (PNG or SVG) supply local Figma exports for comparison. The script asserts native geometry, checks narrow windows and chat, and fails on page errors. It writes screenshots, geometry and tour metadata; this fixture is not a running world.
