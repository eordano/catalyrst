import type { CSSProperties } from "react";
import { useCallback } from "react";
import type { ShouldRevalidateFunctionArgs } from "react-router";
import { useNavigate, useSearchParams } from "react-router";

import ClientStage from "@ui/overlay/panels/ClientStage";
import EmoteWheel from "@ui/explorer/components/EmoteWheel";

import { loadBackpackEmotes } from "@data/lib/catalyst/overlay/backpack-emotes.server";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import type { Route } from "./+types/bevy-overlay.emote-wheel";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "client/emote-wheel";

const FALLBACK: Assignment = {
  variant: "radial-wheel",
  flags: { radial: true },
  experimentKey: "client_emote_wheel",
};

function isPanelOpen(raw: string | null): boolean {
  return raw === null || raw === "emote-wheel";
}

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const address = url.searchParams.get("address")?.trim() || null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const emotes = await loadBackpackEmotes(address, { signal: request.signal });

  const payload = {
    sid,
    address,
    assignment,
    emotes,
  };
  return wrap(payload);
}

export function shouldRevalidate({
  currentUrl,
  nextUrl,
  defaultShouldRevalidate,
}: ShouldRevalidateFunctionArgs) {
  if (
    currentUrl.pathname === nextUrl.pathname &&
    currentUrl.searchParams.get("address") ===
      nextUrl.searchParams.get("address")
  ) {
    return false;
  }
  return defaultShouldRevalidate;
}

const GUEST_EMPTY_STYLE: CSSProperties = {
  position: "fixed",
  inset: 0,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  padding: "0 24px",
  textAlign: "center",
  color: "rgba(255,255,255,0.85)",
  textShadow: "0 1px 4px rgba(0,0,0,0.7)",
};

export default function BevyOverlayEmoteWheel({
  loaderData,
}: Route.ComponentProps) {
  const { address, emotes } = loaderData;
  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useNavigate();

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

  const onCustomise = useCallback(() => {
    navigate(
      address
        ? `/bevy-overlay/backpack-emotes?address=${encodeURIComponent(address)}`
        : "/bevy-overlay/backpack-emotes",
    );
  }, [address, navigate]);

  const guestEmpty = !address && emotes.loadout.length === 0;

  return (
    <ClientStage nojs="Enable JavaScript to use the emote wheel.">
      {open && (
        <main className="bevy-overlay-emote-wheel">
          {guestEmpty ? (
            <p style={GUEST_EMPTY_STYLE}>
              You are visiting as a guest, so there is no saved emote loadout to
              show. Sign in with a wallet to equip emotes.
            </p>
          ) : (
            <EmoteWheel
              catalog={emotes.catalog}
              loadout={emotes.loadout}
              onClose={onClose}
              onCustomise={onCustomise}
            />
          )}
        </main>
      )}
    </ClientStage>
  );
}
