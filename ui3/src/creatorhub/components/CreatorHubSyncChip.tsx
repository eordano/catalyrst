import { useEffect, useState } from "react";

import { SyncIndicatorView } from "./SyncIndicator";
import { getSharedSyncEngine } from "../lib/shared-sync";
import { useSyncSummary } from "../lib/sync-engine";

type Props = { className?: string; onReview?: () => void };

export default function CreatorHubSyncChip({ className, onReview }: Props) {
  const [mounted, setMounted] = useState(false);
  useEffect(() => setMounted(true), []);

  const engine = getSharedSyncEngine();
  const summary = useSyncSummary(engine);

  if (!mounted || summary.scenes.length === 0) return null;

  return <SyncIndicatorView summary={summary} className={className} onReview={onReview} />;
}
