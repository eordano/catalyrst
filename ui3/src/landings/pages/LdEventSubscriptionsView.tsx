import type { ComponentType, CSSProperties, ReactNode } from "react";

type LdLinkComponentProps = {
  to: string;
  prefetch?: "intent" | "render" | "none" | "viewport";
  style?: CSSProperties;
  children?: ReactNode;
};

type LdEventSubscriptionsViewProps = {
  signedOut?: boolean;
  LinkComponent?: ComponentType<LdLinkComponentProps>;
  children?: ReactNode;
};

export default function LdEventSubscriptionsView({
  signedOut = false,
  LinkComponent = undefined,
  children = undefined,
}: LdEventSubscriptionsViewProps) {
  return (
    <main className="landings-event-subscriptions">
      {signedOut ? (
        <div
          className="landings-event-subscriptions__signed-out"
          role="status"
          style={signedOutStyle}
        >
          <strong>You&apos;re signed out.</strong> Sign in to load and manage
          your real email notification settings. Browse{" "}
          {LinkComponent ? (
            <LinkComponent prefetch="intent" to="/whats-on" style={signedOutLinkStyle}>
              what&apos;s on
            </LinkComponent>
          ) : (
            <a href="/whats-on" style={signedOutLinkStyle}>
              what&apos;s on
            </a>
          )}{" "}
          while you decide.
        </div>
      ) : null}

      {children}
    </main>
  );
}

const signedOutStyle: CSSProperties = {
  margin: "12px 16px",
  padding: "12px 16px",
  borderRadius: 12,
  background: "var(--fill-2)",
  border: "1px solid var(--fill-4)",
  color: "var(--ink-85)",
  fontSize: "var(--fs-14)",
  lineHeight: 1.5,
};

const signedOutLinkStyle: CSSProperties = {
  color: "var(--brand)",
  fontWeight: 600,
  textDecoration: "none",
};
