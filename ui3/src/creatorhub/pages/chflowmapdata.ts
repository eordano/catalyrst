import {
  auto,
  click,
  computeStats,
  load,
  node,
  outcome,
  route,
  sep,
  state,
  step,
} from "../../flowmap/flowmapdata";
import type { FlowSection } from "../../flowmap/flowmapdata";

const EDITOR = (chains?: string[]) =>
  state("EDITOR", {
    href: "/creator-hub/scene-editor",
    sub: "/creator-hub/scene-editor",
    chains,
  });


export const SECTIONS: FlowSection[] = [
  {
    id: "entry",
    num: "01",
    title: "Entry & Home",
    blurb:
      "The hub shell at /create: rail navigation, the Start-building fast path, the home cards, and sign-in. Every rail door stays in-product.",
    tracks: [
      {
        chain: "home",
        items: [route("/create", { sub: "HOME" })],
        branches: [
          {
            chain: "home",
            chips: true,
            note: "the rail — Land → /shop and Names → /marketplace/names stay in-product; Curate appears only for committee wallets",
            items: [
              node("chip", "Home", { href: "/create" }),
              node("chip", "Scenes", { href: "/create/scenes" }),
              node("chip", "Templates", { href: "/create/templates" }),
              node("chip", "Collections", { href: "/create/wearables" }),
              node("chip", "Curate", { href: "/create/curate", sub: "committee" }),
              node("chip", "Worlds", { href: "/creator-hub/manage" }),
              node("chip", "Land", { href: "/shop" }),
              node("chip", "Names", { href: "/marketplace/names" }),
              node("chip", "Activity", { href: "/creator-hub/activity" }),
              node("chip", "Metrics", { href: "/creator-hub/metrics" }),
              node("chip", "Data sources", { href: "/creator-hub/data-sources" }),
              node("chip", "Learn", { href: "/create/learn" }),
              node("chip", "⚙ Settings", { href: "/creator-hub/settings" }),
              node("chip", "Sign in", { title: "opens the sign-in modal — see below" }),
            ],
          },
          {
            chain: "start-building",
            items: [
              load("LOAD engine · “Loading scene editor…”", { label: "Start building" }),
              EDITOR(["start-building"]),
            ],
          },
          {
            chain: "home-cards",
            chips: true,
            note: "the home cards — Deploy carries ?from=home so its breadcrumb returns here; the Manage card mirrors the rail doors",
            items: [
              node("chip", "Your published scenes", { href: "/creator-hub/my-scenes" }),
              node("chip", "New scene", { href: "/creator-hub/scene-editor?new=1&from=home" }),
              node("chip", "Browse templates", { href: "/create/templates" }),
              node("chip", "Deploy a scene", { href: "/creator-hub/deploy-world?from=home" }),
              node("chip", "Get the desktop app", { href: "/landings/creator-hub-download" }),
            ],
          },
          {
            chain: "see-all",
            items: [click("See All"), route("/create/scenes"), sep(), route("/create/learn")],
          },
          {
            chain: "sign-in",
            items: [
              click("Sign in"),
              node("modal", "SIGNIN_MODAL"),
              click("OTP / social"),
              outcome("signed-in"),
            ],
          },
        ],
      },
    ],
  },

  {
    id: "create",
    num: "02",
    title: "Create a Scene",
    machines: ["create-project", "delete-project"],
    blurb:
      "Template pick → confirm → straight into the editor. The naming wizard at /creator-hub/create-project survives as the deep-link/legacy path. Reopening a saved or deployed scene hydrates its composite. Deleting writes a signed tombstone.",
    tracks: [
      {
        chain: "template-flow",
        items: [
          route("/create/templates"),
          click("card"),
          node("modal", "CONFIRM_MODAL"),
          load("LOAD engine · “Loading scene editor…”", { label: "Create" }),
          state("EDITOR", {
            href: "/creator-hub/scene-editor",
            sub: "?new=1&template=X&from=templates",
            chains: ["template-flow", "start-building"],
          }),
        ],
        branches: [
          {
            chain: "template-flow",
            items: [click("Cancel / Esc"), outcome("back")],
          },
        ],
      },
      {
        chain: "legacy-wizard",
        note: "deep-link/legacy path — no in-product click leads here anymore",
        items: [
          route("/creator-hub/create-project", { sub: "machine: create-project" }),
          step(),
          state("NAMING", { sub: "name prefilled" }),
          click("Create"),
          state("SCAFFOLDING"),
          load("write files"),
          state("CREATED"),
          click("Open in editor"),
          EDITOR(["legacy-wizard"]),
        ],
        branches: [
          {
            chain: "legacy-wizard",
            items: [
              auto("no template"),
              click("pick"),
              state("TEMPLATING"),
              load(),
              state("SCAFFOLDING"),
            ],
          },
          {
            chain: "scaffold-error",
            items: [
              auto("error"),
              state("ERROR"),
              click("Retry / Choose folder"),
              state("SCAFFOLDING"),
            ],
          },
        ],
      },
      {
        chain: "reopen",
        items: [
          route("/create/scenes"),
          load("hydrate composite", { label: "scene card" }),
          state("EDITOR", {
            href: "/creator-hub/scene-editor",
            sub: "reopen + continue",
          }),
        ],
        branches: [
          {
            chain: "reopen",
            chips: true,
            note: "empty state",
            items: [
              node("chip", "Import"),
              node("chip", "Templates", { href: "/create/templates" }),
              node("chip", "Sign in"),
            ],
          },
        ],
      },
      {
        chain: "published",
        items: [
          route("/creator-hub/my-scenes", { sub: "scenes deployed by your wallet" }),
          click("Open"),
          state("EDITOR", {
            href: "/creator-hub/scene-editor",
            sub: "?pointer=… · reopen the live scene",
          }),
        ],
        branches: [
          {
            chain: "published",
            items: [auto("none yet"), click("Start from a template"), route("/create/templates")],
          },
        ],
      },
      {
        chain: "delete",
        items: [
          route("/creator-hub/delete-project", { sub: "machine: delete-project" }),
          step(),
          state("CONFIRM"),
          load("signed tombstone", { label: "Delete" }),
          outcome("done", { sub: "?local=deleted | kept" }),
        ],
      },
    ],
  },

  {
    id: "editor",
    num: "03",
    title: "Editor Loop",
    machines: ["scene-editor-place-items"],
    blurb:
      "Boot, then the place-and-save loop; preview is an explicit Play. Drafts sync in the background to the /api/creator-hub/drafts JSON API. Exits follow the ?from= breadcrumb.",
    tracks: [
      {
        chain: "start-building",
        items: [
          route("/creator-hub/scene-editor", { sub: "machine: scene-editor-place-items" }),
          load("BOOT"),
          state("EDITING"),
        ],
      },
      {
        chain: "save-loop",
        items: [
          state("EDITING"),
          click("Open Assets"),
          state("BROWSING"),
          click("place"),
          state("PLACING"),
          click("Create entity"),
          state("TRANSFORMING"),
          click("axes"),
          state("MODIFYING"),
          load("FSA write", { label: "Save" }),
          state("SAVED"),
          click("continue"),
          state("EDITING"),
        ],
      },
      {
        chain: "preview",
        note: "no auto-play; Stop ≠ reload",
        items: [
          state("EDITING"),
          load("“Loading preview…”", { label: "▶ Play" }),
          state("PREVIEW"),
          { t: "edge", kind: "reversible", label: "Pause / Play" },
          click("■ Stop"),
          state("EDITING"),
        ],
      },
      {
        chain: "sync",
        note: "background — no click; the rail chip mirrors draft-sync state",
        items: [
          state("EDITING"),
          auto("draft saved"),
          load("PUT /api/creator-hub/drafts/:id · signed auth chain"),
          outcome("sync chip: Synced", { sub: "308 shim covers the old /creator-hub/drafts path" }),
        ],
      },
      {
        chain: "exits",
        items: [
          state("EDITING"),
          click("Exit"),
          outcome("breadcrumb origin", {
            sub: "?from= home | scenes | templates | manage | operator — default /create/scenes",
          }),
          sep(),
          click("Publish"),
          node("jump", "deploy flow", { href: "#deploy" }),
        ],
      },
    ],
  },

  {
    id: "wearables",
    num: "04",
    title: "Wearables & Collections",
    machines: [
      "wearable-create-collection",
      "wearable-item-editor",
      "wearable-publish-collection",
    ],
    blurb:
      "Collections home, the new-collection wizard, per-item editing, and the publish flow (its Pay step is a disclosed stub).",
    tracks: [
      {
        chain: "collections",
        items: [route("/create/wearables", { sub: "COLLECTIONS home" })],
        branches: [
          {
            chain: "new-collection",
            items: [
              click("New collection"),
              route("/create/wearables/collections/new", {
                sub: "machine: wearable-create-collection",
              }),
              step(),
              state("NAMING", { sub: "⏎ submits · “third-party?” → ?type=linked" }),
              step(),
              state("ITEMS", { sub: "dropzone: .zip · .glb · .gltf · .png — remove" }),
              step(),
              state("REVIEW"),
              step(),
              state("SUBMITTING", { busy: true }),
              step(),
              state("DONE"),
              step(),
              outcome("detail"),
            ],
          },
          {
            chain: "collection-detail",
            items: [
              click("collection"),
              route("/create/wearables/collections/:id", {
                href: "/create/wearables",
                sub: "tabs: items ⇄ activity (?tab)",
                title: "parameterized — link opens the collections list",
              }),
            ],
            branches: [
              {
                chain: "item-editor",
                items: [
                  click("item row"),
                  route("/create/wearables/item-editor", { sub: "ITEM-EDITOR" }),
                  step(),
                  state("SELECT"),
                  step(),
                  state("MODEL"),
                  step(),
                  state("CATEGORY"),
                  step(),
                  state("RARITY"),
                  step(),
                  state("PRICE"),
                  load(undefined, { label: "Save" }),
                  outcome("done"),
                ],
              },
              {
                chain: "publish-collection",
                items: [
                  click("Publish"),
                  route("/create/wearables/publish", {
                    sub: "machine: wearable-publish-collection",
                  }),
                  step(),
                  state("SUMMARY"),
                  step(),
                  state("COST"),
                  step(),
                  state("TERMS"),
                  load("disclosed stub", { label: "Pay" }),
                  state("SUBMITTED"),
                ],
              },
            ],
          },
          {
            chain: "single-item",
            items: [
              click("item"),
              route("/create/wearables/items/:id", {
                href: "/create/wearables",
                title: "parameterized — link opens the collections list",
              }),
              click("Edit"),
              route("/create/wearables/item-editor?step=model", {
                href: "/create/wearables/item-editor?step=model",
              }),
            ],
          },
        ],
      },
    ],
  },

  {
    id: "worlds",
    num: "05",
    title: "Worlds",
    machines: ["manage-worlds", "worlds-storage", "world-settings", "world-permissions"],
    blurb:
      "Manage worlds, watch storage quota, edit settings (Unpublish is real), and grant permissions (commit writes the real ACL via signed POST /world/<name>/permissions/access).",
    tracks: [
      {
        chain: "worlds-manage",
        items: [route("/creator-hub/manage")],
        branches: [
          {
            chain: "worlds-manage",
            items: [
              click("card"),
              outcome("settings / layout", { href: "/creator-hub/world-settings" }),
            ],
          },
          {
            chain: "storage",
            items: [
              click("Your Storage"),
              route("/creator-hub/worlds-storage", { sub: "storage panel" }),
            ],
          },
        ],
      },
      {
        chain: "storage",
        items: [
          route("/creator-hub/worlds-storage"),
          click("SELECT world"),
          state("QUOTA PANEL", { sub: "DAO-proposal link" }),
          sep(),
          node("external", "BUY MANA / LAND / NAME"),
        ],
      },
      {
        chain: "world-settings",
        items: [
          route("/creator-hub/world-settings", { sub: "machine: world-settings · tabs" }),
          load("invoke", { label: "Save" }),
          outcome("saved"),
        ],
        branches: [
          { chain: "world-settings", items: [click("Discard"), node("end", "")] },
          {
            chain: "world-settings",
            items: [load("real", { label: "Unpublish" }), node("end", "")],
          },
        ],
      },
      {
        chain: "world-permissions",
        items: [
          route("/creator-hub/world-permissions", { sub: "machine: world-permissions · tabs" }),
          click("invite / add collaborator / password ≥8+2num"),
          state("COMMIT", { busy: true, sub: "signed ACL write" }),
        ],
      },
    ],
  },

  {
    id: "activity",
    num: "05b",
    title: "Activity & Data sources",
    blurb:
      "Occupancy for the worlds you deployed, sampled every ~5 minutes, plus the ledger of every endpoint the hub reads. Nothing here writes. A datum that did not arrive renders as “—” beside the endpoint that failed to produce it — never as a zero.",
    tracks: [
      {
        chain: "activity",
        items: [route("/creator-hub/activity", { sub: "your worlds · headcount only" })],
        branches: [
          {
            chain: "activity",
            items: [
              auto("no address"),
              state("NO ADDRESS", {
                sub: "scoping, not a login — [Connect wallet] or ?address=",
              }),
            ],
          },
          {
            chain: "activity",
            note: "per-row: live count · a real 0 with a note · “no sample” ≠ 0 · NEVER DEPLOYED · BLOCKED",
            items: [
              click("world row"),
              route("/creator-hub/activity/:world", {
                href: "/creator-hub/activity",
                sub: "right now · history · deployed · access · reception",
              }),
            ],
          },
          {
            chain: "activity-parcel",
            note: "a parcel cannot be listed, only looked up — no endpoint maps a wallet to its parcels",
            items: [
              click("Look up x,y"),
              route("/creator-hub/activity", { sub: "?pointer=x,y — inline history" }),
            ],
          },
        ],
      },
      {
        chain: "data-sources",
        items: [
          route("/creator-hub/data-sources", { sub: "the source ledger" }),
          load("probe live + sampled rows (4s timeout)"),
          outcome("● live · ◐ sampled · ◔ snapshot · ⊘ unavailable · ▨ not built · ✕ excluded", {
            sub: "unbuilt and excluded rows are constants and are never probed",
          }),
        ],
      },
    ],
  },

  {
    id: "deploy",
    num: "06",
    title: "Deploy & Claim",
    machines: ["deploy-scene", "claim-name"],
    blurb:
      "Publish a scene to a World name via a signed deploy; claim a new name and come back with it preselected. The breadcrumb honors ?from= — scene-editor entries get “Back to editor”.",
    tracks: [
      {
        chain: "deploy",
        items: [
          route("/creator-hub/deploy-world", { sub: "machine: deploy-scene · ?from= breadcrumb" }),
          step(),
          state("PICK NAME", { sub: "live · ?name= preselect" }),
          load("signed deploy", { label: "Deploy" }),
          state("SUCCESS"),
        ],
        branches: [
          {
            chain: "claim",
            items: [
              click("Claim name"),
              route("/creator-hub/claim-name", { sub: "marketplace claim" }),
              click("Use in Publish to World"),
              outcome("back with ?name"),
            ],
          },
        ],
      },
    ],
  },

  {
    id: "metrics",
    num: "07",
    title: "Metrics",
    machines: ["metrics"],
    blurb:
      "Creator metrics are scoped to your wallet — cards plus a per-scene visits table from the places API. The Network tab is network-wide data, none of it yours.",
    tracks: [
      {
        chain: "metrics",
        items: [route("/creator-hub/metrics")],
        branches: [
          { chain: "metrics", items: [auto("signed-out"), state("GATE")] },
          {
            chain: "metrics",
            note: "per-card “Not available” on failure · EmptyState if none",
            items: [
              auto("creator"),
              outcome("cards", { sub: "real: collections · on-sale · sales · visits" }),
              step(),
              outcome("per-scene visits table", { sub: "real rows · per-row “Not available”" }),
            ],
          },
          {
            chain: "network",
            items: [
              click("Network tab"),
              route("/creator-hub/operator-metrics", { sub: "network-wide — not your scenes" }),
              step(),
              outcome("live presence · deploy funnel · admin activity", {
                sub: "or “Presence unavailable”",
              }),
            ],
          },
        ],
      },
    ],
  },

  {
    id: "curate",
    num: "08",
    title: "Curation",
    machines: ["curate-committee"],
    blurb:
      "Committee-only review queue: pick a row, decide, back to the queue. The rail item shows only for wallets on the committee.",
    tracks: [
      {
        chain: "curate",
        items: [
          route("/create/curate", { sub: "committee only — rail item is membership-gated" }),
          step(),
          state("QUEUE", { sub: "filters ?status ?type ?assignee=me · search · sort" }),
          click("row"),
          state("REVIEW"),
          click("approve / reject + comment"),
          state("DECIDED"),
          step(),
          outcome("queue"),
        ],
      },
    ],
  },

  {
    id: "learn-meta",
    num: "09",
    title: "Learn, Settings & Meta",
    blurb:
      "Learn links out to videos and docs (plus this map), Settings holds preferences, the download landing ships the desktop app — and the map maps itself.",
    tracks: [
      {
        chain: "learn",
        items: [route("/create/learn")],
        branches: [
          {
            chain: "learn",
            chips: true,
            note: "Videos · Creator Docs · More — external tabs, plus one in-product link",
            items: [
              node("external", "Videos"),
              node("external", "Creator Docs"),
              node("external", "Studios"),
              node("chip", "Creator Hub flow map", { href: "/creator-hub/map" }),
            ],
          },
        ],
      },
      {
        chain: "settings",
        items: [
          route("/creator-hub/settings", { sub: "preferences" }),
          click("Close"),
          outcome("back", { sub: "history back · /create fallback" }),
        ],
      },
      {
        chain: "download",
        items: [
          route("/landings/creator-hub-download", { sub: "desktop app landing" }),
          click("Download"),
          outcome("installer", { sub: "per-OS build" }),
        ],
      },
      {
        chain: "map",
        items: [
          route("/creator-hub/map", { sub: "this page — linked from Learn" }),
          sep(),
          route("/explorer-map", { sub: "sibling — the Explorer client, mapped the same way" }),
        ],
      },
    ],
  },
];


