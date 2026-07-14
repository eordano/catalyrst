---
id: bevy-overlay-passport
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Opening a player passport with a tabbed Overview / Badges / Photos layout
    (vs a single scrolling profile pane) increases the share of opened passports
    where the viewer engages a second module -- switching to Badges or Photos, or
    clicking a link/equipped item -- rather than bouncing off the overview.
  because: >-
    Surfacing badges and camera-reel photos as first-class tabs (instead of
    burying them below the about/links fold) makes the identity feel explorable,
    so more viewers who open a passport poke a second surface, which is the
    leading signal that profiles drive social discovery in the HUD.
metric:
  primary: cl_passport_engaged_rate
  numerator: cl_passport_tab_viewed
  denominator: cl_passport_opened
  guardrails:
    - cl_passport_opened
    - cl_passport_profile_empty
    - cl_passport_tab_viewed
decision:
  rule: >-
    Ship if cl_passport_engaged_rate (sessions with >=1 cl_passport_tab_viewed on
    a non-default tab OR a cl_passport_link_clicked / cl_passport_item_clicked,
    over cl_passport_opened) improves by at least the MDE with no guardrail
    regression -- passport opens hold and the empty-profile path stays graceful;
    otherwise hold.
experiment:
  key: cl_passport_tabs
  unit: session
  variants:
    - id: tabbed
      weight: 1
      flags:
        tabs: true
  baseline: 0.4
  mde: 0.05
  min_sample: 4000
---

# View a player passport / profile (other-user + self)

The passport overlay (`/client?panel=passport&address=<addr>`) opens a player's
public identity over the HUD. It is a tabbed panel -- **Overview**, **Badges**,
**Photos** -- with a [3D] avatar preview on the left and the identity header
(name#tag + copy address + CLAIM NAME) on the right.

- **Overview** (`&tab=overview`, default): the BADGES overview row, ABOUT ME
  (`description`), LINKS (`links[]`), and EQUIPPED ITEMS (`avatar.wearables`)
  modules. Own-profile empty states show "No intro." / "No links." with an
  inline edit pencil that deep-links self-edit to existing flows.
- **Badges** (`&tab=badges`): the earned-medallions grid grouped by category,
  with a category filter pill row (live `/badges/categories`).
- **Photos** (`&tab=photos`): the camera-reel photo grid; clicking a photo opens
  the photo lightbox (date / place / people + equipped wearables).

Each step is URL-addressable via `?panel=passport&address=<addr>&tab=<tab>` so
every journey step is a real, screenshot-able URL.

- **Primary metric:** `cl_passport_engaged_rate` = sessions that view a non-
  default tab or click a link/equipped item, over `cl_passport_opened`.
- **Guardrails:** passport opens (`cl_passport_opened`), the empty-profile path
  (`cl_passport_profile_empty`), and tab views (`cl_passport_tab_viewed`).
- **Events:** `cl_passport_opened` on mount (`{ address, self }`),
  `cl_passport_tab_viewed` (`{ tab }`) on each tab, `cl_passport_profile_empty`
  (when the live profile is empty and the fixture seeds the panel),
  `cl_passport_link_clicked` (`{ url }`), `cl_passport_item_clicked`
  (`{ urn, category }`), `cl_passport_photo_opened` (`{ photo_id }`),
  `cl_passport_claim_name` (deep-link to the claim-name flow, SIMULATED), and
  `cl_passport_edit` (`{ field }` -- self-edit deep-link, SIMULATED).

Data reality: the live realm returns **empty** profiles
(`/lambdas/profiles` POST -> `[]`; `/lambdas/profile/{addr}` -> `{avatars:[]}`),
**empty** user badges (`/badges/users/{addr}/badges` ->
`{achieved:[],notAchieved:[]}`), and **404s** camera-reel
(`/camera-reel/users/{addr}`). Only `/badges/categories` is live and real
(`["Builder","Explorer","Socializer"]`). The loader queries all four live first
and falls back to the schema-derived fixture
(`app/fixtures/bevy-overlay-passport.json`, with a `_source` note) when a source
is empty/unreachable. The profile shape mirrors decentraland/schemas
`src/platform/profile/{profile.ts,avatar.ts}`; badges mirror catalyrst-badges
`ports/types.rs`; photos mirror catalyrst-camera-reel `dto.rs`.

CLAIM NAME and self-edit (description / links) are **SIMULATED** here -- they
deep-link to the existing marketplace claim-name / profile-edit flows and emit
the declared events; no on-chain or content write happens in this route. Noted
as deferred.
