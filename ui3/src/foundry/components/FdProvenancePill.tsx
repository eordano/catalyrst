import type { FdProvenance } from "../types";
import "./fdprovenancepill.css";

type FdProvenancePillProps = {
  provenance: FdProvenance;
  className?: string;
};

// One-line glosses lifted from the FdProvenance doc comment in ../types.ts.
const GLOSS: Record<FdProvenance, string> = {
  imported: "read out of a source we do not own, carried as-is",
  recorded: "produced by an execution that actually ran",
  visitor: "someone on this site did it",
};

/** Dashed means machine-made. The word is always present, never colour alone. */
export default function FdProvenancePill({
  provenance,
  className = "",
}: FdProvenancePillProps) {
  return (
    <span
      className={
        "fd-prov fd-prov--" + provenance + (className ? " " + className : "")
      }
      title={GLOSS[provenance]}
    >
      {provenance}
      <span className="u-sr-only"> — {GLOSS[provenance]}</span>
    </span>
  );
}
