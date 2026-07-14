import { redirect } from "react-router";

type RouteArgs = {
  request: Request;
  params: Record<string, string | undefined>;
};

function movedTo(request: Request, params: RouteArgs["params"]): string {
  const url = new URL(request.url);
  const splat = (params["*"] ?? "").trim();
  return `/api/creator-hub/drafts${splat ? `/${splat}` : ""}${url.search}`;
}

export async function loader({ request, params }: RouteArgs) {
  return redirect(movedTo(request, params), 308);
}

export async function action({ request, params }: RouteArgs) {
  return redirect(movedTo(request, params), 308);
}
