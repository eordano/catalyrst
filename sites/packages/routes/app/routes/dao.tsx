import { redirect } from "react-router";

export function loader() {
  return redirect("/governance");
}

export default function DaoRedirect() {
  return null;
}
