import { useEffect, useRef, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import { href } from "@core/lib/router/routes";

import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import ChModalDeleteProject from "@ui/creatorhub/components/ChModalDeleteProject";
import Button from "@ui/atoms/Button";
import EmptyState from "@ui/components/EmptyState";
import "@ui/creatorhub/pages/chscenes.css";
import "@ui/creatorhub/components/chmodaldeleteproject.css";

import CreatorHubBreadcrumb from "@ui/creatorhub/components/CreatorHubBreadcrumb";
import { useAuth } from "@data/lib/auth/index";
import { openSignIn } from "@features/components/auth/signin-store";
import { useProfileName } from "@data/lib/auth/use-profile-name";
import {
  parcelCount,
  type DeleteCopy,
  type Project,
} from "@data/lib/catalyst/creator-hub/delete-project";
import { loadDeleteProjectData } from "@data/lib/catalyst/creator-hub/delete-project.server";
import { deleteScene, resolveActiveScene } from "@data/lib/catalyst/creator-hub/delete-scene";
import {
  handleStore,
  slugifyProjectTitle,
  ensureHandlePermission,
} from "@data/lib/fs/handle-store";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { track } from "@core/lib/telemetry/track";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/creator-hub.delete-project";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => creatorHubMeta("Delete scene");

const STORY: StoryId = "creator-hub/delete-project";

type Step = "scenes" | "confirm" | "deleted";

function normStep(raw: string | null): Step {
  return raw === "confirm" || raw === "deleted" ? raw : "scenes";
}

const FALLBACK: Assignment = {
  variant: "confirm_modal",
  flags: { confirmModal: true, fileCheckboxDefault: false },
  experimentKey: "ch_delete_project_confirm",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = normStep(url.searchParams.get("step"));
  const projectParam = url.searchParams.get("project")?.trim() ?? "";
  const deletedParam = url.searchParams.get("deleted")?.trim() ?? "";
  const filesParam = url.searchParams.get("files") === "1";
  const tombstone = url.searchParams.get("tombstone")?.trim() || null;
  const localFiles = url.searchParams.get("local")?.trim() || null;
  const creator = url.searchParams.get("creator")?.trim() || readWallet(request) || "";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const { data: ds, source } = await loadDeleteProjectData({
    creator: creator || undefined,
  });

  const projects = deletedParam
    ? ds.projects.filter((p) => p.id !== deletedParam)
    : ds.projects;

  const selected = projectParam
    ? (ds.projects.find((p) => p.id === projectParam) ?? null)
    : null;

  const payload = {
    sid,
    source,
    step,
    files: filesParam,
    copy: ds.copy,
    projects,
    selected,
    requestedId: projectParam || null,
    deletedId: deletedParam || null,
    tombstone,
    localFiles,
    creator,
  };

  return wrap(payload);
}

type LoaderData = {
  sid: string;
  source: "live" | "empty" | "unavailable";
  step: Step;
  files: boolean;
  copy: DeleteCopy;
  projects: Project[];
  selected: Project | null;
  requestedId: string | null;
  deletedId: string | null;
  tombstone: string | null;
  localFiles: string | null;
  creator: string;
};

function withCreator(params: Record<string, string>, creator: string): string {
  const sp = new URLSearchParams(params);
  if (creator) sp.set("creator", creator);
  return `?${sp.toString()}`;
}

export default function DeleteProjectRoute({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  if (d.step === "confirm" && d.selected) {
    return <ConfirmStep d={d} />;
  }
  return <ScenesStep d={d} />;
}

function ScenesStep({ d }: { d: LoaderData }) {
  const { isConnected, address } = useAuth();
  const name = useProfileName(address, isConnected);
  const [, setSearchParams] = useSearchParams();
  const isDeleted = d.step === "deleted";

  const rescoping = isConnected && Boolean(address) && !d.creator;

  const notFound =
    d.step === "confirm" && Boolean(d.requestedId) && !d.selected && !rescoping;

  useEffect(() => {
    if (isConnected && address && !d.creator) {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          next.set("creator", address);
          return next;
        },
        { replace: true, preventScrollReset: true },
      );
    }
  }, [isConnected, address, d.creator, setSearchParams]);

  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    track(
      "ch_delete_scenes_viewed",
      { count: d.projects.length, source: d.source },
      { sid: d.sid, story: STORY },
    );
    if (isDeleted) {
      track(
        "ch_delete_done_viewed",
        { remaining: d.projects.length, deleted_id: d.deletedId },
        { sid: d.sid, story: STORY },
      );
    }
  }, [d.sid, d.projects.length, d.source, isDeleted, d.deletedId]);

  return (
    <CreatorHubChrome
      active="scenes"
      signedIn={isConnected}
      account={address ?? ""}
      name={name}
      onSignIn={() => openSignIn()}
    >
      <CreatorHubBreadcrumb to={href("/create/scenes")} LinkComponent={Link} />

      <section className="chscenes">
        <div className="chscenes__container">
          <div className="chscenes__list-wrap">
            <div className="chscenes__menu">
              <div className="chscenes__header">
                <h1 className="chscenes__heading">My Scenes</h1>
              </div>
              {d.projects.length > 0 ? (
                <div className="chscenes__filters">
                  <div className="chscenes__results">
                    {d.projects.length}{" "}
                    {d.projects.length === 1 ? "scene" : "scenes"}
                  </div>
                </div>
              ) : null}
            </div>

            {notFound ? (
              <p
                role="status"
                style={{
                  background: "var(--fill-2)",
                  border: "1px solid var(--line)",
                  color: "var(--ink-85)",
                  borderRadius: "var(--r-control)",
                  padding: "10px 14px",
                  margin: "12px 0 0",
                  fontSize: 13,
                }}
              >
                Project not found &#x2014; it may have already been deleted or belong
                to a different account. Nothing was deleted. Pick a scene below
                to try again.
              </p>
            ) : null}

            {isDeleted ? (
              <p
                role="status"
                style={{
                  background: "var(--fill-2)",
                  border: "1px solid var(--line)",
                  color: "var(--ink-85)",
                  borderRadius: "var(--r-control)",
                  padding: "10px 14px",
                  margin: "12px 0 0",
                  fontSize: 13,
                }}
              >
                Scene deleted &#x2014; the deployment was overridden on the catalyst and
                removed from your parcels.
                {d.tombstone
                  ? ` Tombstone entity ${d.tombstone.slice(0, 12)}\u{2026}`
                  : ""}
                {d.localFiles === "deleted"
                  ? " The scene's local files were removed from the connected folder."
                  : d.localFiles === "kept"
                    ? " The scene's local files were left on your computer (no connected folder, or permission was denied)."
                    : ""}
              </p>
            ) : null}

            <div className="chscenes__cards">
              {d.projects.length > 0 ? (
                d.projects.map((p) => (
                  <ProjectCard
                    key={p.id}
                    project={p}
                    creator={d.creator}
                    deleteLabel={d.copy.menu_delete}
                  />
                ))
              ) : rescoping ? (
                <div
                  role="status"
                  aria-live="polite"
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 10,
                    margin: "12px 0 0",
                  }}
                >
                  <span className="u-spinner" aria-hidden="true" />
                  <span
                    style={{
                      color: "var(--ink-45)",
                      fontSize: 14,
                      lineHeight: 1.5,
                    }}
                  >
                    Loading your scenes&#x2026;
                  </span>
                </div>
              ) : !isConnected && !d.creator ? (
                <EmptyState
                  title="Sign in to see your scenes"
                  subtitle="Sign in to pick the scene you want to delete."
                  icon={undefined}
                  actions={
                    <Button variant="secondary" onClick={() => openSignIn()}>
                      Sign in
                    </Button>
                  }
                  variant={undefined}
                  tone={undefined}
                  actionsGap={undefined}
                  style={undefined}
                />
              ) : (
                <EmptyState
                  title="No scenes yet"
                  subtitle="Scenes you create will appear here."
                  icon={undefined}
                  actions={undefined}
                  variant={undefined}
                  tone={undefined}
                  actionsGap={undefined}
                  style={undefined}
                />
              )}
            </div>
          </div>
        </div>
      </section>
    </CreatorHubChrome>
  );
}

