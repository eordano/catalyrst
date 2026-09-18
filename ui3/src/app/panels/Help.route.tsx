import { useSearchParams } from "react-router";
import Help from "../../explorer/pages/Help";

export default function HelpPanel() {
  const [params] = useSearchParams();
  return <Help key={params.get("section")} initialSection={params.get("section") ?? "controls"} />;
}
