import { Picture } from "./Picture";
import { useEffect, useState } from "react";
import { api, shortWallet } from "./api";
type Profile = { name?: string; description?: string; avatar?: { snapshots?: { face256?: string } } };
const profiles = new Map<string, Promise<Profile | null>>();
export function useProfile(wallet = "") {
  const [profile, setProfile] = useState<Profile | null>(null);
  useEffect(() => {
    let active = true;
    setProfile(null);
    if (!/^0x[0-9a-f]{40}$/i.test(wallet)) return;
    if (!profiles.has(wallet))
      profiles.set(
        wallet,
        api<{ avatars: Profile[] }>(`/profiles/${wallet}`)
          .then((p) => p.avatars?.[0] || null)
          .catch(() => null),
      );
    profiles.get(wallet)!.then((p) => {
      if (active) setProfile(p);
    });
    return () => {
      active = false;
    };
  }, [wallet]);
  return profile;
}
export function Avatar({ wallet = "" }: { wallet?: string }) {
  const p = useProfile(wallet);
  if (!/^0x[0-9a-f]{40}$/i.test(wallet)) return <span className="avatar" aria-label="Unknown member"><Picture fallback="?" /></span>;
  return (
    <button className="avatar profile-trigger" aria-label={`View profile ${p?.name || shortWallet(wallet)}`} onClick={() => window.dispatchEvent(new CustomEvent("social:profile", { detail: wallet }))}>
      <Picture src={p?.avatar?.snapshots?.face256} fallback={wallet.slice(2, 4)} />
    </button>
  );
}
export function Name({ wallet }: { wallet: string }) {
  const p = useProfile(wallet);
  return <>{p?.name || shortWallet(wallet)}</>;
}
