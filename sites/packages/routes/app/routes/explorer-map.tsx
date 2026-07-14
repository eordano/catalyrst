import { Link } from "react-router";

import ExFlowMapPage from "@ui/explorer/pages/ExFlowMapPage";

export const meta = () => [{ title: "Flow map \u{2014} Explorer | Decentraland" }];

export default function ExplorerMapRoute() {
  return <ExFlowMapPage LinkComponent={Link} />;
}
