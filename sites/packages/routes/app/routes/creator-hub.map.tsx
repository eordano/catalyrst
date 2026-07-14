import { Link } from "react-router";

import ChFlowMapPage from "@ui/creatorhub/pages/ChFlowMapPage";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

export const meta = () => creatorHubMeta("Flow map");

export default function CreatorHubMapRoute() {
  return <ChFlowMapPage LinkComponent={Link} />;
}
