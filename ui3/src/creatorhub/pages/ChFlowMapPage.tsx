import type { ComponentType } from "react";

import FlowMapPage, { Hourglass } from "../../flowmap/FlowMapPage";
import type { LinkComponentProps } from "../../flowmap/FlowMapPage";
import { ASCII_SOURCE, SECTIONS, STATS } from "./chflowmapdata";

type ChFlowMapPageProps = {
  LinkComponent?: ComponentType<LinkComponentProps>;
};

export default function ChFlowMapPage({ LinkComponent = undefined }: ChFlowMapPageProps) {
  return (
    <FlowMapPage
      LinkComponent={LinkComponent}
      sections={SECTIONS}
      stats={STATS}
      ascii={ASCII_SOURCE}
      copy={{
        backHref: "/create",
        backLabel: "\u{2190} Creator Hub",
        crumb: "sites / creator-hub / map",
        lede: (
          <>
            The complete click/state sitemap of the Creator Hub. Nodes are routes and
            machine states; every edge is <strong>one user click</strong> &#x2014; or a{" "}
            <strong className="fm-lede-load">
              <Hourglass className="fm-inline-hg" /> load
            </strong>{" "}
            whenever the product makes you wait more than
            100&thinsp;ms: engine boot, folder pickers, signed deploys. Hover or focus
            anything to light its full path. Regenerated against the live route tree.
          </>
        ),
        machineTitle: (m) => `catalyrst/sites/packages/features/src/stories/creator-hub/${m}/machine.ts`,
        asciiLabel: "Creator Hub sitemap, ascii original",
        honesty: (
          <>
            Source of truth: the route files under{" "}
            <code>catalyrst/sites/packages/routes/app/routes/create.*.tsx</code> and{" "}
            <code>catalyrst/sites/packages/routes/app/routes/creator-hub.*.tsx</code> &#xB7; machine states from{" "}
            <code>catalyrst/sites/packages/features/src/stories/creator-hub/*/machine.ts</code>. Redirect shims
            carry no clicks and are listed in the ascii footer. Intentionally absent --
            not implemented: <code>metrics-funnel</code>,{" "}
            <code>integration-create-entry</code>. Publish&nbsp;Pay is a disclosed
            stub; world-permissions commit writes the real ACL; Unpublish is
            real. Sibling map: <a href="/explorer-map">the Explorer client</a>.
          </>
        ),
      }}
    />
  );
}
