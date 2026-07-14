# The Foundry — UX guidelines

Canonical. Every foundry page, component and stylesheet under
`catalyrst/ui3/src/foundry/` follows this document. Where a page disagrees with
this document, the page is wrong.

Derived from five full-surface audits (front-door, games, knowledge, society,
console) measured against the live server and the shipped CSS, and from the
owner's binding taste:

> drastically less text; plain words over jargon; data rides on cards and chips
> with provenance in `title` attributes, never in paragraphs; everything
> clickable to its source; honest empty states — one quiet line, no apologies;
> show, don't label; pages shareable; pretty and fun is a bar, not a bonus.

And the absolute rule: **every rendered value comes from a DB column or a server
fn. Absences render as honest empty states. Never invent, never imply a stronger
claim than the stored rows carry.**

---

## 0. What renders, versus what the token table says

Two facts that invalidate most reasoning done from `primitives.css` alone.

1. **`.ui2` rewrites the radii.** `ChromeShell` roots every foundry document at
   `<div class="cs ui2 fd">`, and `.ui2` (primitives.css) redefines
   `--r-control: 6px`, `--r-card: 12px`, `--r-panel: 12px`. **The foundry's real
   radius scale is 6 / 12 / 12.** `--r-panel` at 18px never renders anywhere.
   This is KEPT — it matches the chrome the foundry sits inside. Always write
   the token; never write a raw px radius; never reason from 10/14/18.
2. **Nothing sets a line-height.** `html, body` sets only background, color and
   font-family, so every element without an explicit `line-height` falls to the
   UA default `normal` (≈1.21 for Inter), and every element without a
   `font-size` falls to the root 16px. Across `foundry/**/*.css` there are
   **15 distinct hand-written leadings** (1, 1.05, 1.15, 1.2, 1.25, 1.3, 1.35,
   1.4, 1.45, 1.5, 1.55, 1.6, 1.65, 1.7, 1.75) and roughly sixty text roles with
   none at all — including every table cell in the product. **This is defect #1
   and §1 fixes it.**

Undefined custom properties currently in use — all of them are bugs, delete the
reference and use the named replacement:

| Referenced | Where | Replacement |
|---|---|---|
| `--ink` | `fddeck.css` | `--ink-85` |
| `--ink-25`, `--ink-60`, `--ink-9` | `fdbench.css` | `--ink-45`, `--ink-6`, `--ink-7` |
| `--surface-2` | `fdbench.css` | `--fill-1` |
| `--fs-10` | `fdtrajectoryreplay.css` | `--fs-11` |
| `--r-input` | `fdpeople.css`, `fdpersona.css`, `fdcontinuity.css` | `--r-control` |
| `--bg` | `fdpeople.css`, `fdpersona.css` | `--fill-1` |

Undefined-token references are forbidden. A `var(--x, fallback)` whose `--x` does
not exist is dead code encoding a wrong assumption — delete the fallback form.

---

## 1. The type ramp

### 1.1 New tokens — add verbatim to `atoms/primitives.css` `:root`

```css
  /* Leading. One canonical value per text role; see foundry/GUIDELINES.md §1.
     No stylesheet under foundry/ may write a bare numeric line-height. */
  --lh-flat:  1.1;   /* single-line numerals and monograms — never wraps */
  --lh-tight: 1.15;  /* display and page titles, 24-34px */
  --lh-snug:  1.3;   /* object titles, 14-20px */
  --lh-ui:    1.45;  /* every non-prose string: chips, labels, cells, controls */
  --lh-body:  1.6;   /* all running prose, 12-14px */
  --lh-code:  1.55;  /* monospace blocks: pre, textarea, log ledgers */

  /* Measure. Prose is capped in ch, always; never in px. */
  --measure:        68ch;  /* every paragraph outside a card or cell */
  --measure-narrow: 44ch;  /* captions inside a grid tile or card */
```

Six leadings, two measures. **That is the entire vocabulary.** Every existing
value maps onto one of them:

| Old value(s) in the tree | Becomes |
|---|---|
| 1, 1.05 | `--lh-flat` |
| 1.15, 1.2 | `--lh-tight` |
| 1.25, 1.3, 1.35 | `--lh-snug` |
| 1.4, 1.45, 1.5 | `--lh-ui` |
| 1.6, 1.65, 1.7, 1.75 | `--lh-body` |
| 1.55 (mono/textarea only) | `--lh-code` |

### 1.2 The scoped default — add to `foundry/components/fdsection.css`

```css
.fd {
  line-height: var(--lh-ui);
}
```

`.fd` is on the ChromeShell root of every foundry document. This makes `--lh-ui`
the floor, so a class that forgets a leading degrades to a defensible 1.45
instead of the UA `normal`. It is scoped to the foundry, so the marketplace, the
HUD and the chrome are untouched. **This does not excuse omitting a leading** —
every role below declares one explicitly.

### 1.3 The complete role table

Every text role in the foundry. One (size, leading, weight, color) tuple each.
If a role you are writing is not in this table, you are inventing a role — use
an existing one instead.

| Role | Class(es) | Size | Leading | Weight | Color | Measure |
|---|---|---|---|---|---|---|
| **Display title** (front door only, opt-in) | `.fd-pagehead--display .fd-pagehead__title` | `--fs-34` | `--lh-tight` | `--fw-bold` | `--text` | 20ch |
| **Page title** (h1) | `.fd-pagehead__title` | `--fs-28` | `--lh-tight` | `--fw-bold` | `--text` | 24ch |
| **Section title** (h2) | `.fd-section__title` | `--fs-18` | `--lh-snug` | `--fw-bold` | `--text` | — |
| **Object title** (h3: card, panel, doc section, spine node) | `.fd-gamecard__title`, `.fd-request__title`, `.fd-session__title`, `.fd-gddcard__title`, `.fd-benchcard__slug`, `.fd-appeal__subject`, `.fd-role__title` | `--fs-16` | `--lh-snug` | `--fw-bold` | `--text` | 24ch |
| **Sub-head** (h4: inside a card or a document body) | `.fd-subhead` | `--fs-14` | `--lh-snug` | `--fw-semibold` | `--text` | — |
| **Sub-sub-head** (h5: a `###` inside a stored document section) | `.fd-gdd__docsubhead` | `--fs-13` | `--lh-snug` | `--fw-semibold` | `--ink-85` | — |
| **Micro-label** (stat label, `thead th`, `dt`, form label) | `.fd-label` | `--fs-11` | `--lh-ui` | `--fw-bold` | `--ink-45` | — |
| **Eyebrow** (collection name above an h1) | `.fd-label--eyebrow` | `--fs-11` | `--lh-ui` | `--fw-bold` | `--brand-ink` | — |
| **Lede** (page intro, at most one) | `.fd-pagehead__intro` | `--fs-14` | `--lh-body` | `--fw-regular` | `--ink-6` | `--measure` |
| **Body prose** | `.fd-section__sub`, `.fd-panelnote`, card bodies, list items, clauses | `--fs-13` | `--lh-body` | `--fw-regular` | `--ink-6` | `--measure` |
| **Note / footnote** | `.fd-note`, `.fd-note-inline` | `--fs-12` | `--lh-body` | `--fw-regular` | `--ink-45` | `--measure` |
| **Caption** (inside a tile or card) | `.fd-stat__note`, `.fd-*__caption` | `--fs-12` | `--lh-body` | `--fw-regular` | `--ink-45` | `--measure-narrow` |
| **Data value** (`td`, `dd`, fact value) | `.fd-table td`, `.fd-table th[scope=row]`, `.fd-facts dd` | `--fs-13` | `--lh-ui` | `--fw-regular` | `--ink-85` | — |
| **Data value, emphasised** (row key) | `.fd-table tbody th[scope=row]` | `--fs-13` | `--lh-ui` | `--fw-semibold` | `--text` | — |
| **Mono data, inline** (id, hash, world, timestamp) | `.fd-mono`, `.fd-table__mono` | `--fs-12` | `--lh-ui` | `--fw-regular` | `--ink-7` | — |
| **Mono block** (`pre`, `textarea`, log ledger) | `.fd-code`, `.fd-*__log`, `.fd-ev__pre` | `--fs-12` | `--lh-code` | `--fw-regular` | `--ink-7` | — |
| **Chip** | `.fd-chip`, `.fd-cellchip`, `.fd-prov` | `--fs-11` | `--lh-ui` | `--fw-semibold` | `--ink-7` | — |
| **Verdict pill** | `.fd-verdict` | `--fs-11` | `--lh-ui` | `--fw-bold` | per state | — |
| **Stat value** | `.fd-stat__value` (+ `--mono`) | `--fs-24` | `--lh-flat` | `--fw-bold` | `--text` | — |
| **Control label** | `.btn--sm` | `--fs-12` | `--lh-ui` | `--ctl-weight` | per variant | — |
| **Control label, large** | `.btn--md` | `--fs-13` | `--lh-ui` | `--ctl-weight` | per variant | — |
| **Input text** | `.fd-form__input`, `.fd-form__textarea` | `--fs-13` | `--lh-ui` (textarea: `--lh-code`) | `--fw-regular` | `--text` | — |
| **Empty-state line** | `.fd-empty` | `--fs-13` | `--lh-body` | `--fw-regular` | `--ink-45` | `--measure` |

