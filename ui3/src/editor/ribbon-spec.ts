// The ribbon's information architecture, transcribed from the dclux editor UX
// observation study (48 coded videos, 1,741 action events, 17-code intent
// taxonomy, plus ~60 creator-hub issues).
//
// The ordering is not taste. Tabs are ranked by observed frequency, and the
// three things creators do most -- preview, undo, snap -- are deliberately NOT
// in any tab: they are pinned chrome, because the corpus's loudest complaints
// are about exactly those three being modal, hidden, or unreliable.
//
// Five tabs, never more and never fewer: a persona split (CODE) is not a
// surface split, so the code tools ride an opt-in GROUP inside Test & Code, and
// the selection surfaces as the last group of Home rather than as a tab that
// appears and disappears under the pointer.
//
// Capability gaps are expressed by the host omitting the handler, never by a
// `pending` string here. `requires` is only ever about state the creator can
// change: nothing selected, no preview running, no engine yet.
//
// Keep `why` on every tab. It is the evidence that justifies the tab existing at
// that rank, and without it the next person reorders this by taste.

export type RibbonTabId = "home" | "insert" | "interact" | "scene";

export type RibbonCommandKind = "big" | "toggle";

export type RibbonRequires = "engine" | "selection" | "playing" | "undoable" | "redoable";

export interface RibbonCommand {
  /** Stable id a host binds a handler to. No handler means the button is not rendered. */
  id: string;
  label: string;
  kind?: RibbonCommandKind;
  requires?: RibbonRequires;
  /** Study citation. Becomes the button title while the command is usable. */
  hint?: string;
  /** Keyboard binding shown as a trailing kbd, OUTSIDE the accessible name.
      shortcuts.ts is canonical; this is its display copy for the ribbon. */
  key?: string;
}

/** Group bodies that are a bespoke widget rather than a list of commands. */
export type RibbonSlot = "numeric" | "wiring" | "selection";

export interface RibbonGroup {
  name: string;
  cmds: RibbonCommand[];
  /** Revealed by the chrome preferences toggle, so scripters opt in once. */
  optIn?: boolean;
  slot?: RibbonSlot;
}

export interface RibbonTab {
  id: RibbonTabId;
  name: string;
  /** Evidence for this tab's rank. Kept in the source, NOT shown to creators. */
  why: string;
  /** What a creator reads on hover. Plain; no taxonomy, no event counts. */
  blurb: string;
  /** Shown when nothing in this tab is wired, instead of a wall of dead chips. */
  empty: string;
  groups: RibbonGroup[];
}

interface CommandOptions {
  kind?: RibbonCommandKind;
  requires?: RibbonRequires;
  hint?: string;
  key?: string;
}

const c = (id: string, label: string, opts: CommandOptions = {}): RibbonCommand => ({
  id,
  label,
  ...opts,
});

