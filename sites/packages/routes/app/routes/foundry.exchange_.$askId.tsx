import { useEffect, useState } from "react";
import { data, useFetcher } from "react-router";

import FdAskPage, { type FdAskEditErrors } from "@ui/foundry/pages/FdAskPage";
import { FD_UNAVAILABLE, FdPageHead } from "@ui/foundry/components/FdSection";

import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdrequestcard.css";
import "@ui/foundry/components/fdpersonachip.css";
import "@ui/foundry/components/fdcellchip.css";
import "@ui/foundry/pages/fdask.css";

import { sidLoader } from "@core/lib/experiments/story-loader";

import {
  FoundryUnavailableError,
  getPool,
  sidBadge,
} from "@data/lib/foundry/db.server";
import {
  getRequest,
  listPledges,
  type PledgeListRow,
  type RequestBoardRow,
} from "@data/lib/foundry/exchange.server";
import { briefsQuotingAsk } from "@data/lib/foundry/gdd.server";
import {
  getRequestReading,
  type RequestReading,
} from "@data/lib/foundry/request-readings.server";
import { activeRoles } from "@data/lib/foundry/roles.server";

import type { Route } from "./+types/foundry.exchange_.$askId";

const BOARD_HREF = "/foundry/exchange";

export function meta({ loaderData }: Route.MetaArgs) {
  const title = (loaderData as { ask?: { title?: string } | null } | undefined)?.ask
    ?.title;
  return [{ title: title ? `${title} — The Foundry` : "Exchange — The Foundry" }];
}

export async function loader({ params, request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);

  let ask: RequestBoardRow | null = null;
  let pledgeList: PledgeListRow[] = [];
  let reading: RequestReading | null = null;
  let quotedInBriefs: { id: string; kind: string }[] = [];
  let viewerIsAdmin = false;
  let unavailable = false;

  try {
    const [row, roles] = await Promise.all([
      getRequest(sid, params.askId),
      activeRoles(sid),
    ]);
    if (!row) throw data("No ask with this id.", { status: 404 });
    ask = row;
    [pledgeList, reading, quotedInBriefs] = await Promise.all([
      listPledges(row.id),
      getRequestReading(row.id),
      briefsQuotingAsk(getPool(), row.id),
    ]);
    viewerIsAdmin = roles.includes("admin");
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    unavailable = true;
  }

  return wrap({
    badge: sidBadge(sid),
    unavailable,
    ask,
    pledgeList,
    reading,
    quotedInBriefs,
    viewerIsAdmin,
  });
}

export default function FoundryAsk({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  // The pledge/withdraw/moderate/edit flow is the exchange action,
  // byte-identical: this page submits to the board route rather than
  // duplicating the handler.
  const fetcher = useFetcher();
  const pending = fetcher.state !== "idle";
  const fdata = fetcher.data as
    | {
        ok?: boolean;
        intent?: string;
        error?: string | null;
        errors?: FdAskEditErrors;
      }
    | undefined;

  const [editOpen, setEditOpen] = useState(false);
  useEffect(() => {
    if (fetcher.state === "idle" && fdata?.ok && fdata.intent === "edit") {
      setEditOpen(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fetcher.state, fetcher.data]);

  if (d.unavailable || !d.ask) {
    return (
      <div className="fd-page fd-stack fd-ask">
        <FdPageHead
          eyebrow="Exchange"
          title="This request"
          crumbs={<a href={BOARD_HREF}>← All requests</a>}
        />
        <p className="fd-empty">{FD_UNAVAILABLE}</p>
      </div>
    );
  }

  const submit = (fields: Record<string, string>) =>
    fetcher.submit(fields, { method: "post", action: BOARD_HREF });

  return (
    <FdAskPage
      ask={d.ask}
      reading={d.reading}
      quotedInBriefs={d.quotedInBriefs}
      pledgeList={d.pledgeList}
      pending={pending}
      error={fdata?.error ?? null}
      viewerIsAdmin={d.viewerIsAdmin}
      onPledge={() => submit({ intent: "pledge", requestId: d.ask!.id })}
      onWithdraw={() => submit({ intent: "withdraw", requestId: d.ask!.id })}
      onModerate={(verdict) =>
        submit({ intent: "moderate", requestId: d.ask!.id, verdict })
      }
      edit={
        d.ask.authoredByMe
          ? {
              open: editOpen,
              onToggle: () => setEditOpen((v) => !v),
              onSave: (values) =>
                submit({ intent: "edit", requestId: d.ask!.id, ...values }),
              errors: fdata?.errors ?? {},
            }
          : null
      }
      backHref={BOARD_HREF}
    />
  );
}