const ParcelIcon = () => (
  <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
    <rect
      x="1.5"
      y="1.5"
      width="13"
      height="13"
      rx="1.5"
      stroke="currentColor"
      strokeWidth="1.3"
      fill="none"
    />
    <path
      d="M1.5 6h13M1.5 10.5h13M6 1.5v13M10.5 1.5v13"
      stroke="currentColor"
      strokeWidth="1.1"
    />
  </svg>
);

function ProjectCard({
  project,
  creator,
  deleteLabel,
}: {
  project: Project;
  creator: string;
  deleteLabel: string;
}) {
  const parcels = parcelCount(project);
  return (
    <div className="chscenes__card">
      <div
        className="chscenes__thumb"
        style={
          project.thumbnail
            ? { backgroundImage: `url(${project.thumbnail})` }
            : { background: project.grad || "var(--fill-2)" }
        }
      />
      {project.published ? (
        <span className="chscenes__badge">Published</span>
      ) : null}
      <div className="chscenes__cardinfo">
        <div className="chscenes__cardtitle">
          <span className="u-truncate">{project.title}</span>
          <CardMenu
            project={project}
            creator={creator}
            deleteLabel={deleteLabel}
          />
        </div>
        <div className="chscenes__cardcontent">
          <ParcelIcon />
          {parcels} {parcels === 1 ? "parcel" : "parcels"}
        </div>
      </div>
    </div>
  );
}

