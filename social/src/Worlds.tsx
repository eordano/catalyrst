import { SurfaceCard } from "@ui/components/Surface";
import { useState } from "react";
import { publicWorlds, usePublicDiscovery } from "./public-feeds";
import FreshnessNotice from "@ui/components/FreshnessNotice";
import { destinationUrl, worldName } from "./destinations";
import { Dialog } from "./Dialog";
import { Picture } from "./Picture";
import "./discovery.css";
export type World = {
  id: string;
  world_name: string;
  title: string;
  description?: string;
  image?: string;
  user_count?: number;
  categories?: string[];
};
function useWorlds(search = "") {
  const page = usePublicDiscovery(publicWorlds, search);
  return { ...page, worlds: page.data, error: page.error && !page.refreshFailed ? "Worlds could not be loaded." : "" };
}
function WorldCard({ world }: { world: World }) {
  const [copied, setCopied] = useState(false),
    [error, setError] = useState("");
  const [open, setOpen] = useState(false), [launching, setLaunching] = useState(false);
  const name = worldName(world.world_name);
  if (!name) return null;
  const url = destinationUrl({ x: 0, y: 0, world: name });
  return (
    <SurfaceCard className="world-card">
      <button type="button" onClick={() => setOpen(true)} className="world-art" aria-label={`View ${world.title || name}`}>
        <Picture src={world.image} fallback="&#x25c8;" />
        {(world.user_count || 0) > 0 && (
          <span className="discovery-live">{world.user_count} online</span>
        )}
      </button>
      <div className="world-card-body">
        <button className="world-title" onClick={() => setOpen(true)}><strong>{world.title || name}</strong></button>
        <small>{name}</small>
        <p>{world.description}</p>
        <div className="discovery-actions">
          <button className="outline-button" onClick={() => setOpen(true)}>Visit world</button>
          <button
            className="text-button"
            onClick={() => {
              setError("");
              navigator.clipboard
                .writeText(url)
                .then(() => {
                  setCopied(true);
                  setTimeout(() => setCopied(false), 2000);
                })
                .catch(() =>
                  setError("Copy failed. Open the world to copy its address."),
                );
            }}
          >
            {copied ? "Copied \u2713" : "Copy link"}
          </button>
        </div>
        {error && <small role="alert">{error}</small>}
      </div>
      {open && <Dialog title={world.title || name} className="world-dialog" onClose={() => {setOpen(false); setLaunching(false);}}>
        <div className="world-detail-art"><Picture src={world.image} fallback="&#x25c8;" /></div>
        <div className="dialog-content"><h2>{world.title || name}</h2><p className="muted">{name}{(world.user_count || 0) > 0 ? ` \u00b7 ${world.user_count} online` : ""}</p><p className="world-description">{world.description}</p>
          <a className="primary world-launch" href={`decentraland://${/Android|iPhone|iPad|iPod/i.test(navigator.userAgent) || (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1) ? "open" : ""}?realm=${encodeURIComponent(name)}`} onClick={() => setLaunching(true)}>Open Decentraland &#x2197;</a>
          {launching && <p className="muted" role="status">App didn&#x2019;t open? <a href={`https://decentraland.org/download?realm=${encodeURIComponent(name)}`} target="_blank" rel="noreferrer">Get Decentraland &#x2197;</a></p>}
        </div>
      </Dialog>}
    </SurfaceCard>
  );
}
export function WorldsPage({search = ""}: {search?: string}) {
  const { worlds, loading, error, refreshFailed, retry, more, loadMore } = useWorlds(search);
  return (
    <section className="discovery-page">
      <FreshnessNotice failed={refreshFailed} onRetry={() => void retry()} />
      <div className="world-grid">
        {worlds.map((world) => (
          <WorldCard key={world.id} world={world} />
        ))}
      </div>
      {loading && (
        <p role="status" className="muted">
          Finding worlds&#x2026;
        </p>
      )}
      {error && (
        <p role="alert">
          {error} <button onClick={retry}>Retry</button>
        </p>
      )}
      {!loading && !error && !worlds.length && (
        <p className="muted">No worlds found. Try a world name.</p>
      )}
      {more && !loading && (
        <button className="outline-button" onClick={loadMore}>
          More worlds
        </button>
      )}
    </section>
  );
}
