import { useMemo, useState } from "react";

import {
  DEFAULT_THRESHOLDS,
  FALLBACK_PROFILES,
  LANES,
  LANE_CAPS,
  LANE_CLAIMS,
  LANE_LABELS,
  SCENARIOS,
  SCENARIO_LABELS,
  SYNC_FIXTURE,
  WINDOW_LIMITS,
  fixturesForLane,
  validateSpec,
  type MpLane,
  type MpLaunchRequest,
  type MpRunSpec,
  type MpScenario,
  type MpSourceKind,
  type MpThresholds,
} from "./rules";

import "./mplaunchform.css";

type PeerOverride = { peer: string; latency_ms: string; loss_pct: string };
type ScheduleRow = { at_ms: string; peer: string; duration_ms: string };

export type MpLaunchFormProps = {
  presets: string[];
  profiles: string[];
  disabled?: boolean;
  error?: string | null;
  onLaunch: (req: MpLaunchRequest) => void;
};

const THRESHOLD_FIELDS: Array<{ key: keyof MpThresholds; label: string }> = [
  { key: "max_diverged_final", label: "Max diverged (final)" },
  { key: "max_converge_ms", label: "Max converge (ms)" },
  { key: "min_synced_pct", label: "Min synced (%)" },
  { key: "max_never_synced", label: "Max never-synced" },
];

