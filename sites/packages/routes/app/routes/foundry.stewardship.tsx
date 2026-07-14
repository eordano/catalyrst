import { useEffect, useRef } from "react";
import { data, useFetcher } from "react-router";

import FdStewardshipPage, {
  type FdAppealVM,
  type FdDecisionVM,
  type FdStewardConsentVM,
} from "@ui/foundry/pages/FdStewardshipPage";

import { FD_UNAVAILABLE, FdPageHead } from "@ui/foundry/components/FdSection";

import "@ui/atoms/button.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdstat.css";
import "@ui/foundry/components/fdtable.css";
import "@ui/foundry/components/fdpersonachip.css";
import "@ui/foundry/pages/fdstewardship.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { consentState } from "@data/lib/foundry/consent.server";
import { FoundryUnavailableError, sidBadge } from "@data/lib/foundry/db.server";
import { actionFailure, clientIp, requireCookie } from "../lib/foundry-action";
import { activeRoles } from "@data/lib/foundry/roles.server";
import {
  APPEAL_LIMITS,
  changeConsent,
  fileAppeal,
  listMyAppeals,
  listMyDecisions,
  listOpenAppeals,
  resolveAppeal,
  withdrawAppeal,
} from "@data/lib/foundry/stewardship.server";
import type { AppealRow, DecisionRow } from "@data/lib/foundry/types";

import type { Route } from "./+types/foundry.stewardship";

export function meta() {
  return [{ title: "Stewardship — The Foundry" }];
}

function toDecisionVM(d: DecisionRow): FdDecisionVM {
  return { kind: d.kind, id: d.id, label: d.label, at: d.at, detail: d.detail };
}

function toAppealVM(a: AppealRow): FdAppealVM {
  return {
    id: a.id,
    subjectKind: a.subjectKind,
    subjectId: a.subjectId,
    subjectLabel: a.subjectLabel,
    body: a.body,
    status: a.status,
    createdAt: a.createdAt,
    resolvedBy: a.resolvedBy,
    resolvedAt: a.resolvedAt,
    resolutionNote: a.resolutionNote,
    ...(a.appellant ? { appellant: a.appellant } : {}),
  };
}

const EMPTY_CONSENT: FdStewardConsentVM = { stewardCode: null, rosterListing: null };

export async function loader({ request }: Route.LoaderArgs) {
  const base = sidLoader(request);
  const sid = base.sid;

  try {
    const [decisions, myAppeals, consent, myRoles] = await Promise.all([
      listMyDecisions(sid),
      listMyAppeals(sid),
      consentState(sid),
      activeRoles(sid),
    ]);
    const isAdmin = myRoles.includes("admin");
    const openAppeals = isAdmin ? await listOpenAppeals() : [];
    const consentVM: FdStewardConsentVM = {
      stewardCode: consent.topics["steward-code"] ?? null,
      rosterListing: consent.topics["roster-listing"] ?? null,
    };
    return base.wrap({
      unavailable: false,
      badge: sidBadge(sid),
      roles: myRoles.length,
      decisions: decisions.map(toDecisionVM),
      myAppeals: myAppeals.map(toAppealVM),
      openAppeals: openAppeals.map(toAppealVM),
      consent: consentVM,
      isAdmin,
      bodyLimit: APPEAL_LIMITS.body,
    });
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    return base.wrap({
      unavailable: true,
      badge: sidBadge(sid),
      roles: 0,
      decisions: [] as FdDecisionVM[],
      myAppeals: [] as FdAppealVM[],
      openAppeals: [] as FdAppealVM[],
      consent: EMPTY_CONSENT,
      isAdmin: false,
      bodyLimit: APPEAL_LIMITS.body,
    });
  }
}

