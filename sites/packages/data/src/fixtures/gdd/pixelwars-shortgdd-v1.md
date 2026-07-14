---
id: pixelwars-shortgdd-v1
title: Pixelwars shortGDD
kind: shortgdd
version: 1
source: slack-import
source_ref: https://decentralandteam.slack.com/archives/C0BDDMXGLU8/p1786561616202039
created_at: 2026-08-12T19:06:00.000Z
hypotheses: pixelwars-hypotheses
---
# Decentraland Experience Proposal

**Creator Success Program — Proposal Template (short GDD)**

| | |
|---|---|
| Experience name | Pixelwars |
| Studio / team name | Luke Escobar (TBD: team/studio name — confirm with owner) |
| Date | 2026-08-12 |
| Contact (Discord + email) | TBD: owner's Discord handle + email |

> Adapted from "Pixelwars Brief" (Luke Escobar, 2026-08-06) by agent, full-AFK mode. Everything the
> brief decided is kept as decided. Markers: `[agent-decided]` = chosen on the owner's behalf,
> pending review · `[HYPOTHESIS]` = a claim only a playtest can settle, parked in Appendix A ·
> `TBD:` = not known yet, with the plan to find out.

What we fund: **social-first, replayable, mobile-ready experiences with clear progression.** Program goal: **more than 20% of new players return within 7 days (D7 retention).**

---

## 0. TL;DR

