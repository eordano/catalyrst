import { useRef, useState } from "react";
import type { CSSProperties } from "react";
import type { DebugSystemsView, EntityDiff } from "../debugger";

export interface DeDebugPanelProps {
  tick: number | null;
  stepping?: boolean;
  error?: string | null;
  entries: EntityDiff[] | null;
  lastStepCount?: number | null;
  unchangedEntities?: number;
  totalEntities?: number;
  timedOut?: boolean;
  systems?: DebugSystemsView | null;
  names?: (id: string) => string;
  onStep?: (count: number) => void;
  onClose?: () => void;
  onSelect?: (id: string) => void;
  height?: number;
  onHeightChange?: (h: number) => void;
  insetLeft?: number;
  insetRight?: number;
}

export const DEBUG_ROW_CAP = 40;
export const DEBUG_MIN_HEIGHT = 160;
export const DEBUG_MAX_HEIGHT = 560;

const KIND_BADGE: Record<EntityDiff["kind"], { label: string; cls: string } | null> = {
  new: { label: "new", cls: "new" },
  gone: { label: "gone", cls: "gone" },
  changed: null,
};

const RESERVED_LABELS: Record<string, string> = {
  "0": "Scene Root",
  "1": "Player",
  "2": "Camera",
};

function EntityRow({
  d,
  name,
  onSelect,
}: {
  d: EntityDiff;
  name: string;
  onSelect?: (id: string) => void;
}) {
  const badge = KIND_BADGE[d.kind];
  const selectable = typeof onSelect === "function" && d.kind !== "gone";
  return (
    <div className={"eui-dbg-entity" + (d.kind !== "changed" ? " is-" + d.kind : "")}>
      <div
        className={"eui-dbg-entity-head" + (selectable ? " selectable" : "")}
        role={selectable ? "button" : undefined}
        tabIndex={selectable ? 0 : undefined}
        title={selectable ? "Select this entity in the editor" : undefined}
        onClick={selectable ? () => onSelect?.(d.id) : undefined}
        onKeyDown={
          selectable
            ? (e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  onSelect?.(d.id);
                }
              }
            : undefined
        }
      >
        <span className="eui-dbg-entity-name">{name}</span>
        <span className="eui-dbg-entity-id">#{d.id}</span>
        {badge && <span className={"eui-dbg-badge " + badge.cls}>{badge.label}</span>}
        {d.unchanged > 0 && (
          <span className="eui-dbg-unchanged" title="Components present but unchanged this step">
            {d.unchanged} unchanged
          </span>
        )}
      </div>
      <div className="eui-dbg-comps">
        {d.comps.map((c) => (
          <div key={c.name} className={"eui-dbg-comp is-" + c.kind}>
            <span className="eui-dbg-comp-name">
              {c.name}
              {c.kind !== "changed" && <em> {c.kind}</em>}
            </span>
            {c.kind === "changed" ? (
              c.changes.map((ch) => (
                <div key={ch.path || "(value)"} className="eui-dbg-change">
                  <span className="path">{ch.path || "value"}</span>
                  <span className="old">{ch.before}</span>
                  <span className="arrow">→</span>
                  <span className="new">{ch.after}</span>
                </div>
              ))
            ) : c.value !== undefined ? (
              <div className="eui-dbg-change">
                <span className="new">{c.value}</span>
              </div>
            ) : null}
          </div>
        ))}
      </div>
    </div>
  );
}

