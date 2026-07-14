import { redirect } from "react-router";

import { ensureSid, serializeSidCookie } from "@core/lib/experiments/assign";
import { track } from "@core/lib/telemetry/track";

export const REDIRECT_EVENT = "creator_builder_redirect";

export const REDIRECT_STORY = "creator-hub/integration-redirect-item-publish-curate";

const PERMANENT = 308;

export type RedirectRule = {
  from: string;
  to: string;
  carry: readonly string[];
  param?: { name: string; queryKey?: string };
};

export const REDIRECTS = {
  "item-editor": {
    from: "item-editor",
    to: "/create/wearables/item-editor",
    carry: ["collection", "item", "step", "variant"],
  },
  "publish-collection": {
    from: "publish-collection",
    to: "/create/wearables/publish",
    carry: ["collection", "step", "variant"],
  },
  curation: {
    from: "curation",
    to: "/create/curate",
    carry: ["step", "id", "decision", "status", "type", "assignee", "committee", "variant"],
  },
  "collection-detail": {
    from: "collection-detail",
    to: "/create/wearables/collections/:id",
    carry: ["tab", "variant"],
    param: { name: "id" },
  },
  "item-detail": {
    from: "item-detail",
    to: "/create/wearables/item-editor",
    carry: ["variant"],
    param: { name: "id", queryKey: "item" },
  },
} as const satisfies Record<string, RedirectRule>;

export type RedirectKey = keyof typeof REDIRECTS;

export function buildDestination(
  rule: RedirectRule,
  url: URL,
  params: Record<string, string | undefined>,
): string {
  const out = new URLSearchParams();

  for (const key of rule.carry) {
    const v = url.searchParams.get(key);
    if (v != null && v !== "") out.set(key, v);
  }

  let to = rule.to;
  if (rule.param) {
    const raw = params[rule.param.name];
    if (raw != null && raw !== "") {
      if (rule.param.queryKey) out.set(rule.param.queryKey, raw);
      else to = to.replace(`:${rule.param.name}`, encodeURIComponent(raw));
    } else if (!rule.param.queryKey) {
      to = "/create/wearables";
    }
  }

  const qs = out.toString();
  return qs ? `${to}?${qs}` : to;
}

export function builderRedirect(
  key: RedirectKey,
  request: Request,
  params: Record<string, string | undefined> = {},
): Response {
  const rule = REDIRECTS[key];
  const url = new URL(request.url);
  const to = buildDestination(rule, url, params);

  const { sid, created } = ensureSid(request);

  track(
    REDIRECT_EVENT,
    {
      from: rule.from,
      to,
      fromPath: url.pathname + url.search,
    },
    { sid, story: REDIRECT_STORY },
  );

  const headers = new Headers();
  headers.set("Location", to);
  if (created) headers.append("Set-Cookie", serializeSidCookie(sid));

  return redirect(to, { status: PERMANENT, headers });
}
