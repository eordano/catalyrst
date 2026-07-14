import { useState } from "react";

import ChDialogShell from "../components/ChDialogShell";
import { FALLBACK_PROFILES } from "./rules";
import type { MpReplayOutcome } from "./types";

import "./mpreplaydialog.css";

export type MpReplayDialogProps = {
  runId: string;
  profiles: string[];
  requesting?: boolean;
  outcome?: MpReplayOutcome | null;
  error?: string | null;
  onSubmit: (args: { tier: "a" | "b"; profile: string; seed: number }) => void;
  onClose: () => void;
};

const TIERS: Array<{
  tier: "a" | "b";
  title: string;
  copy: string;
}> = [
  {
    tier: "a",
    title: "Replay (exact, offline)",
    copy:
      "Re-delivers this run's recorded frames through the CRDT oracle under the profile below. Same bundle + profile + seed produces a byte-identical outcome. Scene code is not re-executed.",
  },
  {
    tier: "b",
    title: "Reproduce (live, same conditions)",
    copy:
      "Re-runs the live harness with the same scene, seeded bot scripts, and profile. Assertions are outcome-level (converged hash, verdict thresholds) — honestly labeled non-bit-exact.",
  },
];

export default function MpReplayDialog({
  runId,
  profiles,
  requesting = false,
  outcome = null,
  error = null,
  onSubmit,
  onClose,
}: MpReplayDialogProps) {
  const [tier, setTier] = useState<"a" | "b">("a");
  const [profile, setProfile] = useState("");
  const [seed, setSeed] = useState("42");
  const profileNames = profiles.length ? profiles : FALLBACK_PROFILES;
  const activeProfile = profile || profileNames[0] || "";

  return (
    <ChDialogShell
      title={`Replay run ${runId}`}
      ariaLabel="Replay run"
      onClose={onClose}
      width={560}
      className="mp-replay"
    >
      {outcome ? (
        <div className="mp-replay__result" data-mp-replay-result={outcome.tier}>
          <h4 className="mp-replay__result-title">
            {outcome.tier === "a"
              ? "Replay (exact, offline) finished"
              : "Reproduce (live, same conditions) started"}
          </h4>
          {outcome.tier === "a" ? (
            <>
              <div className="mp-replay__row">
                <span>Converged-state hash</span>
                <code data-mp-outcome-hash="">{outcome.hash ?? "—"}</code>
              </div>
              {outcome.decisionHash && (
                <div className="mp-replay__row">
                  <span>Decision-log hash</span>
                  <code>{outcome.decisionHash}</code>
                </div>
              )}
              <p className="mp-replay__note">
                Bit-deterministic: the same bundle, profile, and seed always
                yields these hashes.
              </p>
            </>
          ) : (
            <>
              {outcome.id && (
                <div className="mp-replay__row">
                  <span>New run</span>
                  <code data-mp-replay-run="">{outcome.id}</code>
                </div>
              )}
              <p className="mp-replay__note">
                Live reproduction — compare at the outcome level, not
                frame-by-frame.
              </p>
            </>
          )}
          <button
            type="button"
            className="mp-replay__secondary"
            data-mp-action="close-replay"
            onClick={onClose}
          >
            Close
          </button>
        </div>
      ) : (
        <form
          className="mp-replay__form"
          onSubmit={(e) => {
            e.preventDefault();
            if (!requesting) {
              onSubmit({ tier, profile: activeProfile, seed: Number(seed) || 0 });
            }
          }}
        >
          <div className="mp-replay__tiers" role="radiogroup" aria-label="Replay tier">
            {TIERS.map((t) => (
              <label
                key={t.tier}
                className={"mp-replay__tier" + (tier === t.tier ? " is-selected" : "")}
                data-mp-tier={t.tier}
              >
                <input
                  type="radio"
                  name="mp-replay-tier"
                  value={t.tier}
                  checked={tier === t.tier}
                  onChange={() => setTier(t.tier)}
                />
                <span className="mp-replay__tier-title">{t.title}</span>
                <span className="mp-replay__tier-copy">{t.copy}</span>
              </label>
            ))}
          </div>

          <div className="mp-replay__fields">
            <label className="mp-replay__field">
              <span>Network profile</span>
              <select
                name="mp-replay-profile"
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
            <label className="mp-replay__field">
              <span>Seed</span>
              <input
                type="number"
                name="mp-replay-seed"
                value={seed}
                onChange={(e) => setSeed(e.target.value)}
              />
            </label>
          </div>

          {error && (
            <p className="mp-replay__error" role="alert" data-mp-error="replay">
              {error}
            </p>
          )}

          <div className="mp-replay__actions">
            <button
              type="submit"
              className="mp-replay__primary"
              data-mp-action="replay"
              disabled={requesting}
            >
              {requesting
                ? "Working…"
                : tier === "a"
                  ? "Replay offline"
                  : "Reproduce live"}
            </button>
            <button
              type="button"
              className="mp-replay__secondary"
              data-mp-action="close-replay"
              onClick={onClose}
            >
              Cancel
            </button>
          </div>
        </form>
      )}
    </ChDialogShell>
  );
}
