import { useEffect, useRef, useState } from "react";

import GvTransparency from "@ui/governance/pages/GvTransparency";

import { loadTransparencyData, type MonthlyTotal } from "@data/lib/catalyst/governance/transparency";
import {
  fetchAuthorProfiles,
  type AuthorProfile,
} from "@data/lib/catalyst/governance/index";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/governance.transparency";
import type { AgentMarkdownHandle } from "@data/lib/agent/markdown";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const handle = { agentMarkdown: "transparency" } satisfies AgentMarkdownHandle;

const STORY: StoryId = "governance/transparency";

const DASHBOARD_HREF = "https://governance.decentraland.org/transparency";

type ResolvedMember = {
  name: string;
  address: string;
  addressShort: string;
  face: string | null;
  hue: number;
};
type ResolvedCommittee = {
  name: string;
  description: string;
  members: ResolvedMember[];
};

const DEFAULT_ASSIGNMENT: Assignment = {
  variant: "with-transparency",
  flags: { showTransparency: true },
  experimentKey: "gv_transparency_page",
};

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    DEFAULT_ASSIGNMENT,
  );

  const transparency = await loadTransparencyData({ signal: request.signal });

  const memberAddresses = [
    ...new Set(
      transparency.committees.flatMap((c) => c.members.map((m) => m.address)),
    ),
  ];
  let profiles: Record<string, AuthorProfile> | null = null;
  try {
    profiles = await fetchAuthorProfiles(memberAddresses, {
      signal: request.signal,
    });
  } catch {
    profiles = null;
  }

  const committees: ResolvedCommittee[] = transparency.committees.map((c) => ({
    name: c.name,
    description: c.description,
    members: c.members.map((m) => {
      const resolved = profiles?.[m.address]?.name?.trim();
      return {
        name: resolved || m.addressShort,
        address: m.address,
        addressShort: m.addressShort,
        face: profiles?.[m.address]?.face ?? null,
        hue: m.hue,
      };
    }),
  }));

  const payload = { sid, assignment, transparency, committees };

  return wrap(payload);
}

export default function GovernanceTransparency({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const t = d.transparency;
  const committees =
    d.committees ??
    t.committees.map((c) => ({
      name: c.name,
      description: c.description,
      members: c.members.map((m) => ({
        name: m.name || m.addressShort,
        address: m.address,
        addressShort: m.addressShort,
        face: null,
        hue: m.hue,
      })),
    }));

  const ctx = {
    sid: d.sid,
    story: STORY,
    variant: d.assignment.variant,
    experimentKey: d.assignment.experimentKey,
  };

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track(
      "gv_transparency_viewed",
      {
        source: t.source,
        committees: t.committees.length,
      },
      ctx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.sid]);

  function onDashboardClick() {
    track("gv_transparency_dashboard_clicked", { href: DASHBOARD_HREF }, ctx);
  }

  function onBreakdownToggle(card: "income" | "expenses", open: boolean) {
    if (!open) return;
    track("gv_transparency_committee_expanded", { card }, ctx);
  }

  return (
    <div className="governance-transparency-route">
      <GvTransparency
        committees={
          committees as React.ComponentProps<
            typeof GvTransparency
          >["committees"]
        }
      />
    </div>
  );
}

type FooterProps = {
  vestingHref: string;
  income: MonthlyTotal;
  expenses: MonthlyTotal;
  onDashboardClick: () => void;
  onBreakdownToggle: (card: "income" | "expenses", open: boolean) => void;
};

function usd(n: number): string {
  return "$" + Math.round(n).toLocaleString("en-US");
}

function InstrumentedFooter({
  vestingHref,
  income,
  expenses,
  onDashboardClick,
  onBreakdownToggle,
}: FooterProps) {
  return (
    <div
      style={{
        maxWidth: 1180,
        margin: "0 auto",
        padding: "8px 24px 56px",
        display: "flex",
        flexDirection: "column",
        gap: 16,
        color: "rgba(22,22,22,0.86)",
      }}
    >
      <div>
        <a
          href={vestingHref}
          target="_blank"
          rel="noreferrer"
          onClick={onDashboardClick}
          style={{
            display: "inline-block",
            padding: "11px 20px",
            borderRadius: 10,
            background: "var(--brand-cta)",
            color: "#fff",
            fontWeight: 600,
            textDecoration: "none",
          }}
        >
          Open the full Transparency Dashboard {"\u{2192}"}
        </a>
      </div>

      <BreakdownDetails
        card="income"
        label="View income breakdown"
        total={income}
        onToggle={onBreakdownToggle}
      />
      <BreakdownDetails
        card="expenses"
        label="View expenses breakdown"
        total={expenses}
        onToggle={onBreakdownToggle}
      />
    </div>
  );
}

type BreakdownProps = {
  card: "income" | "expenses";
  label: string;
  total: MonthlyTotal;
  onToggle: (card: "income" | "expenses", open: boolean) => void;
};

function BreakdownDetails({ card, label, total, onToggle }: BreakdownProps) {
  const wasOpen = useRef(false);
  const [, force] = useState(0);

  return (
    <details
      style={{
        border: "1px solid rgba(22,22,22,0.12)",
        borderRadius: 12,
        padding: "12px 16px",
        background: "#fff",
      }}
      onToggle={(e) => {
        const open = (e.currentTarget as HTMLDetailsElement).open;
        if (open && !wasOpen.current) onToggle(card, true);
        wasOpen.current = open;
        force((n) => n + 1);
      }}
    >
      <summary style={{ cursor: "pointer", fontWeight: 600 }}>
        {label} {"\u{2014}"} {usd(total.total)}{" "}
        <span style={{ color: "rgba(22,22,22,0.5)", fontWeight: 400 }}>
          ({total.previous >= 0 ? "+" : ""}
          {total.previous}% vs prev 30d)
        </span>
      </summary>
      <ul style={{ margin: "10px 0 0", paddingLeft: 18, lineHeight: 1.7 }}>
        {total.details.map((it) => (
          <li key={it.name}>
            <strong>{it.name}</strong> {"\u{2014}"} {usd(it.value)}
            <span style={{ color: "rgba(22,22,22,0.55)" }}> {"\u{B7}"} {it.description}</span>
          </li>
        ))}
      </ul>
    </details>
  );
}
