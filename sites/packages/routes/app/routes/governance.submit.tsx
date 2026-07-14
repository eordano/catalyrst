import { Outlet, useMatches } from "react-router";

import { getSubmitHub, getGroups, getGroupIds, getChooser, getChooserTypes } from "@data/lib/catalyst/governance/submit";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import GvSubmitProposal from "@features/components/governance/GvSubmitProposal";

import type { Route } from "./+types/governance.submit";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/submit-proposal";

const FALLBACK: Assignment = {
  variant: "grouped-picker",
  flags: { groupedHub: true, addRemoveChooser: true },
  experimentKey: "gv_submit_category_picker",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const groupRaw = url.searchParams.get("group")?.trim() ?? "";
  const categoryRaw = url.searchParams.get("category")?.trim() ?? "";
  const requestRaw = url.searchParams.get("request")?.trim() ?? "";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const hub = getSubmitHub();

  const validGroups = getGroupIds();
  const group = validGroups.includes(groupRaw) ? groupRaw : "";
  const groups = getGroups(group);

  const chooserTypes = getChooserTypes();
  const chooserType = chooserTypes.includes(categoryRaw) ? categoryRaw : "";
  const chooser = chooserType ? getChooser(chooserType) : null;
  const requestVal =
    requestRaw === "add" || requestRaw === "remove"
      ? requestRaw
      : chooserType
        ? "add"
        : "";

  const totalCategories = hub.groups.reduce(
    (n, g) => n + g.categories.length,
    0,
  );

  const payload = {
    sid,
    pageTitle: hub.page.title,
    pageLead: hub.page.lead,
    group,
    groups,
    totalCategories,
    chooser,
    chooserType,
    request: requestVal,
  };

  return wrap(payload);
}

export default function GovernanceSubmitRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;

  const matches = useMatches();
  const hasChild = matches[matches.length - 1]?.id !== "routes/governance.submit";
  if (hasChild) return <Outlet />;

  return (
    <GvSubmitProposal
      sid={d.sid}
      pageTitle={d.pageTitle}
      pageLead={d.pageLead}
      group={d.group}
      groups={d.groups}
      totalCategories={d.totalCategories}
      chooser={d.chooser}
      chooserType={d.chooserType}
      request={d.request}
    />
  );
}
