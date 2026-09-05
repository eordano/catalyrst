import { useEffect, useRef } from "react";
import { Link } from "react-router";

import AdminSystemsPage from "@ui/admin/pages/AdminSystemsPage";
import type { Panel } from "@ui/admin/pages/AdminSystemsTypes";
import SitesChrome from "@ui/web/frames/SitesChrome";

import { readVerifiedWallet } from "@core/lib/experiments/assign";
import { isCommitteeMember } from "@data/lib/catalyst/creator-hub/committee-membership.server";
import type { ControlResult } from "@data/lib/catalyst/admin/availability";
import {
  loadSystemStatus,
  deployedSurfaces,
} from "@data/lib/catalyst/admin/system-status.server";
import { loadRecentDeployments } from "@data/lib/catalyst/admin/latest-actions.server";
import { loadExperimentsStatus } from "@data/lib/catalyst/admin/experiments-status.server";
import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/admin._index";

type AuthorizedData = Extract<Route.ComponentProps["loaderData"], { authorized: true }>;

function toPanel<T>(r: ControlResult<T>): Panel<T> {
  return r.ok ? { ok: true, data: r.data } : { ok: false, message: r.message, fix: r.fix };
}

/**
 * Committee membership is read through catalyrst-builder's curation endpoint,
 * which is itself gated: with no CATALYRST_BUILDER_ADMIN_TOKEN configured the
 * read fails closed and every wallet reads as non-member. ADMIN_WALLETS is the
 * operator escape hatch -- a comma-separated allowlist checked in addition to
 * committee membership, so this page stays reachable on a node that has not
 * wired the builder token.
 */
function isAllowlistedWallet(wallet: string): boolean {
  const raw = process.env.ADMIN_WALLETS;
  if (!raw) return false;
  const set = new Set(
    raw
      .split(",")
      .map((w) => w.trim().toLowerCase())
      .filter(Boolean),
  );
  return set.has(wallet.toLowerCase());
}

export async function loader({ request }: Route.LoaderArgs) {
  const wallet = readVerifiedWallet(request);
  const authorized = wallet
    ? isAllowlistedWallet(wallet) ||
      (await isCommitteeMember(wallet, request.signal).catch(() => false))
    : false;

  if (!authorized) {
    return { authorized: false as const };
  }

  const { sid, wrap } = sidLoader(request);

  const [systems, actions, experiments] = await Promise.all([
    loadSystemStatus(),
    loadRecentDeployments(20, request.signal),
    loadExperimentsStatus(request.signal),
  ]);

  return wrap({
    authorized: true as const,
    sid,
    now: Date.now(),
    systems: toPanel(systems),
    links: deployedSurfaces(),
    actions: toPanel(actions),
    experiments: toPanel(experiments),
  });
}

export default function AdminIndexRoute({ loaderData }: Route.ComponentProps) {
  if (!loaderData.authorized) {
    return (
      <SitesChrome>
        <main style={{ maxWidth: 640, margin: "0 auto", padding: "48px 20px" }}>
          <h1>Operations</h1>
          <p>
            This page is restricted to curation-committee members. Connect a
            committee wallet to view system status.
          </p>
        </main>
      </SitesChrome>
    );
  }

  return <AdminOpsView data={loaderData} />;
}

function AdminOpsView({ data }: { data: AuthorizedData }) {
  const probes = data.systems.ok ? data.systems.data.probes : [];
  const units = data.systems.ok ? data.systems.data.units : [];
  const probesOk = probes.filter((p) => p.ok).length;
  const unitsActive = units.filter((u) => u.active_state === "active").length;
  const experiments = data.experiments.ok
    ? data.experiments.data.readable.length
    : 0;
  const deployments = data.actions.ok ? data.actions.data.length : 0;

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track(
      "admin_ops_viewed",
      {
        probes_ok: probesOk,
        probes_total: probes.length,
        units_active: unitsActive,
        units_total: units.length,
        experiments,
        deployments,
      },
      { sid: data.sid },
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data.sid]);

  return (
    <SitesChrome>
      <AdminSystemsPage
        systems={data.systems}
        links={data.links}
        actions={data.actions}
        experiments={data.experiments}
        now={data.now}
        LinkComponent={Link}
      />
    </SitesChrome>
  );
}
