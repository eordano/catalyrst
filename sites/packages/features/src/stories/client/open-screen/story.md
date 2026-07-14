---
id: client-open-screen
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Replacing the events/places "things to do" list a player sees when the
    client opens with either a busiest-scene auto-spawn or a three-card chooser
    raises the share of sessions that actually jump into a scene.
  because: >-
    The browse grid asks a cold-open player to make a many-option choice before
    anything happens; dropping them straight into the single busiest live scene
    (genesis) or collapsing the choice to three intents -- jump into the action,
    surprise me, customize your avatar (three-cards) -- removes the stall, so
    more open-screen sessions should convert to a /places/:id jump-in.
metric:
  primary: cl_open_jumped_in_rate
  guardrails:
    - cl_open_screen_shown
    - place_card_clicked
    - cl_open_genesis_spawn
    - cl_open_card_clicked
experiment:
  key: client_open_screen
  unit: session
  variants:
    - id: base
      weight: 90
      flags:
        openScreen: base
    - id: genesis
      weight: 5
      flags:
        openScreen: genesis
    - id: three-cards
      weight: 5
      flags:
        openScreen: three-cards
  baseline: 0.0
  mde: 0.05
decision:
  rule: >-
    Draft -- do not ship from this readout. base is control. Promote an arm only
    if cl_open_jumped_in_rate (cl_open_jumped_in / experiment_exposed) beats
    base with 95% confidence and no guardrail collapses. Cross-arm guardrail:
    cl_open_screen_shown must track experiment_exposed (genesis no-live sessions
    redirect BEFORE exposure, so they are not counted -- the two should agree per
    arm). Per-arm engagement guardrails must not crater vs their own arm's
    baseline: base place_card_clicked, three-cards cl_open_card_clicked (these
    are arm-specific, not comparable across arms). Otherwise keep base and
    revisit the open-screen affordance.
---

# Client -- What a player sees when the client opens

The CLIENT-OPEN screen at `/client/open-screen`. Three arms, assigned per
session (`sid` hash), all landing on the existing `/places/:id` jump-in flow:

- **base** -- today's behavior: the events/places "things to do" list -- the same
  live `/places` browse grid (`loadPlaces`, SSR) as the
  `client/explore-open` surface, place cards -> `/places/:id` jump-in. Reused,
  not duplicated.
- **genesis** -- drop the player straight into where the action is: a brief
  "Now entering: <place>" screen naming the live place with the
  highest `user_count` (via the existing `fetchMostActivePlaces` loader), then
  the existing `/places/:id` jump-in for that place. If no live reading is
  available it redirects straight to the Places grid (`/places`, server-side in
  the loader) -- never a dead-end prompt, never an invented destination.
- **three-cards** -- a main-screen chooser with exactly three cards:
  (1) Genesis / "Jump into the action" -> the busiest-place jump-in (same target
  as the genesis arm); (2) "Surprise me" -> a random live place
  (`user_count > 0`) jump-in; (3) "Customize your avatar" -> the existing
  backpack route (`/bevy-overlay/backpack-equip`).

Journey:

1. `/client/open-screen` -- loader resolves the arm (deterministic sid bucket;
   `?arm=` / `?variant=client_open_screen:<arm>` force an arm for preview).
2. `experiment_exposed` fires in the loader -- but only AFTER the genesis
   no-live redirect check, so a genesis session with no live scene (redirected
   to `/places`) is not counted as exposed. `cl_open_screen_shown {variant}`
   fires when the surface paints (so it tracks `experiment_exposed` per arm).
3. base: `cl_explore_opened {place_count}` on grid render; a card click fires
   `place_card_clicked {place_id}`. genesis: `cl_open_genesis_spawn {place_id,
   user_count}` when the busiest scene resolves. three-cards:
   `cl_open_card_clicked {target}` per card.
4. Any jump-in conversion fires the shared `cl_open_jumped_in {variant,
   place_id}` -- the primary-metric numerator -- then hands off to the existing
   `/places/:id` jump-in flow.

Primary metric `cl_open_jumped_in_rate` = `cl_open_jumped_in` /
`experiment_exposed`, per arm -- derivable with `npm run story:readout --
client/open-screen` and the catalyrst-telemetry experiment readout.

Preview (each arm forceable regardless of bucket):

- base: `/client/open-screen?arm=base`
- genesis: `/client/open-screen?arm=genesis`
- three-cards: `/client/open-screen?arm=three-cards`