| | |
|---|---|
| **One-line concept** | Team-based turf war in a maze that regenerates every 5-minute round — you paint territory by walking, live in Decentraland today. |
| **Team & total hours/week** | Luke Escobar (design + SDK7 code) + Stom (optimization); TBD: hours/week per person |
| **Current status** | **Small vertical slice live in a World: [pixelwars.dcl.eth](https://decentraland.org/jump?realm=pixelwars.dcl.eth&position=5,5)** — V0 runs on desktop, mobile and three.js clients, with multiplayer server, procedural mazes, portals and solo-play bots. [Gameplay video](https://youtu.be/Mocy6Xly7D4) |
| **Live at end of Week 6** | A player joins at any hour, gets a team, and fights 5-minute turf rounds in a fresh maze with dual team bases, a paint launcher, pickup items and a weekly league board — on desktop and mobile. |
| **Requested round** | v1 |

---

## 1. The Hook

It's like Splatoon's Turf War, but the arena is a maze that rebuilds itself every five-minute round — and your feet are the paintbrush: you claim territory just by walking.

---

## 2. First Session — written as "you"

You spawn on your team's base — the floor under your feet is already red, your red. A timer hangs overhead: 4:58. The tiles ahead are blank, and your first step flips one red with a pop. That's the whole tutorial: you walk, the world becomes yours.

A blue trail cuts across your path — someone is repainting your ground. You chase them down and fire your paint launcher; the blob splats them and three tiles at once. They respawn far away at their base, and their trail is yours again. The coverage bar up top shifts: 41% red, 38% blue. A glowing ink canister sits in a contested corridor — you grab it mid-run and your tank refills. You duck through a shimmering portal and pop out across the maze in untouched corridors. Free territory.

The horn sounds: red took 52%. Your team wins — and the maze tears itself down and rebuilds into a shape nobody has ever walked. New corridors, new portals. The next round is already counting down.

On the way out you pass the league board at your base: 3rd in this week's league of twenty. It resets Sunday. Two places from promotion.

*(Program gate — 80% of testers start playing within 5 seconds: `[HYPOTHESIS]` → H2-08.)*

---

## 3. Core Loop

| # | Step (verb) | What the player does | Why do it again? |
|---|---|---|---|
| 1 | **Paint** | Walk over tiles to flip them to your color | Enemies repaint your ground behind you — coverage is never safe |
| 2 | **Fight** | Chase intruders, shoot the paint launcher, dodge blobs (v1) | Splatting an enemy sends them home and frees their trail |
| 3 | **Grab** | Pick up items at contested midfield spots | A paint bomb or ink refill can flip a losing round |
| 4 | **Win** | Round ends at 5:00 — highest coverage % wins | League points; the maze regenerates → back to step 1 in a layout nobody has seen |

- **One loop takes:** one claim–contest–reclaim cycle ≈ 30–60 s; a full round is exactly 5 minutes.
- **One session lasts:** 2–3 rounds ≈ 10–15 minutes `[agent-decided]` — to be confirmed by V0 telemetry.
- **Why is the 10th repetition still fun?** Two named variability sources: **other players** (every round is a different fight — bots fill in when humans are absent, so this never drops to zero) and **procedural regeneration** (a fresh maze plus randomized portals every round; v1 adds hazards and shortcuts to the roll). Whether that carries a median session past round 2 is `[HYPOTHESIS]` → H3-03; whether the launcher deepens the loop instead of replacing walking is `[HYPOTHESIS]` → H2-03. *Evidence note: V0 is live and playable right now (link in §0) — the walk-to-paint verb is already proven in the field, not on paper.*

---

## 4. Why Players Come Back

*The brief contained no retention section; everything below is `[agent-decided]` design pending owner review, built to reuse the existing loop at near-zero content cost.*

### 4.1 The Day-2 sentence

> "A player who enjoyed Day 1 comes back on Day 2 because their weekly league standing is two places from promotion and the league resets **Sunday 00:00 UTC** — and because bots guarantee a live match at any hour, 'one quick round' never fails." `[agent-decided]` `[HYPOTHESIS]` → H3-02

### 4.2 The Day-7 player

A Day-7 player has persistent, visible state a Day-1 player does not: a **league rank** (league of ~20, shown on the base board), a **win record** and **lifetime tiles-painted count** next to their name. `[agent-decided]` TBD: confirm the multiplayer server supports per-player persistent stats; if not, this is the first infrastructure task of Week 1.

### 4.3 Return hooks — two

**Hook 1 — Weekly leaderboard reset / small leagues.** Every tile you flip and every round your team wins feeds a personal weekly score inside a league of ~20 players. The board stands at each team base — you pass it every spawn. Top slice promotes, bottom relegates, everything resets Sunday 00:00 UTC, so mid-table players always have a live race. Near-zero content cost; automatic weekly event. `[agent-decided]` `[HYPOTHESIS]` → H3-02

**Hook 2 — Recurring scheduled event.** One weekly match night at a fixed hour (e.g. Friday 20:00 UTC), announced in-world and on Discord — the platform has no push notifications, so the calendar is the notification system. The night concentrates scattered visits into real 4v4+ rounds, and doubles as the live playtest venue for H3-01. One reusable template, ≤1 person-day/week to run. `[agent-decided]` `[HYPOTHESIS]` → H3-04

### 4.4 The long-term goal

Climb to the top league tier — the rank is displayed next to the player's name on the base board, so every teammate and opponent sees progress toward it at every spawn. `[agent-decided]` V2 extends this with the alt-weapon sidegrade collection swapped at base (brief's own plan — unlockable rewards for returning players), teased in v1 but not shipped.

---

## 5. Social by Design

- **What is better — not just possible — with 2+ players?** The win condition itself is adversarial: territory only has value because someone else is taking it. Every extra player raises contest density — more repaint skirmishes, bigger coverage swings, real flanks through portals. Bots make the game playable alone; humans make it the actual game.

- **The quiet-hour test.** A solo player at 4 a.m. is not alone by design: **a ghost bot spawns whenever there is only one human** (already live in V0), so a full round is always available. The next human to arrive is auto-assigned to the opposite team — instant opponent, no coordination needed. The base league board shows this week's names and scores — asynchronous traces of everyone who played before you. TBD: verify team auto-balance assigns a joining second human against the first, not beside them.

- **The bystander test.** The floor **is** the game state. A bystander sees runners leaving colored trails, blobs flying, tiles flipping, and two coverage bars racing — the whole loop is legible in five seconds of watching, with nothing hidden in UI panels.

- **Bring-a-friend.** Friends who join together land on the same team when balance allows and feed the same team's coverage — "we held 60%" is a shared story. The weekly match night is the natural invite occasion ("come Friday, we need a defender"). `[agent-decided]` — no dedicated invite reward in v1; parked in ideas.md for v2.

