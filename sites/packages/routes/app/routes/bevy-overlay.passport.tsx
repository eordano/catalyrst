import { useCallback, useEffect, useRef } from "react";
import { useSearchParams } from "react-router";

import { loadPassport, type PassportData } from "@data/lib/catalyst/overlay/passport.server";
import { normalizeAddress, isEthAddress } from "@data/lib/catalyst/overlay/profile";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import PassportPanel, {
  type PassportTab,
} from "@ui/overlay/panels/PassportPanel";

import type { Route } from "./+types/bevy-overlay.passport";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "overlay/passport";

const DEFAULT_ADDRESS = "0x3fe27e8c6d2bd3a2e0d6f3a9b0c1d2e3f4a5b6c7";

function parseTab(raw: string | null): PassportTab {
  if (raw === "badges" || raw === "photos" || raw === "overview") return raw;
  return "overview";
}

function parseAddress(raw: string | null): string {
  const addr = normalizeAddress(raw);
  return addr && isEthAddress(addr) ? addr : DEFAULT_ADDRESS;
}

const FALLBACK: Assignment = {
  variant: "tabbed",
  flags: { tabs: true },
  experimentKey: "cl_passport_tabs",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const address = parseAddress(url.searchParams.get("address"));
  const tab = parseTab(url.searchParams.get("tab"));
  const self = url.searchParams.get("self") === "1";
  const openPhotoId = url.searchParams.get("photo");

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const passport = await loadPassport(address, { signal: request.signal });

  const payload = {
    sid,
    address,
    tab,
    self,
    openPhotoId,
    passport,
    assignment,
  };

  return wrap(payload);
}

type LoaderData = {
  sid: string;
  address: string;
  tab: PassportTab;
  self: boolean;
  openPhotoId: string | null;
  passport: PassportData;
  assignment: Assignment;
};

export default function PassportRoute({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  return <PassportStage {...d} />;
}

function PassportStage({
  sid,
  address,
  tab,
  self,
  openPhotoId,
  passport,
  assignment,
}: LoaderData) {
  const [, setSearchParams] = useSearchParams();

  const ctx = {
    sid,
    story: STORY,
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
  };

  const reportedEmpty = useRef(false);
  useEffect(() => {
    if (reportedEmpty.current || !passport.profileEmpty) return;
    reportedEmpty.current = true;
    track(
      "cl_passport_profile_empty",
      { address, from_fixture: passport.usedFixture },
      ctx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [passport.profileEmpty]);

  const onMounted = useCallback(
    (addr: string, isSelf: boolean) => {
      track(
        "cl_passport_opened",
        { address: addr, self: isSelf, from_fixture: passport.usedFixture },
        ctx,
      );
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sid, passport.usedFixture],
  );

  const onTabViewed = useCallback(
    (t: PassportTab) => {
      track("cl_passport_tab_viewed", { tab: t }, ctx);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sid],
  );

  const onTab = useCallback(
    (t: PassportTab) => {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          next.set("panel", "passport");
          next.set("tab", t);
          next.delete("photo");
          return next;
        },
        { preventScrollReset: true },
      );
    },
    [setSearchParams],
  );

  const onOpenPhoto = useCallback(
    (id: string | null) => {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          if (id) next.set("photo", id);
          else next.delete("photo");
          return next;
        },
        { preventScrollReset: true },
      );
    },
    [setSearchParams],
  );

  const onPhotoOpened = useCallback(
    (id: string) => {
      track("cl_passport_photo_opened", { photo_id: id }, ctx);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sid],
  );

  const onLinkClicked = useCallback(
    (url: string) => {
      track("cl_passport_link_clicked", { url }, ctx);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sid],
  );

  const onItemClicked = useCallback(
    (name: string, category: string) => {
      track("cl_passport_item_clicked", { name, category }, ctx);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sid],
  );

  const onClaimName = useCallback(
    () => {
      track("cl_passport_claim_name", { address, simulated: true }, ctx);
      window.location.assign("/marketplace/claim-name");
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sid, address],
  );

  const onEdit = useCallback(
    (field: string) => {
      track("cl_passport_edit", { field, simulated: true }, ctx);
      window.location.assign("/marketplace/account");
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sid],
  );

  const onClose = useCallback(() => {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.set("panel", "closed");
        next.delete("photo");
        return next;
      },
      { preventScrollReset: true },
    );
  }, [setSearchParams]);

  return (
    <PassportPanel
      data={passport}
      tab={tab}
      openPhotoId={openPhotoId}
      self={self}
      onTab={onTab}
      onOpenPhoto={onOpenPhoto}
      onClose={onClose}
      onMounted={onMounted}
      onTabViewed={onTabViewed}
      onLinkClicked={onLinkClicked}
      onItemClicked={onItemClicked}
      onPhotoOpened={onPhotoOpened}
      onClaimName={onClaimName}
      onEdit={onEdit}
    />
  );
}
