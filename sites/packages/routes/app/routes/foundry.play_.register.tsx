import { data, useFetcher } from "react-router";

import FdRegisterGamePage from "@ui/foundry/pages/FdRegisterGamePage";
import { FD_UNAVAILABLE, FdPageHead } from "@ui/foundry/components/FdSection";

import "@ui/atoms/button.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/pages/fdregister.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { FoundryUnavailableError, sidBadge } from "@data/lib/foundry/db.server";
import { activeRoles } from "@data/lib/foundry/roles.server";
import { registerScene } from "@data/lib/foundry/scene-register.server";

import { actionFailure, clientIp, requireCookie } from "../lib/foundry-action";

import type { Route } from "./+types/foundry.play_.register";

export function meta() {
  return [{ title: "Register a game — The Foundry" }];
}

export async function loader({ request }: Route.LoaderArgs) {
  const base = sidLoader(request);
  const sid = base.sid;
  try {
    const roles = await activeRoles(sid);
    return base.wrap({
      unavailable: false,
      badge: sidBadge(sid),
      // Presentation gate only: registerScene re-checks the host role in-tx.
      canHost: roles.includes("host"),
    });
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    return base.wrap({ unavailable: true, badge: sidBadge(sid), canHost: false });
  }
}

export async function action({ request }: Route.ActionArgs) {
  const base = sidLoader(request);
  const sid = base.sid;
  const ip = clientIp(request);

  let form: FormData;
  try {
    form = await request.formData();
  } catch {
    return data(
      { ok: false, slug: null, error: "Could not read the form." },
      { status: 400 },
    );
  }
  const intent = String(form.get("intent") ?? "");

  try {
    requireCookie(base.created);
    if (intent === "register") {
      const { id } = await registerScene({
        sid,
        id: String(form.get("id") ?? ""),
        title: String(form.get("title") ?? ""),
        repoPath: String(form.get("repoPath") ?? ""),
        gddDocId: String(form.get("gddDocId") ?? ""),
        sourceNote: String(form.get("sourceNote") ?? ""),
        ip,
      });
      track("fd_scene_registered", { slug: id }, { sid: sidBadge(sid) });
      return { ok: true, slug: id, error: null };
    }
    return { ok: false, slug: null, error: "Unknown action." };
  } catch (err) {
    return actionFailure("foundry.play.register", intent, err, {
      slug: null as string | null,
    });
  }
}

export default function FoundryRegisterGame({
  loaderData,
  actionData,
}: Route.ComponentProps) {
  const d = loaderData;
  const fetcher = useFetcher<typeof action>();
  // fetcher.data on a JS submit; actionData is the no-JS (native form) fallback.
  const result = fetcher.data ?? actionData;

  if (d.unavailable) {
    return (
      <div className="fd-page fd-stack">
        <FdPageHead
          eyebrow="Games"
          title="Register a game"
          crumbs={<a href="/foundry/play">← All games</a>}
        />
        <p className="fd-empty">{FD_UNAVAILABLE}</p>
      </div>
    );
  }

  return (
    <FdRegisterGamePage
      canHost={d.canHost}
      error={result && !result.ok ? result.error : null}
      registeredSlug={result?.ok === true ? result.slug : null}
      pending={fetcher.state !== "idle"}
      onRegister={(values) =>
        fetcher.submit({ intent: "register", ...values }, { method: "post" })
      }
    />
  );
}
