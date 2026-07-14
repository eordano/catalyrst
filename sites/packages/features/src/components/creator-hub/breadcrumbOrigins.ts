import type { CreatorHubNavId } from "@ui/creatorhub/frames/CreatorHubChrome";

export type BreadcrumbOrigin = {
  to: string;
  label: string;
  active: CreatorHubNavId;
};

export const ORIGINS: Record<string, BreadcrumbOrigin> = {
  scenes: { to: "/create/scenes", label: "Back to scenes", active: "scenes" },
  templates: { to: "/create/templates", label: "Back to templates", active: "templates" },
  home: { to: "/create", label: "Back to Creator Hub", active: "home" },
  manage: { to: "/creator-hub/manage", label: "Back to Worlds", active: "manage" },
  activity: {
    to: "/creator-hub/activity",
    label: "Back to Activity",
    active: "activity",
  },
  operator: {
    to: "/creator-hub/operator-metrics",
    label: "Back to network metrics",
    active: "metrics",
  },
};

export function resolveBreadcrumbOrigin(key?: string | null): BreadcrumbOrigin {
  return (key && ORIGINS[key]) || ORIGINS.scenes;
}
