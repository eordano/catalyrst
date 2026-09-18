import { loadCreateScreen } from "@data/lib/screens/create.server";

export async function loader({ request }: { request: Request }) {
  const { data, serverTiming } = await loadCreateScreen(request);
  return Response.json({ version: 1, ...data }, {
    headers: { "Cache-Control": "private, no-store", "Server-Timing": serverTiming },
  });
}
