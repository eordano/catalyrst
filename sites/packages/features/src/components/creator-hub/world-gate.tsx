import type { ReactNode } from "react";
import { Link } from "react-router";
import { href } from "@core/lib/router/routes";

import EmptyState from "@ui/components/EmptyState";

export type WorldGate =
  | "none"
  | "no-world"
  | "load-failed"
  | "not-found"
  | "no-access";

const MANAGE_LINK = (
  <Link
    to={href("/creator-hub/manage")}
    prefetch="intent"
    className="es__cta es__cta--outline"
  >
    Go to Manage
  </Link>
);

export function WorldGateView({
  gate,
  worldName,
  surface,
  checkingAccess,
  signedIn,
  onSignIn,
}: {
  gate: Exclude<WorldGate, "none">;
  worldName: string;
  surface: "settings" | "permissions";
  checkingAccess: boolean;
  signedIn: boolean;
  onSignIn: () => void;
}): ReactNode {
  const noun = surface === "settings" ? "settings" : "permissions";

  if (gate === "no-world") {
    return (
      <EmptyState
        title="No world selected"
        subtitle={`Select a world from Manage to edit its ${noun}.`}
        actions={MANAGE_LINK}
        icon={undefined}
        variant={undefined}
        tone={undefined}
        actionsGap={undefined}
        style={undefined}
      />
    );
  }

  if (checkingAccess && (gate === "no-access" || gate === "not-found")) {
    return (
      <p role="status" aria-live="polite" style={{ color: "var(--ink-45)" }}>
        Checking your access to {worldName}&#x2026;
      </p>
    );
  }

  if (gate === "load-failed") {
    return (
      <EmptyState
        title={surface === "permissions" ? "Couldn't load permissions" : "Couldn't load world"}
        subtitle={
          surface === "permissions"
            ? `We couldn't reach the worlds service to load permissions for ${worldName}. Try again shortly.`
            : `We couldn't reach the worlds service to load ${worldName}. Try again shortly.`
        }
        actions={MANAGE_LINK}
        tone="error"
        icon={undefined}
        variant={undefined}
        actionsGap={undefined}
        style={undefined}
      />
    );
  }

  if (gate === "not-found") {
    return (
      <EmptyState
        title="World not found"
        subtitle={`There's no world named "${worldName}" \u{2014} the NAME may be unclaimed or the link mistyped. Select one of your worlds from Manage.`}
        actions={MANAGE_LINK}
        icon={undefined}
        variant={undefined}
        tone={undefined}
        actionsGap={undefined}
        style={undefined}
      />
    );
  }

  return (
    <EmptyState
      title="You don't manage this world"
      subtitle={
        signedIn
          ? `${worldName} belongs to a different account, so its ${noun} can't be edited here. Select one of your worlds from Manage.`
          : `Sign in with the account that manages ${worldName}, or select one of your worlds from Manage.`
      }
      actions={
        <>
          {!signedIn ? (
            <button
              type="button"
              className="es__cta"
              onClick={onSignIn}
            >
              Sign in
            </button>
          ) : null}
          {MANAGE_LINK}
        </>
      }
      icon={undefined}
      variant={undefined}
      tone={undefined}
      actionsGap={undefined}
      style={undefined}
    />
  );
}
