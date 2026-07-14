import type { ActionFunctionArgs } from "react-router";

import { submitSuspension } from "@data/lib/catalyst/admin/community-moderation.server";

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

export async function action({ request }: ActionFunctionArgs) {
  if (request.method !== "POST") {
    return json({ error: "Method not allowed." }, 405);
  }

  let body: unknown = null;
  try {
    body = await request.json();
  } catch {
    return json({ error: "Invalid JSON body." }, 400);
  }

  const out = await submitSuspension(body, { signal: request.signal });
  if (out.ok) return json(out.result);
  return json({ error: out.error }, out.status);
}
