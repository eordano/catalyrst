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
            note: "the rail \u{2014} Land \u{2192} /shop and Names \u{2192} /marketplace/names stay in-product; Curate appears only for committee wallets",
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
              node("chip", "\u{2699} Settings", { href: "/creator-hub/settings" }),
              node("chip", "Sign in", { title: "opens the sign-in modal \u{2014} see below" }),
            ],
          },
          {
            chain: "start-building",
            items: [
              load("LOAD engine \u{B7} \u{201C}Loading scene editor\u{2026}\u{201D}", { label: "Start building" }),
              EDITOR(["start-building"]),
            ],
          },
          {
            chain: "home-cards",
            chips: true,
            note: "the home cards \u{2014} Deploy carries ?from=home so its breadcrumb returns here; the Manage card mirrors the rail doors",
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
      "Template pick \u{2192} confirm \u{2192} straight into the editor. The naming wizard at /creator-hub/create-project survives as the deep-link/legacy path. Reopening a saved or deployed scene hydrates its composite. Deleting writes a signed tombstone.",
    tracks: [
      {
        chain: "template-flow",
        items: [
          route("/create/templates"),
          click("card"),
          node("modal", "CONFIRM_MODAL"),
          load("LOAD engine \u{B7} \u{201C}Loading scene editor\u{2026}\u{201D}", { label: "Create" }),
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
        note: "deep-link/legacy path \u{2014} no in-product click leads here anymore",
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
            sub: "?pointer=\u{2026} \u{B7} reopen the live scene",
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
        note: "no auto-play; Stop \u{2260} reload",
        items: [
          state("EDITING"),
          load("\u{201C}Loading preview\u{2026}\u{201D}", { label: "\u{25B6} Play" }),
          state("PREVIEW"),
          { t: "edge", kind: "reversible", label: "Pause / Play" },
          click("\u{25A0} Stop"),
          state("EDITING"),
        ],
      },
      {
        chain: "sync",
        note: "background \u{2014} no click; the rail chip mirrors draft-sync state",
        items: [
          state("EDITING"),
          auto("draft saved"),
          load("PUT /api/creator-hub/drafts/:id \u{B7} signed auth chain"),
          outcome("sync chip: Synced", { sub: "308 shim covers the old /creator-hub/drafts path" }),
        ],
      },
      {
        chain: "exits",
        items: [
          state("EDITING"),
          click("Exit"),
          outcome("breadcrumb origin", {
            sub: "?from= home | scenes | templates | manage | operator \u{2014} default /create/scenes",
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
              state("NAMING", { sub: "\u{23CE} submits \u{B7} \u{201C}third-party?\u{201D} \u{2192} ?type=linked" }),
              step(),
              state("ITEMS", { sub: "dropzone: .zip \u{B7} .glb \u{B7} .gltf \u{B7} .png \u{2014} remove" }),
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
                sub: "tabs: items \u{21C4} activity (?tab)",
                title: "parameterized \u{2014} link opens the collections list",
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
                title: "parameterized \u{2014} link opens the collections list",
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
          route("/creator-hub/world-settings", { sub: "machine: world-settings \u{B7} tabs" }),
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
          route("/creator-hub/world-permissions", { sub: "machine: world-permissions \u{B7} tabs" }),
          click("invite / add collaborator / password \u{2265}8+2num"),
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
      "Occupancy for the worlds you deployed, sampled every ~5 minutes, plus the ledger of every endpoint the hub reads. Nothing here writes. A datum that did not arrive renders as \u{201C}\u{2014}\u{201D} beside the endpoint that failed to produce it \u{2014} never as a zero.",
    tracks: [
      {
        chain: "activity",
        items: [route("/creator-hub/activity", { sub: "your worlds \u{B7} headcount only" })],
        branches: [
          {
            chain: "activity",
            items: [
              auto("no address"),
              state("NO ADDRESS", {
                sub: "scoping, not a login \u{2014} [Connect wallet] or ?address=",
              }),
            ],
          },
          {
            chain: "activity",
            note: "per-row: live count \u{B7} a real 0 with a note \u{B7} \u{201C}no sample\u{201D} \u{2260} 0 \u{B7} NEVER DEPLOYED \u{B7} BLOCKED",
            items: [
              click("world row"),
              route("/creator-hub/activity/:world", {
                href: "/creator-hub/activity",
                sub: "right now \u{B7} history \u{B7} deployed \u{B7} access \u{B7} reception",
              }),
            ],
          },
          {
            chain: "activity-parcel",
            note: "a parcel cannot be listed, only looked up \u{2014} no endpoint maps a wallet to its parcels",
            items: [
              click("Look up x,y"),
              route("/creator-hub/activity", { sub: "?pointer=x,y \u{2014} inline history" }),
            ],
          },
        ],
      },
      {
        chain: "data-sources",
        items: [
          route("/creator-hub/data-sources", { sub: "the source ledger" }),
          load("probe live + sampled rows (4s timeout)"),
          outcome("\u{25CF} live \u{B7} \u{25D0} sampled \u{B7} \u{25D4} snapshot \u{B7} \u{2298} unavailable \u{B7} \u{25A8} not built \u{B7} \u{2715} excluded", {
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
      "Publish a scene to a World name via a signed deploy; claim a new name and come back with it preselected. The breadcrumb honors ?from= \u{2014} scene-editor entries get \u{201C}Back to editor\u{201D}.",
    tracks: [
      {
        chain: "deploy",
        items: [
          route("/creator-hub/deploy-world", { sub: "machine: deploy-scene \u{B7} ?from= breadcrumb" }),
          step(),
          state("PICK NAME", { sub: "live \u{B7} ?name= preselect" }),
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
      "Creator metrics are scoped to your wallet \u{2014} cards plus a per-scene visits table from the places API. The Network tab is network-wide data, none of it yours.",
    tracks: [
      {
        chain: "metrics",
        items: [route("/creator-hub/metrics")],
        branches: [
          { chain: "metrics", items: [auto("signed-out"), state("GATE")] },
          {
            chain: "metrics",
            note: "per-card \u{201C}Not available\u{201D} on failure \u{B7} EmptyState if none",
            items: [
              auto("creator"),
              outcome("cards", { sub: "real: collections \u{B7} on-sale \u{B7} sales \u{B7} visits" }),
              step(),
              outcome("per-scene visits table", { sub: "real rows \u{B7} per-row \u{201C}Not available\u{201D}" }),
            ],
          },
          {
            chain: "network",
            items: [
              click("Network tab"),
              route("/creator-hub/operator-metrics", { sub: "network-wide \u{2014} not your scenes" }),
              step(),
              outcome("live presence \u{B7} deploy funnel \u{B7} admin activity", {
                sub: "or \u{201C}Presence unavailable\u{201D}",
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
          route("/create/curate", { sub: "committee only \u{2014} rail item is membership-gated" }),
          step(),
          state("QUEUE", { sub: "filters ?status ?type ?assignee=me \u{B7} search \u{B7} sort" }),
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
      "Learn links out to videos and docs (plus this map), Settings holds preferences, the download landing ships the desktop app \u{2014} and the map maps itself.",
    tracks: [
      {
        chain: "learn",
        items: [route("/create/learn")],
        branches: [
          {
            chain: "learn",
            chips: true,
            note: "Videos \u{B7} Creator Docs \u{B7} More \u{2014} external tabs, plus one in-product link",
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
          outcome("back", { sub: "history back \u{B7} /create fallback" }),
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
          route("/creator-hub/map", { sub: "this page \u{2014} linked from Learn" }),
          sep(),
          route("/explorer-map", { sub: "sibling \u{2014} the Explorer client, mapped the same way" }),
        ],
      },
    ],
  },
];


export const STATS = computeStats(SECTIONS);


export const ASCII_SOURCE = `# Creator Hub \u{2014} click/state sitemap

Every edge is one user CLICK, or a LOAD\u{23F3} when the transition invokes work >100ms
(engine boot, folder picker, signed deploy, scaffolding). States in CAPS are
machine states; \`/paths\` are routes. Regenerated against the live route tree.

\`\`\`
/create (HOME)
\u{251C}\u{2500}\u{2500} rail: Home \u{B7} Scenes \u{B7} Templates \u{B7} Collections \u{2503} Curate(committee-gated) \u{B7}
\u{2502}         Worlds \u{B7} Land(/shop) \u{B7} Names(/marketplace/names) \u{B7} Metrics \u{2503} Learn
\u{2502}         (+ \u{2699} Settings \u{B7} Sign in \u{2014} every door stays in-product)
\u{251C}\u{2500}\u{2500} cards: Your published scenes \u{2192} /creator-hub/my-scenes \u{B7} New scene \u{2192} EDITOR \u{B7}
\u{2502}          Browse templates \u{B7} Deploy a scene(?from=home) \u{B7} Get the desktop app
\u{2502}          (+ a Manage card mirroring the rail doors)
\u{2502}
\u{251C}\u{2500}[Start building]\u{2500}\u{2500}\u{23F3} LOAD engine ("Loading scene editor\u{2026}")\u{2500}\u{2500}\u{25B6} EDITOR
\u{251C}\u{2500}[See All]\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{25B6} /create/scenes \u{B7} /create/learn
\u{251C}\u{2500}[Sign in]\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{25B6} SIGNIN_MODAL \u{2500}\u{2500}[OTP/social]\u{2500}\u{2500}\u{25B6} signed-in
\u{2502}
\u{251C}\u{2500}\u{2500} /create/templates
\u{2502}    \u{2514}\u{2500}[card]\u{2500}\u{2500}\u{25B6} CONFIRM_MODAL \u{2500}\u{2500}[Create]\u{2500}\u{2500}\u{23F3} LOAD engine \u{2500}\u{2500}\u{25B6} EDITOR
\u{2502}                     \u{2502}            (?new=1&template=X&from=templates)
\u{2502}                     \u{2514}\u{2500}[Cancel/Esc]\u{2500}\u{2500}\u{25B6} back
\u{251C}\u{2500}\u{2500} /creator-hub/create-project        (machine: create-project \u{2014} LEGACY,
\u{2502}    deep-link only; no in-product click leads here anymore)
\u{2502}    NAMING \u{2500}\u{2500}[Create]\u{2500}\u{2500}\u{25B6} SCAFFOLDING \u{2500}\u{2500}\u{23F3} write files \u{2500}\u{2500}\u{25B6} CREATED \u{2500}\u{2500}[Open in
\u{2502}    editor]\u{2500}\u{2500}\u{25B6} EDITOR   \u{2514}\u{2500} error \u{2500}\u{2500}\u{25B6} ERROR \u{2500}\u{2500}[Retry]/[Choose folder]\u{2500}\u{2500}\u{25B6} retry
\u{2502}
\u{251C}\u{2500}\u{2500} EDITOR  /creator-hub/scene-editor   (machine: scene-editor-place-items)
\u{2502}    \u{23F3} BOOT \u{25B6} EDITING \u{2500}[Open Assets]\u{25B6} BROWSING \u{2500}[place]\u{25B6} PLACING \u{2500}[Create entity]\u{25B6}
\u{2502}    TRANSFORMING \u{2500}[axes]\u{25B6} MODIFYING \u{2500}[Save]\u{23F3} FSA write \u{25B6} SAVED \u{2500}[continue]\u{25B6} EDITING
\u{2502}    EDITING \u{2500}[\u{25B6} Play]\u{23F3} "Loading preview\u{2026}" \u{25B6} PREVIEW \u{2500}[\u{23F8} Pause]\u{21C4}[\u{25B6}]\u{2500}[\u{25A0} Stop]\u{25B6} EDITING
\u{2502}    background: draft saved \u{2500}\u{2500}\u{23F3} PUT /api/creator-hub/drafts/:id (signed) \u{2500}\u{2500}\u{25B6}
\u{2502}                sync chip Synced   (308 shim at the old /creator-hub/drafts)
\u{2502}    \u{2500}[Exit]\u{25B6} breadcrumb origin (?from= home|scenes|templates|manage|operator;
\u{2502}             default /create/scenes) \u{B7} \u{2500}[Publish]\u{25B6} deploy flow
\u{2502}
\u{251C}\u{2500}\u{2500} /create/scenes \u{2500}\u{2500} [scene card]\u{2500}\u{2500}\u{23F3} hydrate composite \u{2500}\u{2500}\u{25B6} EDITOR (reopen+continue)
\u{2502}    \u{2514}\u{2500} empty: [Import]/[Templates]/[Sign in]
\u{251C}\u{2500}\u{2500} /creator-hub/my-scenes \u{2500}\u{2500} [Open]\u{2500}\u{2500}\u{25B6} EDITOR (?pointer=\u{2026} reopen live scene)
\u{2502}    \u{2514}\u{2500} empty: [Start from a template]\u{2500}\u{2500}\u{25B6} /create/templates
\u{2502}
\u{251C}\u{2500}\u{2500} /create/wearables (COLLECTIONS)
\u{2502}    \u{251C}\u{2500}[New collection]\u{25B6} /collections/new   (machine: wearable-create-collection)
\u{2502}    \u{2502}   NAMING(\u{23CE} submits; "third-party?"\u{2192}?type=linked) \u{2500}\u{25B6} ITEMS(upload dropzone,
\u{2502}    \u{2502}   .zip/.glb/.gltf/.png, remove) \u{2500}\u{25B6} REVIEW \u{2500}\u{25B6} SUBMITTING\u{23F3} \u{2500}\u{25B6} DONE \u{2500}\u{25B6} detail
\u{2502}    \u{251C}\u{2500}[collection]\u{25B6} /collections/:id  tabs items\u{21C4}activity (?tab)
\u{2502}    \u{2502}   \u{251C}\u{2500}[item row]\u{25B6} ITEM-EDITOR: SELECT\u{25B6}MODEL\u{25B6}CATEGORY\u{25B6}RARITY\u{25B6}PRICE\u{25B6}SAVE\u{23F3}\u{25B6} done
\u{2502}    \u{2502}   \u{2514}\u{2500}[Publish]\u{25B6} PUBLISH: SUMMARY\u{25B6}COST\u{25B6}TERMS\u{25B6}PAY\u{23F3}(disclosed stub)\u{25B6} SUBMITTED
\u{2502}    \u{2514}\u{2500}[item]\u{25B6} /items/:id \u{2500}[Edit]\u{25B6} item-editor?step=model
\u{2502}
\u{251C}\u{2500}\u{2500} WORLDS
\u{2502}    \u{251C}\u{2500} /creator-hub/manage \u{2500}[card]\u{25B6} settings/layout \u{B7} \u{2500}[Your Storage]\u{25B6} storage panel
\u{2502}    \u{251C}\u{2500} storage: SELECT world \u{25B6} QUOTA panel (DAO-proposal link) \u{B7} BUY MANA/LAND/NAME\u{2197}
\u{2502}    \u{251C}\u{2500} world-settings: tabs \u{2500}[Save]\u{23F3} invoke \u{25B6} saved \u{B7} [Discard] \u{B7} [Unpublish]\u{23F3} real
\u{2502}    \u{2514}\u{2500} world-permissions: tabs \u{2500}[invite]/[add collaborator]/[password \u{2265}8+2num]\u{2500}\u{25B6}
\u{2502}                          COMMIT\u{23F3} (signed ACL write)
\u{2502}
\u{251C}\u{2500}\u{2500} /creator-hub/deploy-world  (machine: deploy-scene; breadcrumb honors ?from=,
\u{2502}    from=scene-editor \u{25B6} "Back to editor")
\u{2502}    PICK NAME(live; ?name= preselect) \u{2500}[Deploy]\u{23F3} signed deploy \u{2500}\u{25B6} SUCCESS
\u{2502}    \u{2514}\u{2500}[Claim name]\u{25B6} /creator-hub/claim-name \u{2500}\u{25B6} [Use in Publish to World]\u{2500}\u{2500}back w/ ?name
\u{2502}
\u{251C}\u{2500}\u{2500} /creator-hub/metrics   signed-out\u{25B6}GATE \u{B7} creator\u{25B6}cards (real: collections/
\u{2502}    on-sale/sales/visits) + per-scene visits table (real rows from the places
\u{2502}    API; per-row "Not available" on gaps)
\u{2502}    \u{2514}\u{2500} Network tab \u{25B6} /creator-hub/operator-metrics \u{2014} network-wide presence,
\u{2502}       deploy funnel, admin activity (zero of-your-scenes data)
\u{2502}
\u{251C}\u{2500}\u{2500} /create/curate (committee; rail item shows per-membership): QUEUE(filters
\u{2502}    ?status ?type ?assignee=me, search, sort) \u{2500}[row]\u{25B6} REVIEW \u{2500}[approve/reject
\u{2502}    +comment]\u{25B6} DECIDED \u{2500}\u{25B6} queue
\u{2502}
\u{251C}\u{2500}\u{2500} /create/learn \u{2500}\u{2500} Videos\u{2197} \u{B7} Creator Docs\u{2197} \u{B7} Studios\u{2197} \u{B7} Creator Hub flow map
\u{2502}    (\u{25B6} /creator-hub/map \u{2014} this document)
\u{251C}\u{2500}\u{2500} /creator-hub/settings \u{2500}\u{2500} preferences \u{2500}[Close]\u{25B6} back (history \u{B7} /create)
\u{251C}\u{2500}\u{2500} /landings/creator-hub-download \u{2500}\u{2500} [Download]\u{25B6} per-OS installer
\u{251C}\u{2500}\u{2500} delete-project: CONFIRM \u{2500}[Delete]\u{23F3} signed tombstone \u{25B6} done (?local=deleted|kept)
\u{2514}\u{2500}\u{2500} /creator-hub/map \u{2014} this map \u{B7} sibling: /explorer-map (the Explorer client)
\`\`\`

Legend: \`[x]\` = click edge \u{B7} \`\u{23F3}\` = >100ms invoked load (spinner/status shown) \u{B7}
\`\u{21C4}\` = reversible pair \u{B7} \`\u{2197}\` = external tab. Redirect shims carry no clicks and
are compressed: /creator-hub \u{2192} /create \u{B7} /create/about \u{2192} /create \u{B7}
/creator-hub/deploy-alternative \u{2192} deploy-world \u{B7} /builder/* \u{2192} in-product
equivalents (/shop, /marketplace/names, \u{2026}) \u{B7} /creator-hub/drafts/* \u{2192}
/api/creator-hub/drafts/* (308). Orphans/gaps (metrics-funnel,
integration-create-entry) are intentionally absent \u{2014} not implemented.
`;
