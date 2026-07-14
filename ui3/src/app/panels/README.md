# Panel module contract

The SPA shell auto-discovers every panel via `import.meta.glob<PanelModule>("./panels/*.route.{jsx,tsx}")` in `src/app/router.tsx`. Add a file here - never edit a shared shell file.

## Where

`src/app/panels/<Name>.route.tsx`. The route id (and hash url) is the filename lowercased, minus `.route.tsx` - e.g. `Map.route.tsx` -> id `map` -> `#/map`. 15 panels today: Backpack, Camera, Chat, Communities, Events, Friends, Gallery, Map, Notifications, Passport, Places, Settings, Skybox, SmartWearables, VoiceChat.

Where a panel corresponds to a chrome tab, the id MUST equal the `id` of the tab in `src/explorer/frames/ExploreChrome.tsx` `EXPLORE_TABS` (or the `passport` alias) so the tab highlights + hover-prefetch resolve.

## Exports

1. `export default` - the panel React component. Rendered inside `ExploreChrome`'s body via `<Outlet/>`, already wrapped in `<Suspense>`. Receives no required props: it calls its own data hook and passes results into the reused ui3 page. An unwired/loading panel must still render a real loading/empty state.
2. `export function prefetch(queryClient)` (optional but recommended) - warms the TanStack Query cache for instant render; called on hover/focus intent over the tab. Use the same `qk.*` keys + `STALE.*` + query fn your hook uses. Must be best-effort (never throw to the caller).

## Rules

- Data: build HTTP wrappers at `src/data/catalyst/<domain>.ts` over `getJSON` from `src/data/catalyst/client.ts`. Wrap reads in `src/data/hooks/use<Resource>.ts` (`useQuery`). Keys come from `qk` in `src/data/queryKeys.ts`; staleTime from `STALE`.
- AbortSignal: forward `{ signal }` from the query fn ctx into `getJSON` (`queryFn: ({ signal }) => fetchX(addr, { signal })`) so panel switches cancel in-flight reads.
- Identity (read-only): `import { useBridgeState } from "../../overlay/bridge"` -> `useBridgeState().identity.address` / `.isGuest`. Never assume an inbound open/close event; the overlay is self-driven.
- Engine actions (outbound only): teleport via `window.engine?.teleport(x, y)`, realm switch via `window.engine?.changerealm(url)`. No other engine coupling.
- Auth-gated / edge-404 surfaces (notifications, badges, photos, friends, live communities list): render real empty/error states when signed-out or unavailable - fixtures were removed in the ui3 fixture-removal sweep; no fixture data on pages. Signed reads/writes attach identity headers through the same `getJSON` path.
- Reuse ui3 pages from `src/explorer/{pages,components,frames}` - do not rebuild UI.

See `_TEMPLATE.route.jsx.txt` for a copy-paste skeleton.
