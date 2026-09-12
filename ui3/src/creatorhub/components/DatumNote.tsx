import { datumEndpoint, noteLines, type Datum } from "../lib/datum";
import "./datumnote.css";

export type DatumNoteProps = {
  datum: Datum<unknown>;
  now?: number;
};

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
