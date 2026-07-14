import GvNotFound from "@ui/governance/pages/GvNotFound";

import {
  getLinkAccountsData,
  toProvider,
  type Provider,
  type LinkAccountsData,
} from "@data/lib/catalyst/governance/link-accounts";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { trackExposure } from "@core/lib/telemetry/track";
import LinkAccountsWizard from "@features/stories/governance/link-accounts/LinkAccountsWizard";

import type { Route } from "./+types/governance.profile.link-accounts";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/link-accounts";

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "gv_link_accounts_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;
  const action = url.searchParams.get("action")?.trim()?.toLowerCase() || null;

  const rawAccount = url.searchParams.get("account");
  const provider = toProvider(rawAccount);
  const unlinkRequested = action === "unlink";

  const { sid, assignment, wrap } = await storyLoader(request, STORY, FALLBACK, {
    skipExposure: true,
  });

  if (unlinkRequested && rawAccount && !provider) {
    return wrap({ notFound: true as const, sid }, { status: 404 });
  }

  const account: Provider = provider ?? "forum";

  const linkData = getLinkAccountsData();

  trackExposure({
    sid,
    story: STORY,
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
  });

  const payload = {
    notFound: false as const,
    sid,
    step,
    account,
    unlinkOnMount: unlinkRequested,
    assignment,
    data: linkData,
  };
  return wrap(payload);
}

type LoaderData =
  | { notFound: true; sid: string }
  | {
      notFound: false;
      sid: string;
      step: string | null;
      account: Provider;
      unlinkOnMount: boolean;
      assignment: Assignment;
      data: LinkAccountsData;
    };

export default function GovernanceLinkAccounts({ loaderData }: Route.ComponentProps) {
  const d = loaderData as LoaderData;

  if (d.notFound) {
    return <GvNotFound />;
  }

  return (
    <main className="governance-link-accounts">
      <LinkAccountsWizard
        trackCtx={{
          sid: d.sid,
          story: STORY,
          variant: d.assignment.variant,
          experimentKey: d.assignment.experimentKey,
        }}
        account={d.account}
        data={d.data}
        initialStep={d.step ?? undefined}
        unlinkOnMount={d.unlinkOnMount}
      />
    </main>
  );
}
