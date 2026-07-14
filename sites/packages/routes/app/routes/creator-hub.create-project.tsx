import type { ShouldRevalidateFunctionArgs } from "react-router";
import { Link } from "react-router";

import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import "@ui/creatorhub/frames/creatorhubchrome.css";
import { STARTER_TEMPLATES } from "@ui/creatorhub/pages/ChTemplates";
import CreatorHubBreadcrumb from "@ui/creatorhub/components/CreatorHubBreadcrumb";

import { resolveBreadcrumbOrigin } from "@features/components/creator-hub/breadcrumbOrigins";
import { useAuth } from "@data/lib/auth/index";
import { openSignIn } from "@features/components/auth/signin-store";
import { useProfileName } from "@data/lib/auth/use-profile-name";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import CreateProjectWizard, {
  type TemplateOption,
} from "@features/stories/creator-hub/create-project/CreateProjectWizard";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/creator-hub.create-project";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => creatorHubMeta("New scene");

const STORY: StoryId = "creator-hub/create-project";

const PROJECT_DEFAULTS = { name: "My Awesome Scene", path: "" };
const TAKEN_PATHS: string[] = [];

type StarterTemplate = {
  id: string;
  title: string;
  description: string;
  tags: string[];
  difficulty_level: string;
  github_link: string;
};

const PROJECT_TEMPLATES: TemplateOption[] = [
  {
    id: "empty",
    title: "Empty Scene",
    description: "Start your own scene from scratch.",
    tags: [],
    difficulty: "Easy",
    default: true,
    github_link: "https://github.com/decentraland/sdk7-scene-template",
  },
  ...(STARTER_TEMPLATES as StarterTemplate[]).map((t) => ({
    id: t.id,
    title: t.title,
    description: t.description,
    tags: t.tags,
    difficulty: t.difficulty_level,
    default: false,
    github_link: t.github_link,
  })),
];

export function shouldRevalidate({
  currentUrl,
  nextUrl,
  defaultShouldRevalidate,
}: ShouldRevalidateFunctionArgs) {
  const loaderParams = ["template", "from", "variant"] as const;
  if (
    currentUrl.pathname === nextUrl.pathname &&
    loaderParams.every(
      (k) => currentUrl.searchParams.get(k) === nextUrl.searchParams.get(k),
    )
  ) {
    return false;
  }
  return defaultShouldRevalidate;
}

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "ch_create_project_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;
  const template = url.searchParams.get("template")?.trim() || null;
  const from = url.searchParams.get("from")?.trim() || null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const templateTitle =
    template && template !== "empty"
      ? PROJECT_TEMPLATES.find((t) => t.id === template)?.title
      : undefined;

  const payload = {
    sid,
    step,
    template,
    from,
    assignment,
    defaults: templateTitle ? { name: templateTitle, path: "" } : PROJECT_DEFAULTS,
    takenPaths: TAKEN_PATHS,
    templates: PROJECT_TEMPLATES,
  };
  return wrap(payload);
}

export default function CreatorHubCreateProject({ loaderData }: Route.ComponentProps) {
  const { sid, step, template, from, assignment, defaults, takenPaths, templates } =
    loaderData;
  const { isConnected, address } = useAuth();
  const name = useProfileName(address, isConnected);

  const origin = resolveBreadcrumbOrigin(from || (template ? "templates" : null));

  return (
    <CreatorHubChrome
      active={origin.active}
      signedIn={isConnected}
      account={address ?? ""}
      name={name}
      onSignIn={() => {
        openSignIn();
      }}
    >
      <CreatorHubBreadcrumb to={origin.to} label={origin.label} LinkComponent={Link} />

      <section className="creator-hub-create-project">
        <CreateProjectWizard
          trackCtx={{
            sid,
            story: STORY,
            variant: assignment.variant,
            experimentKey: assignment.experimentKey,
          }}
          defaults={defaults}
          takenPaths={takenPaths}
          templates={templates}
          originFrom={from || (template ? "templates" : "scenes")}
          initialStep={step ?? undefined}
          initialTemplate={template ?? undefined}
        />
      </section>
    </CreatorHubChrome>
  );
}
