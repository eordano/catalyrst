import "./freshnessnotice.css";

export default function FreshnessNotice({ failed, onRetry }: { failed?: boolean; onRetry?: () => void }) {
  if (!failed) return null;
  return <div className="freshness-notice" role="status">
    <span>Showing saved results. We couldn&rsquo;t refresh them.</span>
    {onRetry && <button type="button" onClick={onRetry}>Try again</button>}
  </div>;
}
