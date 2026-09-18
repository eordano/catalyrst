export const FEATURE_FLAGS = [
  { id: "2026-09-scene-feedback-tip", name: "Feedback with an optional tip", description: "Combine scene feedback and tips in one form. Off keeps separate feedback and tip forms.", defaultEnabled: false },
  { id: "2026-09-interaction-flash", name: "Interaction feedback", description: "Highlight an activated scene interaction prompt for 100 milliseconds.", defaultEnabled: true },
  { id: "2026-09-connection-status", name: "Connection status", description: "Show the connection-status button and performance diagnostics in the Explorer HUD.", defaultEnabled: true },
  { id: "2026-09-unity-shaders", name: "Unity-style shaders", description: "Use the new scene and avatar shaders for rendering closer to the Unity Explorer.", defaultEnabled: true },
] as const;

export type FeatureFlagId = typeof FEATURE_FLAGS[number]["id"];
export type FlagOverride = "default" | "enabled" | "disabled";
export const flagStorageKey = (id: FeatureFlagId) => `dcl.feature.${id}`;

export function flagState(id: FeatureFlagId): { override: FlagOverride; source: "url" | "browser" | "default"; enabled: boolean } {
  const fallback = FEATURE_FLAGS.find(flag => flag.id === id)!.defaultEnabled;
  if (typeof window !== "undefined") {
    const query = new URLSearchParams(window.location.search);
    if (query.has(id)) {
      const enabled = ["", "1", "true"].includes(query.get(id) ?? "");
      return { override: enabled ? "enabled" : "disabled", source: "url", enabled };
    }
    try {
      const saved = window.localStorage.getItem(flagStorageKey(id));
      if (saved === "1" || saved === "0") return { override: saved === "1" ? "enabled" : "disabled", source: "browser", enabled: saved === "1" };
    } catch {}
  }
  return { override: "default", source: "default", enabled: fallback };
}

export function saveFlagOverride(id: FeatureFlagId, value: FlagOverride): void {
  if (value === "default") window.localStorage.removeItem(flagStorageKey(id));
  else window.localStorage.setItem(flagStorageKey(id), value === "enabled" ? "1" : "0");
  const url = new URL(window.location.href);
  url.searchParams.delete(id);
  window.history.replaceState(window.history.state, "", url);
}

export const initialFeatureFlags = Object.fromEntries(FEATURE_FLAGS.map(flag => [flag.id, flagState(flag.id).enabled])) as Record<FeatureFlagId, boolean>;
