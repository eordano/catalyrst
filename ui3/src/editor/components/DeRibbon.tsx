import { useRef, useState, type ReactElement, type ReactNode } from "react";
import {
  DEFAULT_TAB,
  RIBBON_TABS,
  type RibbonCommand,
  type RibbonGroup,
  type RibbonRequires,
  type RibbonTab,
  type RibbonTabId,
} from "../ribbon-spec";
import RibbonNumeric, { type RibbonNumericProps } from "./RibbonNumeric";
import RibbonWiring, { type RibbonWiringState } from "./RibbonWiring";
import "./deribbon.css";
import { saveChip } from "./DeToolbar";

export interface RibbonMeter {
  label: string;
  value: number;
  limit: number;
}

export interface DeRibbonProps {
  /** id -> handler. A command with no entry is not rendered at all. */
  commands?: Record<string, (() => void) | undefined>;
  /** id -> pressed, for `toggle` commands and the active gizmo tool. */
  pressed?: Record<string, boolean>;
  /** id -> live label, for the commands whose text is their current value. */
  labels?: Record<string, string>;
  hasSelection?: boolean;
  selectionLabel?: string;
  /** False while the engine has not handshaken, which is what `requires: engine` reads. */
  busLive?: boolean;
  showDeveloper?: boolean;
  onToggleDeveloper?: (next: boolean) => void;
  saveLabel?: string;
  saveClass?: string;
  playing?: boolean;
  canUndo?: boolean;
  canRedo?: boolean;
  /** Status-bar meters. Empty renders the honest "not measured here" note. */
  meters?: RibbonMeter[];
  snapLabel?: string;
  /** Absent means the editor cannot set an absolute transform, so the group is omitted. */
  numeric?: RibbonNumericProps;
  /** Shown in the numeric slot while nothing is selected: where the camera is. */
  cameraPose?: { x: number; y: number; z: number; yaw: number; pitch: number };
  wiring?: RibbonWiringState;
  onOpenWiring?: () => void;
  tab?: RibbonTabId;
  onTab?: (tab: RibbonTabId) => void;
}

const BLOCKED_REASON: Record<RibbonRequires, string> = {
  selection: "Select something in the scene first",
  playing: "Only while a preview is running",
  engine: "Waiting for the engine to connect",
  undoable: "Nothing to undo",
  redoable: "Nothing to redo",
};

function useControlled(
  value: RibbonTabId | undefined,
  onChange: ((t: RibbonTabId) => void) | undefined,
): [RibbonTabId, (t: RibbonTabId) => void] {
  const [own, setOwn] = useState<RibbonTabId>(DEFAULT_TAB);
  const active = value ?? own;
  return [
    active,
    (t: RibbonTabId) => {
      if (value === undefined) setOwn(t);
      onChange?.(t);
    },
  ];
}

const fmtPose = (n: number): string => String(Math.round(n * 10) / 10);

function CameraReadout({ pose }: { pose: { x: number; y: number; z: number; yaw: number; pitch: number } }) {
  const cells: Array<[string, string]> = [
    ["X", fmtPose(pose.x)],
    ["Y", fmtPose(pose.y)],
    ["Z", fmtPose(pose.z)],
    ["YAW", `${fmtPose(pose.yaw)}\u{00B0}`],
    ["PITCH", `${fmtPose(pose.pitch)}\u{00B0}`],
  ];
  return (
    <div className="rb-numeric" role="status" aria-label="Camera position and orientation">
      {cells.map(([ax, v]) => (
        <span className="rb-num rb-num-readonly" key={ax}>
          <span className="rb-num-ax" aria-hidden="true">{ax}</span>
          <span className="rb-num-value">{v}</span>
        </span>
      ))}
    </div>
  );
}

