import Spinner from "../atoms/Spinner";
import "./bootgate.css";

export default function ShellLoading({ label = "Opening Decentraland\u2026" }: { label?: string }) {
  return (
    <div className="boot" role="status">
      <div className="boot__stalled">
        <Spinner size={34} aria-hidden />
        <p>{label}</p>
      </div>
    </div>
  );
}
