import { loadShopScreen } from "@data/lib/screens/shop.server";

export async function loader({ request }: { request: Request }) {
  const { data, serverTiming } = await loadShopScreen(new URL(request.url).searchParams, request.signal);
  return Response.json({ version: 1, ...data }, {
    headers: { "Cache-Control": "no-store", "Server-Timing": serverTiming },
  });
}
