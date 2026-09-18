import { redirect } from "react-router";

export function loader({ request }: { request: Request }) {
  return redirect(`/whats-on${new URL(request.url).search}`);
}
