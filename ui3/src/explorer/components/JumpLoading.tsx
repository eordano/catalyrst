import Spinner from "../../atoms/Spinner";
import "./jumploading.css";

export default function JumpLoading({ name }: { name?: string }) {
  return (
    <div className="jl" role="status" aria-live="polite">
      <Spinner size={54} aria-hidden />
      <div className="jl__text">Teleporting{name ? ` to ${name}` : ""}…</div>
    </div>
  );
}
