import type { ReactNode } from "react";

import "../admin.css";

type AdPlacesModerationPageProps = {
  nav?: ReactNode;
  children?: ReactNode;
};

export default function AdPlacesModerationPage({
  nav = undefined,
  children = undefined,
}: AdPlacesModerationPageProps) {
  return (
    <main className="adm">
      {nav && (
        <nav className="adm__nav" aria-label="Admin consoles">
          {nav}
        </nav>
      )}
      <div className="adm__page">
        <div className="adm__inner">{children}</div>
      </div>
    </main>
  );
}
