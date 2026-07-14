# dcl-react-ui

[![CI](https://github.com/eordano/dcl-react-ui/actions/workflows/ci.yml/badge.svg)](https://github.com/eordano/dcl-react-ui/actions/workflows/ci.yml)

A full product-UI implementation of the Decentraland client surfaces in React + TypeScript: marketplace flows, the explorer HUD and the in-world overlay bundle, the creator hub, and governance. It is an application UI -- pages, workflows, and state wiring -- not a component kit; everything renders with `react-dom` alone, no UI framework.

## Quickstart

```bash
npm ci --ignore-scripts
npm run storybook   # catalog on http://localhost:5006
npm test            # jsdom suite
npm run dev         # explorer SPA against live public endpoints
```

## Install as a library

The package is not published to npm. Install it from a packed tarball (or a git checkout built the same way):

```bash
git clone https://github.com/eordano/dcl-react-ui
cd dcl-react-ui
npm ci --ignore-scripts
npm run build:lib
npm pack            # -> dcl-react-ui-0.9.9.tgz

cd ../your-app
npm install ../dcl-react-ui/dcl-react-ui-0.9.9.tgz
```

```tsx
import { ManaPill } from "dcl-react-ui";
```

`build:lib` compiles `src/` module-for-module to ESM in `dist/` (with `.d.ts`), keeping CSS imports in place. The package sets `sideEffects: ["*.css"]`, so bundlers tree-shake unused modules and keep only the styles of what you import. The root export is a curated barrel; everything else is reachable as `dcl-react-ui/dist/<path>`.

## Endpoints

All network access goes through two seams in `src/data`: `siteBase()` (the marketing/dapp site) and, for the Decentraland services, `catalystBase()` plus a per-service base map in `src/data/catalyst/client.ts`. The published defaults point at the public Decentraland infrastructure so the SPA and Storybook show real data out of the box:

| Service | Env override | Published default |
|---|---|---|
| catalyst (content + lambdas) | `VITE_CATALYST_URL` | `https://peer.decentraland.org` |
| places | `VITE_PLACES_URL` | `https://places.decentraland.org` |
| events | `VITE_EVENTS_URL` | `https://events.decentraland.org` |
| communities | `VITE_COMMUNITIES_URL` | `https://social-api.decentraland.org` |
| communities thumbnails | `VITE_COMMUNITIES_CDN_URL` | `https://cdn.decentraland.org` |
| notifications | `VITE_NOTIFICATIONS_URL` | `https://notifications.decentraland.org` |
| badges | `VITE_BADGES_URL` | `https://badges.decentraland.org` |
| camera reel | `VITE_CAMERA_REEL_URL` | `https://camera-reel-service.decentraland.org` |
| map renders | `VITE_MAP_URL` | `https://api.decentraland.org` |
| satellite tiles | `VITE_SATELLITE_URL` | none public -- the map skips the layer |
| site | `VITE_SITE_URL` | `https://decentraland.org` |

Every base is also injectable at runtime (`window.__SITE_BASE__`, `window.__CATALYST_BASE__`, `window.__SERVICE_BASES__ = { places: "...", ... }`), and per-call via the `base` request option. A self-hosted node that fronts all services on one host just points every var at it. Services that need a signed identity (notifications, favorites/likes, uploads) degrade to their empty states when no engine bridge or wallet is present.

## The catalog

Storybook is the map of the repo: 191 story files cover the atoms, components, and product pages, with network calls mocked by MSW (`.storybook/public/mockServiceWorker.js`), theme + viewport toolbars, and an a11y panel. `npm run storybook` serves it on :5006; `npm run build-storybook` writes a static build to `storybook-static/`.

### Story authoring: Controls, not variant stories

A component with eight stories that differ only by prop values is one story with `argTypes`. Default
to one `Default` export whose variant space lives in `meta.argTypes` / `meta.args`; add a second
export only when args cannot express it. Reference implementations: `atoms/Button.stories.tsx`
(simplest), `components/EmptyState.stories.tsx` (`ReactNode` props via `mapping`),
`governance/components/SubmitProposalForm.stories.tsx` (fixture picked by name).

```tsx
// before -- 4 exports that differ by one prop each
export const AccessPublic = { args: { accessType: "unrestricted" } };
export const AccessAllowList = { args: { accessType: "allowList" } };
export const AccessPassword = { args: { accessType: "sharedSecret" } };
export const Loading = { args: { loading: true } };

// after -- one export; every state reachable from the Controls panel
const meta = {
  title: "CreatorHub/Components/World Permissions",
  component: ChModalWorldPermissions,
  argTypes: {
    accessType: { control: "select", options: ["unrestricted", "allowList", "sharedSecret"] },
    loading: { control: "boolean" },
  },
  args: { accessType: "allowList", loading: false },
} satisfies Meta<typeof ChModalWorldPermissions>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
```

Rules:

- `satisfies Meta<typeof Component>` + `StoryObj<typeof meta>` keeps `args` type-checked against the
  real props. Enumerate the option list yourself -- do not rely on inference.
- Optional enum props use `control: "select"`; its empty option clears the arg. `inline-radio`
  cannot be cleared, so use it only for props with no meaningful "unset".
- `ReactNode` / object props belong in Controls too, via `options` + `mapping`. `EmptyState`
  exposes `icon`, `subtitle` and `actions` this way, with a `none` key mapping to `undefined` so
  "absent" is a first-class option.
- A whole fixture becomes one named-preset arg. Where stories differed by which data blob was
  passed, add a synthetic arg naming the fixture and resolve it in `meta.render`
  (`SubmitProposalForm` turns 17 `GOVERNANCE_FORMS` presets into a `form` select). Type the meta
  against the *args* -- `satisfies Meta<FormStoryArgs>` -- so the synthetic arg type-checks while
  `component:` still points at the real component.
- Controls-only args that the component does not accept must never be spread onto it; destructure
  them out in `render`.

Keep a separate export only for something args cannot reach:

- a different component (`EmptyState`'s `Card` renders `EmptyStateCard`);
- a different `parameters` block (`ScreenCover` needs `layout: "fullscreen"` -- parameters are
  not args);
- a genuinely different composition -- a different child arrangement, not a different prop value
  (`ManaMark`'s `InBalance` puts the atom inside a balance row);
- a `Catalog`, below.

`Catalog` stories buy back the coverage a collapse removes. `vitest.browser.config.ts` turns
every story into a render + axe test (`preview.tsx` sets `a11y: { test: "error" }`, so violations
fail), and `tools/story-shots` takes one screenshot per story. Collapsing N variant stories drops
N-1 rendered states out of both gates. When the collapsed states exercise structurally different
subtrees, add one export that renders them all together with
`parameters: { controls: { disable: true } }` -- precedent `Atoms/Primitives`, then
`EmptyState`, `World Permissions` and `SubmitProposalForm`. One story, one baseline, every state
still gated.

A `Catalog` stacks several instances of the same component on one page, which surfaces
page-scoped a11y rules and layout collisions the single-instance stories never hit. Four seen so
far, each with a component-side fix -- never disable an axe rule to get a catalog green; zero are
disabled in this repo and that is the invariant:

- `landmark-unique`, unnamed landmarks -- N bare `<aside>`/`<nav>`. Wrap each entry in a bare
  `<section>`: under the HTML-AAM scoped mapping an unnamed `aside` inside a `section` demotes to
  `generic`. (`SubmitProposalForm`'s catalog does exactly this; without it axe fails 17 times.)
- `landmark-unique`, named landmarks -- the harder case. axe compares landmarks by accessible
  name, not by id, so `useId()` does nothing for a `nav[aria-label="Profile sections"]` or a
  `[role="region"][aria-label="Your Storage"]`: N copies give N landmarks with one identical name,
  and the `<section>` wrapper does not demote a *named* landmark. The fix is an optional
  `labelSuffix` prop on the component that owns the landmark, threaded through every landmark
  `aria-label` it renders via `suffixLabel()` from `components/labelSuffix.ts`. Omitting it (the
  default) leaves today's names byte-identical, so no consumer changes -- the same "optional prop,
  default preserves current behaviour" shape as `MarketplaceChromeMaybe`'s `chrome`. Reference:
  `web/pages/StProfileCommunitiesTab`.
- portalled dialogs -- `components/Modal` `createPortal`s to `document.body` on a
  `position: fixed; inset: 0` backdrop, so catalog entries land on top of one another and one
  screenshot captures only the topmost. This usually *passes* axe (which scopes to the story root
  the portalled dialog escaped) while silently failing the visual gate. `<Modal portal={false}>`
  renders the same card in normal document flow with no portal, no body scroll lock and no focus
  trap; components that own a Modal forward an optional `portal?: boolean` (default `true`)
  straight to it. Reference: `creatorhub/components/ChModalDeleteProject`.
- duplicate `id` attributes when a component derives ids from prop names instead of `useId` --
  a component bug, not a story bug, and one axe 4.12 no longer reports, so the catalog goes
  green while every `<label htmlFor>` silently points at the first instance. Fix the component with
  `useId()`; this is the one blocker where `useId` is the right tool, because it is about ids,
  not names.
- Use plain `<div>` labels between catalog entries, not headings -- interleaved `<h3>` labels break
  `heading-order` against the component's own headings.
- Pass `chrome={false}` to every entry where the component supports it, or the stack emits N
  `<main>` landmarks and fails `landmark-unique` for that reason instead.

Never collapse interaction stories. Files named `*.interactions.stories.tsx`, and any story
carrying a `play()` function, are behavioural tests run by both `npm test` and
`npm run test:browser`. Each `play()` is a distinct scenario with its own assertions; merging two of
them deletes test coverage. Do not merge them, do not fold them into `argTypes`, and do not add a
`play()` to a `Default` that other stories inherit args from. They are out of scope for any
story-collapsing pass.

## Builds

| Command | Output |
|---|---|
| `npm run dev` | vite dev server for the explorer SPA (`src/app`) |
| `npm run build:app` | SPA -> `dist-app/` |
| `npm run build:overlay` | in-world HUD bundle (`src/overlay/overlay-main.tsx`) -> `dist-overlay/overlay.js` |
| `npm run build:lib` | tree-shakeable ESM library -> `dist/` (see "Install as a library") |
| `npm run typecheck` | `tsc --noEmit` |

`npm run overlay:publish` syncs `dist-overlay/` into the `../../bevy-explorer` checkout; without one it prints a notice and exits 0.

## Testing

Interaction tests live in the stories as Storybook `play` functions (files named `*.interactions.stories.tsx`). They use `storybook/test` (`userEvent` / `within` / `expect` / `fn` / `fireEvent`), are debuggable in the Storybook Interactions panel, and run headlessly by two runners over the same `play` functions:

| Command | Runner | DOM | Scope |
|---|---|---|---|
| `npm test` | vitest + jsdom, portable stories (`composeStories`) | jsdom | 35 files / 254 tests |
| `npm run test:browser` | `@storybook/addon-vitest` + `@vitest/browser` + playwright | real Chromium | every story: 191 files / 681 tests |
| `npm run test:all` | both | | |

- jsdom -- `vitest.config.ts` (jsdom + `@vitejs/plugin-react`): unit tests and bridge-mocking `*.interactions.test.tsx` files. No browser needed.
- real-DOM -- `vitest.browser.config.ts`: the `storybookTest` plugin turns EVERY story into a browser test (opt out per story with `tags: ["no-test"]`), running its `play` function and the a11y (axe) gate in headless Chromium. The browser resolves in order: `$CHROMIUM_BIN`, a `/nix/store/*chromium*` install, playwright's managed Chromium.
- Shared setup: `.storybook/vitest.setup.ts` applies the Storybook preview decorators (`setProjectAnnotations`) + `@testing-library/jest-dom` matchers.

### Add a test

1. Add a story (a `play` function makes it an interaction test; without one it is still a render + a11y test). The browser runner picks up every story automatically -- no registration. Opt out with `tags: ["no-test"]`.
2. If it needs a mocked module dependency, give it a dedicated `*.interactions.test.tsx` that `vi.mock`s the module and asserts on the mock (jsdom only; the browser runner asserts the observable UI effect via the `play`).