function CardMenu({
  project,
  creator,
  deleteLabel,
}: {
  project: Project;
  creator: string;
  deleteLabel: string;
}) {
  const [searchParams] = useSearchParams();
  const open = searchParams.get("menu") === project.id;

  const toConfirm = withCreator(
    { step: "confirm", project: project.id },
    creator,
  );
  const toMenu = open
    ? withCreator({ step: "scenes" }, creator)
    : withCreator({ step: "scenes", menu: project.id }, creator);

  return (
    <div className={"chscenes__cardmenu" + (open ? " is-open" : "")}>
      <Link
        to={toMenu}
        className="chscenes__kebab"
        aria-label="Project actions"
        aria-haspopup="menu"
        aria-expanded={open}
        preventScrollReset
        style={{ display: "inline-flex" }}
      >
        <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
          <circle cx="12" cy="5" r="1.7" fill="currentColor" />
          <circle cx="12" cy="12" r="1.7" fill="currentColor" />
          <circle cx="12" cy="19" r="1.7" fill="currentColor" />
        </svg>
      </Link>
      {open ? (
        <div className="chscenes__menuanchor">
          <Link
            to={toConfirm}
            className="chscenes__delete-link"
            preventScrollReset
            style={{
              display: "block",
              padding: "10px 14px",
              marginTop: 6,
              color: "var(--brand)",
              textDecoration: "none",
              fontSize: 13,
              background: "var(--panel)",
              border: "1px solid var(--line)",
              borderRadius: "var(--r-card, 12px)",
              boxShadow: "var(--shadow-modal)",
            }}
          >
            {deleteLabel}
          </Link>
        </div>
      ) : null}
    </div>
  );
}

async function deleteLocalProjectFiles(title: string): Promise<boolean> {
  try {
    const slug = slugifyProjectTitle(title);
    const handle = await handleStore.get(slug);
    if (!handle) return false;
    if (!(await ensureHandlePermission(handle, "readwrite"))) return false;
    const names: string[] = [];
    for await (const name of (
      handle as unknown as { keys(): AsyncIterable<string> }
    ).keys()) {
      names.push(name);
    }
    for (const name of names) {
      await handle.removeEntry(name, { recursive: true });
    }
    await handleStore.clear(slug);
    return true;
  } catch {
    return false;
  }
}

type DeletePhase = "idle" | "deleting" | "error";

