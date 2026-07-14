import { redirect } from "react-router";

import type { Route } from "./+types/legal.$doc";

export async function loader({ params }: Route.LoaderArgs) {
  const doc = params.doc ?? "";
  throw redirect(doc ? `/${doc}` : "/", 301);
}

export default function LegalDocRedirect() {
  return null;
}
