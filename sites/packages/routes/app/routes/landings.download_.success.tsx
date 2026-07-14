import { redirect } from "react-router";

export async function loader() {
  return redirect("/play/");
}

export default function LandingsDownloadSuccessRedirect() {
  return null;
}