Sizes in the ramp: **11, 12, 13, 14, 16, 18, 24, 28** (+34 for the one opt-in
display title). Sizes **15 and 20 are retired** — `--fs-15` and `--fs-20` must
not appear in any foundry stylesheet. No foundry class may omit `font-size`
(the deck quote, `.fd-ask__readingline` and `.fd-copilot__gateway` currently do,
and render at the root 16px — the largest prose on the site by accident).

### 1.4 Hierarchy rules that follow from the table

- **h1 > h2 > h3 > h4, in size, weight and contrast, always.** The current
  inversion (h2 at 12px uppercase `--ink-45` under h3 card titles at 15px
  semibold `--text`) is a defect on every populated page, and pathological on
  `/foundry/continuity`, where the selected scene's own name renders as a 12px
  grey uppercase h2.
- The 12px-uppercase-`--ink-45` treatment is **not a heading**. It survives only
  as `.fd-label` — for stat labels, table headers, `dt`s and form labels — never
  as an `h2`, `h3`, or a slide title.
- An `<h2>` is always sentence case, always `--text`. `text-transform:
  uppercase` never applies to a heading (it currently misprints
  `/foundry/console/evidence`'s section headings as `RUN.LOG` and `DATA.JSON`).
- Same concept → same heading level, everywhere. "Bot playtests" is an `h2` on
  the game page and an `h3` on the response page today; form titles are `h2` on
  exchange and sessions and `h3` on stewardship. One level each.

---

## 2. Page anatomy

### 2.1 The skeleton

Every route renders exactly this and nothing else:

```tsx
<div className="fd-page fd-stack fd-<page>">
  <FdPageHead eyebrow? title intro? crumbs? aside? />
  {/* optional, in this order, at most one of each */}
  <FdStatRow … />
  <FdFilter … />
  <FdSection … />   {/* one or more */}
</div>
```

Fixed rules:

1. **One `.fd-page` per document.** `FdBenchPage` and `FdCostsPage` wrap
   themselves in a second `.fd-page` inside `.fd-console__main`, which is itself
   inside `.fd-page.fd-console` — content lands 24px lower and 24px right of the
   sibling console tabs and the H1 visibly jumps when you switch rail tabs.
   Layout owns `.fd-page`; a page component nested in a layout uses
   `fd-stack` only.
2. **`.fd-pagehead` has no `margin-bottom`.** Delete it. `.fd-stack`'s 32px gap
   already owns that space; flex gaps do not collapse with margins, so today
   every foundry page has a 56px head gap against a 32px rhythm everywhere else.
3. **Nothing is a bare stack child except the head, the stat row, the filter and
   sections.** No floating back-links (`/foundry/play/:slug/response`), no
   trailing footnote paragraphs (`/foundry/console/costs` ends on three),
   no unlabelled panels (`/foundry/exchange/:askId` puts a naked card there).
   If content needs a home, it needs a section.
4. **Section order is: what it is → what is in it → where it came from.**
   Provenance sections go last; `/foundry/play/:slug` currently puts "Where this
   row came from" after three unrelated sections and an entire foreign subsystem.
5. **Failure and empty pages keep their head.** Every DB-unavailable branch
   currently returns a bare `<EmptyState>` with no `FdPageHead`, so an outage
   produces a page with no `h1`, no identity, and an operator-facing subtitle
   ("Set FOUNDRY_DATABASE_URL on this deployment") shown to a public reader.
   Render the head, then one `.fd-empty` line in place of the body. One shared
   string across all routes: **"This record is not available right now."**

### 2.2 `FdPageHead` slot rules

| Slot | When | Content rule |
|---|---|---|
| `eyebrow` | **Only** on a detail route — a record inside a collection. | The **collection's** name, matching its nav tab exactly: `Timeline`, `Design doc`, `Exchange`, `Evidence`, `Trajectories`. Never the record's own data. `/foundry/play/:slug` (world name) and `/foundry/play/:slug/response` (game title) violate this — the game name belongs in the title or a fact, not the eyebrow. |
| `title` | Always. | The record's **human** name. Never a raw DB column (`/foundry/timeline/:eventId` renders `<h1>changelog</h1>`), never a truncated id (replay, evidence), never a bare generic noun that could name any page (`Response`). If the record's name is a slug, resolve it; if it cannot be resolved, show the id in a fact row and title the page with what it is. |
| `intro` | Rarely. | **One sentence, ≤20 words**, only if it states something the page cannot show. Default is absent — `/foundry/play` is the reference. An intro may never restate the title, the tab, or a note further down the page. Never a methodology sentence. |
| `crumbs` | Every detail route. | The **only** back-navigation pattern. Points at the collection the reader came from (`← All episodes`), not a lateral entity. Delete `.fd-ask__back`, `.fd-tlevent__back`, `.fd-response`'s `<p class="fd-note">` back link and `FdEvidencePage`'s "← Back". |
| `aside` | At most one page-level action. | Always the `Button` atom (`variant="primary"` for the page's own act, `"secondary"` otherwise, `size="sm"`). Never a hand-rolled anchor. If the action is role-gated, render the disabled `Button` with a `title` explaining the gate — never an empty slot that changes the head's shape between readers. |

### 2.3 Section rules

- `FdSection` title is a **noun naming what is in it**, not a label restating
  the h1 ("Documents" under "The design docs") and not a filename.
- `sub` is body prose and is **optional and rare**. It must describe what is on
  screen, not a rule with no on-screen referent. Nine of sixteen society
  sections currently carry a 12+ word `sub`; several teach sort orders and state
  machines that no live row exercises. If the rule matters, show it: render the
  grouping, render the sort control, render the state.
- `badge` carries the count. Every collection section shows its count there —
  `/foundry/play` (8 games), `/foundry/exchange` (10 asks), `/foundry/gdd`
  (5 docs) currently show none.
- Sections are separated by the stack's 32px and by their titles. At `--fs-18`
  bold `--text` (§1.3) a title is a real boundary; no rules, no backgrounds.

### 2.4 Where things live

| Thing | Home | Never |
|---|---|---|
| A fact about the page's subject | head `aside` chip, or a `.fd-facts` `dl` | a sentence |
| A fact about a row | that row's chip row or `dl` | a sentence, a paragraph under the table |
| Provenance for a value | the `title` attribute on the element carrying the value | a paragraph, a table column, a `.fd-note` |
| A page-level action | head `aside` | inline in prose |
| A row-level action | the card's foot or the row's last cell | floated into body text (`.fd-gdd__sectiontools` floats 14 Edit buttons into the doc prose) |
| A section-level action | `FdSection` `aside` | a trailing note |
| A count | `FdSection` `badge` or an `FdStat` | prose, and never in three places at once (`/foundry/exchange/:askId` states the pledge count three times) |

### 2.5 The empty-state pattern

`EmptyState` is a general ui3 component whose typography is raw px, weight 800,
centred, and capped at 340px. **The foundry does not use it in-page.** Replace
every in-page use with:

```tsx
<p className="fd-empty">Nothing here yet.</p>
```

```css
.fd-empty {
  margin: 0;
  max-width: var(--measure);
  font-size: var(--fs-13);
  line-height: var(--lh-body);
  color: var(--ink-45);
}
```

Rules:

- **One line. Left-aligned. No title, no icon, no CTA row, no apology, no
  explanation of the mechanism that would fill it.** Not "Runs come from the bot
  harness; a run appears here only after it is recorded". Not "The registry
  fills from Worlds deployments via `foundry:import-real`" (that leaks an
  internal command name). Not three buttons (`/foundry/sessions`).
- **The line must be true of the current filter.** `/foundry/console/trajectories?scene=nope`
  renders "No episodes recorded" while three episodes exist. Filtered empties
  say so: "No episodes for this scene."
- It sits **in the shape of the thing it replaces** — inside the section, at the
  section's left edge, not as a centred island.
- If a form to fix the absence is already on screen, **there is no empty state**
  (`/foundry/persona` renders a banner above the form that fills it).
- The one legitimate `EmptyState` use is the route-level 404/outage screen
  outside `.fd`, and even there the head renders first (§2.1 rule 5).

---

## 3. Component vocabulary

### 3.1 Chip vs pill vs stat vs note

| Use | When | Class |
|---|---|---|
| **Chip** | A stored fact about the object, short enough to read at a glance: a date, a size, a count, a world, a tag, a source. Solid border, `--fill-2`. | `.fd-chip` |
| **Chip, machine-read** | A value this program *derived* rather than read: a cell classification, an emotional job, a verdict of fit. **Dashed border, transparent background** — the existing dashed/solid distinction is a good convention, KEEP it and generalise it. | `.fd-cellchip` |
| **Chip, mono** | The value is an identifier (hash, id, world, path). | `.fd-chip--mono` |
| **Verdict pill** | A single categorical state with a color meaning: pass / fail / open / closed / live. One word, lower case, from a fixed enum. | `.fd-verdict` |
| **Stat** | A number a reader would compare or track over time, at the page or section level. Never a count of 0 or 1 the page shows in full below. | `FdStat` |
| **Note** | The residue: a caveat that could not be attached to a value. Rare. If you are writing a third note on a page, the page has a structure problem. | `.fd-note` |

Geometry is shared and non-negotiable — all chip-shaped things use
`padding: 2px 9px; border-radius: var(--r-pill); font-size: var(--fs-11);
line-height: var(--lh-ui)`. `.fd-hyp` (1px 8px, no leading) and the four verbatim
copies of the "mono micro-pill" recipe (`.fd-timeline__lanechip`,
`.fd-session__cadence`, `.fd-stew__state`, `.fd-appeal__status`) collapse into
`.fd-chip` + `.fd-chip--mono`.

`FdStat` must accept and render a `title` — today it has no provenance
affordance at all (label, value, note, delta only), which is why the copilot
page's five headline numbers carry no hover source.

### 3.2 Provenance

**Provenance lives in the `title` attribute of the element carrying the value,
and nowhere else.**

- Every chip, stat, timestamp and derived value carries `title`.
- The `title` names **the source and the read date**, in plain words:
  `"mirror snapshot, read Aug 15, 2026"`. Under 12 words.
- The `title` is **specific to that value**. `/foundry/play/:slug`'s job chips
  all share one boilerplate methodology paragraph; the cell chip beside them
  carries a dated, game-specific rationale. The second is right.
- A provenance sentence is never a paragraph (`/foundry/exchange/:askId` renders
  ~110 words of it), never a table column (`/foundry/copilot`'s "Where the number
  comes from"), and never a section (`/foundry/play/:slug`'s "Where this row
  came from" — fold it into the facts' `title`s).
- If a value has no recorded provenance, it renders without a `title`. Do not
  synthesise one.
- Every `title`-only fact also needs an `.u-sr-only` twin, since `title` is not
  reachable by touch or AT.

### 3.3 Links

The single highest-severity defect in the surface: **there is no bare `a` rule
anywhere in ui3.** Fifteen of fifteen anchors on `/foundry/play/:slug/response`
and nine of fifteen on `/foundry/play/:slug` render in the UA default `#0000EE`
on a `#0e0a16` background — roughly 1.3:1 contrast, invisible, a hard WCAG
failure, and a direct contradiction of "everything clickable to its source".

Add to `fdsection.css`:

```css
.fd a {
  color: var(--brand-ink);
  text-decoration-color: color-mix(in srgb, var(--brand-ink) 45%, transparent);
  text-underline-offset: 2px;
}
.fd a:hover { text-decoration-color: currentColor; }
.fd a:focus-visible {
  outline: var(--focus-ring-width) solid var(--focus-ring-color);
  outline-offset: var(--focus-ring-offset);
  border-radius: 2px;
}
.fd a.btn,
.fd a.fd-chip,
.fd a.fd-gamecard__cardlink,
.fd .fd-pagehead__crumbs a { text-decoration: none; }
.fd a.fd-chip:hover { border-color: var(--brand-ink); color: var(--brand-ink); }
```

Affordance rules:

- **Underlined in prose, not underlined in chrome.** A link inside a paragraph,
  a list item or a table cell is underlined. A link that is itself an object
  (chip, button, card) is not, and shows its state on hover instead.
- **Nothing non-clickable may look clickable.** `.fd-copilot__skill` paints 30
  `<span>`s brand-ink with a hover underline. Delete the rule.
- **Nothing clickable may look inert.** `<a class="fd-chip">` and
  `<span class="fd-chip">` are pixel-identical today (28 GitHub links vs 2 static
  chips on `/foundry/copilot`); the rule above separates them. The same applies
  to `FdCountCell`'s numbers on `/foundry/people`, which are links, plain text,
  or a link to a different destination depending on invisible data — render the
  three cases differently or make them all inert.
- **Disabled actions are `<button disabled>`, never `<span aria-disabled>`** —
  the copilot's offline CTA is currently absent from the tab order entirely.
- **A link's label names its destination.** No "← Back". No double arrows
  (`FdIdeaCard` appends `→` to a `surfaceLabel` that already contains one,
  producing "Console → Bench →"). No promise the destination cannot keep
  ("Open the editor" opens a new empty scene for every game).
- **Whole-card links** use the `.fd-gamecard__cardlink::after { inset: 0 }`
  pattern with a `:has()` focus ring — one interaction model for every card
  board. `.fd-gddcard` (inert except its title text, no hover, no focus ring) and
  `.fd-role` (whose `::after` also blocks text selection) adopt it.
- `FdPersonaChip` accepts `href` and **no society call site passes it**. Pass it
  wherever a person has a destination; where they do not, that is a missing page,
  not a styling choice — record it, do not paper over it.

### 3.4 Buttons

Three vocabularies are visible on one screen today: `.fd-role__cta`
(`--brand-cta`, `--r-control`, 10/24, 14px semibold), `.fd-copilot__open`
(`--brand` #ff2d55, `--r-card`, 10/18, 14px bold) and `.btn--sm` from the chrome.

**One vocabulary: the `Button` atom.** `variant="primary"` uses `--brand-cta`;
`--brand` is never a button background. Delete `.fd-role__cta`,
`.fd-copilot__open`, `.fd-people__claim`, `.fd-scrub__btn` and every hand-rolled
`<a class="btn …">`; render `<Button as="a" href=…>` instead. Sizes: `sm` for
row and section actions, `md` for the one primary action of a page.

### 3.5 Forms

Four form systems exist: `.fd-exchange__*`, `.fd-sessions__*` and `.fd-stew__*`
are byte-identical, and `.fd-people__*` / `.fd-persona__*` are a fourth with
uppercase labels, a mono input, an undefined radius token and a different fill.

**One system: `.fd-form`**, with `.fd-form__title` (h3, `--fs-16`),
`.fd-form__label` (`.fd-label`), `.fd-form__input` / `__textarea`
(`--fs-13`, `--r-control`, `--fill-1`), `.fd-form__actions`. Vertical rhythm:
`gap: var(--s-3)` between fields, `--s-2` between a label and its control.

### 3.6 Tables vs cards

- **Cards** for collections of objects a reader browses. **Tables** for values a
  reader compares column-wise.
- A table whose numeric columns are overwhelmingly zero is not a comparison —
  it is a card board wearing a grid (`/foundry/continuity`'s ten columns,
  ~40 count cells almost all `0`; `/foundry/people`'s six columns for one row).
- A table always declares its leading (`fdtable.css` declares none today, so
  every cell in the product runs at UA `normal`; the copilot Skills description
  column runs 403-character prose cells that way).
- A prose column in a table is a smell. Cap data cells; move real prose to the
  object's own page.

### 3.7 Numbers, dates, plurals, machine strings

- **One date function per shape, all from `foundry/fmt.ts`.** Seven renderings
  of the same kind of fact are live today (`dayUTC`, raw ISO, `stampDayUTC`,
  `stampShort`, `whenLabel`, `dayLabel`, a hardcoded `MEASURED_SINCE_LABEL`),
  plus two local `toLocaleDateString` copies in `FdPeoplePage` and
  `FdPersonaPage` and a local `utcStamp` in `FdCostsPage`. `fmt.ts` exists
  precisely because Intl output moves with the runtime's ICU build and breaks
  hydration. Allowed: **`dayUTC`** ("Aug 15, 2026") for dates, **`stampUTC`**
  ("Aug 15, 2026 · 01:56 UTC") for instants. Nothing else. Raw ISO never renders.
- Every rendered time is an `FdTime` with `dateTime` and a `title`.
- **`plural(n, word)`** (currently unexported inside `FdResponse.tsx`) moves to
  `fmt.ts` and every count uses it. "1 runs" and "1 sims" are live on
  `/foundry/play`.
- **No `toLocaleString()`** for digit grouping — `groupDigits` in `FdCostsPage`
  exists for this reason; move it to `fmt.ts`.
- **No raw machine strings reach the reader.** `publish_gdd_draft on
  flagrush-v1 — detail not rendered`, `<h1>changelog</h1>`, `traj-flagtag-are…`,
  `RUN.LOG`. Every enum passes through a human label map; every id shown to a
  reader is accompanied by the name of the thing it identifies.
- **One name per concept.** A recorded bot run is called *runs, sims, sandbox
  sims, sandbox simulation, bot run, playtest, Bot playtests, Bot bench,
  Recorded runs, episode, trajectory, event log, replay* across the surface.
  The canonical vocabulary:

  | Concept | The word | Nowhere else |
  |---|---|---|
  | A recorded bot playthrough | **run** | sim, episode, trajectory, playtest, event log |
  | The stored record of a run | **run log** | ledger, event log, transcript |
  | The console tab listing runs | **Runs** | Trajectories |
  | The files a run produced | **evidence** | artifacts, captures |
  | A game on the shelf | **game** | scene, cartridge, title |
  | The deployed world | **world** | World, deployment |
  | A verdict | **passed** / **failed** | pass, fail, playtest failed |

---

## 4. Variability budget

Every per-page deviation the audits found, ruled. **KEEP** means the content
justifies it. **NORMALIZE** means change it to the target.

### 4.1 Type and leading

| Deviation | Ruling |
|---|---|
| 15 distinct line-heights; ~60 roles with none | **NORMALIZE** → the six `--lh-*` tokens, §1.1. Zero bare numbers in foundry CSS. |
| `fdtable.css` declares no leading at all | **NORMALIZE** → `--lh-ui` on `.fd-table`, `--lh-body` on any prose cell. |
| `.fd-ask__readingline`, `.fd-deck__quote p`, `.fd-copilot__gateway` render at the root 16px | **NORMALIZE** → the roles they are (body prose / quote / note). No class ships without a size. |
| `--fs-15` (`.fd-gddcard__title`, `.fd-session__title`, `.es__title`, `.fd-benchcard__slug`) | **NORMALIZE** → `--fs-16` object title. |
| `--fs-20` mono stat value vs `--fs-28` sans stat value, sometimes in one row (`/foundry/timeline`) | **NORMALIZE** → `--fs-24` for both; `--mono` changes family only. |
| `.fd-select__bandlede` at `--fs-15`/1.75 | **NORMALIZE** → body prose. |
| h1 `--fs-34` on `/foundry` and `/foundry/select` vs `--fs-28` on 22 others | **KEEP the size, NORMALIZE the mechanism** → a documented `.fd-pagehead--display` modifier, not two page-scoped overrides. |
| h1 letter-spacing `-0.02em` on `/foundry/select` | **NORMALIZE** → `-0.015em`. |
| Intro color `--text` / `--ink-85` / `--ink-6` on three adjacent pages | **NORMALIZE** → `--ink-6`. |
| Six measures (68ch, 72ch, 62ch, 46ch, 44ch, 340px) + six uncapped prose classes | **NORMALIZE** → `--measure` / `--measure-narrow`; nothing uncapped, nothing capped in px. |
| `.fd-gdd__sectionbody` at 1.7 while its own duplicate `<pre>` runs 1.65 | **NORMALIZE** → `--lh-body`. |
| `.fd-response__sub` (h3, 13px) larger and brighter than its parent h2 (12px) | **NORMALIZE** → §1.4 hierarchy. |

### 4.2 Spacing and geometry

| Deviation | Ruling |
|---|---|
| `.fd-pagehead` `margin-bottom` + `.fd-stack` gap = 56px head gap on every page | **NORMALIZE** → delete the margin. |
| Page-scoped head margins: 12px (`/foundry`), 8px (`/foundry/select`), 24px (base) | **NORMALIZE** → delete both overrides. |
| Double `.fd-page` on `/foundry/console/{bench,costs}` | **NORMALIZE** → layout owns `.fd-page`. |
| Card grid gaps: 8px (ideas), 12px (doors, play, exchange, sessions, appeals, gdd), 16px (beats), 32px (crowd) | **NORMALIZE** → `--s-3` (12px) for every card board. The crowd's 32px is a 3D tile layout — **KEEP**. |
| Card min-widths: 140 / 220 / 280 / 300 / 320 | **NORMALIZE** → `auto-fill, minmax(300px, 1fr)` for object cards, `auto-fit, minmax(200px, 1fr)` for stat tiles. |
| Card padding: 12/16 (idea head), 16 (game, panel, stat, request, session, appeal), 24 (role), 32/24 (band) | **NORMALIZE** → `--s-4` (16px). The band panel is a one-off editorial block — **KEEP** at `--s-6`/`--s-5`. |
| `--r-panel` (18px→12px) used only by `/foundry/select`'s two panels | **NORMALIZE** → `--r-card`, matching `.fd-panel`. |
| `--r-input, 8px` on people/persona/continuity inputs | **NORMALIZE** → `--r-control`. |
| `.fd-continuity__steward` at `--r-control` where every sibling card uses `--r-card` | **NORMALIZE** → `--r-card`. |
| Rail item radius `0 6px 6px 0` | **KEEP** — it is a rail item flush to an edge. |
| Four stat-grid declarations (`.fd-statrow`, `.fd-timeline__stats`, `.fd-continuity__stats`, `.fd-continuity__strip`, `.fd-costs__stats`) | **NORMALIZE** → `.fd-statrow` only. |
| Three card-board grid copies (`.fd-exchange__board`, `.fd-sessions__board`, `.fd-stew__appeals`) + two door-grid copies | **NORMALIZE** → one `.fd-board`. |
| Breakpoints 560 / 600 / 620 / 640 / 720 / 860 / 900 / 980 | **NORMALIZE** → 600 (page padding, single column) and 860 (chrome nav, two-column panels). Anything else needs a written reason in the CSS. |
| 2px internal rhythm on `.fd-tlevent__facts` | **NORMALIZE** → the `--s-*` scale. |
| `.fd-persona__preview` has no padding, clipping its own caption | **NORMALIZE** → `--s-4`, with the avatar bleeding via a negative-margin child. |
| `.fd-select__band` border-left 3px `--brand` sharing an idiom with `.fd-panel--failed` | **NORMALIZE** → drop the accent bar; the accent-bar idiom means "failed". |

### 4.3 Components

| Deviation | Ruling |
|---|---|
| Three primary-button vocabularies, two brand reds | **NORMALIZE** → `Button`, `--brand-cta` (§3.4). |
| Four form systems | **NORMALIZE** → `.fd-form` (§3.5). |
| Four mono micro-pill copies | **NORMALIZE** → `.fd-chip--mono`. |
| Five card shells (`.fd-request`, `.fd-session`, `.fd-appeal`, `.fd-stew__consent`, `.fd-continuity__steward`) | **NORMALIZE** → one `.fd-card`. |
| Two back-link classes, byte-identical | **NORMALIZE** → `crumbs`. |
| Two local `fmtDate` copies, one local `utcStamp`, one hardcoded date string | **NORMALIZE** → `fmt.ts`. |
| Three `<dl>` treatments (flex-wrap, grid card, flex column) | **NORMALIZE** → one `.fd-facts` grid, `auto-fit minmax(180px, 1fr)`. |
| Three renderings of the four honesty markers, two zero-policies | **NORMALIZE** → one `FdMarkers` component, chips, zeros hidden. |
| `.fd-request__count` (20px sans, vertical) vs `.fd-session__count` (18px mono, horizontal) | **NORMALIZE** → `FdStat`-style, `--fs-24`. |
| `EmptyState` in-page | **NORMALIZE** → `.fd-empty` (§2.5). |
| `FdStat` has no `title` prop | **NORMALIZE** → add it; every stat carries provenance. |
| Dashed vs solid chip = derived vs stored | **KEEP** and generalise. |
| Whole-card stretched link | **KEEP** and generalise to every card board. |
| `/foundry/play` has no intro | **KEEP** — this is the reference, not the deviation. Every other page moves toward it. |
| `/foundry/select`'s 3D avatar crowd | **KEEP** the idea, **NORMALIZE** the DOM order so the heading precedes the tiles (at ≤860px the label currently renders below what it labels). |
| Console rail as a second nav layer | **KEEP** — it is a real sub-hierarchy — but see §5.1. |

### 4.4 Dead code to delete

`.fd-role__blur`, `.fd-role__same`, `.fd-select__crowdbody`, `.fd-select__crowdnote`,
`.fd-select__bandkicker`, `.fd-play__demand`, `.fd-play__demandlist`, `.fd-play`
(no rules), `.fd-gdd__vdate` (applied, never defined), `.fd-gdd .fd-table
tr.is-open`, `fdcosts.css`'s duplicate `vertical-align`, and the seven unread
`FdGameCardVM` fields (`source`, `sourceNote`, `description`, `entityHref`,
`entityId`, `editorHref`, `note`) serialised into every client payload.

---

## 5. Information structure — findings and fixes

### 5.1 Navigation

1. **The front-door rail duplicates the nav.** Ten of fourteen `FOUNDRY_RAIL`
   labels are byte-identical to a chrome tab; "Games" renames Play;
   Bench/Trajectories/Costs are flattened out from under Console. **Fix:** the
   front door shows the three doors and live state, not a menu. Delete the rail.
2. **`/foundry/deck` has no tab and marks Overview active.** It is a deep-link
   target from ten places. **Fix:** add a `TABS` entry and an `activeTab()`
   branch; add a `:target` highlight rule for slide anchors.
3. **The console rail lies.** `activeTab()` falls back to `bench`, so every
   evidence page marks Bench `aria-current="page"`. **Fix:** return `null` for
   unknown tails and add an Evidence entry (or render evidence as a section of
   the run page). The tab list is also declared twice, in two packages —
   one source.
4. **Two front doors for one decision.** `/foundry` and `/foundry/select` render
   the same three cards from the same constant with different titles, different
   intros, different heading levels and duplicated grid CSS; nothing links from
   one to the other. **Fix:** one door page. `/foundry` keeps the doors and gains
   the live state (games live, next session, open asks); `/foundry/select`
   redirects to it, or becomes the thing the doors are not.
5. **Detail routes have no crumbs.** `FdPageHead.crumbs` — which carries the
   only correctly-coloured link styling in the product — is used by no society
   page. **Fix:** §2.2.
6. **A bad id ejects the reader.** Both console detail routes 404 to the
   site-wide error page, dropping the rail entirely, and making the replay's own
   "cannot be replayed" state unreachable. **Fix:** render the layout with a
   `.fd-empty`.
7. **The room dock is unreserved.** `FdRoomDock` is fixed bottom-right on every
   route and overlaps card actions, table scrollbars and the console's five
   horizontal scroll containers; `FdTourCard` stacks on top of it on `/foundry`.
   **Fix:** `.fd-page { padding-bottom: calc(var(--s-7) + 56px) }` and give the
   dock a z-index above the tour or a shared corner stack.

### 5.2 Text volume

Words of explanatory prose above the first datum, measured live: stewardship
~209, exchange ~103, persona ~68, continuity ~63, people ~55, timeline ~48,
sessions ~40; `/foundry` 360 words of `<p>`, `/foundry/select` 315,
`/foundry/copilot` 366, `/foundry/play/:slug/response` ~15 full explanatory
sentences. `/foundry/play` has zero. **`/foundry/play` is the standard.**

Mechanical rule: **a page may render at most one lede (≤20 words) and at most
two notes.** Everything else becomes a chip, a fact row, a `title`, or is
deleted. Specifically delete:

- Every methodology sentence used as a control label ("The full document,
  exactly as it was written — no rendering pass, no reflow, no summary.").
- Every restatement of the intro (`/foundry/copilot` repeats "whether it is up,
  what it has spent, and what it carries" twice within 200 words).
- Every rule with no on-screen referent (exchange's approved/closed sort over
  ten uniformly-open asks; sessions' "soonest first" over zero rows; the
  sandbox-counting rule stated five times on continuity).
- Every prose provenance block (§3.2).
- Every show-don't-label line: "Three doors. One society.", "This is where your
  people gather.", "The only question this page asks is which ten seconds you
  want first.", "shows nothing the log does not hold", "Nothing is summarised in
  its place", "not a bill", "counted as failed" (as a declaration; the per-check
  fact stays as data).

### 5.3 Honesty

These are the findings where the copy claims more or less than the rows carry.
Fix the data path, not the wording.

1. `/foundry/gdd` renders "No hypothesis log filed" for a doc whose linked
   document contains an Appendix A hypothesis table. The rows are unparsed, not
   absent. **Fix:** parse the appendix, or say "none stored".
2. A copilot-authored doc gets no machine-authorship chip because the chip is
   gated on `source === "program"` only. **Fix:** gate on `source !== "human"`.
3. `/foundry/copilot` renders five 20px headline numbers and then retracts them
   in a 12px note ("All of it is the deploy pipeline's verification probe").
   Same on `/foundry/console/costs`, where the entire ledger *is* the probe.
   **Fix:** exclude probe rows from the headline totals and show the probe as its
   own labelled row. A number that needs a retraction is the wrong number.
4. The "Create / Start building" door promises the scene editor and lands on a
   password-gated page; the "Operate" door admits its own dead end mid-sentence.
   **Fix:** the destination line states what the destination *is*.
5. `/foundry/play/:slug`'s "Open the editor" opens a new empty scene for every
   game. **Fix:** link the game, or relabel.
6. The `TOOLS` array on the response page is editorial content rendered in
   success-tinted chips indistinguishable from DB-backed ones. **Fix:** render
   editorial claims as prose in a clearly marked block, or store them.
7. `/foundry/play/:slug`'s history spine renders the same deployment as two
   nodes with the same timestamp; its seven dot treatments have no legend, and a
   machine judgment ("reading", dated today) sorts into the same time axis as
   things the game did. **Fix:** dedupe the nodes, add a legend or drop the
   encodings to three, and separate readings from events.
8. `/foundry/gdd/$id` renders the whole document twice (~110 KB duplicated) and
   ships a 70-line hand-rolled markdown subset that dumps every table, bold
   span, inline code, blockquote and ordered list as literal characters. **Fix:**
   render markdown properly and keep one copy; this is why the product's primary
   content reads as raw source.
9. Bare letters A–F render as job chips while the legend lives in a component the
   page imports for its constants and never renders. **Fix:** render names.

### 5.4 Density and comparison

- `/foundry/continuity`: ten columns, ~40 near-all-zero count cells, a master
  table that stays above an appended detail view with no anchor, no title and no
  way to close. **Fix:** cards per scene; the detail becomes its own route.
- `/foundry/people`: six columns for one row. **Fix:** cards until the roster
  earns a table.
- `/foundry/copilot`: a 30×5 Skills table is the largest contributor to an
  84 KB page and mostly em-dashes. **Fix:** chips, or a linked index.
- `/foundry/console/bench`: three cards whose central fact block reads
  "not reported / none" on all three, and no aggregate anywhere. **Fix:** hide
  absent facts; add the stat row.
- `/foundry/console/trajectories`: the Scene column is the only column naming a
  game and it is dead text; the loader computes an evidence label for every row
  and drops it. **Fix:** link the scene, add an evidence column.
- `/foundry/console/trajectories/$id`: finish reason renders as a fact *and* as
  a 24px stat; the event count renders twice; the seat table renders three
  times. **Fix:** once each.
- Collections show counts, and gain a sort or filter at >20 rows — not before.

---

## 6. Per-page NORMALIZE checklist

Shared work, applied everywhere first: **(S1)** add the `--lh-*` and `--measure`
tokens + `.fd { line-height }`; **(S2)** add the `.fd a` link rules; **(S3)**
delete `.fd-pagehead`'s `margin-bottom`; **(S4)** apply the §1.3 role table and
retire `--fs-15`/`--fs-20`; **(S5)** replace in-page `EmptyState` with
`.fd-empty`; **(S6)** route every date, plural and grouped number through
`fmt.ts`; **(S7)** collapse forms, chips, cards, boards, stat rows and buttons
onto the single primitives; **(S8)** every DB-unavailable branch keeps its
`FdPageHead` and renders one shared line; **(S9)** delete the dead CSS and the
unread VM fields; **(S10)** reserve room-dock clearance in `.fd-page`.

### `/foundry` — `FoundryHome`
- [ ] Delete the 14-card rail; the nav already carries it.
- [ ] Render the live state the loader already pays for (games live, next session, open asks) on the door cards, or drop `homeSnapshot()` and `EMPTY_STATS`.
- [ ] Doors: `Button` primary, one red; remove the selection-blocking `::after` (use the card-link pattern).
- [ ] Cut the three 42–45-word door bodies to one line each; destination line states what the destination is.
- [ ] `.fd-pagehead--display` modifier instead of the page-scoped h1/intro overrides.
- [ ] Delete `.fd-role__blur`, `.fd-role__same`; card padding → `--s-4`.
- [ ] Fix `FdIdeaCard`'s double arrow if any idea card survives.
- [ ] Resolve the `FdTourCard` / `FdRoomDock` overlap.

### `/foundry/select` — `FdSelectPage`
- [ ] Decide the IA: redirect to `/foundry`, or keep and delete the doors from one of the two pages. Do not ship two door pages.
- [ ] If kept: delete the verbatim "This is where your people gather." duplication, the three show-don't-label lines and the BEATS manifesto.
- [ ] Move the crowd heading above the tiles in DOM order.
- [ ] `--fs-15`, 1.75, 1.7 → the ramp; `--r-panel` → `--r-card`; drop the `--brand` accent bar.
- [ ] Share one door grid with `/foundry`; delete `.fd-select__crowdbody`, `__crowdnote`, `__bandkicker`.
- [ ] Held-role chip renders a date or does not render.

### `/foundry/copilot` — `FdCopilotPage`
- [ ] Exclude the deploy probe from the headline stats; label it as its own row.
- [ ] Six notes → at most two; move the probe timestamp, the opencode version and the pricing constant onto chips/stat `title`s.
- [ ] Delete `.fd-copilot__skill`'s fake-link styling; give `a.fd-chip` its affordance.
- [ ] Offline CTA → `<Button disabled>`; the offline pill text → a chip-length phrase.
- [ ] Drop the receipts table's provenance column into `title`s.
- [ ] Skills table → chips or a linked index; `.fd-copilot__open` → `Button`.
- [ ] `.fd-copilot__gateway` gets a size; the mono pipeline line stops carrying an English clause.

### `/foundry/play` — `FdPlayPage`
- [ ] `plural()` on run/sim counts; ISO cell dates → `dayUTC`.
- [ ] Provenance `title` on every chip, not just Deployed.
- [ ] Add the `h2` + section `badge` (count); keep no intro.
- [ ] Empty `.fd-gamecard__links` renders nothing (no dead `margin-top: auto` block).
- [ ] Delete `.fd-play__demand*`, `.fd-play`, the seven unread VM fields.
- [ ] Grid → `auto-fill minmax(300px, 1fr)` shared as `.fd-board`.

### `/foundry/play/:slug` — `FdGameDetail`
- [ ] Eyebrow → `Games`; the world moves to a fact row; crumbs carry the back-nav.
- [ ] Job chips → names, not bare letters; each chip's `title` → the game-specific dated rationale, not the shared methodology paragraph.
- [ ] One verdict wording; `plural()`; dedupe the built/live spine nodes.
- [ ] Spine dot encodings → three, with a legend, and readings separated from events.
- [ ] Fold "Where this row came from" into the facts' `title`s; delete the section.
- [ ] The Response link appears once, in crumbs.
- [ ] `.fd-benchcard__slug` → `--fs-16`; delete `--ink-25`/`--ink-60`/`--surface-2`/`--ink-9`.
- [ ] The continuity block gets its own route or a section boundary, not a bare `<div id="memory">`.
- [ ] Aside stops right-aligning when it wraps under the title.
- [ ] "Open the editor" links the game or is relabelled.

### `/foundry/play/:slug/response` — `FdResponse`
- [ ] Title = the game; eyebrow = `Games`; crumbs replace the `.fd-note` back link.
- [ ] Six stacked panels → one card board; `.fd-response__sub` → the h4 role.
- [ ] QualifyStrip prose, the three "not connected yet" panels and the `TOOLS` editorial → chips + one line each, or stored data.
- [ ] Deck jargon (T0, "Choose B or C", A–F) decoded or dropped.
- [ ] Label the two undifferentiated gap lists.
- [ ] Remove the dead `#jobs` deep link (or render the target).
- [ ] Four date formats → `fmt.ts`; retire `MEASURED_SINCE_LABEL`.

### `/foundry/gdd` — `FdGddListPage`
- [ ] Delete the 38-word intro and the publish lecture; section `badge` carries the count.
- [ ] `.fd-gddcard` adopts the card-link + hover + focus-ring model; title → `--fs-16`.
- [ ] One `FdMarkers` component; zeros hidden.
- [ ] "No hypothesis log filed" → what the data supports; machine-authorship chip gates on `source !== "human"`.
- [ ] Session-id chip links the copilot or is not a chip.
- [ ] Publish error → `--error`, matching the doc page.

### `/foundry/gdd/$id` — `FdGddDocPage`
- [ ] Render markdown properly (tables, bold, code, quotes, ordered lists, rules); drop the duplicate whole-document copy.
- [ ] Document sections become real headings; restore a visible disclosure affordance.
- [ ] `.fd-gdd__sectionbody` 1.7 → `--lh-body`; `--fs-15`/`--fs-13` heads → the ramp.
- [ ] `.fd-hyp` → `.fd-chip`; `.fd-gdd__vdate` defined or deleted.
- [ ] Edit buttons leave the prose flow (section `aside`), and only render for a reader who can edit.
- [ ] Eyebrow = `Design docs`; kind, created date and source move to a `.fd-facts` grid.
- [ ] Delete the methodology control label; one play-tier control, via `Button`.

### `/foundry/deck` — `FdDeckPage`
- [ ] Add the `TABS` entry + `activeTab()` branch; add `:target` styling.
- [ ] Slide headings → the h2 role (sentence case, `--fs-18`, `--text`); quotes → body prose at `--fs-13`.
- [ ] `--ink` → `--ink-85`.
- [ ] Link the archived capture; state the omitted slides.
- [ ] Render the two tabular structures as chips/lists, not `·`-joined sentences; drop the gloss about the links.
- [ ] Preserve the strings pinned by `foundry.deck.test.ts`.

### `/foundry/exchange` — `FdExchangePage`
- [ ] Delete both explanatory paragraphs; show the states instead (group by status, or drop the model until rows exercise it).
- [ ] Section `badge` = count; pledge `0` stops being the card's loudest element.
- [ ] Reading chip encodes verdict (dashed = derived; negative verdicts read as negative).
- [ ] Mark elided quotes; head action → `Button` (already correct — keep).
- [ ] `.fd-exchange__form*` → `.fd-form`; `.fd-exchange__board` → `.fd-board`.

### `/foundry/exchange/:askId` — `FdAskPage`
- [ ] `.fd-ask__readingline` gets size, leading and measure — it is the only 16px prose on the site.
- [ ] The reading section becomes chips + one line; delete the ~110 words of methodology.
- [ ] Title renders once (drop the repeated `.fd-request__title`).
- [ ] The card sits inside a labelled section.
- [ ] Pledge count stated once; pledges sub → one clause.
- [ ] `.fd-ask__back` → crumbs; `#reading` gets `:target` styling.

### `/foundry/people` — `FdPeoplePage`
- [ ] Directory → cards until the roster earns a table; kill the six-column-one-row grid.
- [ ] `FdCountCell`: links look like links, or all cells are inert.
- [ ] Roster empty state → one line, no mechanism lecture.
- [ ] "Redeem an invite code" moves out of the page's conclusion (own section at the top for holders, or its own route).
- [ ] `.fd-people__claim` → `Button`; local `fmtDate` → `fmt.ts`; `--r-input`/`--bg` → real tokens; labels → `.fd-label`.
- [ ] `FdPersonaChip` gets an `href` or the missing destination is recorded.
- [ ] `.fd-people .fd-chip + .fd-chip` margin → flex `gap`.

### `/foundry/persona` — `FdPersonaPage`
- [ ] Delete the empty state above the form that fills it; one explanation of the form, not three.
- [ ] Body/Skin/Hair/Eyes become real `<label>`s or stop looking like labels.
- [ ] Primary action → `Button size="md"`.
- [ ] `.fd-persona__preview` gets padding; the selected swatch's 4px ring gets clearance.
- [ ] `--fs-14` input → `--fs-13`; local `fmtDate` → `fmt.ts`.

### `/foundry/timeline` — `FdTimelinePage`
- [ ] Human label map for actions — no `publish_gdd_draft`, no "detail not rendered".
- [ ] Lane filter labels match the row chips exactly; lane colors are distinguishable or the encoding is dropped.
- [ ] Stats respond to the active lane, or say they are totals.
- [ ] One stamp format; every stamp an `FdTime`.
- [ ] Clickable and non-clickable stamps look different.
- [ ] `.fd-timeline__stats` → `.fd-statrow`; `.fd-timeline__lanechip` → `.fd-chip--mono`; "Load older" → `Button`.
- [ ] Row grid stops baseline-drifting when the body wraps.

### `/foundry/timeline/:eventId` — `FdTimelineEventPage`
- [ ] `<h1>` = the human sentence, not `event.action`; the raw kind moves to a fact with a `title`.
- [ ] Delete the "what" row's duplication of the h1.
- [ ] Absences render as one quiet value, not a sentence.
- [ ] The `<dl>` sits in a titled section; crumbs return to the timeline.
- [ ] `stampUTC` from `fmt.ts`; `--s-*` rhythm instead of 2px; `.fd-tlevent__back` deleted.

### `/foundry/sessions` — `FdSessionsPage`
- [ ] Empty state → one line; delete the three-CTA row.
- [ ] Say the horizon once.
- [ ] "Schedule a session" renders for everyone, disabled with a `title` when ungated.
- [ ] Card: one answer (when) leads; the other six fragments become chips.
- [ ] Delete the closing navigation note.
- [ ] "Retire series" leaves the RSVP row and gains a confirm.
- [ ] `.fd-sessions__form*` → `.fd-form`; count → `FdStat` shape; `whenLabel` → `stampUTC`.

### `/foundry/stewardship` — `FdStewardshipPage`
- [ ] ~209 words above the first datum → the five clauses, capped at `--measure`, and nothing else.
- [ ] Clause language drops implementation words ("write a row", "checks that this consent is currently granted").
- [ ] Two consecutive empty states → one line each; the appeal form's coupling to the decisions list is shown, not explained.
- [ ] Form title → `h3` matching siblings, via `.fd-form`.
- [ ] `.fd-stew__state` / `.fd-appeal__status` → `.fd-chip--mono`; `.fd-stew__appeals` → `.fd-board`.
- [ ] One stamp format on the page.
- [ ] `.fd-appeal__meta` stops nesting a 13px chip inside 11px mono.

### `/foundry/continuity` — `FdContinuityPage`
- [ ] Ten-column table → scene cards; the five count columns become chips.
- [ ] Detail moves to its own route (or gains an anchor, a real `h1`-level title and a way to close). The scene's name never renders as a 12px grey section label.
- [ ] Say the sandbox rule once.
- [ ] Bundle note → a chip row; no zero counts spelled out, no repeated parenthetical.
- [ ] "Download" is a row action, styled as one.
- [ ] `.fd-continuity__stats` and `__strip` → `.fd-statrow`; `__steward` → `--r-card`; `__basis` and `__bundlenote` get leading and measure.
- [ ] "no world" reads as an absence, not a value.

### `/foundry/console` (layout) — `FdConsoleLayout`
- [ ] One tab list, in one place; `activeTab()` returns `null` for unknown tails.
- [ ] Evidence gets a rail entry or becomes a section of the run page.
- [ ] "The games →" joins the list or leaves the rail.
- [ ] The layout owns `.fd-page`; children use `fd-stack`.
- [ ] Rail active state matches the chrome tab's active language.

### `/foundry/console/bench` — `FdBenchPage`
- [ ] Delete the inner `.fd-page`.
- [ ] Hide the fact rows that are absent on every card; add the stat row (runs, failing).
- [ ] Card headline = the game's name; the slug is a mono fact.
- [ ] Stamps → `FdTime` + `stampUTC`.
- [ ] Move `benchTargetsSentence()` here, or delete it; the trailing note leaves the section.
- [ ] Delete `--ink-25`, `--ink-60`, `--surface-2`, `--ink-9`.
- [ ] Rename to the canonical word: **Runs**.

### `/foundry/console/trajectories` — `FdTrajectoriesPage`
- [ ] Link the Scene cell; add an evidence column (the loader already computes the label).
- [ ] Empty state respects the `?scene` filter.
- [ ] Filtered head states the filter once (crumb or chip, not both).
- [ ] Provenance cell → one chip with a `title`, not word + chip + gloss.
- [ ] Stat values → `--fs-24`; the cap fact is expressed the same way as on every sibling.
- [ ] Ids show the game name; the truncated id is a mono fact.

### `/foundry/console/trajectories/$id` — `FdTrajectoryReplay`
- [ ] `<h1>` = the game + run, not a truncated id.
- [ ] One back affordance (crumbs).
- [ ] Finish reason, event count and the seat table render once each.
- [ ] "Finish reason: completed" is not a 24px stat — words are not statistics.
- [ ] One numbering form across facts, scrubber and slider.
- [ ] Scrubber keys scope to the transport, not `window`.
- [ ] Controls → `Button`; `--fs-10` → `--fs-11`; `toLocaleString()` → `groupDigits`.
- [ ] Delete the self-declared honesty sentences.

### `/foundry/console/costs` — `FdCostsPage`
- [ ] Delete the inner `.fd-page`.
- [ ] Probe rows leave the headline totals.
- [ ] "By day" and "Recent messages" do not both render one row — one table until there is data.
- [ ] Three trailing notes → at most one; "The copilot →" becomes a real link in the head or a section `aside`.
- [ ] Local `utcStamp` → `fmt.ts`; `.fd-costs__stats` → `.fd-statrow`; mono stat value → `--fs-24`.
- [ ] Empty variant uses the same section names as the populated one.

### `/foundry/console/evidence/$runId` — `FdEvidencePage`
- [ ] Section titles stop being uppercase filenames; `run.log` and `data.json` render as mono facts inside a titled section.
- [ ] `<h1>` = the game + run; the evidence label becomes a linked mono fact, not prose.
- [ ] Back goes where the reader came from (crumbs).
- [ ] The rail marks Evidence, not Bench.
- [ ] Empty frames → one line; the nested 420px scroll region goes.
- [ ] An unknown id renders the console layout with `.fd-empty`, not the site 404.
- [ ] `data.json` `dt`s → `.fd-label`; the log block → `--lh-code` (matching the ledger's rendering of the same bytes).

---

## 7. Gates

Every change lands green on, from `catalyrst/sites`:

1. `npx tsc -b --noEmit`
2. `(cd ../ui3 && npx tsc --noEmit)`
3. `npm test`
4. `npm run schemas:honesty:check`
5. `npm run test:e2e`

Copy pinned by tests (`foundry.deck.test.ts`, `foundry.gdd-program.test.ts`,
`foundry.play-response.test.ts`, `foundry.timeline.test.ts`) is changed
deliberately, with the assertion updated in the same commit — never worked
around.

Comments in code stay minimal: only non-obvious constraints. Rationale goes in
the commit message.
