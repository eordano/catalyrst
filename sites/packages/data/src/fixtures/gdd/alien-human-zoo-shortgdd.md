---
id: alien-human-zoo-shortgdd-v1
title: Alien Human Zoo shortGDD
kind: shortgdd
version: 1
source: slack-import
source_ref: https://decentralandteam.slack.com/archives/C0BDDMXGLU8/p1786568416552999
created_at: 2026-08-12T21:02:00.000Z
---
# Decentraland Experience Proposal

**Creator Success Program — Proposal Template (short GDD)**

| | |
|---|---|
| Experience name | Alien Human Zoo (working title) |
| Studio / team name | TBD: confirm with owner |
| Date | 2026-08-12 |
| Contact (Discord + email) | nearnshaw@decentraland.org (Discord TBD) |

> **Draft status:** Stage 1 interview in progress (Solidify route). Sections marked `TBD:` /
> `[OPEN]` are not yet decided. Claims marked `[HYPOTHESIS]` have a row in Appendix A.

---

## 0. TL;DR

*Filled last, after everything else.*

| | |
|---|---|
| **One-line concept** | Reigns-style advice runs: aliens ask broken-translation questions about their caged humans, you answer with their own odd options, and they implement everything literally |
| **Team & total hours/week** | TBD: §11 interview (stage 2) |
| **Current status** | idea — stage-1 document done; find-the-fun test (H1-01) not yet run |
| **Live at end of Week 6** | [OPEN] — after §9 |
| **Requested round** | v1 |

---

## 1. The Hook

> **"It's like Reigns, but you're the consultant telling aliens how to keep the humans in their
> zoo alive — and they take everything you say catastrophically literally."**

("Give them water" → 100% air humidity. "They need air circulation" → a 100 km/h turbine.)

Supporting mechanics, not the twist: consequences only appear after the aliens implement them, and
the caged human comments on each result.

---

## 2. First Session — written as "you"

*Rough (stage 1). Structure set by the owner; wording `[agent-decided]` until reviewed.*

You spawn facing a glass cage. Inside, a human in pajamas paces; above the glass, four big meters
glow — one of them, WATER, is blinking red. A screen lights up with a string of abstract alien
symbols, then a shaky translation: **"HUMAN GETTING DRY."** Under it, three alien buttons pulse,
each with an odd glyph and its own dubious caption: **WATER** · **ICE CRISTALS** · **GOOD**.
That's the whole room asking you to press one.

You press WATER. Machinery cheers somewhere and floods the cage with hot vapor. The WATER meter
fills… the AIR meter dives. The human wipes the fog off the glass and stares at you: "Really.
REALLY." The screen is already printing the next question.

Ten minutes in, your human has survived four days of your consulting. He has also been frozen,
sunburned by three suns, and gifted a predator as a roommate ("COMPANION?"). When he finally dies —
magnificently — a plaque rises: DAYS SURVIVED: 6. The symbols on the screen politely applaud.

[OPEN → §4: the last thing you see before leaving — the tomorrow-hook — is decided at stage 2.]

---

## 3. Core Loop

*Stage 1: resolved at core-mechanic level. Cycle/session numbers and the full 10th-repetition
answer mature at stage 2, after the mechanic is proven fun once.*

| # | Step (verb) | What the player does | Why do it again? |
|---|---|---|---|
| 1 | **Inspect** | Look at the humans in the cage and their vital meters (e.g. hydration, oxygen, temperature, morale) | The last "solution" changed the meters |
| 2 | **Read** | An alien question arrives in abstract glyphs; a human translator narrates a doubtful interpretation (their uncertainty and laments are part of the humor, and license obtuse phrasing) | A new problem demands an answer |
| 3 | **Answer** | Press 1 of 2–3 alien buttons. The options are the *alien's* proposals, oddly framed ("Water" / "Ice cristals" / "Good" = change nothing) — you never state what you mean, you pick the least-wrong misunderstanding. Some options kill instantly; most swing meters hard | It's the only way to help — and every option smells like a trap |
| 4 | **Watch** | The aliens implement the answer literally ("Water" → hot vapor). Key moments staged physically in the 3D cage (hybrid: the funniest consequences get props/effects; the long tail resolves in UI) | The punchline — you caused it. The question/options half pulls on its own (H1-01 survived, paper, owner self-test); whether staged punchlines beat text is `[HYPOTHESIS → H1-02]` |
| 5 | **Hear** | The caged human reacts and comments; meters move; a new problem now exists | ↩ the "solution" caused the next problem — back to Inspect |

