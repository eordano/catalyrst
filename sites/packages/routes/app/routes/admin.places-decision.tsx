import type { ActionFunctionArgs } from "react-router";

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

const CALLER_IS_NOT_AUTHENTICATED =
  "Places moderation writes are disabled: this endpoint does not yet verify who is calling, " +
  "and it holds a privileged token. Enabling it without a caller check would let any " +
  "anonymous request act as the places administrator.";

export async function action({ request }: ActionFunctionArgs) {
  if (request.method !== "POST") {
    return json({ error: "Method not allowed." }, 405);
  }

  return json(
    {
      error: CALLER_IS_NOT_AUTHENTICATED,
      reason: "caller-not-authenticated",
      serverCheck: "catalyrst-places/src/auth.rs:88-100 (reachable, but by anyone)",
    },
    503,
  );
}