export const RIBBON_TABS: RibbonTab[] = [
  {
    id: "home",
    name: "Home",
    why: "The arrange loop: XFORM 163 + PLACE-manipulation + HIER 31 events, the widest everyday use after preview. Everything here operates on the current selection and never switches your gizmo mode for you.",
    blurb: "Move, copy and arrange what is in your scene.",
    empty: "Connect the engine to arrange the scene.",
    groups: [
      {
        name: "Edit",
        cmds: [
          c("duplicate", "Duplicate", {
            kind: "big",
            requires: "selection",
            hint: "Copies this item's components. The copy lands in the same spot -- move it to see it",
          }),
          c("delete", "Delete", { requires: "selection" }),
        ],
      },
      {
        name: "Transform",
        cmds: [
          c("tool.translate", "Move", { kind: "big", key: "W", requires: "engine" }),
          c("tool.rotate", "Rotate", { key: "E", requires: "engine" }),
          c("tool.scale", "Scale", { key: "R", requires: "engine" }),
          c("tool.select", "Select", { key: "Q", requires: "engine" }),
        ],
      },
      { name: "Numeric", slot: "numeric", cmds: [] },
      {
        name: "Snap",
        cmds: [
          c("snap", "Snap", {
            kind: "toggle",
            hint: "Snaps gizmo moves and rotations to the grid",
          }),
          c("snap.step", "Snap step", { hint: "Grid size for moving -- click to cycle" }),
          c("snap.angle", "Snap angle", { hint: "Grid size for rotating -- click to cycle" }),
          c("align.world", "Align to world", { kind: "toggle", requires: "engine" }),
        ],
      },
      {
        name: "Selection",
        slot: "selection",
        cmds: [
          c("item.focus", "Focus camera", { kind: "big", requires: "selection" }),
          c("item.inspector", "Inspector", { requires: "selection" }),
          c("item.addComponent", "Add component", { requires: "selection" }),
        ],
      },
    ],
  },
  {
    id: "insert",
    name: "Insert",
    why: "Getting things in: ASSET 104 + PLACE-entry 170 events, 274 together, and ASSET->PLACE is the strongest cross-intent chain (37).",
    blurb: "Add models and smart items to your scene.",
    empty: "Connect the engine to add items to the scene.",
    groups: [
      {
        name: "Catalog",
        cmds: [
          c("assets.open", "Browse catalog", { kind: "big" }),
          c("assets.search", "Search catalog", {
            hint: "Search the model catalog",
          }),
        ],
      },
      {
        name: "Smart items",
        cmds: [
          c("smart.doors", "Doors"),
          c("smart.buttons", "Buttons"),
          c("smart.platforms", "Platforms"),
          c("smart.seats", "Seats"),
          c("smart.all", "All smart items"),
        ],
      },
      { name: "Basics", cmds: [c("entity.new", "Empty entity", { kind: "big" })] },
      {
        name: "Import",
        cmds: [
          c("import", "Import files", { kind: "big", hint: "Opens your project models, where you can add .glb or .gltf files" }),
        ],
      },
    ],
  },
  {
    id: "interact",
    name: "Interact",
    why: "The wire loop: SMART 196 events, and SMART<->TEST is the heaviest editor couplet (103 adjacent transitions). Quick-wire prefills the default trigger->action pair, because creators place a smart item and wonder why nothing happens. Merged with the verify loop (user call 2026-08-30): TEST 275 events across 44 of 48 videos sits in the same wire-try cycle (SMART<->TEST 103 adjacent transitions), so trying the scene lives beside wiring it; the code tools stay an opt-in group.",
    blurb: "Make items react, then run the scene to try them.",
    empty: "Connect the engine to wire up smart items.",
    groups: [
      {
        name: "Wire",
        cmds: [
          c("wire.quick", "Make interactive", {
            kind: "big",
            requires: "selection",
            hint: "Opens the when/do form for this item; press Add interaction to apply",
          }),
        ],
      },
      { name: "Selected wiring", slot: "wiring", cmds: [] },
      {
        name: "Triggers",
        cmds: [
          c("trigger.click", "When clicked", { requires: "selection" }),
          c("trigger.input", "When E is pressed", { requires: "selection" }),
        ],
      },
      {
        name: "Actions",
        cmds: [
          c("action.tween", "Move the item", { requires: "selection" }),
          c("action.visibility", "Show / hide", { requires: "selection" }),
          c("action.sound", "Play a sound", { requires: "selection" }),
          c("action.animate", "Play an animation", { requires: "selection" }),
        ],
      },
      {
        name: "Preview",
        cmds: [
          c("pause", "Pause", { kind: "big", requires: "playing" }),
          c("step", "Step one tick", { requires: "playing" }),
        ],
      },
      { name: "Debug", cmds: [c("debug", "Step ticks", {
            kind: "toggle",
            requires: "playing",
            hint: "Pause the preview and step one tick at a time to see what changes",
          })] },
      { name: "Code", optIn: true, cmds: [c("code", "Open code editor", { kind: "big" })] },
      {
        name: "Reference",
        optIn: true,
        cmds: [c("ref.docs", "SDK docs"), c("ref.playground", "Playground")],
      },
    ],
  },
  {
    id: "scene",
    name: "Scene & Publish",
    why: "The ship rail, walked once in order: PROJ 115 + PUB 53 + LAND 21 + ADMIN 19 events. Rare but high-stakes, and every complaint here is about opacity, so deploy honesty matters more than deploy buttons.",
    blurb: "Set up your scene and put it online.",
    empty: "Saving and publishing run from the app bar in this build.",
    groups: [
      { name: "Project", cmds: [c("save", "Save to disk", { kind: "big" })] },
      {
        name: "Deploy",
        cmds: [c("publish", "Publish", { kind: "big" })],
      },
    ],
  },
];


