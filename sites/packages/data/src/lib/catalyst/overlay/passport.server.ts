import {
  fetchProfile,
  mapProfile,
  normalizeAddress,
  emptyProfile,
  type ProfileVM,
} from "./profile";
import {
  fetchBadgeCategories,
  fetchUserBadges,
  fetchUserPhotos,
  type BadgeData,
  type GalleryImage,
} from "./passport";

export type PassportEquippedItem = {
  name: string;
  rarity: string;
  category: string;
  cat: string;
};
export type PassportLink = { title: string; url: string };
export type PassportInfoField = { key: string; label: string; value: string; icon: string };

export type PassportProfile = {
  address: string;
  name: string;
  tag: string;
  hasClaimedName: boolean;
  nameColor: string;
  description: string;
  links: PassportLink[];
  info: PassportInfoField[];
  equipped: PassportEquippedItem[];
};

export type PassportBadgeMedallion = {
  id: string;
  name: string;
  category: string;
  tier: string;
  completedAt: string | null;
  tint: string;
  shape: string;
};
export type PassportBadgeCard = {
  id: string;
  name: string;
  tier: string;
  unlocked: boolean;
  isNew: boolean;
  completedAt: string | null;
};
export type PassportBadgeSection = {
  id: string;
  label: string;
  badges: PassportBadgeCard[];
};
/**
 * `unavailable` is the reason the badge read failed, and is the only thing that
 * separates "this player has earned no badges" from "we never got an answer" --
 * both used to arrive here as an empty `earned`.
 *
 * `categories` is null when the category list itself was not read; an empty
 * array would claim this node publishes no badge categories.
 */
export type PassportBadges = {
  categories: string[] | null;
  earned: PassportBadgeMedallion[];
  sections: PassportBadgeSection[];
  unavailable: string | null;
};

export type PassportPhotoPerson = { name: string; tag: string; wearables: string[] };
export type PassportPhoto = {
  id: string;
  url: string;
  thumbnailUrl: string;
  dateTime: string;
  hue: number;
  place: { name: string; x: string; y: string } | null;
  people: PassportPhotoPerson[];
};

export type PassportData = {
  profile: PassportProfile;
  badges: PassportBadges;
  photos: PassportPhoto[];
  /** Set when the photo read failed. `photos` is then empty and means nothing. */
  photosUnavailable: string | null;
  /** True only when a profile was actually read and carried nothing. */
  profileEmpty: boolean;
  /** Set when the profile read failed; the fields below are placeholders. */
  profileUnavailable: string | null;
  usedFixture: boolean;
};

const MEDALLION_TINTS = ["#e23a6a", "#8a5bd6", "#3f8fd0", "#ffb04a", "#3fb27f", "#d65bff"];
const MEDALLION_SHAPES = ["x", "gem", "card", "cone"];

function tagFromAddress(addr: string): string {
  const clean = addr.replace(/^0x/i, "");
  return "#" + (clean.slice(-4) || "0000");
}

function profileFromLive(vm: ProfileVM, wearables: string[]): PassportProfile {
  const equipped: PassportEquippedItem[] = wearables.slice(0, 8).map((urn) => {
    const parts = urn.split(":");
    const cat = parts.length > 3 ? parts[3] : "wearable";
    const last = parts[parts.length - 1] || urn;
    const name = last.replace(/[-_]/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
    return { name, rarity: "BASE", category: cat, cat: "\u{2726}" };
  });
  return {
    address: vm.address,
    name: vm.name,
    tag: tagFromAddress(vm.address),
    hasClaimedName: vm.hasClaimedName,
    nameColor: vm.nameColor,
    description: vm.bio,
    links: vm.links,
    info: vm.info,
    equipped,
  };
}

function emptyPassportProfile(addr: string): PassportProfile {
  return profileFromLive(emptyProfile(addr), []);
}

function medallionsFromLive(achieved: BadgeData[]): PassportBadgeMedallion[] {
  return achieved.slice(0, 6).map((b, i) => ({
    id: b.id || `badge-${i}`,
    name: b.name || b.id,
    category: b.category ?? "",
    tier: b.progress?.lastCompletedTierName ?? "",
    completedAt: b.completedAt ?? null,
    tint: MEDALLION_TINTS[i % MEDALLION_TINTS.length],
    shape: MEDALLION_SHAPES[i % MEDALLION_SHAPES.length],
  }));
}

function photosFromLive(images: GalleryImage[]): PassportPhoto[] {
  return images.map((img, i) => ({
    id: img.id,
    url: img.url,
    thumbnailUrl: img.thumbnailUrl,
    dateTime: img.dateTime,
    hue: (i * 47 + 200) % 360,
    place: null,
    people: [],
  }));
}

function reasonOf(err: unknown): string {
  const message = (err as Error)?.message;
  return message && message.trim() ? message : "the request failed";
}

export async function loadPassport(
  address: string,
  opts: { signal?: AbortSignal } = {},
): Promise<PassportData> {
  const addr = normalizeAddress(address);

  const [profileRes, badgesRes, photosRes] = await Promise.all([
    (async (): Promise<{
      profile: PassportProfile;
      profileEmpty: boolean;
      unavailable: string | null;
    }> => {
      let profile = emptyPassportProfile(addr);
      let profileEmpty = false;
      let unavailable: string | null = null;
      try {
        const avatar = await fetchProfile(addr, { signal: opts.signal });
        if (avatar && (avatar.name || (avatar.avatar?.wearables?.length ?? 0) > 0)) {
          profile = profileFromLive(
            mapProfile(avatar, addr),
            avatar.avatar?.wearables ?? [],
          );
        } else {
          profileEmpty = true;
        }
      } catch (err) {
        unavailable = reasonOf(err);
      }
      return { profile, profileEmpty, unavailable };
    })(),
    (async (): Promise<PassportBadges> => {
      const [cats, user] = await Promise.allSettled([
        fetchBadgeCategories({ signal: opts.signal }),
        fetchUserBadges(addr, { signal: opts.signal }),
      ]);
      const categories = cats.status === "fulfilled" ? cats.value : null;
      if (user.status !== "fulfilled") {
        return {
          categories,
          earned: [],
          sections: [],
          unavailable: reasonOf(user.reason),
        };
      }
      const achieved: BadgeData[] = user.value.achieved;
      return {
        categories,
        earned: medallionsFromLive(achieved),
        sections: [],
        unavailable: null,
      };
    })(),
    (async (): Promise<{ photos: PassportPhoto[]; unavailable: string | null }> => {
      let photos: PassportPhoto[] = [];
      let unavailable: string | null = null;
      try {
        photos = photosFromLive(await fetchUserPhotos(addr, { signal: opts.signal }));
      } catch (err) {
        unavailable = reasonOf(err);
      }
      return { photos, unavailable };
    })(),
  ]);

  return {
    profile: profileRes.profile,
    badges: badgesRes,
    photos: photosRes.photos,
    photosUnavailable: photosRes.unavailable,
    profileEmpty: profileRes.profileEmpty,
    profileUnavailable: profileRes.unavailable,
    usedFixture: false,
  };
}