export const STATS = computeStats(SECTIONS);


export const ASCII_SOURCE = `# Creator Hub — click/state sitemap

Every edge is one user CLICK, or a LOAD⏳ when the transition invokes work >100ms
(engine boot, folder picker, signed deploy, scaffolding). States in CAPS are
machine states; \`/paths\` are routes. Regenerated against the live route tree.

\`\`\`
/create (HOME)
├── rail: Home · Scenes · Templates · Collections ┃ Curate(committee-gated) ·
│         Worlds · Land(/shop) · Names(/marketplace/names) · Metrics ┃ Learn
│         (+ ⚙ Settings · Sign in — every door stays in-product)
├── cards: Your published scenes → /creator-hub/my-scenes · New scene → EDITOR ·
│          Browse templates · Deploy a scene(?from=home) · Get the desktop app
│          (+ a Manage card mirroring the rail doors)
│
├─[Start building]──⏳ LOAD engine ("Loading scene editor…")──▶ EDITOR
├─[See All]────────▶ /create/scenes · /create/learn
├─[Sign in]────────▶ SIGNIN_MODAL ──[OTP/social]──▶ signed-in
│
├── /create/templates
│    └─[card]──▶ CONFIRM_MODAL ──[Create]──⏳ LOAD engine ──▶ EDITOR
│                     │            (?new=1&template=X&from=templates)
│                     └─[Cancel/Esc]──▶ back
├── /creator-hub/create-project        (machine: create-project — LEGACY,
│    deep-link only; no in-product click leads here anymore)
│    NAMING ──[Create]──▶ SCAFFOLDING ──⏳ write files ──▶ CREATED ──[Open in
│    editor]──▶ EDITOR   └─ error ──▶ ERROR ──[Retry]/[Choose folder]──▶ retry
│
├── EDITOR  /creator-hub/scene-editor   (machine: scene-editor-place-items)
│    ⏳ BOOT ▶ EDITING ─[Open Assets]▶ BROWSING ─[place]▶ PLACING ─[Create entity]▶
│    TRANSFORMING ─[axes]▶ MODIFYING ─[Save]⏳ FSA write ▶ SAVED ─[continue]▶ EDITING
│    EDITING ─[▶ Play]⏳ "Loading preview…" ▶ PREVIEW ─[⏸ Pause]⇄[▶]─[■ Stop]▶ EDITING
│    background: draft saved ──⏳ PUT /api/creator-hub/drafts/:id (signed) ──▶
│                sync chip Synced   (308 shim at the old /creator-hub/drafts)
│    ─[Exit]▶ breadcrumb origin (?from= home|scenes|templates|manage|operator;
│             default /create/scenes) · ─[Publish]▶ deploy flow
│
├── /create/scenes ── [scene card]──⏳ hydrate composite ──▶ EDITOR (reopen+continue)
│    └─ empty: [Import]/[Templates]/[Sign in]
├── /creator-hub/my-scenes ── [Open]──▶ EDITOR (?pointer=… reopen live scene)
│    └─ empty: [Start from a template]──▶ /create/templates
│
├── /create/wearables (COLLECTIONS)
│    ├─[New collection]▶ /collections/new   (machine: wearable-create-collection)
│    │   NAMING(⏎ submits; "third-party?"→?type=linked) ─▶ ITEMS(upload dropzone,
│    │   .zip/.glb/.gltf/.png, remove) ─▶ REVIEW ─▶ SUBMITTING⏳ ─▶ DONE ─▶ detail
│    ├─[collection]▶ /collections/:id  tabs items⇄activity (?tab)
│    │   ├─[item row]▶ ITEM-EDITOR: SELECT▶MODEL▶CATEGORY▶RARITY▶PRICE▶SAVE⏳▶ done
│    │   └─[Publish]▶ PUBLISH: SUMMARY▶COST▶TERMS▶PAY⏳(disclosed stub)▶ SUBMITTED
│    └─[item]▶ /items/:id ─[Edit]▶ item-editor?step=model
│
├── WORLDS
│    ├─ /creator-hub/manage ─[card]▶ settings/layout · ─[Your Storage]▶ storage panel
│    ├─ storage: SELECT world ▶ QUOTA panel (DAO-proposal link) · BUY MANA/LAND/NAME↗
│    ├─ world-settings: tabs ─[Save]⏳ invoke ▶ saved · [Discard] · [Unpublish]⏳ real
│    └─ world-permissions: tabs ─[invite]/[add collaborator]/[password ≥8+2num]─▶
│                          COMMIT⏳ (signed ACL write)
│
├── /creator-hub/deploy-world  (machine: deploy-scene; breadcrumb honors ?from=,
│    from=scene-editor ▶ "Back to editor")
│    PICK NAME(live; ?name= preselect) ─[Deploy]⏳ signed deploy ─▶ SUCCESS
│    └─[Claim name]▶ /creator-hub/claim-name ─▶ [Use in Publish to World]──back w/ ?name
│
├── /creator-hub/metrics   signed-out▶GATE · creator▶cards (real: collections/
│    on-sale/sales/visits) + per-scene visits table (real rows from the places
│    API; per-row "Not available" on gaps)
│    └─ Network tab ▶ /creator-hub/operator-metrics — network-wide presence,
│       deploy funnel, admin activity (zero of-your-scenes data)
│
├── /create/curate (committee; rail item shows per-membership): QUEUE(filters
│    ?status ?type ?assignee=me, search, sort) ─[row]▶ REVIEW ─[approve/reject
│    +comment]▶ DECIDED ─▶ queue
│
├── /create/learn ── Videos↗ · Creator Docs↗ · Studios↗ · Creator Hub flow map
│    (▶ /creator-hub/map — this document)
├── /creator-hub/settings ── preferences ─[Close]▶ back (history · /create)
├── /landings/creator-hub-download ── [Download]▶ per-OS installer
├── delete-project: CONFIRM ─[Delete]⏳ signed tombstone ▶ done (?local=deleted|kept)
└── /creator-hub/map — this map · sibling: /explorer-map (the Explorer client)
\`\`\`

Legend: \`[x]\` = click edge · \`⏳\` = >100ms invoked load (spinner/status shown) ·
\`⇄\` = reversible pair · \`↗\` = external tab. Redirect shims carry no clicks and
are compressed: /creator-hub → /create · /create/about → /create ·
/creator-hub/deploy-alternative → deploy-world · /builder/* → in-product
equivalents (/shop, /marketplace/names, …) · /creator-hub/drafts/* →
/api/creator-hub/drafts/* (308). Orphans/gaps (metrics-funnel,
integration-create-entry) are intentionally absent — not implemented.
`;
