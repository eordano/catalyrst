type QueryKeyParams = Record<string, unknown>;

export const qk = {
  profile: (addr?: string | null) => ["profile", addr],
  badges: (addr?: string | null) => ["badges", addr],
  photos: (addr?: string | null) => ["photos", addr],

  wearables: (addr: string) => ["wearables", addr],
  emotes: (addr: string) => ["emotes", addr],
  outfits: (addr: string) => ["outfits", addr],

  places: (params?: QueryKeyParams) => ["places", params ?? {}],
  place: (id?: string | null) => ["place", id],
  categories: () => ["categories"],
  worlds: (params?: QueryKeyParams) => ["worlds", params ?? {}],

  events: (params?: QueryKeyParams) => ["events", params ?? {}],
  eventCategories: () => ["event-categories"],

  communities: (params?: QueryKeyParams) => ["communities", params ?? {}],
  community: (id?: string | null) => ["community", id],
  communityMembers: (id?: string | null) => ["community-members", id],
  communityPosts: (id?: string | null) => ["community-posts", id],
  communityPlaces: (id?: string | null) => ["community-places", id],

  friends: (addr?: string | null) => ["friends", addr],
  notifications: (addr?: string | null) => ["notifications", addr],
};

export const STALE = {
  categories: 60 * 60 * 1000,
  eventCategories: 60 * 60 * 1000,
  profile: 5 * 60 * 1000,
  badges: 5 * 60 * 1000,
  photos: 5 * 60 * 1000,
  wearables: 5 * 60 * 1000,
  emotes: 5 * 60 * 1000,
  outfits: 5 * 60 * 1000,
  places: 30 * 1000,
  place: 30 * 1000,
  worlds: 30 * 1000,
  events: 5 * 60 * 1000,
  communities: 60 * 1000,
  community: 60 * 1000,
  communityMembers: 60 * 1000,
  communityPosts: 60 * 1000,
  communityPlaces: 60 * 1000,
  friends: 60 * 1000,
  notifications: 30 * 1000,
};
