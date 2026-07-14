import { useEffect, useRef } from "react";
import { data, redirect, useFetcher } from "react-router";

import FdGddDocPage, {
  type FdGddPlayVM,
  type FdGddVersionVM,
} from "@ui/foundry/pages/FdGddDocPage";
import { FD_UNAVAILABLE, FdPageHead } from "@ui/foundry/components/FdSection";

import "@ui/atoms/button.css";
import "@ui/foundry/components/fdcellchip.css";
import "@ui/foundry/components/fdmarkdown.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdtable.css";
import "@ui/foundry/pages/fdgdd.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import {
  FoundryUnavailableError,
  canonicalSid,
  getPool,
  sidBadge,
} from "@data/lib/foundry/db.server";
import { editGddDoc } from "@data/lib/foundry/gdd-edit.server";
import {
  approvalCounts,
  approvalsForDoc,
  approveGddDoc,
  hasApproved,
  type GddApprovalRecord,
} from "@data/lib/foundry/gdd-approve.server";
import { getPersona } from "@data/lib/foundry/persona.server";
import {
  asksByIds,
  docEditor,
  getGddChain,
  changedSections,
  getGddDoc,
  readingsForDocs,
  splitGddSections,
  type GddSectionContent,
} from "@data/lib/foundry/gdd.server";
import { getScene } from "@data/lib/foundry/scenes.server";
import { countTrajectories } from "@data/lib/foundry/trajectory.server";
import type { GddDoc } from "@data/lib/foundry/types";

import { actionFailure, clientIp, requireCookie } from "../lib/foundry-action";

import type { Route } from "./+types/foundry.gdd_.$id";

const LIST_HREF = "/foundry/gdd";

export function meta({ loaderData }: Route.MetaArgs) {
  const title = loaderData?.doc?.title;
  return [{ title: title ? `${title} — The Foundry` : "Design doc — The Foundry" }];
}

export async function loader({ params, request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);

  let doc: GddDoc | null = null;
  let sections: GddSectionContent[] = [];
  let chain: FdGddVersionVM[] = [];
  let superseded = false;
  let changed: string[] | null = null;
  let play: FdGddPlayVM | null = null;
  let groundedAsks: { id: string; title: string }[] = [];
  let approvals: GddApprovalRecord[] = [];
  let editedBy: string | null = null;
  let viewerApproved = false;
  let viewerHasPersona = false;
  let unavailable = false;

  try {
    const db = getPool();
    doc = await getGddDoc(db, params.id);
    if (!doc) throw data(null, { status: 404 });
    // Split by the same headings the marker grid counts by, so each grid row
    // can expand to exactly the text its counts were counted in.
    sections = splitGddSections(doc.bodyMd);

    groundedAsks = await asksByIds(db, doc.groundingRequestIds);
    if (doc.source === "session") editedBy = await docEditor(db, params.id);
    [approvals, viewerApproved] = await Promise.all([
      approvalsForDoc(db, params.id),
      hasApproved(db, params.id, sid),
    ]);
    // The persona row lives on the CANONICAL sid; a session rebound onto a
    // persona carries an alias sid, which a raw lookup would read as
    // persona-less and hide the approve button behind "claim a persona first".
    viewerHasPersona = (await getPersona(await canonicalSid(sid))) !== null;

    const rows = await getGddChain(db, params.id);
    const chainSignatures = await approvalCounts(db, rows.map((r) => r.id));
    chain = rows.map((r) => ({
      id: r.id,
      version: r.version,
      createdAt: r.createdAt,
      open: r.honesty.open,
      approvals: chainSignatures.get(r.id) ?? 0,
    }));
    superseded = rows.some((r) => r.supersedes === doc?.id);

    // "Publish and explain": what this version changed, derived from the two
    // stored bodies on read — never authored, never cached.
    if (doc.supersedes) {
      const prev = await getGddDoc(db, doc.supersedes);
      if (prev) changed = changedSections(doc.bodyMd, prev.bodyMd);
    }

    // Exactly one play tier: the truly-linked game; else this program's dated
    // same-concept reading; else the claim-a-seat invitation.
    if (doc.sceneId) {
      const [scene, runCount] = await Promise.all([
        getScene(db, doc.sceneId),
        countTrajectories(db, { sceneId: doc.sceneId }),
      ]);
      play = {
        tier: "play",
        sceneId: doc.sceneId,
        sceneTitle: scene?.title ?? doc.sceneId,
        runCount,
        deployed: scene?.worldName != null,
      };
    } else {
      // Readings are keyed to the exact doc id they were read against, and
      // supersede-on-edit mints a new id (gdd-edit.server.ts) — resolve across
      // the whole chain so the affordance follows the version people land on.
      const chainIds = rows.length > 0 ? rows.map((r) => r.id) : [params.id];
      const readings = await readingsForDocs(db, chainIds);
      const reading =
        readings.find((r) => r.gddDocId === params.id) ?? readings[0];
      play = reading
        ? {
            tier: "same-concept",
            sceneId: reading.sceneId,
            sceneTitle: reading.sceneTitle,
            readAt: reading.readAt,
            rationale: reading.rationale,
            confidence: reading.confidence,
            readAgainstVersion:
              reading.gddDocId === params.id ? null : reading.docVersion,
          }
        : {
            tier: "take-on",
            docTitle: doc.title,
            groundedAskId: groundedAsks[0]?.id ?? null,
          };
    }
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    unavailable = true;
  }

  return wrap({
    badge: sidBadge(sid),
    unavailable,
    doc,
    sections,
    chain,
    superseded,
    changed,
    play,
    groundedAsks,
    approvals,
    viewerApproved,
    viewerHasPersona,
    editedBy,
  });
}

