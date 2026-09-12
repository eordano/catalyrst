import type { ReactNode } from "react";

import "../../web/pages/stwhatsonadminpendingevents.css";
import "./placesmoderation.css";

export type AdPlacesModerationPageProps = {
  nav?: ReactNode;
  children?: ReactNode;
};

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
