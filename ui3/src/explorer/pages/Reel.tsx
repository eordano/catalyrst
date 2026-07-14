import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router";
import ExploreChrome, { type TabId } from "../frames/ExploreChrome";
import JumpLoading from "../components/JumpLoading";
import PhotoDetail, { photoTime, type ReelPhoto } from "../components/PhotoDetail";
import "./reel.css";
import { sendBridge, useBridgeState } from "../../overlay/bridge";
import { serviceBase, signedFetch } from "../../data/catalyst/client";

const STORAGE_MAX = 500;
const PAGE = 100;
const PARCEL_SIZE = 16;

type ReelStorage = { current: number; max: number };
type ReelGroup = { key: string; items: { photo: ReelPhoto; index: number }[] };

function monthLabel(ms: number): string {
  return new Date(ms).toLocaleDateString("en-US", { month: "long", year: "numeric" });
}

export default function Reel() {
  const navigate = useNavigate();
  const identity = useBridgeState((s) => s.identity);
  const address = identity?.address || null;
  const [tab, setTab] = useState<TabId>("gallery");
  const [photos, setPhotos] = useState<ReelPhoto[]>([]);
  const [storage, setStorage] = useState<ReelStorage | null>(null);
  const [loading, setLoading] = useState(false);
  const [index, setIndex] = useState<number | null>(null);
  const [jumping, setJumping] = useState<string | null>(null);

  const load = useCallback(async () => {
    if (!address) return;
    setLoading(true);
    try {
      const { status, body } = await signedFetch(
        `${serviceBase("cameraReel")}/api/users/${address}/images?limit=${PAGE}&offset=0`,
        { method: "GET" }
      );
      if (status >= 200 && status < 300) {
        const data = JSON.parse(body);
        const images = Array.isArray(data?.images) ? (data.images as ReelPhoto[]) : [];
        setPhotos(images);
        setStorage({
          current: Number.isFinite(data?.currentImages) ? data.currentImages : images.length,
          max: Number.isFinite(data?.maxImages) ? data.maxImages : 0,
        });
      }
    } catch {
    } finally {
      setLoading(false);
    }
  }, [address]);

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
    [address]
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
      sendBridge("Teleport", {
        x: x * PARCEL_SIZE + PARCEL_SIZE / 2,
        z: y * PARCEL_SIZE + PARCEL_SIZE / 2,
      });
      setJumping("destination");
      setTimeout(() => {
        setJumping(null);
        navigate("/");
      }, 3500);
    },
    [navigate]
  );

  const onViewPerson = useCallback(
    (personAddress: string) => {
      setIndex(null);
      navigate(`/passport?address=${encodeURIComponent(personAddress)}`);
    },
    [navigate]
  );

  return (
    <ExploreChrome active={tab} onTab={setTab}>
      <div className="rl">
        <div className="rl__head">
          <h1 className="rl__title">Gallery</h1>
          <div className="rl__storage">
            <div className="rl__storagetxt">
              Storage <b>{current}</b>/{max} photos taken
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
            <div className="rl__empty">
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
                {loading ? "Loading your reel…" : "There are no photos yet"}
              </div>
              <div className="rl__emptynote">
                Use the <b>camera</b> to save incredible memories with your friends!
              </div>
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
      {jumping && <JumpLoading name={jumping} />}
    </ExploreChrome>
  );
}
