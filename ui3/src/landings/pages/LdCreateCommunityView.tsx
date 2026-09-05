import type { ComponentType, ReactNode } from "react";

import "./ldcreatecommunityview.css";

type LdLinkComponentProps = {
  className?: string;
  to: string;
  prefetch?: "intent" | "render" | "none" | "viewport";
  "aria-current"?: "page";
  children?: ReactNode;
};

type LdCreateCommunityViewProps = {
  mode?: "create" | "edit";
  source?: "live" | "empty" | "error";
  editHref?: string;
  LinkComponent?: ComponentType<LdLinkComponentProps>;
  children?: ReactNode;
};

export default function LdCreateCommunityView({
  mode = "create",
  source = "empty",
  editHref = "?mode=edit",
  LinkComponent = undefined,
  children = undefined,
}: LdCreateCommunityViewProps) {
  const showFallbackNotice = mode === "edit" && source !== "live";

  const modeLink = (href: string, current: boolean, label: string) =>
    LinkComponent ? (
      <LinkComponent
        to={href}
        prefetch="intent"
        className="create-community-route__modelink"
        aria-current={current ? "page" : undefined}
      >
        {label}
      </LinkComponent>
    ) : (
      <a
        href={href}
        className="create-community-route__modelink"
        aria-current={current ? "page" : undefined}
      >
        {label}
      </a>
    );

  return (
    <main className="create-community-route">
      <nav className="create-community-route__modes" aria-label="Community wizard mode">
        {modeLink("?mode=create", mode === "create", "Create community")}
        {modeLink(editHref, mode === "edit", "Edit community")}
      </nav>
      {showFallbackNotice && (
        <p className="create-community-route__notice" role="status">
          {source === "error"
            ? "Couldn't load that community to edit; starting from a blank draft."
            : "No community selected to edit; starting from a blank draft."}
        </p>
      )}
      {children}
    </main>
  );
}