- **One loop takes:** [OPEN] — target 30–90 s; measure at stage 2.
- **One session lasts:** [OPEN] — expected: one run (human survives N days or dies), minutes not hours.
- **Why is the 10th repetition still fun?** Decided mechanism: cards resolve deterministically
  (each advice always produces the same literal misreading); variety comes from random draw order
  and the meter state you are in when a card appears. Surprise is the Day-1 engine; from run 2
  onward the engine is optimization — you know what "give them water" does now, and the question
  becomes whether you can afford it at current oxygen. The claim that optimization carries replay
  once the jokes are known is `[HYPOTHESIS → H2-01]`.

---

## 4. Why Players Come Back

[OPEN] — Step E, after the core loop exists.

---

## 5. Social by Design

[OPEN] — Step F. Flagged in phase 0: the raw concept is single-player as told; social must be
designed, not hoped for. At stage 1 we record the intention and design it at stage 2.

---

## 6. Mobile-First

[OPEN] — Step G, after §3 verbs exist.

---

## 7. World, Look & Story

**Story:** An alien zoo has acquired humans, and the humans keep dying, so the keepers hired you —
a consultant reachable only through a broken translation channel. Everything you answer gets
implemented perfectly, and literally.

**Visual direction:** TBD: pick 2–3 references after the first greybox screenshots exist — an
invented reference is worse than an honest gap. Mood target: stylized alien facility, one lit
glass cage as the room's only bright object.

**Image:** [space-sketch.svg](space-sketch.svg) — labeled layout sketch of the single-room space;
to be replaced by a greybox screenshot from the H1-02 build.

---

## 8. Comparables — exactly two

| | Comparable A — Reigns (outside DCL) | Comparable B — Slay the Princess (outside DCL) |
|---|---|---|
| What worked | Instant readability; one-thumb input; short runs with funny deaths; clear metrics you learn to balance, which drive replay | Reactive narration — the game answers your choices in a voice with personality; the same scenario mutates on each loop, so replaying feels intended |
| What didn't work | Once players memorize card outcomes, ambiguity collapses into optimization and runs get samey | Every branch needs custom art and voice — a content cost a small team cannot pay; once all branches are seen, it is finished |
| What we do differently | Invert the interaction: the options are the *alien's* proposals, not your statements — ambiguity is structural, so it isn't destroyed the moment outcomes are learned; and the funniest consequences are staged physically in a 3D cage you stand in front of | Reactive narration built from a systemic pool — reusable props, meters and text lines recombined per run — instead of custom art and voice per branch |

No Decentraland comparable named — the owner knows the catalog and none is genuinely close; two
outside games is the honest answer.

---

## 9. Six-Week Plan (v1 scope)

[OPEN] — Step J, after §11 team hours are known.

---

## 10. Success Criteria — what you will measure

[OPEN] — Step K.

---

## 11. Team

TBD: Step I of the interview.

---

## 12. Deliverables & Declaration

[OPEN] — Step L.

---

## Appendix A — Hypothesis Log

*Generated from experiment files in `design/<stage>/` at gates. No rows yet — hypotheses park here
as the interview surfaces them.*

| ID | IF / THEN (falsifiable) | Source section | Cheapest killing test | Status | Verdict / date | Tested on |
|---|---|---|---|---|---|---|
| H1-01 | IF the core cycle (inspect → read odd question → answer with an alien option → learn literal implementation → hear reaction) runs as placeholder text only, THEN a first-time player voluntarily continues ≥5 consecutive cycles | §3 Core Loop ("Watch") | Paper/text prototype, ~10 handwritten questions, no build | survived | kill-check held: genuine pull after cycle 5 on the question/options half; punchlines partially landed post-hoc · 2026-08-12 | paper (chat) |
| H2-01 | IF card outcomes are deterministic and the player has seen every card once, THEN a third consecutive run is still voluntarily completed — optimization replaces surprise | §3 Core Loop (10th repetition) | Paper prototype, same deck, 3 runs back-to-back | parked | | — |
| H1-02 | IF the same consequence is shown staged in the greybox cage vs text-only, THEN the staged version gets the stronger reaction and reads as loop, not cutscene | §3 Core Loop (hybrid staging) | Desktop Explorer greybox, 2–3 propped consequences, owner self-test | active | | — |
