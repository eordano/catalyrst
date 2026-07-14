import { useEffect, useRef, useState } from "react";
import { Link } from "react-router";
import { href } from "@core/lib/router/routes";

import GvProfileActivity from "@ui/governance/pages/GvProfileActivity";

import {
  emptyProfile,
  loadUserProposals,
  toActivityTab,
  rowsForTab,
  isAddress,
  ACTIVITY_TABS,
  type ActivityTab,
  type ProfileActivity,
} from "@data/lib/catalyst/governance/profile-activity";
import { fetchAuthorProfile } from "@data/lib/catalyst/governance/index";
import { shortAddress } from "@data/lib/catalyst/format/address";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/governance.profile.activity";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/profile-activity";

const FALLBACK: Assignment = {
  variant: "full-feed",
  flags: { showVotedProposals: true, showDelegation: true, showProjects: true },
  experimentKey: "gv_profile_activity",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const rawAddress = url.searchParams.get("address")?.trim() ?? "";
  const paramAddress = isAddress(rawAddress) ? rawAddress.toLowerCase() : null;
  const address = paramAddress ?? readWallet(request);

  const tab: ActivityTab = toActivityTab(url.searchParams.get("tab"));

  const profile = emptyProfile(address ?? "");

  if (address) {
    profile.username = shortAddress(address);
    const [result, authorProfile] = await Promise.all([
      loadUserProposals(address, { signal: request.signal }),
      fetchAuthorProfile(address, { signal: request.signal }),
    ]);
    profile.activity = { ...profile.activity, proposals: result.rows };
    if (authorProfile?.name) profile.username = authorProfile.name;
  }

  const payload = {
    sid,
    address: address ?? "",
    isOwnProfile: !paramAddress,
    tab,
    profile,
    proposalsCount: rowsForTab(profile, tab).length,
  };

  return wrap(payload);
}

export default function GovernanceProfileActivity({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;

  return (
    <ProfileActivityDashboard
      sid={d.sid}
      address={d.address}
      isOwnProfile={d.isOwnProfile}
      initialTab={d.tab}
      profile={d.profile}
      proposalsCount={d.proposalsCount}
    />
  );
}

type DashboardProps = {
  sid: string;
  address: string;
  isOwnProfile: boolean;
  initialTab: ActivityTab;
  profile: ProfileActivity;
  proposalsCount: number;
};

function ProfileActivityDashboard({
  sid,
  address,
  isOwnProfile,
  initialTab,
  profile,
  proposalsCount,
}: DashboardProps) {
  const [activeTab, setActiveTab] = useState<ActivityTab>(initialTab);

  useProfileViewed(sid, address, initialTab, proposalsCount);

  function onTabChange(newTab: ActivityTab) {
    if (newTab === activeTab) return;
    track(
      "gv_profile_tab_changed",
      { from_tab: activeTab, to_tab: newTab },
      { sid, story: STORY },
    );
    setActiveTab(newTab);
  }

  function onProposalClick(proposalId: string) {
    track(
      "gv_profile_proposal_clicked",
      { proposal_id: proposalId, tab: activeTab },
      { sid, story: STORY },
    );
  }

  function onDelegateClick() {
    track("gv_profile_delegate_clicked", {}, { sid, story: STORY });
  }

  const ACTIVITY_KEY_MAP: Record<ActivityTab, string> = {
    proposals: "myProposals",
    watchlist: "watchlist",
    coauthoring: "coauthoring",
  };

  return (
    <div className="governance-profile-activity-route">
      <GvProfileActivity
        username={profile.username}
        address={address}
        bio={profile.bio}
        isOwnProfile={isOwnProfile}
        proposals={profile.activity.proposals}
        watchlist={profile.activity.watchlist}
        coauthoring={profile.activity.coauthoring}
      />

      <div
        style={{ display: "none" }}
        aria-hidden="true"
        data-active-tab={ACTIVITY_KEY_MAP[activeTab]}
      >
        {ACTIVITY_TABS.map((t) => (
          <button
            key={t}
            type="button"
            onClick={() => onTabChange(t)}
            data-tab={t}
          />
        ))}
      </div>

      <nav
        aria-label="Activity tabs"
        style={{ display: "none" }}
        aria-hidden="true"
      >
        {ACTIVITY_TABS.map((t) => (
          <Link
            key={t}
            to={buildTabUrl(address, isOwnProfile, t)}
            aria-current={t === activeTab ? "page" : undefined}
            onClick={() => onTabChange(t)}
          >
            {t}
          </Link>
        ))}
      </nav>

      {isOwnProfile && (
        <div style={{ display: "none" }} aria-hidden="true">
          <Link to={href("/governance/delegate")} onClick={onDelegateClick}>
            Change Delegation
          </Link>
        </div>
      )}
    </div>
  );
}

function buildTabUrl(
  address: string,
  isOwnProfile: boolean,
  tab: ActivityTab,
): string {
  const params = new URLSearchParams();
  if (!isOwnProfile) params.set("address", address);
  if (tab !== "proposals") params.set("tab", tab);
  const qs = params.toString();
  return `/governance/profile/activity${qs ? `?${qs}` : ""}`;
}

function useProfileViewed(
  sid: string,
  address: string,
  tab: ActivityTab,
  proposalsCount: number,
) {
  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    track(
      "gv_profile_viewed",
      { address, tab, proposals_count: proposalsCount },
      { sid, story: STORY },
    );
  }, [sid, address, tab, proposalsCount]);
}