function ConfirmStep({ d }: { d: LoaderData }) {
  const navigate = useNavigate();
  const selected = d.selected as Project;
  const { identity, isConnected } = useAuth();
  const [phase, setPhase] = useState<DeletePhase>("idle");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    track(
      "ch_delete_opened",
      { project_id: selected.id, published: selected.published },
      { sid: d.sid, story: STORY },
    );
  }, [d.sid, selected.id, selected.published]);

  function onToggleFiles(checked: boolean) {
    track(
      "ch_delete_files_toggled",
      { checked, project_id: selected.id },
      { sid: d.sid, story: STORY },
    );
  }

  function onClose() {
    if (phase === "deleting") return;
    track(
      "ch_delete_cancelled",
      { project_id: selected.id },
      { sid: d.sid, story: STORY },
    );
    navigate(withCreator({ step: "scenes" }, d.creator), {
      preventScrollReset: true,
    });
  }

  async function onSubmit(_project: unknown, shouldDeleteFiles: boolean) {
    if (phase === "deleting") return;
    setErrorMsg(null);

    if (shouldDeleteFiles) {
      track(
        "ch_delete_files_opted_in",
        { project_id: selected.id },
        { sid: d.sid, story: STORY },
      );
    }

    let id = identity;
    if (!isConnected || !id) {
      openSignIn();
      setPhase("error");
      setErrorMsg("Sign in to delete this scene, then try again.");
      return;
    }

    setPhase("deleting");

    const base = selected.scene.base;
    let live;
    try {
      live = await resolveActiveScene(base);
    } catch {
      live = null;
    }

    if (!live) {
      setPhase("error");
      setErrorMsg(
        "This scene isn't deployed to the catalyst, so there's nothing to delete there. " +
          "Only published scenes can be deleted from the network.",
      );
      track(
        "ch_delete_no_deployment",
        { project_id: selected.id, base },
        { sid: d.sid, story: STORY },
      );
      return;
    }

    const res = await deleteScene(
      id,
      { pointers: live.pointers, base },
      { expectedOwner: d.creator || id.signer },
    );

    if (!res.ok) {
      setPhase("error");
      setErrorMsg(
        res.errors[0] ?? "The catalyst rejected the deletion. Please try again.",
      );
      track(
        "ch_delete_failed",
        {
          project_id: selected.id,
          status: res.status,
          error: res.errors[0] ?? "unknown",
          simulated: false,
        },
        { sid: d.sid, story: STORY },
      );
      return;
    }

    let localOutcome: "deleted" | "kept" | "" = "";
    if (shouldDeleteFiles) {
      localOutcome = (await deleteLocalProjectFiles(selected.title))
        ? "deleted"
        : "kept";
    }

    track(
      "ch_delete_confirmed",
      {
        project_id: selected.id,
        delete_files: shouldDeleteFiles,
        local_files: localOutcome || undefined,
        simulated: false,
        tombstone_id: res.tombstoneId,
        overrode_pointers: res.overrode.length,
        status: res.status,
        replaced_entity: live.id,
      },
      { sid: d.sid, story: STORY },
    );
    const doneParams: Record<string, string> = {
      step: "deleted",
      deleted: selected.id,
      tombstone: res.tombstoneId,
    };
    if (localOutcome) doneParams.local = localOutcome;
    navigate(withCreator(doneParams, d.creator), { preventScrollReset: true });
  }

  return (
    <>
      <ChModalDeleteProject
        open
        project={{
          id: selected.id,
          title: selected.title,
          path: selected.path,
        }}
        deleteFiles={d.files}
        onClose={onClose}
        onSubmit={onSubmit as unknown as () => void}
        onToggleFiles={onToggleFiles}
      />
      {phase === "deleting" ? (
        <div
          role="status"
          aria-live="polite"
          style={{
            position: "fixed",
            inset: 0,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 10,
            background: "rgba(0,0,0,0.45)",
            zIndex: 1000,
            color: "#fff",
            fontSize: 14,
          }}
        >
          <span className="u-spinner" aria-hidden="true" />
          Deleting from the catalyst&#x2026;
        </div>
      ) : null}
      {phase === "error" && errorMsg ? (
        <div
          role="alert"
          style={{
            position: "fixed",
            left: "50%",
            bottom: 24,
            transform: "translateX(-50%)",
            maxWidth: 520,
            background: "var(--panel, #1a1a1a)",
            border: "1px solid var(--line, #f55)",
            color: "var(--ink-85, #fff)",
            borderRadius: "var(--r-control, 8px)",
            padding: "12px 16px",
            fontSize: 13,
            zIndex: 1001,
            boxShadow: "var(--shadow-modal)",
          }}
        >
          {errorMsg}
        </div>
      ) : null}
    </>
  );
}