export async function action({ request }: Route.ActionArgs) {
  const base = sidLoader(request);
  const sid = base.sid;
  const ip = clientIp(request);

  let form: FormData;
  try {
    form = await request.formData();
  } catch {
    return data(
      { ok: false, intent: "", error: "Could not read the form." },
      { status: 400 },
    );
  }
  const intent = String(form.get("intent") ?? "");

  try {
    requireCookie(base.created);
    const ctx = { sid: sidBadge(sid) };
    switch (intent) {
      case "consent": {
        const topic = String(form.get("topic") ?? "");
        const state = String(form.get("state") ?? "");
        if (topic !== "steward-code" && topic !== "roster-listing") {
          return data(
            { ok: false, intent, error: "That is not a consent topic here." },
            { status: 400 },
          );
        }
        if (state !== "granted" && state !== "withdrawn") {
          return data(
            { ok: false, intent, error: "Consent is granted or withdrawn, nothing else." },
            { status: 400 },
          );
        }
        await changeConsent({ sid, topic, state, ip });
        track("fd_consent_changed", { state, topic }, ctx);
        return { ok: true, intent, error: null };
      }
      case "file": {
        const subjectKind = String(form.get("subjectKind") ?? "");
        const subjectId = String(form.get("subjectId") ?? "");
        if (
          subjectKind !== "request" &&
          subjectKind !== "role_grant" &&
          subjectKind !== "session_series"
        ) {
          return data(
            { ok: false, intent, error: "That is not an appealable decision." },
            { status: 400 },
          );
        }
        await fileAppeal({
          sid,
          subjectKind,
          subjectId,
          body: String(form.get("body") ?? ""),
          ip,
        });
        track("fd_appeal_filed", { subject_kind: subjectKind }, ctx);
        return { ok: true, intent, error: null };
      }
      case "withdraw": {
        const appealId = String(form.get("appealId") ?? "");
        await withdrawAppeal({ sid, appealId, ip });
        track("fd_appeal_withdrawn", { appeal_id: appealId }, ctx);
        return { ok: true, intent, error: null };
      }
      case "resolve": {
        const appealId = String(form.get("appealId") ?? "");
        const verdict = String(form.get("verdict") ?? "");
        if (verdict !== "upheld" && verdict !== "declined") {
          return data(
            { ok: false, intent, error: "A resolution is upheld or declined, nothing else." },
            { status: 400 },
          );
        }
        await resolveAppeal({
          appealId,
          sid,
          verdict,
          note: String(form.get("note") ?? ""),
          ip,
        });
        track("fd_appeal_resolved", { verdict }, ctx);
        return { ok: true, intent, error: null };
      }
      default:
        return { ok: false, intent, error: "Unknown action." };
    }
  } catch (err) {
    return actionFailure("foundry.stewardship", intent, err);
  }
}

export default function FoundryStewardship({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const fetcher = useFetcher<typeof action>();
  const pending = fetcher.state !== "idle";

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current || d.unavailable) return;
    viewed.current = true;
    track("fd_stewardship_viewed", { roles: d.roles }, { sid: d.badge });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge]);

  if (d.unavailable) {
    return (
      <div className="fd-page fd-stack fd-stew">
        <FdPageHead title="The steward code" />
        <p className="fd-empty">{FD_UNAVAILABLE}</p>
      </div>
    );
  }

  const submit = (fields: Record<string, string>) =>
    fetcher.submit(fields, { method: "post" });

  return (
    <FdStewardshipPage
      consent={d.consent}
      decisions={d.decisions}
      myAppeals={d.myAppeals}
      isAdmin={d.isAdmin}
      openAppeals={d.openAppeals}
      bodyLimit={d.bodyLimit}
      error={fetcher.data && !fetcher.data.ok ? fetcher.data.error : null}
      pending={pending}
      onConsent={({ topic, state }) => submit({ intent: "consent", topic, state })}
      onFile={({ subjectKind, subjectId, body }) =>
        submit({ intent: "file", subjectKind, subjectId, body })
      }
      onWithdraw={(appealId) => submit({ intent: "withdraw", appealId })}
      onResolve={({ appealId, verdict, note }) =>
        submit({ intent: "resolve", appealId, verdict, note })
      }
    />
  );
}
