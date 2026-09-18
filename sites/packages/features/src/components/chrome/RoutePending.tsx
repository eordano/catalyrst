import { useNavigation, useRevalidator } from "react-router";
import Spinner from "@ui/atoms/Spinner";
import "./routepending.css";

export default function RoutePending() {
  const navigation = useNavigation();
  const revalidator = useRevalidator();
  const pending = navigation.state !== "idle" || revalidator.state !== "idle";
  if (!pending) return null;
  const label = navigation.state === "submitting" ? "Saving changes\u2026"
    : navigation.state === "loading" ? "Loading page\u2026" : "Refreshing content\u2026";
  return <div className="route-pending" role="status" aria-live="polite">
    <Spinner size={20} aria-hidden />
    <span>{label}</span>
  </div>;
}
