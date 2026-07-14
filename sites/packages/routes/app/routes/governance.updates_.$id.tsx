import { useEffect, useRef } from "react";

import GvProjectUpdateDetail from "@ui/governance/pages/GvProjectUpdateDetail";
import GvNotFound from "@ui/governance/pages/GvNotFound";

import { loadProjectUpdateDetail } from "@data/lib/catalyst/governance/project-update-detail";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/governance.updates_.$id";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/project-update-detail";

const FALLBACK: Assignment = {
  variant: "focused-read",
  flags: { showFinancials: true, showComments: true },
  experimentKey: "gv_update_detail",
};

export async function loader({ request, params }: Route.LoaderArgs) {
  const { id } = params;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const detail = await loadProjectUpdateDetail(id, { signal: request.signal });

  const resolved = detail.source === "live";
  const notFound = !resolved;

  const payload = {
    id,
    detail: notFound ? null : detail,
    sid,
  };

  return wrap(payload);
}

export default function GovernanceUpdateDetailRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const { id, detail, sid } = d;

  const notFoundFired = useRef(false);
  useEffect(() => {
    if (detail || notFoundFired.current) return;
    notFoundFired.current = true;
    track("gv_update_notfound", { project_id: id }, { sid, story: STORY });
  }, [detail, id, sid]);

  const viewed = useRef(false);
  useEffect(() => {
    if (!detail || viewed.current) return;
    viewed.current = true;
    track(
      "gv_update_viewed",
      {
        project_id: detail.project.id,
        update_id: detail.update.id,
        update_index: detail.update.index,
        health: detail.update.health,
        status: detail.update.status,
        source: detail.source,
      },
      { sid, story: STORY },
    );
  }, [detail, sid]);

  const commentsRef = useRef<HTMLDivElement | null>(null);
  const commentsSeen = useRef(false);
  useEffect(() => {
    if (!detail) return;
    const el = commentsRef.current;
    if (!el || typeof IntersectionObserver === "undefined") return;
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting && !commentsSeen.current) {
            commentsSeen.current = true;
            track(
              "gv_update_comments_viewed",
              {
                project_id: detail.project.id,
                update_id: detail.update.id,
                total_comments: detail.totalComments,
              },
              { sid, story: STORY },
            );
            io.disconnect();
          }
        }
      },
      { threshold: 0.2 },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [detail, sid]);

  if (!detail) {
    return (
      <main className="governance-update-route governance-update-route--notfound">
        <GvNotFound />
      </main>
    );
  }

  return (
    <main className="governance-update-route">
      <GvProjectUpdateDetail
        update={
          detail.update as React.ComponentProps<
            typeof GvProjectUpdateDetail
          >["update"]
        }
        project={
          detail.project as React.ComponentProps<
            typeof GvProjectUpdateDetail
          >["project"]
        }
        comments={detail.comments}
        state="ready"
      />

      <div ref={commentsRef} aria-hidden="true" style={{ height: 1 }} />
    </main>
  );
}
