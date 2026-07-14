import { redirect } from "react-router";

export async function loader() {
  return redirect("/marketplace", 308);
}

export default function MarketplaceRentLandRedirect() {
  return null;
}
