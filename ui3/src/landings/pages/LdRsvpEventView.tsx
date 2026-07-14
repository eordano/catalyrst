import type { ReactNode } from "react";

import SitesChrome from "../../web/frames/SitesChrome";

type LdRsvpEventViewProps = {
  children?: ReactNode;
};

export default function LdRsvpEventView({
  children = undefined,
}: LdRsvpEventViewProps) {
  return (
    <SitesChrome active="whatson">
      <main className="landings-rsvp-route">{children}</main>
    </SitesChrome>
  );
}
