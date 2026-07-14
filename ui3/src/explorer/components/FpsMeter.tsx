import { useFps } from "./useFps";
import "./fpsmeter.css";

export type FpsTone = "good" | "warn" | "bad";

export const FPS_GOOD = 55;
export const FPS_WARN = 30;

export function fpsTone(page: number): FpsTone {
  if (page >= FPS_GOOD) return "good";
  if (page >= FPS_WARN) return "warn";
  return "bad";
}

type FpsMeterProps = {
  /** Injectable for stories and tests; defaults to live measurement. */
  stats?: { page: number; engine: number | null; ms: number };
};

export default function FpsMeter({ stats }: FpsMeterProps = {}) {
  const live = useFps(stats === undefined);
  const { page, engine, ms } = stats ?? live;

  return (
    <div className="fpsmeter" aria-hidden="true">
      <span className={`fpsmeter__fps is-${fpsTone(page)}`}>
        {page}
        <span className="fpsmeter__unit">fps</span>
      </span>
      <span className="fpsmeter__dim">{ms}ms</span>
      {engine !== null ? (
        <span className="fpsmeter__dim">
          engine <b className="fpsmeter__engine">{engine}</b>
        </span>
      ) : null}
    </div>
  );
}
