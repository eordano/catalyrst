import Button from "../atoms/Button";
import Spinner from "../atoms/Spinner";
import "./contentstatus.css";

export default function ContentStatus({ message, pending = false, onRetry }: {
  message: string; pending?: boolean; onRetry?: () => void;
}) {
  return <div className="content-status" role={pending ? "status" : "alert"} aria-busy={pending || undefined}>
    {pending && <Spinner size={28} aria-hidden />}
    <p>{message}</p>
    {onRetry && <Button variant="secondary" size="sm" onClick={onRetry}>Retry</Button>}
  </div>;
}
