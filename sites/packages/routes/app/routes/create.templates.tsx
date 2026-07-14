import { useEffect, useRef } from "react";
import { useNavigate } from "react-router";

import ChTemplates from "@ui/creatorhub/pages/ChTemplates";

import { useAuth } from "@data/lib/auth/index";
import { openSignIn } from "@features/components/auth/signin-store";
import { useProfileName } from "@data/lib/auth/use-profile-name";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/create.templates";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => creatorHubMeta("Templates");

const STORY: StoryId = "create/templates-gallery";

const FALLBACK: Assignment = {
  variant: "gallery",
  flags: { gallery: true },
  experimentKey: "ch_templates_gallery",
};

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const payload = { sid };
  return wrap(payload);
}

type LoaderData = { sid: string };

type TemplateVM = {
  id: string;
  title: string;
  play_link?: string | null;
  github_link?: string;
};

export default function CreateTemplates({ loaderData }: Route.ComponentProps) {
  const d = loaderData as LoaderData;
  const navigate = useNavigate();
  const { isConnected, address } = useAuth();
  const name = useProfileName(address, isConnected);

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track("ch_templates_viewed", {}, { sid: d.sid, story: STORY });
  }, [d.sid]);

  function onSelectTemplate(template: TemplateVM | null) {
    track(
      "ch_template_selected",
      { template_id: template?.id ?? "empty", title: template?.title ?? "Empty Scene" },
      { sid: d.sid, story: STORY },
    );
  }

  function onCreate({ template }: { template: TemplateVM | null }) {
    track(
      "ch_studio_opened",
      { source: template ? "template" : "empty", template_id: template?.id ?? "empty" },
      { sid: d.sid, story: "studio-open-scene-lab" },
    );
    const to =
      template?.id != null
        ? `/creator-hub/scene-editor?new=1&template=${encodeURIComponent(String(template.id))}&name=${encodeURIComponent(template.title)}&from=templates`
        : "/creator-hub/scene-editor?new=1&from=templates";
    if (typeof window !== "undefined") {
      window.location.assign(to);
    } else {
      navigate(to);
    }
  }

  function onPreview(template: TemplateVM) {
    track("ch_template_previewed", { template_id: template.id }, { sid: d.sid, story: STORY });
  }
  function onViewCode(template: TemplateVM) {
    track("ch_template_view_code", { template_id: template.id }, { sid: d.sid, story: STORY });
  }

  return (
    <div className="create-templates">
      <ChTemplates
        signedIn={isConnected}
        account={address ?? ""}
        name={name}
        onSignIn={() => {
          track("ch_templates_signin_clicked", {}, { sid: d.sid, story: STORY });
          openSignIn();
        }}
        onSelectTemplate={onSelectTemplate}
        onCreate={onCreate}
        onPreview={onPreview}
        onViewCode={onViewCode}
      />
    </div>
  );
}