- **The memorable moment.** The last-ten-seconds flip: your team is down 44–48, someone triggers a paint bomb at midfield, and the coverage bar crosses over as the horn sounds. Clip-ready because the whole reversal is visible on the floor and on one bar. `[agent-decided]` — requires items (§9 Week 3) and the coverage bar (live in V0).

- **Roles emerge without being assigned** — runners claim, defenders hold, attackers open paths: `[HYPOTHESIS]` → H3-01.

---

## 6. Mobile-First

V0 already runs on the mobile client today (§0 link) — mobile is a maintained reality, not a promise.

| Core-loop verb | How it works with touch controls |
|---|---|
| **Paint** (walk) | Virtual joystick — walking is painting; the core verb needs zero extra input. Touch-native by construction. |
| **Fight** (shoot) | Single on-screen action button (E-equivalent) with an aim-assist cone — no precision aiming on the critical path. `[agent-decided]` `[HYPOTHESIS]` → H2-06 |
| **Grab** (items) | Walk-over pickup — no button at all. |
| **Win** / context (F) | One contextual button that changes meaning by location: interact at base, hide/quick-move on own paint (if H2-04/05 land). |

**UI plan.** The entire HUD is a round timer and two coverage bars, top-center — already shipping on mobile in V0 and readable at arm's length; v1 adds only an ammo indicator near the action button, inside thumb reach. `[agent-decided]`

**Performance.** Biggest single risk: **paint-tile entity count** — the planned 2m→1m grid quadruples tiles. Plan: arithmetic on entity budget first, then a load-time measurement on Stom's refactored branch (`[HYPOTHESIS]` → H2-01); bounded fallback: stay at 2m and port Stom's material optimization back, which V0 already proves works.

**Desktop-only dependencies.** None known — V0 runs on desktop, mobile and three.js clients. The hide mechanic leans on `AvatarModifierArea` / `InputModifier`: TBD: check both against the Desktop vs Mobile Feature Gap tracker during the Week-4 spike; the design seam is clean — if unsupported on mobile, the named fallback (sprint-on-own-paint buff) uses no gated API.

---

## 7. World, Look & Story

**Story.** Two paint factions endlessly repaint a living maze that rebuilds itself after every battle. The floor itself is the only story — a five-minute mural of who fought where.

