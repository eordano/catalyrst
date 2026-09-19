import { useBridgeState } from "../../overlay/bridge";
import { formatBytes, formatWait, useTransferProgress } from "../../overlay/loadingTransfers";

export default function JumpProgress() {
  const loading = useBridgeState(s => s.loading);
  const progress = useTransferProgress();
  const hasTransfers = progress.received > 0 || progress.active > 0 || progress.completed > 0;
  const pending = loading && !loading.ready ? loading.pendingAssets : 0;
  const stage = progress.active > 0 ? "Loading scene assets" : pending > 0 ? "Preparing models and textures" : hasTransfers ? "Starting the scene" : "Finding your destination";
  const percent = progress.known && progress.total > 0 ? Math.floor(progress.received / progress.total * 100) : undefined;
  return <section className="jl__progress" aria-label="Teleport progress">
    <div className="jl__stage" role="status">{stage}</div>
    <progress className="jl__bar" aria-label="Asset data loaded" max={100} value={percent} />
    <div className="jl__metrics">
      <span>{formatBytes(progress.received)} received{progress.known && progress.remainingBytes > 0 ? ` of ${formatBytes(progress.total)} requested` : ""}</span>
      <span>{progress.speed > 0 && progress.active > 0 ? `${formatBytes(progress.speed)}/s` : `${formatWait(progress.elapsed)} elapsed`}</span>
    </div>
    {hasTransfers && <p>{progress.completed} {progress.completed === 1 ? "download" : "downloads"} complete{progress.active > 0 ? ` \u00b7 ${progress.active} in progress` : ""}</p>}
    {pending > 0 && <p>{pending} {pending === 1 ? "model" : "models"} left to prepare</p>}
    <p className="jl__estimate">{progress.estimate != null ? `About ${formatWait(progress.estimate)} for current downloads` : progress.active > 0 ? "Estimating remaining download time\u2026" : "Waiting for the scene to be ready\u2026"}</p>
    {hasTransfers && <p className="jl__note">More assets may be discovered as the scene loads. Cached assets can load faster.</p>}
  </section>;
}
