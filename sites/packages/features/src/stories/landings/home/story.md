---
id: landings-home
status: running
owner: owner@example.com
hypothesis:
  statement: >-
    Leading the decentraland.org root landing with a single, prominent
    "Download for desktop" CTA in the hero (above the feature rails) instead of
    burying the download choice will increase the share of first-time visitors
    who start a desktop-client download from the home page.
  because: >-
    The home page is the highest-traffic, lowest-intent surface; most arrivals
    are evaluating whether to try Decentraland at all. A confident, single
    primary CTA in the first viewport removes the "where do I start?" friction,
    so a larger share of home views convert into a download click before the
    visitor scrolls away.
metric:
  primary: lp_home_download_ctr
  numerator: lp_home_download_clicked
  denominator: lp_home_viewed
  guardrails:
    - lp_home_viewed
    - lp_home_rail_viewed
    - lp_home_rail_clicked
    - lp_home_download_clicked
experiment:
  key: lp_home_hero_cta
  unit: session
  variants:
    - id: hero_cta
      weight: 50
      flags:
        heroDownloadCta: true
        showFeatureRails: true
    - id: rails_first
      weight: 50
      flags:
        heroDownloadCta: false
        showFeatureRails: true
  baseline: 0.12
  mde: 0.03
  min_sample: 12000
decision:
  rule: >-
    Ship hero_cta if lp_home_download_ctr (lp_home_download_clicked /
    lp_home_viewed) beats rails_first with 95% confidence and no guardrail
    regresses beyond tolerance (feature-rail engagement, lp_home_rail_clicked,
    must NOT drop materially); otherwise keep rails_first.
---

# Story -- Sites marketing home landing (decentraland.org root)

The decentraland.org front door: ui3's canonical `SitesHome` composition (the
marketing chrome + dark-violet hero, download CTAs, and the What's On / Catch
the Vibe / Weekly Rituals feature rails + closing "Come Hang Out" banner)
carried under its own route + story id so hero/CTA conversion is attributable.

This is the SAME surface `app/routes/_index.tsx` renders for `/`, but `_index`
carries no landings story id -- so `/landings/home` adds the conversion telemetry
the audit calls for (track hero/CTA conversion + feature-rail engagement).

Priority **spec**: loader + components from `loaderData` (NO XState wizard). The
loader returns the validated static marketing content with the What's On rail
OPTIONALLY hydrated from live catalyst highlights.

## Journey (URL-addressable steps)

1. **view** -- `/landings/home` -- page mounts. Emits `lp_home_viewed`
   ({ rails, live_rail }).
2. **hero** -- `/landings/home#hero` -- the hero + primary download CTA scroll
   target.
3. **feature-rails** -- `/landings/home#feature-rails` -- the What's On / Catch
   the Vibe / Weekly Rituals rails. Each rail emits `lp_home_rail_viewed`
   ({ rail }) once, the first time it scrolls into view (IntersectionObserver);
   a rail "View All" / item click emits `lp_home_rail_clicked` ({ rail, target }).
4. **download-cta** -- `/landings/home#download-cta` -- the hero download buttons.
   A hero or closing-banner download click emits `lp_home_download_clicked`
   ({ store, placement }).

Primary metric `lp_home_download_ctr = lp_home_download_clicked / lp_home_viewed`.

## Variants

- **hero_cta** (`heroDownloadCta: true`): the primary "Download for desktop" CTA
  leads the hero, above the feature rails -- the treatment.
- **rails_first** (`heroDownloadCta: false`): the feature rails lead and the
  download CTA is de-emphasized to the closing "Come Hang Out" banner -- the
  control. (Flag is plumbed in the story for the readout; both arms render the
  same `SitesHome` surface in this spec build -- the placement swap is the future
  build behind the flag.)

## Data

Static marketing content from `app/fixtures/landings-home.json` -- copy mirrored
verbatim from `decentraland/sites` intl `en.json` (`page.home.*`: hero,
whats_on, catch_the_vibe, weekly_rituals, come_hang_out) and the ui3
`SitesHome.jsx` composition. See the fixture's `_source`.

The **What's On** rail is OPTIONALLY hydrated with LIVE highlighted events from
catalyst (`GET /events/api/events?list=highlight`) via
`lib/catalyst/home.server.ts` (`loadHome`) -> `lib/catalyst/home.ts`
(`withLiveEvents` / `eventToRailItem`). Best-effort: an empty/unreachable
catalyst degrades to the static fixture rail, so the page always renders
JS-free. There is no catalyst crate for the home page itself; the hero copy,
download CTAs, Catch-the-Vibe clips and Weekly-Rituals cards are static.

## Simulated / deferred

- The download CTAs are real navigations to the canonical decentraland.org
  destinations (desktop launcher + Epic Games Store + mobile stores); no install
  is performed here.
- `totalDownloads` is REAL: `loadDownloadsBadge()` sums GitHub
  release-asset `download_count` across `decentraland/launcher` +
  `decentraland/unity-explorer` (6h in-process cache), formatted as `+NNNK`.
  When the source is unreachable the badge row is hidden -- no static fallback.
- The variant placement swap (hero-first vs rails-first) is plumbed via the
  experiment flag for the readout but renders the same `SitesHome` surface in
  this spec build.