export default function DeDebugPanel({
  tick,
  stepping = false,
  error = null,
  entries,
  lastStepCount = null,
  unchangedEntities = 0,
  totalEntities = 0,
  timedOut = false,
  systems = null,
  names = (id) => RESERVED_LABELS[id] ?? `Entity ${id}`,
  onStep,
  onClose,
  onSelect,
  height = 280,
  onHeightChange,
  insetLeft = 288,
  insetRight = 344,
}: DeDebugPanelProps) {
  const [showAll, setShowAll] = useState(false);
  const dragRef = useRef<{ startY: number; startH: number } | null>(null);

  const startResize = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!onHeightChange) return;
    dragRef.current = { startY: e.clientY, startH: height };
    const el = e.currentTarget;
    el.setPointerCapture?.(e.pointerId);
    e.preventDefault();
  };
  const moveResize = (e: React.PointerEvent<HTMLDivElement>) => {
    const d = dragRef.current;
    if (!d || !onHeightChange) return;
    const next = Math.min(
      DEBUG_MAX_HEIGHT,
      Math.max(DEBUG_MIN_HEIGHT, d.startH + (d.startY - e.clientY)),
    );
    onHeightChange(next);
  };
  const endResize = () => {
    dragRef.current = null;
  };

  const shown = entries === null || showAll ? entries : entries.slice(0, DEBUG_ROW_CAP);
  const hidden = entries !== null && shown !== null ? entries.length - shown.length : 0;
  const style: CSSProperties = { height, left: insetLeft, right: insetRight };

  return (
    <div className="eui-panel eui-debug" style={style} role="region" aria-label="Step debugger">
      <div
        className="eui-dbg-grip"
        title="Drag to resize"
        onPointerDown={startResize}
        onPointerMove={moveResize}
        onPointerUp={endResize}
        onPointerCancel={endResize}
      />
      <div className="eui-panel-head" style={{ height: 44 }}>
        <div className="eui-head-text">
          <span className="eui-overline">Debug — scene frozen</span>
          <span className="eui-title">
            {tick !== null ? `Engine tick ${tick}` : "Engine tick —"}
            {stepping ? " · stepping…" : ""}
          </span>
        </div>
        <div className="eui-tool-group" role="group" aria-label="Step controls">
          {[1, 10, 60].map((n) => (
            <button
              key={n}
              type="button"
              className="eui-btn"
              disabled={!onStep || stepping}
              title={n === 1 ? "Advance 1 tick (.)" : `Advance ${n} ticks`}
              onClick={() => onStep?.(n)}
            >
              +{n}
            </button>
          ))}
        </div>
        <button
          type="button"
          className="eui-btn icon"
          title="Close debug panel (scene stays paused)"
          aria-label="Close debug panel"
          onClick={onClose}
        >
          ✕
        </button>
      </div>
      <div className="eui-panel-body eui-dbg-body">
        <section className="eui-dbg-entities" aria-label="Entity changes">
          {error !== null && <div className="eui-dbg-error" role="alert">{error}</div>}
          {error === null && entries === null && (
            <div className="eui-empty">
              Baseline captured{tick !== null ? ` at engine tick ${tick}` : ""} — step to see
              per-tick changes.
            </div>
          )}
          {error === null && entries !== null && (
            <>
              <div className="eui-dbg-summary">
                {lastStepCount !== null ? `+${lastStepCount} tick${lastStepCount === 1 ? "" : "s"}: ` : ""}
                {entries.length === 0
                  ? `no component changes (${totalEntities} entities)`
                  : `${entries.length} of ${totalEntities} entities changed · ${unchangedEntities} unchanged`}
                {timedOut ? " · step barrier timed out — partial" : ""}
              </div>
              {shown!.map((d) => (
                <EntityRow key={d.id + d.kind} d={d} name={names(d.id)} onSelect={onSelect} />
              ))}
              {hidden > 0 && (
                <button type="button" className="eui-link" onClick={() => setShowAll(true)}>
                  Show all {entries.length} changed entities (+{hidden} more)
                </button>
              )}
            </>
          )}
        </section>
        <section className="eui-dbg-systems" aria-label="Scene systems">
          <div className="eui-group-label">Systems</div>
          {systems === null ? (
            <div className="eui-dbg-note">
              No system telemetry from this scene yet. Template games start reporting within a
              few stepped ticks; other scene runtimes aren&rsquo;t introspectable from the editor.
            </div>
          ) : (
            <>
              {systems.game && (
                <div className="eui-dbg-sys-game" title="Running game (template)">
                  {systems.game}
                  {systems.harnessTick !== null && (
                    <span className="dim"> · game tick {systems.harnessTick}</span>
                  )}
                </div>
              )}
              {systems.rows.length === 0 ? (
                <div className="eui-dbg-note">No systems registered by the game.</div>
              ) : (
                systems.rows.map((s) => (
                  <div key={s.name} className={"eui-dbg-sys" + (s.ran ? " ran" : "")}>
                    <span className="dot" aria-hidden="true" />
                    <span className="name">{s.name}</span>
                    <span className="runs" title="Executions since game start">
                      {s.ran ? "ran" : "idle"} · {s.runs}
                    </span>
                  </div>
                ))
              )}
              <div className="eui-dbg-note">{systems.handlers} pointer handler{systems.handlers === 1 ? "" : "s"} wired</div>
            </>
          )}
        </section>
      </div>
    </div>
  );
}
