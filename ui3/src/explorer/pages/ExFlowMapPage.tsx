import type { ComponentType } from "react";

import FlowMapPage, { Hourglass } from "../../flowmap/FlowMapPage";
import type { LinkComponentProps } from "../../flowmap/FlowMapPage";
import { ASCII_SOURCE, MACHINE_PATHS, SECTIONS, STATS } from "./exflowmapdata";

type ExFlowMapPageProps = {
  LinkComponent?: ComponentType<LinkComponentProps>;
};

export default function ExFlowMapPage({ LinkComponent = undefined }: ExFlowMapPageProps) {
  return (
    <FlowMapPage
      LinkComponent={LinkComponent}
      sections={SECTIONS}
      stats={STATS}
      ascii={ASCII_SOURCE}
      copy={{
        backHref: "/play/",
        backLabel: "\u{2190} Play",
        backPlain: true,
        crumb: "sites / explorer / map",
        lede: (
          <>
            The complete click/state sitemap of the Explorer &#x2014; the catalyst.example.com/play web
            client. Nodes are URLs and component states; every edge is{" "}
            <strong>one user click</strong> &#x2014; or a{" "}
            <strong className="fm-lede-load">
              <Hourglass className="fm-inline-hg" /> load
            </strong>{" "}
            whenever the product makes you wait more than 100&thinsp;ms: engine boot,
            signed deploys, teleports. Hover or focus anything to light its full path.
            Surveyed against the live overlay, the engine page, and the engine
            crates.
          </>
        ),
        machineTitle: (m) => MACHINE_PATHS[m] ?? m,
        asciiLabel: "Explorer sitemap, ascii original",
        honesty: (
          <>
            Source of truth: the overlay sources under <code>catalyrst/ui3/src/app</code>,{" "}
            <code>catalyrst/ui3/src/explorer</code> and <code>catalyrst/ui3/src/overlay</code>, plus the
            engine page <code>bevy-explorer/deploy/web/index.html</code>; every bridge
            claim checked against <code>catalyrst/ui3/src/generated/bridge/BridgeAction.ts</code>.
            Honesty notes: the jump-in loading overlay dismisses on a{" "}
            <strong>3.5&thinsp;s timer</strong>, not on arrival confirmation; the chat
            dock is <strong>Nearby-only</strong> (no DM or channel tabs on web); the
            login-code modal only appears on an engine push; the fatal-error modal
            covers only overlay render errors &#x2014; the native crash overlay owns engine
            launch and crash cases; friend actions here ride{" "}
            <code>SignRequest</code> (<code>upsert_friendship</code>) while the typed{" "}
            <code>friends.request</code> bridge action is dispatched only by the sites
            friend-request wizard (<code>/bevy-overlay/friend-request</code>). Scene-lifecycle
            notes (all constants quoted from{" "}
            <code>bevy-explorer/crates</code>): a broken scene shows no banner on web &#x2014;
            the &#x201C;not responding&#x201D; countdown streams engine-side only; as coded the
            unload check compares against the 15&thinsp;m <em>extra</em> distance
            alone, leaving the hysteresis band empty so scenes despawn right at the
            50&thinsp;m load radius; a scene-pack hash mismatch silently falls back to
            per-file fetch (log-only); imposter states surface only behind{" "}
            <code>/debug_imposters</code>.
            Deliberately excluded: <code>/bevy-overlay/*</code> (native-client + dev
            harness), the engine page's dev launcher form, and <code>?preview</code>{" "}
            editor mode. Sibling map:{" "}
            <a href="/creator-hub/map">the Creator Hub</a>.
          </>
        ),
      }}
    />
  );
}
