import { datumEndpoint, noteLines, type Datum } from "../lib/datum";
import "./datumnote.css";

export type DatumNoteProps = {
  datum: Datum<unknown>;
  /** Pinned clock, for deterministic stories and tests. */
  now?: number;
};

/**
 * The `METHOD host/path` line plus the reason sentence(s). Mandatory beneath
 * any tile whose state is `no-sample`, `unavailable` or `unbuilt`; the endpoint
 * line is worth keeping on the showable states too, so a reader can check the
 * number themselves.
 *
 * `unbuilt` has no endpoint -- there is nothing to name -- so it renders reason
 * and "Today:" only.
 */
export default function DatumNote({ datum, now }: DatumNoteProps) {
  const endpoint = datumEndpoint(datum);
  const lines = noteLines(datum, now);
  const prose = endpoint === null ? lines : lines.slice(1);

  return (
    <p className="dv-note">
      {endpoint === null ? null : (
        <span className="dv-note__endpoint">{endpoint}</span>
      )}
      {prose.map((line, i) => (
        <span className="dv-note__line" key={i}>
          {line}
        </span>
      ))}
    </p>
  );
}
