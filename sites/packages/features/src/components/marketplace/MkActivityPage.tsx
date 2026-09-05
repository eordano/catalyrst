import { Link, useSearchParams } from "react-router";

import MkActivityPageView from "@ui/marketplace/pages/MkActivityPage";

import {
  formatMana,
  type ActivityEntry,
  type ActivityType,
} from "@data/lib/catalyst/marketplace/activity";
import { track } from "@core/lib/telemetry/track";
import type { TrackContext } from "@core/lib/telemetry/track";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "marketplace/activity";

export type MkActivityPageProps = {
  sid: string;
  entries: ActivityEntry[];
  type: ActivityType | "";
  page: number;
  pageSize: number;
  hasNext: boolean;
  fallback: boolean;
  salesTotal: number;
  tradesTotal: number;
};

export function MkActivityPage({
  sid,
  entries,
  type,
  page,
  pageSize,
  hasNext,
  fallback,
  salesTotal,
  tradesTotal,
}: MkActivityPageProps) {
  const [, setSearchParams] = useSearchParams();
  const ctx: TrackContext = { sid, story: STORY };

  function setType(next: ActivityType | "") {
    track("mk_activity_filter_applied", { type: next || null }, ctx);
    setSearchParams(
      (prev) => {
        const p = new URLSearchParams(prev);
        if (next) p.set("type", next);
        else p.delete("type");
        p.delete("page");
        return p;
      },
      { preventScrollReset: true },
    );
  }

  function goToPage(next: number, direction: "prev" | "next") {
    track("mk_activity_paginated", { page: next, direction }, ctx);
    setSearchParams(
      (prev) => {
        const p = new URLSearchParams(prev);
        if (next > 0) p.set("page", String(next));
        else p.delete("page");
        return p;
      },
      { preventScrollReset: true },
    );
  }

  function onRowClick(entry: ActivityEntry) {
    track("mk_activity_row_clicked", { id: entry.id, kind: entry.kind }, ctx);
  }

  const sales = entries.filter((e) => e.kind === "sale" && e.from);
  const me = sales[0]?.from ?? "";
  const mine = me ? sales.filter((e) => e.from === me) : [];
  const totalMana = mine.reduce((acc, e) => {
    const n = e.price ? Number(e.price.replace(/,/g, "")) : 0;
    return acc + (Number.isFinite(n) ? n : 0);
  }, 0);
  const manaEarned = formatMana(String(Math.round(totalMana * 1e18))) ?? "0";

  return (
    <MkActivityPageView
      entries={entries}
      type={type}
      page={page}
      pageSize={pageSize}
      hasNext={hasNext}
      fallback={fallback}
      salesTotal={salesTotal}
      tradesTotal={tradesTotal}
      mySales={{ me, mine, manaEarned }}
      LinkComponent={Link}
      onSelectType={setType}
      onPage={goToPage}
      onEntryClick={onRowClick}
    />
  );
}
