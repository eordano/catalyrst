import type { FdProvenance } from "../types";
import "./fdprovenancepill.css";

type FdProvenancePillProps = {
  provenance: FdProvenance;
  className?: string;
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
    >
      {provenance}
    </span>
  );
}
