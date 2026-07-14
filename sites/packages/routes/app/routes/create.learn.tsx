import { useEffect, useRef } from "react";

import ChLearn from "@ui/creatorhub/pages/ChLearn";

import { useAuth } from "@data/lib/auth/index";
import { openSignIn } from "@features/components/auth/signin-store";
import { useProfileName } from "@data/lib/auth/use-profile-name";
import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/create.learn";

export const meta = () => creatorHubMeta("Learn");

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);
  const payload = { sid };
  return wrap(payload);
}

export default function CreateLearn({ loaderData }: Route.ComponentProps) {
  const { sid } = loaderData as { sid: string };
  const { isConnected, address } = useAuth();
  const name = useProfileName(address, isConnected);

  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    track("ch_learn_viewed", {}, { sid, story: "create-hub-to-scenes" });
  }, [sid]);

  return (
    <ChLearn
      signedIn={isConnected}
      account={address ?? ""}
      name={name}
      onSignIn={() => {
        track("ch_learn_signin_clicked", {}, { sid, story: "create-hub-to-scenes" });
        openSignIn();
      }}
    />
  );
}
