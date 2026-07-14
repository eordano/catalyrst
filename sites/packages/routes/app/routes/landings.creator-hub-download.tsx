import { useEffect, useRef } from "react";
import { useSearchParams } from "react-router";

import LdCreatorHubDownloadPage from "@ui/landings/pages/LdCreatorHubDownloadPage";

import {
  loadDownloadFixture,
  resolveDownload,
  type DownloadOption,
  type OsKey,
} from "@core/lib/landings/creator-hub-download.server";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/landings.creator-hub-download";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "landings/creator-hub-download";

const FALLBACK: Assignment = {
  variant: "os_detected",
  flags: { autoDetectOs: true },
  experimentKey: "lp_creatorhub_download",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() ?? "view";
  const osParam = url.searchParams.get("os");
  const archParam = url.searchParams.get("arch");

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const fixture = loadDownloadFixture();
  const resolved = resolveDownload(fixture, request.headers.get("user-agent"), {
    os: osParam,
    arch: archParam,
  });

  const payload = {
    sid,
    step: step === "success" ? "success" : "view",
    version: resolved.version,
    releaseUrl: resolved.releaseUrl,
    repo: resolved.repo,
    overridden: resolved.overridden,
    detectedOs: resolved.detectedOs,
    detectedArch: resolved.detectedArch,
    primary: resolved.primary,
    secondary: resolved.secondary,
    systemRequirements: resolved.systemRequirements,
  };

  return wrap(payload);
}

export default function CreatorHubDownloadRoute({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  return d.step === "success" ? (
    <DownloadSuccess sid={d.sid} os={d.detectedOs} />
  ) : (
    <DownloadPage
      sid={d.sid}
      version={d.version}
      detectedOs={d.detectedOs}
      detectedArch={d.detectedArch}
      overridden={d.overridden}
      primary={d.primary}
      secondary={d.secondary}
    />
  );
}

type PageProps = {
  sid: string;
  version: string;
  detectedOs: OsKey;
  detectedArch: string;
  overridden: boolean;
  primary: DownloadOption;
  secondary: DownloadOption[];
};

function DownloadPage({
  sid,
  version,
  detectedOs,
  detectedArch,
  overridden,
  primary,
  secondary,
}: PageProps) {
  const [, setSearchParams] = useSearchParams();

  useDownloadView(sid, detectedOs, detectedArch, version, overridden);

  function selectOs(os: OsKey) {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.set("os", os);
        return next;
      },
      { preventScrollReset: true },
    );
  }

  function onSurfaceClick(e: React.MouseEvent<HTMLDivElement>) {
    const el = (e.target as HTMLElement).closest<HTMLElement>("[data-os]");
    if (!el) return;
    const osText = el.getAttribute("data-os") ?? "";
    const isSecondary = el.tagName === "BUTTON";

    if (isSecondary) {
      const osKey: OsKey = /windows/i.test(osText) ? "windows" : "macos";
      const opt = secondary.find((s) => s.osKey === osKey);
      track(
        "lp_creatorhub_download_clicked",
        {
          os: osKey,
          arch: opt?.arch ?? detectedArch,
          file_name: opt?.fileName ?? null,
          kind: "secondary",
        },
        { sid, story: STORY },
      );
      selectOs(osKey);
      return;
    }

    track(
      "lp_creatorhub_download_clicked",
      {
        os: primary.osKey,
        arch: primary.arch,
        file_name: primary.fileName,
        kind: "primary",
      },
      { sid, story: STORY },
    );
  }

  return (
    <LdCreatorHubDownloadPage
      version={version}
      primary={primary}
      secondary={secondary}
      successHref={`?step=success&os=${primary.osKey}`}
      onSurfaceClick={onSurfaceClick}
      onSuccessLinkClick={(e) => {
        e.preventDefault();
        setSearchParams(
          (prev) => {
            const next = new URLSearchParams(prev);
            next.set("step", "success");
            next.set("os", primary.osKey);
            return next;
          },
          { preventScrollReset: true },
        );
      }}
    />
  );
}

function DownloadSuccess({ sid, os }: { sid: string; os: OsKey }) {
  useSuccessView(sid, os);
  return <LdCreatorHubDownloadPage step="success" os={os} />;
}

function useDownloadView(
  sid: string,
  os: OsKey,
  arch: string,
  version: string,
  overridden: boolean,
) {
  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    const key = `${os}|${arch}`;
    if (lastKey.current === key) return;
    lastKey.current = key;
    track(
      "lp_creatorhub_download_viewed",
      { os, arch, version, overridden },
      { sid, story: STORY },
    );
  }, [sid, os, arch, version, overridden]);
}

function useSuccessView(sid: string, os: OsKey) {
  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    track("lp_creatorhub_download_success", { os }, { sid, story: STORY });
  }, [sid, os]);
}