export default function DeRibbon({
  commands = {},
  pressed = {},
  labels = {},
  hasSelection = false,
  selectionLabel = "Selection",
  busLive = false,
  showDeveloper = false,
  onToggleDeveloper,
  saveLabel = "Saved",
  saveClass = "ok",
  playing = false,
  canUndo = false,
  canRedo = false,
  meters = [],
  snapLabel,
  numeric,
  cameraPose,
  wiring,
  onOpenWiring,
  tab,
  onTab,
}: DeRibbonProps) {
  const [active, setActive] = useControlled(tab, onTab);
  const tablistRef = useRef<HTMLDivElement | null>(null);
  const chip = saveChip(playing, saveLabel, saveClass);

  // The strip is the same five buttons in every state. Nothing appears, nothing
  // disappears, so a tab never moves out from under the pointer.
  const tabs: readonly RibbonTab[] = RIBBON_TABS;

  const current = tabs.find((t) => t.id === active) ?? tabs[0];

  const onTabKey = (e: React.KeyboardEvent): void => {
    const i = tabs.findIndex((t) => t.id === active);
    if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      const edge = e.key === "Home" ? 0 : tabs.length - 1;
      const t = tabs[edge];
      if (t) {
        setActive(t.id);
        tablistRef.current?.querySelectorAll<HTMLButtonElement>("[role=tab]")[edge]?.focus();
      }
      return;
    }
    if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
    e.preventDefault();
    const next = e.key === "ArrowRight" ? (i + 1) % tabs.length : (i - 1 + tabs.length) % tabs.length;
    const target = tabs[next];
    if (target === undefined) return;
    setActive(target.id);
    const el = tablistRef.current?.querySelectorAll<HTMLButtonElement>("[role=tab]")[next];
    el?.focus();
  };

  const ready: Record<RibbonRequires, boolean> = {
    selection: hasSelection,
    playing,
    engine: busLive,
    undoable: canUndo,
    redoable: canRedo,
  };
  const blockedBy = (cmd: RibbonCommand): RibbonRequires | null =>
    cmd.requires !== undefined && !ready[cmd.requires] ? cmd.requires : null;

  const renderCmd = (cmd: RibbonCommand): ReactElement | null => {
    const run = commands[cmd.id];
    if (run === undefined) return null;
    const blocked = blockedBy(cmd);
    const isToggle = cmd.kind === "toggle";
    const on = pressed[cmd.id] === true;
    const text = labels[cmd.id] ?? cmd.label;
    return (
      <button
        key={cmd.id}
        type="button"
        className={
          "rb-cmd" + (cmd.kind === "big" ? " big" : "") + (isToggle ? " rb-toggle" : "") + (on ? " on" : "")
        }
        title={blocked ? BLOCKED_REASON[blocked] : (cmd.hint ?? text)}
        aria-label={text}
        aria-pressed={
          (isToggle || cmd.id.startsWith("tool.")) && cmd.id in pressed ? on : undefined
        }
        disabled={blocked !== null}
        onClick={run}
      >
        {text}
        {cmd.key !== undefined && <kbd className="rb-kbd" aria-hidden="true">{cmd.key}</kbd>}
      </button>
    );
  };

  const renderGroup = (g: RibbonGroup): ReactElement | null => {
    if (g.optIn === true && !showDeveloper) return null;

    if (g.slot === "numeric" || g.slot === "wiring") {
      // Both widget slots share the group chrome; only the capability guard,
      // the widget and the no-selection fallback differ. The numeric slot never
      // sits empty: with nothing selected it reads out the camera instead.
      const slot =
        g.slot === "numeric"
          ? numeric === undefined
            ? null
            : {
                widget: hasSelection ? (
                  <RibbonNumeric {...numeric} />
                ) : cameraPose !== undefined ? (
                  <CameraReadout pose={cameraPose} />
                ) : null,
                hint: "Select an item to type exact values.",
                label: hasSelection || cameraPose === undefined ? g.name : "Camera",
              }
          : wiring === undefined
            ? null
            : {
                widget: hasSelection ? <RibbonWiring state={wiring} onOpen={onOpenWiring} /> : null,
                hint: "Select a smart item to see its trigger and action.",
                label: g.name,
              };
      if (slot === null) return null;
      return (
        <div className="rb-group" key={g.name}>
          <div className="rb-slot">{slot.widget ?? <span className="rb-hint">{slot.hint}</span>}</div>
          <div className="rb-grouplabel">{slot.label}</div>
        </div>
      );
    }

    // The selection group is the contextual surface, so it is the only group
    // whose presence changes -- and it is last, so nothing before it shifts.
    if (g.slot === "selection" && !hasSelection) return null;
    const cmds = g.cmds.map(renderCmd).filter((el): el is ReactElement => el !== null);
    if (cmds.length === 0) return null;
    return (
      <div className="rb-group" key={g.name}>
        <div className="rb-cmds">{cmds}</div>
        <div className="rb-grouplabel">{g.slot === "selection" ? selectionLabel : g.name}</div>
      </div>
    );
  };

  const chromeButton = (
    id: string,
    label: string,
    text: ReactNode,
    className: string,
    idleTitle: string,
    disabledTitle: string,
    ready = true,
  ): ReactElement => {
    const run = commands[id];
    const off = run === undefined || !ready;
    return (
      <button
        type="button"
        className={className}
        title={off ? (busLive ? disabledTitle : BLOCKED_REASON.engine) : idleTitle}
        aria-label={label}
        disabled={off}
        onClick={run}
      >
        {text}
      </button>
    );
  };

  const deck = current ? current.groups.map(renderGroup).filter((el) => el !== null) : [];

  return (
    <div className="rb" role="region" aria-label="Editor ribbon">
      <div className="rb-chrome">
        {/* T0: never behind a tab. Undo/redo, save state, preview and publish
            stay reachable from every tab, because the study's three loudest
            complaints are about exactly these being modal or hidden. */}
        <div className="rb-qat" role="group" aria-label="Quick access">
          {chromeButton("undo", "Undo", "\u{21B6}", "rb-icon", "Undo", "Nothing to undo", canUndo)}
          {chromeButton("redo", "Redo", "\u{21B7}", "rb-icon", "Redo", "Nothing to redo", canRedo)}
          
        </div>

        <div
          className="rb-tabs"
          role="tablist"
          aria-label="Ribbon tabs"
          ref={tablistRef}
          onKeyDown={onTabKey}
        >
          {tabs.map((t) => (
            <button
              key={t.id}
              type="button"
              role="tab"
              id={`rb-tab-${t.id}`}
              aria-selected={t.id === active}
              aria-controls={t.id === active ? `rb-deck-${t.id}` : undefined}
              tabIndex={t.id === active ? 0 : -1}
              className={"rb-tab" + (t.id === active ? " active" : "")}
              title={t.blurb}
              onClick={() => setActive(t.id)}
            >
              {t.name}
            </button>
          ))}
        </div>

        <div className="rb-always" role="group" aria-label="Always available">
          {playing
            ? chromeButton(
                "stop",
                "Stop preview",
                "Stop",
                "rb-btn primary",
                "Stop the preview",
                "Preview is not running",
              )
            : chromeButton(
                "play",
                "Play",
                "Play",
                "rb-btn primary",
                "Run the scene in the editor",
                "Play is not available here",
                busLive,
              )}
          <button
            type="button"
            className={"rb-icon" + (showDeveloper ? " on" : "")}
            title={
              showDeveloper ? "Hide the code tools in Test & Code" : "Show the code tools in Test & Code"
            }
            aria-label="Code tools"
            aria-pressed={showDeveloper}
            onClick={() => onToggleDeveloper?.(!showDeveloper)}
          >
            {"\u{22EF}"}
          </button>
        </div>
      </div>

      {current ? (
        <div
          className="rb-deck"
          id={`rb-deck-${current.id}`}
          role="tabpanel"
          aria-labelledby={`rb-tab-${current.id}`}
        >
          {deck.length > 0 ? deck : <span className="rb-hint rb-empty">{current.empty}</span>}
        </div>
      ) : null}

      <div className="rb-status" role="status">
        {meters.length > 0 ? (
          meters.map((m) => (
            <span
              key={m.label}
              className={"rb-meter" + (m.value > m.limit ? " over" : "")}
              title={`${m.label}: ${m.value} of ${m.limit} \u{2014} as the engine counts them for publish limits; includes internals the entity list does not show`}
            >
              {m.label} {m.value}/{m.limit}
            </span>
          ))
        ) : null}
        <span
          className="rb-meter"
          title="Snap state is always visible: no action changes it as a side effect"
        >
          {snapLabel ?? "snap n/a"}
        </span>
        <span className={"rb-save " + chip.cls}>{chip.label}</span>
      </div>
    </div>
  );
}
