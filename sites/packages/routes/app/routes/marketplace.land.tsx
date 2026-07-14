import { redirect } from "react-router";

export async function loader() {
  return redirect("/shop", 308);
}

export default function MarketplaceLandRedirect() {
  return null;
}
