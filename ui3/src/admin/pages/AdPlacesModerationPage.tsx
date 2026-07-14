import type { ReactNode } from "react";

import "../../web/pages/stwhatsonadminpendingevents.css";
import "./placesmoderation.css";

export type AdPlacesModerationPageProps = {
  nav?: ReactNode;
  children?: ReactNode;
};

/**
 * Frame only.
 *
 * The old `degraded` banner said "Showing an empty queue until an admin bearer
 * is provisioned" while the console below it still rendered all its controls.
 * An empty queue and a queue you are not allowed to read are not the same
 * thing. The route now renders `AdControlNotice` *instead of* the console
 * whenever the server's answer is not `ok`, so there is nothing left for this
 * frame to qualify.
 */
export default function AdPlacesModerationPage({
  nav = undefined,
  children = undefined,
}: AdPlacesModerationPageProps) {
  return (
    <main className="admin-places-moderation-route">
      <nav className="admin-places-moderation-route__nav" aria-label="Admin consoles">
        {nav}
      </nav>

      {children}
    </main>
  );
}