export const DEFAULT_TAB: RibbonTabId = "home";

// Pinned chrome, reachable from every tab and never repeated inside one: the
// study's loudest complaints (undo distrust, modal preview) are about exactly
// these disappearing behind a tab. DeRibbon owns their presentation; this list
// is the invariant the disjointness test enforces.
export const RIBBON_CHROME_IDS: readonly string[] = ["undo", "redo", "play", "stop"];

export interface RibbonDeferred {
  id: string;
  label: string;
  tab: RibbonTabId;
  /** Why this study finding is not a button. One line, and it must stay true. */
  why: string;
}

// The study's full inventory. Keeping it as data rather than as greyed buttons
// is what lets the shipped deck be all-live: an entry graduates by gaining a
// host handler and a row in RIBBON_TABS, never by having its reason deleted.
export const RIBBON_DEFERRED: RibbonDeferred[] = [
  {
    id: "snapshots",
    label: "Snapshots",
    tab: "home",
    why: "No named checkpoint store; Undo and Redo already cover recovery, and a greyed Snapshots teaches nothing.",
  },
  {
    id: "copy",
    label: "Copy",
    tab: "home",
    why: "Buildable on the cached component values, but Duplicate already does the job; a clipboard is new scope, not reconciliation.",
  },
  {
    id: "paste",
    label: "Paste",
    tab: "home",
    why: "Same as Copy: there is no clipboard to paste from yet.",
  },
  {
    id: "group",
    label: "Group",
    tab: "home",
    why: "Blocked on multi-select: the hierarchy only ever sends a single active id, so the UI cannot produce a multi-selection to group.",
  },
  {
    id: "parent",
    label: "Parent",
    tab: "home",
    why: "Transform.parent is writable today, but there is no drag-reparent UI and a button with no target is meaningless.",
  },
  {
    id: "align",
    label: "Align",
    tab: "home",
    why: "Same multi-select blocker; aligning one entity to itself is a no-op.",
  },
  {
    id: "drop",
    label: "Drop to ground",
    tab: "home",
    why: "No surface probe on the bus, and a naive y = 0 is a lie in any scene with terrain.",
  },
  {
    id: "lock",
    label: "Lock",
    tab: "home",
    why: "inspector::Lock round-trips through the CRDT but nothing in the engine reads it, so the flag would badge while the entity stayed draggable.",
  },
  {
    id: "hide",
    label: "Hide",
    tab: "home",
    why: "inspector::Hide is registered and unread, exactly as Lock is.",
  },
  {
    id: "assets.theme",
    label: "By theme",
    tab: "insert",
    why: "Catalog items carry a category and a pack, and no theme field to group by.",
  },
  {
    id: "primitive",
    label: "Cube",
    tab: "insert",
    why: "Adding a primitive means authoring a MeshRenderer payload we have not verified against the engine registry, and a button that adds a broken component is worse than no button.",
  },
  {
    id: "text",
    label: "Text",
    tab: "insert",
    why: "Same unverified payload risk as the other primitives.",
  },
  {
    id: "light",
    label: "Light",
    tab: "insert",
    why: "Same unverified payload risk as the other primitives.",
  },
  {
    id: "particles",
    label: "Particles",
    tab: "insert",
    why: "Same unverified payload risk as the other primitives.",
  },
  {
    id: "media.image",
    label: "Image",
    tab: "insert",
    why: "Unverified payload plus a URL, which means a dialog this pass is not building.",
  },
  {
    id: "media.video",
    label: "Video screen",
    tab: "insert",
    why: "Unverified payload plus a URL, which means a dialog this pass is not building.",
  },
  {
    id: "media.audio",
    label: "Audio",
    tab: "insert",
    why: "Unverified payload plus a URL, which means a dialog this pass is not building.",
  },
  {
    id: "media.nft",
    label: "NFT frame",
    tab: "insert",
    why: "Unverified payload plus a URL, which means a dialog this pass is not building.",
  },
  {
    id: "import.batch",
    label: "Batch import",
    tab: "insert",
    why: "Identical capability to Import files: a second button for one code path.",
  },
  {
    id: "ground",
    label: "Set ground",
    tab: "insert",
    why: "inspector::Ground is registered and unread, same as Lock and Hide.",
  },
  {
    id: "wire.trigger",
    label: "Add trigger",
    tab: "interact",
    why: "The interactions composer writes a trigger and an action together, so a separate Add trigger button would open the same form Quick-wire already opens.",
  },
  {
    id: "wire.action",
    label: "Add action",
    tab: "interact",
    why: "Same single composer as Add trigger: one form, so one button.",
  },
  {
    id: "trigger.all",
    label: "All 19 triggers",
    tab: "interact",
    why: "The vocabulary exists in the component registry; the picker that would browse it does not.",
  },
  {
    id: "action.all",
    label: "All 63 actions",
    tab: "interact",
    why: "The vocabulary exists in the component registry; the picker that would browse it does not.",
  },
  {
    id: "logic.states",
    label: "States",
    tab: "interact",
    why: "Component registered, no editor UI, and no honest default payload.",
  },
  {
    id: "logic.counter",
    label: "Counter",
    tab: "interact",
    why: "Component registered, no editor UI, and no honest default payload.",
  },
  {
    id: "logic.conditions",
    label: "Conditions",
    tab: "interact",
    why: "Component registered, no editor UI, and no honest default payload.",
  },
  {
    id: "sync",
    label: "Sync item",
    tab: "interact",
    why: "The SyncComponents payload shape is unverified, and a half-written multiplayer component is a silent scene break.",
  },
  {
    id: "preview.desktop",
    label: "Desktop client",
    tab: "interact",
    why: "The app bar already owns the Preview split button; duplicating it half-threaded is worse than one good copy.",
  },
  {
    id: "preview.web",
    label: "Web",
    tab: "interact",
    why: "The app bar already owns the Preview split button.",
  },
  {
    id: "preview.qr",
    label: "Mobile QR",
    tab: "interact",
    why:
      "The app bar's Preview split button owns the QR modal, which now encodes the real preview URL.",
  },
  {
    id: "errors",
    label: "Runtime errors",
    tab: "interact",
    why: "It would be a filtered view of the scene log the Console already shows unfiltered but complete.",
  },
  {
    id: "metrics",
    label: "Metrics panel",
    tab: "interact",
    why: "The panel is an empty render in this build.",
  },
  {
    id: "optimize",
    label: "Optimize assets",
    tab: "interact",
    why: "No such capability in the engine, the bus, or any console command.",
  },
  {
    id: "clean",
    label: "Clean unused",
    tab: "interact",
    why: "No such capability in the engine, the bus, or any console command.",
  },
  {
    id: "hot",
    label: "Hot reload",
    tab: "interact",
    why: "No hot-reload concept anywhere in the bus protocol.",
  },
  {
    id: "script.attach",
    label: "Attach script",
    tab: "interact",
    why: "No script CRUD in this build.",
  },
  {
    id: "script.new",
    label: "New script",
    tab: "interact",
    why: "No script CRUD in this build.",
  },
  {
    id: "agent",
    label: "Scene agent",
    tab: "interact",
    why: "The MCP bridge exists but has no ribbon-level entry point to hang an agent off.",
  },
  {
    id: "mcp",
    label: "MCP server",
    tab: "interact",
    why: "The relay opts in from the URL or local storage; a ribbon toggle would report a state it does not own.",
  },
  {
    id: "scene.name",
    label: "Name and description",
    tab: "scene",
    why: "Scene metadata lives in scene.json, which is not reachable over the editor bus at all.",
  },
  {
    id: "scene.thumb",
    label: "Thumbnail",
    tab: "scene",
    why: "Scene metadata lives in scene.json, which is not reachable over the editor bus at all.",
  },
  {
    id: "scene.category",
    label: "Category",
    tab: "scene",
    why: "Scene metadata lives in scene.json, which is not reachable over the editor bus at all.",
  },
  {
    id: "scene.parcels",
    label: "Parcels",
    tab: "scene",
    why: "Scene layout lives in scene.json, which is not reachable over the editor bus at all.",
  },
  {
    id: "scene.spawn",
    label: "Spawn points",
    tab: "scene",
    why: "Scene layout lives in scene.json, which is not reachable over the editor bus at all.",
  },
  {
    id: "scene.skybox",
    label: "Skybox time",
    tab: "scene",
    why: "Environment settings are not on the editor bus.",
  },
  {
    id: "scene.terrain",
    label: "Terrain",
    tab: "scene",
    why: "Environment settings are not on the editor bus.",
  },
  {
    id: "scene.voice",
    label: "Voice chat",
    tab: "scene",
    why: "Scene restrictions live in scene.json, which is not reachable over the editor bus at all.",
  },
  {
    id: "scene.wearables",
    label: "Wearable restrictions",
    tab: "scene",
    why: "Scene restrictions live in scene.json, which is not reachable over the editor bus at all.",
  },
  {
    id: "deploy.status",
    label: "Deploy status",
    tab: "scene",
    why: "No deploy telemetry surface exists in the editor tree; the wizard owns publishing and does not report progress back.",
  },
  {
    id: "deploy.history",
    label: "History",
    tab: "scene",
    why: "No deploy telemetry surface exists in the editor tree.",
  },
  {
    id: "jumpin",
    label: "Jump In",
    tab: "scene",
    why: "Needs a published target and a deploy record the editor never receives.",
  },
  {
    id: "world.permissions",
    label: "Permissions",
    tab: "scene",
    why: "A separate product behind a separate API.",
  },
  {
    id: "world.storage",
    label: "Storage",
    tab: "scene",
    why: "A separate product behind a separate API.",
  },
  {
    id: "world.names",
    label: "NAMEs",
    tab: "scene",
    why: "A separate product behind a separate API.",
  },
  {
    id: "item.material",
    label: "Material",
    tab: "home",
    why: "The inspector's material editor renders representative defaults, not scene values, so a ribbon button would point at a decorative surface.",
  },
  {
    id: "item.colliders",
    label: "Colliders",
    tab: "home",
    why: "Same decorative inspector body: nothing there is read from the scene.",
  },
  {
    id: "item.animations",
    label: "Animations",
    tab: "home",
    why: "Same decorative inspector body: nothing there is read from the scene.",
  },
  {
    id: "item.billboard",
    label: "Billboard",
    tab: "home",
    why: "Same decorative inspector body: nothing there is read from the scene.",
  },
  {
    id: "item.saveCustom",
    label: "Save as custom item",
    tab: "home",
    why: "No custom-item store, and no export path for one entity's subtree.",
  },
  {
    id: "search",
    label: "Search",
    tab: "home",
    why: "The study's search spans assets, entities and commands; this build has two unrelated search boxes and no palette, so one button under a universal label would overpromise.",
  },
];
