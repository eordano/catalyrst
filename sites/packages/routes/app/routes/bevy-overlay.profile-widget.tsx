import { useCallback } from "react";
import { useSearchParams } from "react-router";

import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import ProfileWidget from "@ui/explorer/components/ProfileWidget";

import type { Route } from "./+types/bevy-overlay.profile-widget";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "client/hud-overlay";

const GUEST_NAME = "Guest";

type Anchor = "rail" | "topbar";

function parseAnchor(raw: string | null): Anchor {
  return raw === "topbar" ? "topbar" : "rail";
}

function isPanelOpen(raw: string | null): boolean {
  return raw === null || raw === "profile-widget";
}

const FALLBACK: Assignment = {
  variant: "overlay",
  flags: { domOverlay: true },
  experimentKey: "client_hud_overlay",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const anchor = parseAnchor(url.searchParams.get("anchor"));

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const payload = {
    sid,
    anchor,
    assignment,
  };

  return wrap(payload);
}

export default function ProfileWidgetRoute({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  return <ProfileWidgetStage anchor={d.anchor} />;
}

function ProfileWidgetStage({ anchor }: { anchor: Anchor }) {
  const [searchParams, setSearchParams] = useSearchParams();
  const open = isPanelOpen(searchParams.get("panel"));

  const onClose = useCallback(() => {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.set("panel", "closed");
        return next;
      },
      { preventScrollReset: true },
    );
  }, [setSearchParams]);

  return (
    <ProfileWidget
      open={open}
      name={GUEST_NAME}
      isGuest
      anchor={anchor}
      onClose={onClose}
    />
  );
}