**Visual direction.** Stylized low-poly blocks with bold two-faction color language, as already live in V0 (screenshot below) — recognizable Decentraland quality in line with Genesis Plaza. References: **Pixelwars V0 itself** (live), **Flagtag** (the team's shipped DCL game — readable arena play), and Splatoon's turf-color readability as *design* inspiration only (no third-party assets). v1 explores one new architectural skin for the 6 modular blocks — remodel-and-swap keeps this cheap.

**Image.**

![Pixelwars V0 gameplay — live scene](img/v0-gameplay.png)
![The 6 modular level blocks that assemble every maze](img/level-modules.png)

---

## 8. Comparables — exactly two

| | Comparable A — Decentraland: **Flagtag** (our own shipped game) | Comparable B — outside: **Splatoon (Nintendo)** |
|---|---|---|
| What worked | Projectile system, item spawn framework, chest/loadout UI — all proven live and directly portable to Pixelwars (this is why combat feasibility is high) | Paint as a unified resource — score, territory and mobility in one substance; turf war readable at a glance; short 3-minute rounds |
| What didn't work | TBD: owner to state honestly — what did Flagtag's retention outside events look like, and why? | Depends on precision aiming and high-tickrate netcode — impossible at DCL's 5Hz server tick; the movement-shooter skill floor excludes casual and mobile players `[agent-decided]` |
| What we do differently | Always-on via bots instead of event-dependent concurrency; the game state lives on the floor, visible to bystanders | Walking paints — a zero-skill-floor core verb; procedural maze regeneration replaces authored maps; combat is a layer on top of the loop, not the loop |

---

## 9. Six-Week Plan (v1 scope)

| Week | What is playable / done |
|---|---|
| 1 — Prototype definition | Dual bases: second seed tile, red/blue spawns at opposite ends. H2-01 arithmetic done (1m grid go/no-go on paper). Projectile port from Flagtag started. TBD: per-player persistence check for the league (§4.2). |
| 2 — Core interaction test version | **Program milestone:** functional core loop, single-player + multiplayer — dual-base rounds live, paint launcher v0 firing and claiming tiles. |
| 3 — Core systems refinement | Combat tuning: health bar, respawn-with-cooldown, hit registration (H2-02). First 3 items in: speed boost, ink refill, paint bomb. |
| 4 — Playable prototype, final design direction (mobile playtest) | **Program milestone:** mobile playtest incl. touch aiming (H2-06). Hide/quick-move timeboxed spike runs and gets its verdict (H2-04, H2-05) — in for weeks 5–6 polish, or fallback locked. Weekly league board v0 at bases. |
| 5–6 — Live testing and sign-off | One new level skin set for the 6 blocks; portals + one hazard type (paint drain zone) in the generator; optimization pass (1m grid or 2m + material opt per H2-01); live on pixelwars.dcl.eth, public repo delivered. |

**Not building in v1 — exactly 3 cuts:**

1. **Alt weapons and the chest/loadout system** — the paint launcher ships alone and ships well; weapon variety is V2 (brief's own split, and it hurts: it delays the returning-player reward).
2. **Combat-capable bot AI** — bots stay at V0 level (walk, paint, chase); they do not aim or shoot in v1. Solo play keeps working, but solo players won't feel the full combat game until v2.
3. **Skin variety** — exactly one new architectural skin set, no seasonal or partner themes in v1, even though the 6-block swap makes them tempting and cheap.

**Standing non-goals:**

- Never a second simultaneous weapon key — E is the single primary, F stays contextual, because DCL exposes two action keys and mobile has even less input room (this already refused in-round weapon switching; variety lives at the base swap in V2).

**Top risk + fallback.** Projectile-vs-player hit registration at the server's 5Hz tick — too coarse for combat (`[HYPOTHESIS]` → H2-02). Plan A: raise projectile tick / client-side prediction (Flagtag code as the base). Plan B if it still feels bad by Week 3: the launcher ships as a **territory tool only** (paint at range, no damage), and v1 is walk-paint + ranged-paint + items — still a strict upgrade over V0. The hide/quick-move mechanic is deliberately *not* the top risk: it is a Week-4 timeboxed spike with a pre-named fallback (sprint on own paint), so it cannot sink the schedule (H2-04, H2-05).

---

## 10. Success Criteria — what you will measure

Honest calibration up front: 5-minute-round arcade games rarely reach the program's 20% D7 on fun alone — that is exactly why §4 adds a league meta and a weekly appointment rather than promising the number.

- **The 3 numbers from Week 1 live:**
  1. % of new players who flip a tile within 10 s of spawn — target ≥80% (program gate; → H2-08);
  2. median completed rounds per session — target ≥2 (→ H3-03);
  3. % of new players returning within 7 days — program target ≥20%, honesty bar ≥8% (industry median; → H3-02).

- **First-session funnel:** spawn → first tile painted → first round completed → second round started → league board seen → session end. `[agent-decided]`

- **Pivot thresholds (2 weeks live):** D7 < 8% → the league/event hooks failed (H3-02 failed), redesign the meta before adding content. Median rounds/session < 2 → loop-depth problem: rebalance combat/items, not visuals. First-tile-within-10s < 60% → FTUE/spawn problem: fix spawn placement and readability first. `[agent-decided]`

---

## 11. Team

| Person (name/handle) | Role in this project | Hours/week | Links to past work |
|---|---|---|---|
| Luke Escobar | Design, SDK7/TypeScript code, live-ops | TBD: hours/week | [Pixelwars V0 live](https://decentraland.org/jump?realm=pixelwars.dcl.eth&position=5,5) · [gameplay video](https://youtu.be/Mocy6Xly7D4) · [Flagtag live](https://decentraland.org/jump?realm=flagtag.dcl.eth&position=24,24) |
| Stom | Rendering/optimization (built the 1m-grid + material-optimization branch) | TBD: hours/week + role confirmation | TBD: links |

**Coverage check:** Code (SDK7/TS) — covered, two shipped multiplayer DCL games prove it. Design — covered (V0 is live and, per its players, fun). 3D & art — **named gap:** TBD: who remodels the 6 block skins and the original ghost models; plan: the modular 6-block structure keeps the art surface small, and Week 5–6 scopes exactly one skin set to fit it.

---

## 12. Deliverables & Declaration

**With the v1 round (6 weeks) we will deliver:**

- Players fight 5-minute turf rounds with dual team bases and a tuned paint launcher — on desktop and mobile.
- Players grab midfield items (speed boost, ink refill, paint bomb) that can flip a losing round.
- Players climb a weekly small league shown at their base; it resets every Sunday.
- A solo player at any hour gets a full round against bots and can complete every loop verb except PvP.
- Every round runs in a freshly regenerated maze with portals and one hazard type, in a new architectural skin.

**Content & IP declaration.** All 3D assets, code and audio are original work by the team (Pixelwars V0 and Flagtag codebases are our own). Splatoon is cited strictly as design inspiration — no Nintendo assets, names or trade dress are used. The V0 placeholder ghost will be replaced by **original** red/blue cartoon ghost designs, deliberately kept legally distinct from Pac-Man (the brief's "pacman styling" phrasing is retired — see decisions.md 2026-08-12). TBD: confirm no licensed audio/fonts are currently in the V0 repo before the public-repo delivery in Week 6.

---

## Appendix A — Hypothesis Log

*Generated from `design/02-core-loop/` and `design/03-vertical-slice/` — do not edit statuses by hand; rename the experiment file and regenerate. Ordered by cheapest killing test (arithmetic → desktop → mobile → live).*

| ID | IF / THEN (falsifiable) | Source section | Cheapest killing test | Status | Verdict / date | Tested on |
|---|---|---|---|---|---|---|
| H2-01 | IF the paint grid drops 2m→1m (≈4× tiles), THEN load time ≤1.5× V0 and entity count within SDK7 limits | §6 performance; §9 W1 | Arithmetic in doc, then load test on Stom's branch (desktop) | parked | | — |
| H2-02 | IF projectile hits use client prediction / raised tick, THEN ≥90% of visually on-target shots register | §9 top risk; §3 fight | Desktop, 2 clients, 20 counted shots | parked | | — |
| H2-03 | IF the launcher claims at range with limited ammo, THEN walking still flips ≥50% of tiles | §3 loop | Desktop round with bots, log claim source | parked | | — |
| H2-04 | IF hiding = AvatarModifierArea + InputModifier + marker tile, THEN a 3-day spike delivers all 3 behaviors | §9 W4 spike | Desktop greybox spike, hard 3-day timebox | parked | | — |
| H2-05 | IF quick-move teleports along own paint at 2× speed, THEN owner self-test finds motion jitter-free | §3; brief fallback named | Desktop, owner self-test, 1 corridor | parked | | — |
| H2-07 | IF items spawn at contested midfield, THEN ≥50% of pickups → an encounter within 10 s | §3 grab; §5 moment | Desktop round with bots, log intervals | parked | | — |
| H2-08 | IF spawn faces unclaimed midfield, THEN ≥80% of first-timers flip a tile within 10 s unprompted | §2; §10 funnel | 3 first-time testers on live V0, stopwatch | parked | | — |
| H2-06 | IF the launcher fires from one button with aim assist, THEN mobile self-test hits ≥50% on a moving bot | §6 touch mapping | Mobile QR pass at W4 playtest | parked | | — |
| H3-03 | IF the maze regenerates instantly at round end, THEN median session ≥2 completed rounds | §3 10th repetition | Live telemetry — instrumentable on V0 now | parked | | — |
| H3-04 | IF a weekly match night is announced, THEN event peak CCU ≥3× weekday median | §4.3 hook 2 | One announced event on live V0 vs baseline | parked | | — |
| H3-01 | IF combat + dual bases ship, THEN ≥50% of players in a ≥6-player round are classifiable as runner/defender/attacker by observation | §5; brief §2 | Observed live round on event night | parked | | — |
| H3-02 | IF weekly ~20-player leagues reset Sunday, visible at base, THEN D7 ≥20% (honesty bar ≥8%) | §4.1, §4.3 hook 1 | Paper (persistence check) → 2 weeks live telemetry | parked | | — |