export default function MpLaunchForm({
  presets,
  profiles,
  disabled = false,
  error = null,
  onLaunch,
}: MpLaunchFormProps) {
  const [lane, setLane] = useState<MpLane>("protocol");
  const [sourceKind, setSourceKind] = useState<MpSourceKind>("fixture");
  const [fixture, setFixture] = useState<string>(SYNC_FIXTURE);
  const [sourceRef, setSourceRef] = useState("");
  const [bots, setBots] = useState("2");
  const [mode, setMode] = useState<MpScenario>("burst");
  const [windowSecs, setWindowSecs] = useState(String(WINDOW_LIMITS.fallback));
  const [profile, setProfile] = useState("");
  const [preset, setPreset] = useState("");
  const [seed, setSeed] = useState("");
  const [thresholds, setThresholds] = useState<Record<keyof MpThresholds, string>>({
    max_diverged_final: String(DEFAULT_THRESHOLDS.max_diverged_final),
    max_converge_ms: String(DEFAULT_THRESHOLDS.max_converge_ms),
    min_synced_pct: String(DEFAULT_THRESHOLDS.min_synced_pct),
    max_never_synced: String(DEFAULT_THRESHOLDS.max_never_synced),
  });
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [peerRows, setPeerRows] = useState<PeerOverride[]>([]);
  const [scheduleRows, setScheduleRows] = useState<ScheduleRow[]>([]);

  const profileNames = profiles.length ? profiles : FALLBACK_PROFILES;
  const activeProfile = profile || profileNames[0];
  const fixtures = fixturesForLane(lane);
  const caps = LANE_CAPS[lane];

  const pickLane = (next: MpLane) => {
    setLane(next);
    if (sourceKind === "fixture" && !fixturesForLane(next).includes(fixture)) {
      setFixture(SYNC_FIXTURE);
    }
  };

  const request: MpLaunchRequest = useMemo(() => {
    if (preset) return { preset };
    const peers: Record<string, Record<string, number>> = {};
    for (const row of peerRows) {
      if (!row.peer.trim()) continue;
      const o: Record<string, number> = {};
      if (row.latency_ms.trim()) o.latency_ms = Number(row.latency_ms);
      if (row.loss_pct.trim()) o.loss_pct = Number(row.loss_pct);
      if (Object.keys(o).length) peers[row.peer.trim()] = o;
    }
    const schedule = scheduleRows
      .filter((r) => r.peer.trim() && r.at_ms.trim())
      .map((r) => ({
        at_ms: Number(r.at_ms),
        peer: r.peer.trim(),
        partition: { duration_ms: Number(r.duration_ms || "10000") },
      }));
    const spec: MpRunSpec = {
      lane,
      scene:
        sourceKind === "fixture"
          ? { kind: "fixture", ref: fixture }
          : { kind: sourceKind, ref: sourceRef.trim() },
      bots: Number(bots),
      mode,
      window: Number(windowSecs) || WINDOW_LIMITS.fallback,
      shape: {
        profile: profile || profileNames[0] || "",
        ...(Object.keys(peers).length ? { peers } : {}),
        ...(schedule.length ? { schedule } : {}),
        ...(seed.trim() ? { seed: Number(seed) } : {}),
      },
      thresholds: {
        max_diverged_final: Number(thresholds.max_diverged_final),
        max_converge_ms: Number(thresholds.max_converge_ms),
        min_synced_pct: Number(thresholds.min_synced_pct),
        max_never_synced: Number(thresholds.max_never_synced),
      },
    };
    return spec;
  }, [
    preset, lane, sourceKind, fixture, sourceRef, bots, mode, windowSecs,
    profile, profileNames, seed, thresholds, peerRows, scheduleRows,
  ]);

  const check = validateSpec(request);

  return (
    <form
      className="mp-launch"
      data-mp-form="launch"
      onSubmit={(e) => {
        e.preventDefault();
        if (!disabled && check.ok) onLaunch(request);
      }}
    >
      <fieldset className="mp-launch__group">
        <legend className="mp-launch__legend">Lane</legend>
        <div className="mp-launch__lanes" role="radiogroup" aria-label="Lane">
          {LANES.map((l) => (
            <label
              key={l}
              className={"mp-launch__lane" + (lane === l ? " is-selected" : "")}
            >
              <input
                type="radio"
                name="mp-lane"
                value={l}
                checked={lane === l}
                onChange={() => pickLane(l)}
              />
              <span className="mp-launch__lane-name">{LANE_LABELS[l]}</span>
              <span className="mp-launch__lane-cap">
                {LANE_CAPS[l].min}&#x2013;{LANE_CAPS[l].max} clients
              </span>
            </label>
          ))}
        </div>
        <p className="mp-launch__claim">{LANE_CLAIMS[lane]}</p>
      </fieldset>

      <fieldset className="mp-launch__group">
        <legend className="mp-launch__legend">One-click preset</legend>
        <select
          name="mp-preset"
          value={preset}
          onChange={(e) => setPreset(e.target.value)}
          aria-label="Run preset"
        >
          <option value="">Custom run (fields below)</option>
          {presets.map((p) => (
            <option key={p} value={p}>
              {p}
            </option>
          ))}
        </select>
      </fieldset>

      <fieldset className="mp-launch__group" disabled={!!preset}>
        <legend className="mp-launch__legend">Scene</legend>
        <div className="mp-launch__row">
          <select
            name="mp-source-kind"
            value={sourceKind}
            onChange={(e) => setSourceKind(e.target.value as MpSourceKind)}
            aria-label="Scene source"
          >
            <option value="fixture">Named fixture</option>
            <option value="dir">Project scene dir</option>
            <option value="realm">Realm URL</option>
          </select>
          {sourceKind === "fixture" ? (
            <select
              name="mp-fixture"
              value={fixture}
              onChange={(e) => setFixture(e.target.value)}
              aria-label="Fixture"
            >
              {fixtures.map((f) => (
                <option key={f} value={f}>
                  {f}
                </option>
              ))}
            </select>
          ) : (
            <input
              type="text"
              name="mp-source-ref"
              value={sourceRef}
              onChange={(e) => setSourceRef(e.target.value)}
              placeholder={
                sourceKind === "dir" ? "/path/to/scene" : "https://realm\u{2026}/about"
              }
              aria-label={sourceKind === "dir" ? "Scene dir" : "Realm URL"}
            />
          )}
        </div>
        {lane !== "protocol" && (
          <p className="mp-launch__hint">
            Game fixtures are protocol-lane only for now; {SYNC_FIXTURE} works in
            every lane.
          </p>
        )}
      </fieldset>

      <fieldset className="mp-launch__group" disabled={!!preset}>
        <legend className="mp-launch__legend">Run</legend>
        <div className="mp-launch__row">
          <label className="mp-launch__field">
            <span>Clients</span>
            <input
              type="number"
              name="mp-bots"
              min={caps.min}
              max={caps.max}
              value={bots}
              onChange={(e) => setBots(e.target.value)}
            />
          </label>
          <label className="mp-launch__field">
            <span>Scenario</span>
            <select
              name="mp-mode"
              value={mode}
              onChange={(e) => setMode(e.target.value as MpScenario)}
            >
              {SCENARIOS.map((s) => (
                <option key={s} value={s}>
                  {SCENARIO_LABELS[s]}
                </option>
              ))}
            </select>
          </label>
          <label className="mp-launch__field">
            <span>Window (s)</span>
            <input
              type="number"
              name="mp-window"
              min={WINDOW_LIMITS.min}
              max={WINDOW_LIMITS.max}
              value={windowSecs}
              onChange={(e) => setWindowSecs(e.target.value)}
            />
          </label>
          <label className="mp-launch__field">
            <span>Network profile</span>
            <select
              name="mp-profile"
              value={activeProfile}
              onChange={(e) => setProfile(e.target.value)}
            >
              {profileNames.map((p) => (
                <option key={p} value={p}>
                  {p}
                </option>
              ))}
            </select>
          </label>
          <label className="mp-launch__field">
            <span>Seed</span>
            <input
              type="number"
              name="mp-seed"
              value={seed}
              placeholder="auto"
              onChange={(e) => setSeed(e.target.value)}
            />
          </label>
        </div>
        {!check.ok && check.field === "bots" && (
          <p className="mp-launch__error" role="alert" data-mp-error="bots-cap">
            {check.error}
          </p>
        )}
      </fieldset>

      <fieldset className="mp-launch__group" disabled={!!preset}>
        <legend className="mp-launch__legend">Pass/fail thresholds</legend>
        <div className="mp-launch__row">
          {THRESHOLD_FIELDS.map(({ key, label }) => (
            <label key={key} className="mp-launch__field">
              <span>{label}</span>
              <input
                type="number"
                name={`mp-threshold-${key}`}
                min={0}
                value={thresholds[key]}
                onChange={(e) =>
                  setThresholds((t) => ({ ...t, [key]: e.target.value }))
                }
              />
            </label>
          ))}
        </div>
      </fieldset>

      <details
        className="mp-launch__advanced"
        open={advancedOpen}
        onToggle={(e) => setAdvancedOpen((e.target as HTMLDetailsElement).open)}
      >
        <summary>Advanced &#x2014; per-peer profiles and shaping schedule</summary>
        <div className="mp-launch__group" data-mp-editor="peers">
          <div className="mp-launch__legend">Per-peer overrides</div>
          {peerRows.map((row, i) => (
            <div key={i} className="mp-launch__row">
              <input
                type="text"
                placeholder="peer (b3)"
                value={row.peer}
                aria-label="Peer"
                onChange={(e) =>
                  setPeerRows((rs) =>
                    rs.map((r, j) => (j === i ? { ...r, peer: e.target.value } : r)),
                  )
                }
              />
              <input
                type="number"
                placeholder="latency ms"
                value={row.latency_ms}
                aria-label="Latency ms"
                onChange={(e) =>
                  setPeerRows((rs) =>
                    rs.map((r, j) =>
                      j === i ? { ...r, latency_ms: e.target.value } : r,
                    ),
                  )
                }
              />
              <input
                type="number"
                placeholder="loss %"
                value={row.loss_pct}
                aria-label="Loss percent"
                onChange={(e) =>
                  setPeerRows((rs) =>
                    rs.map((r, j) =>
                      j === i ? { ...r, loss_pct: e.target.value } : r,
                    ),
                  )
                }
              />
              <button
                type="button"
                className="mp-launch__remove"
                aria-label="Remove peer override"
                onClick={() => setPeerRows((rs) => rs.filter((_, j) => j !== i))}
              >
                &#x2715;
              </button>
            </div>
          ))}
          <button
            type="button"
            className="mp-launch__add"
            onClick={() =>
              setPeerRows((rs) => [...rs, { peer: "", latency_ms: "", loss_pct: "" }])
            }
          >
            + peer override
          </button>
        </div>
        <div className="mp-launch__group" data-mp-editor="schedule">
          <div className="mp-launch__legend">Schedule (partitions)</div>
          {scheduleRows.map((row, i) => (
            <div key={i} className="mp-launch__row">
              <input
                type="number"
                placeholder="at ms"
                value={row.at_ms}
                aria-label="At ms"
                onChange={(e) =>
                  setScheduleRows((rs) =>
                    rs.map((r, j) => (j === i ? { ...r, at_ms: e.target.value } : r)),
                  )
                }
              />
              <input
                type="text"
                placeholder="peer (b3)"
                value={row.peer}
                aria-label="Peer"
                onChange={(e) =>
                  setScheduleRows((rs) =>
                    rs.map((r, j) => (j === i ? { ...r, peer: e.target.value } : r)),
                  )
                }
              />
              <input
                type="number"
                placeholder="partition ms"
                value={row.duration_ms}
                aria-label="Partition duration ms"
                onChange={(e) =>
                  setScheduleRows((rs) =>
                    rs.map((r, j) =>
                      j === i ? { ...r, duration_ms: e.target.value } : r,
                    ),
                  )
                }
              />
              <button
                type="button"
                className="mp-launch__remove"
                aria-label="Remove schedule entry"
                onClick={() =>
                  setScheduleRows((rs) => rs.filter((_, j) => j !== i))
                }
              >
                &#x2715;
              </button>
            </div>
          ))}
          <button
            type="button"
            className="mp-launch__add"
            onClick={() =>
              setScheduleRows((rs) => [...rs, { at_ms: "", peer: "", duration_ms: "" }])
            }
          >
            + schedule entry
          </button>
        </div>
      </details>

      {!check.ok && check.field !== "bots" && (
        <p className="mp-launch__error" role="alert" data-mp-error={check.field}>
          {check.error}
        </p>
      )}
      {error && (
        <p className="mp-launch__error" role="alert" data-mp-error="launch">
          {error}
        </p>
      )}

      <button
        type="submit"
        className="mp-launch__submit"
        data-mp-action="launch"
        disabled={disabled || !check.ok}
      >
        Launch test
      </button>
    </form>
  );
}