// Same open-write posture as the exchange: any visitor with a session cookie,
// every save attributed in foundry.action_log, rate-capped per sid and IP in
// the data layer. A save never mutates this doc — it mints v(n+1) with
// `supersedes` pointing here (see gdd-edit.server.ts), then navigates to it.
export async function action({ params, request }: Route.ActionArgs) {
  const { sid, created } = sidLoader(request);
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
    requireCookie(created);
    switch (intent) {
      case "edit-section": {
        const index = Number(form.get("sectionIndex"));
        const res = await editGddDoc({
          docId: params.id,
          section: {
            index,
            name: String(form.get("sectionName") ?? ""),
            contentMd: String(form.get("contentMd") ?? ""),
          },
          sid,
          ip,
        });
        track(
          "fd_gdd_edited",
          { doc_id: params.id, new_id: res.id, section: index },
          { sid: sidBadge(sid) },
        );
        return redirect(`/foundry/gdd/${res.id}`);
      }
      case "edit-doc": {
        const res = await editGddDoc({
          docId: params.id,
          bodyMd: String(form.get("bodyMd") ?? ""),
          sid,
          ip,
        });
        track(
          "fd_gdd_edited",
          { doc_id: params.id, new_id: res.id, section: null },
          { sid: sidBadge(sid) },
        );
        return redirect(`/foundry/gdd/${res.id}`);
      }
      case "approve": {
        const res = await approveGddDoc({ docId: params.id, sid, ip });
        track("fd_gdd_approved", { doc_id: params.id }, { sid: sidBadge(sid) });
        return data({
          ok: true as const,
          intent,
          error: null,
          approvedBy: res.name,
        });
      }
      default:
        return data({ ok: false, intent, error: "Unknown action." }, { status: 400 });
    }
  } catch (err) {
    return actionFailure("foundry.gdd", intent, err);
  }
}

export default function FoundryGddDoc({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const ctx = { sid: d.badge };
  const doc = d.doc;
  const fetcher = useFetcher<typeof action>();
  const approveFetcher = useFetcher<typeof action>();

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current || !doc) return;
    viewed.current = true;
    track(
      "fd_gdd_viewed",
      {
        doc_id: doc.id,
        kind: doc.kind,
        open_sections: doc.honesty.sections.filter((s) => s.open > 0).length,
        hypotheses: doc.hypotheses.length,
      },
      ctx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge, doc?.id]);

  if (d.unavailable || !doc) {
    return (
      <div className="fd-page fd-stack fd-gdd">
        <FdPageHead
          eyebrow="Design docs"
          title="Design doc"
          crumbs={<a href={LIST_HREF}>← All design docs</a>}
        />
        <p className="fd-empty">{FD_UNAVAILABLE}</p>
      </div>
    );
  }

  return (
    <FdGddDocPage
      doc={doc}
      sections={d.sections}
      backHref={LIST_HREF}
      chain={d.chain}
      changed={d.changed}
      play={d.play}
      groundedAsks={d.groundedAsks}
      editedBy={d.editedBy}
      approval={{
        approvals: d.approvals,
        viewerApproved: d.viewerApproved,
        viewerHasPersona: d.viewerHasPersona,
        pending: approveFetcher.state !== "idle",
        error:
          approveFetcher.data && approveFetcher.data.ok === false
            ? approveFetcher.data.error
            : null,
        onApprove: () =>
          approveFetcher.submit({ intent: "approve" }, { method: "post" }),
      }}
      onVersionOpen={(id, version) =>
        track("fd_gdd_version_opened", { doc_id: id, version }, ctx)
      }
      onPlayOpen={(tier) =>
        track("fd_gdd_play_opened", { doc_id: doc.id, tier }, ctx)
      }
      edit={
        d.superseded
          ? null
          : {
              onSaveSection: (index, name, contentMd) =>
                fetcher.submit(
                  {
                    intent: "edit-section",
                    sectionIndex: String(index),
                    sectionName: name,
                    contentMd,
                  },
                  { method: "post" },
                ),
              onSaveDoc: (bodyMd) =>
                fetcher.submit({ intent: "edit-doc", bodyMd }, { method: "post" }),
              pending: fetcher.state !== "idle",
              error:
                fetcher.data && fetcher.data.ok === false
                  ? fetcher.data.error
                  : null,
            }
      }
    />
  );
}
