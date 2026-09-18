import { useWorldEntry } from "../../app/WorldEntry";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { useQueryClient } from "@tanstack/react-query";
import { reelKey, reelQuery, type ReelPage } from "../../data/catalyst/reel";
import ExploreChrome, { type TabId } from "../frames/ExploreChrome";
import JumpLoading, { useJump } from "../components/JumpLoading";
import PhotoDetail, { photoTime, type ReelPhoto } from "../components/PhotoDetail";
import "./reel.css";
import { sendBridge, useBridgeState } from "../../overlay/bridge";
import { serviceBase, signedFetch } from "../../data/catalyst/client";

const STORAGE_MAX = 500;
const PARCEL_SIZE = 16;

type ReelStorage = { current: number; max: number };
type ReelGroup = { key: string; items: { photo: ReelPhoto; index: number }[] };

function monthLabel(ms: number): string {
  return new Date(ms).toLocaleDateString("en-US", { month: "long", year: "numeric" });
}

export default function Reel({ embedded = false }: { embedded?: boolean }) {
  const address = useBridgeState((s) => s.identity.address) || null;
  return <AccountReel key={address ?? "guest"} embedded={embedded} address={address} />;
}

function AccountReel({ embedded, address }: { embedded: boolean; address: string | null }) {
  const queryClient = useQueryClient();
  const cached = address ? queryClient.getQueryData<ReelPage>(reelKey(address)) : undefined;
  const navigate = useNavigate();
  const entry = useWorldEntry();
  const [tab, setTab] = useState<TabId>("gallery");
  const [photos, setPhotos] = useState<ReelPhoto[]>(cached?.images ?? []);
  const [storage, setStorage] = useState<ReelStorage | null>(cached ? { current: cached.currentImages, max: cached.maxImages } : null);
  const [loading, setLoading] = useState(Boolean(address && !cached));
  const [failed, setFailed] = useState(false);
  const [index, setIndex] = useState<number | null>(null);
  const { jumping, stalled, beginJump, cancelJump, confirmJump } = useJump(() => navigate("/"));

  const load = useCallback(async () => {
    if (!address) return;
    setLoading(!queryClient.getQueryData(reelKey(address)));
    setFailed(false);
    try {
      const data = await queryClient.fetchQuery(reelQuery(address));
      setPhotos(data.images);
      setStorage({ current: data.currentImages, max: data.maxImages });
    } catch {
      setFailed(true);
    } finally {
      setLoading(false);
    }
  }, [address, queryClient]);

  useEffect(() => {
    load();
  }, [load]);

  const removePhoto = useCallback(
    async (id: string) => {
      if (!address) return;
      try {
        const { status, body } = await signedFetch(
          `${serviceBase("cameraReel")}/api/images/${encodeURIComponent(id)}`,
          { method: "DELETE" }
        );
        if (status >= 200 && status < 300) {
          queryClient.setQueryData<ReelPage>(reelKey(address), previous => previous && {
            ...previous, images: previous.images.filter(photo => photo.id !== id), currentImages: Math.max(0, previous.currentImages - 1),
          });
          await queryClient.invalidateQueries({ queryKey: reelKey(address), refetchType: "none" });
          setPhotos((prev) => prev.filter((p) => p.id !== id));
          try {
            const data = JSON.parse(body);
            if (Number.isFinite(data?.currentImages)) {
              setStorage((prev) => ({
                current: data.currentImages,
                max: Number.isFinite(data?.maxImages) ? data.maxImages : prev?.max || 0,
              }));
            }
          } catch {
          }
        }
      } catch {
      }
    },
    [address, queryClient]
  );

  const current = storage?.current ?? photos.length;
  const max = storage?.max || STORAGE_MAX;
  const pct = max > 0 ? Math.round((Math.min(current, max) / max) * 100) : 0;

  const groups = useMemo<ReelGroup[]>(() => {
    const out: ReelGroup[] = [];
    photos.forEach((photo, i) => {
      const key = photo.metadata?.dateTime ? monthLabel(photoTime(photo.metadata.dateTime)) : "Undated";
      let g = out[out.length - 1];
      if (g == null || g.key !== key) {
        g = { key, items: [] };
        out.push(g);
      }
      g.items.push({ photo, index: i });
    });
    return out;
  }, [photos]);

  const onTeleport = useCallback(
    (x: number, y: number) => {
      setIndex(null);
      if (entry?.pending) { entry.enter({ kind: "parcel", x, y }); return; }
      sendBridge("Teleport", {
        x: x * PARCEL_SIZE + PARCEL_SIZE / 2,
        z: y * PARCEL_SIZE + PARCEL_SIZE / 2,
      });
      beginJump("destination");
    },
    [beginJump, entry]
  );

  const onViewPerson = useCallback(
    (personAddress: string) => {
      setIndex(null);
      navigate(`/passport?address=${encodeURIComponent(personAddress)}`);
    },
    [navigate]
  );

  const content = (
    <>
      <div className="rl">
        <div className="rl__head">
          {!embedded && <h1 className="rl__title">Gallery</h1>}
          <div className="rl__storage">
            <div className="rl__storagetxt">
              {loading ? "Loading storage\u2026" : !address ? "Sign in to view storage" : failed ? "Storage unavailable" : <>Storage <b>{current}</b>/{max} photos taken</>}
            </div>
            <div className="rl__storagebar">
              <span className="rl__storagefill" style={{ width: pct + "%" }} />
            </div>
          </div>
        </div>

        <div className="rl__body">
          {photos.length > 0 ? (
            <div className="rl__scroll">
              {groups.map((g) => (
                <section key={g.key} className="rl__group">
                  <h2 className="rl__month">{g.key}</h2>
                  <div className="rl__grid">
                    {g.items.map(({ photo, index: i }) => (
                      <button
                        key={photo.id}
                        type="button"
                        className="rl__cell"
                        title={photo.metadata?.dateTime || "Reel photo"}
                        onClick={() => setIndex(i)}
                      >
                        <img
                          className="rl__cellimg"
                          src={photo.thumbnailUrl || photo.url}
                          alt="Reel photo"
                          loading="lazy"
                        />
                      </button>
                    ))}
                  </div>
                </section>
              ))}
            </div>
          ) : (
            <div className="rl__empty" role={failed ? "alert" : loading ? "status" : undefined} aria-busy={loading || undefined}>
              <div className="rl__emptyicon">
                <svg viewBox="0 0 24 24" width="40" height="40" aria-hidden="true">
                  <defs>
                    <linearGradient id="rl-emptygrad" x1="0" y1="0" x2="1" y2="1">
                      <stop offset="0" stopColor="#ff2d8e" />
                      <stop offset="1" stopColor="#a524b3" />
                    </linearGradient>
                  </defs>
                  <rect x="2.5" y="4.5" width="19" height="15" rx="4" fill="none" stroke="url(#rl-emptygrad)" strokeWidth="2" />
                  <circle cx="8.2" cy="9.6" r="1.9" fill="url(#rl-emptygrad)" />
                  <path d="M3.5 18.5l5.2-5.4 3.5 3.6 3.6-3.7 4.7 4.9" fill="none" stroke="url(#rl-emptygrad)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
              </div>
              <div className="rl__emptytitle">
                {!address ? "Sign in to see your photos" : loading
                  ? "Loading your reel\u{2026}"
                  : failed
                    ? "We couldn't reach your photo gallery"
                    : "There are no photos yet"}
              </div>
              <div className="rl__emptynote">
                {loading ? "Your photos will appear here when they're ready." : failed ? (
                  <>Your photos are still there &#x2014; try again.</>
                ) : (
                  <>
                    Use the <b>camera</b> to save incredible memories with your friends!
                  </>
                )}
              </div>
              {failed && !loading && (
                <button type="button" className="rl__retry" onClick={load}>
                  Retry
                </button>
              )}
            </div>
          )}
        </div>
      </div>

      {index != null && photos[index] && (
        <PhotoDetail
          photos={photos}
          index={index}
          isSelf
          onIndex={setIndex}
          onClose={() => setIndex(null)}
          onTeleport={onTeleport}
          onDelete={(id) => {
            removePhoto(id);
            setIndex(null);
          }}
          onViewPerson={onViewPerson}
        />
      )}
      {jumping && (
        <JumpLoading
          name={jumping}
          stalled={stalled}
          onCancel={cancelJump}
          onEnterAnyway={confirmJump}
        />
      )}
    </>
  );
  return embedded ? content : <ExploreChrome active={tab} onTab={setTab}>{content}</ExploreChrome>;
}
