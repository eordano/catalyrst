import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { Link, useSearchParams } from "react-router";

import ExploreChrome from "@ui/explorer/frames/ExploreChrome";
import SearchField from "@ui/atoms/SearchField";
import CommunityMembers from "@ui/explorer/pages/CommunityMembers";
import CommunityStream from "@ui/explorer/components/CommunityStream";
import "@ui/explorer/pages/communities.css";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  actionFor,
  type CommunityRow,
  type CommunityJoinDetail,
  type JoinAction,
  type CommitFn,
} from "@data/lib/catalyst/overlay/community-join";
import { track } from "@core/lib/telemetry/track";
import {
  communityJoinMachine,
  resolveCommunityJoinSnapshot,
  slugToState,
  stateToSlug,
  type TrackFn,
} from "./machine";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "overlay/community-join";

export type CommunityJoinWizardProps = {
  trackCtx: TrackContext;
  rows: CommunityRow[];
  total?: number;
  selected: CommunityJoinDetail | null;
  search: string;
  source: "live" | "fixture";
  commit?: CommitFn;
  track?: TrackFn;
};

export default function CommunityJoinWizard(props: CommunityJoinWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = searchParams.get("step")?.trim() || undefined;
  const stateId = slugToState(urlStep);
  const selectedId =
    searchParams.get("select")?.trim() || props.selected?.community.id || "";

  return (
    <Inner
      key={`${stateId}:${selectedId}`}
      stateId={stateId}
      selectedId={selectedId}
      {...props}
    />
  );
}

type InnerProps = CommunityJoinWizardProps & {
  stateId: ReturnType<typeof slugToState>;
  selectedId: string;
};

function Inner({
  stateId,
  selectedId,
  trackCtx,
  rows,
  total,
  selected,
  search,
  source,
  commit,
  track: trackInjected,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const emit = trackInjected ?? track;

  const action: JoinAction = selected
    ? actionFor(selected.community)
    : stateId === "requesting"
      ? "request"
      : "join";

  const snapshot = useRef(
    resolveCommunityJoinSnapshot({
      step: stateId,
      trackCtx,
      commit,
      track: trackInjected,
      communityId: selectedId || undefined,
      action,
    }),
  ).current;

  const [state, send] = useMachine(communityJoinMachine, {
    input: {
      trackCtx,
      commit,
      track: trackInjected,
      communityId: selectedId || undefined,
      action,
    },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const ctxId = state.context.communityId || selectedId;

  const browseSeen = useRef(false);
  const detailSeen = useRef<string | null>(null);
  useEffect(() => {
    if (value === "browsing" && !browseSeen.current) {
      browseSeen.current = true;
      emit(
        "cl_community_browse_viewed",
        { count: rows.length, search: search || null, source },
        { sid: trackCtx.sid, story: STORY },
      );
    }
    if (value === "detail" && selected && detailSeen.current !== selected.community.id) {
      detailSeen.current = selected.community.id;
      emit(
        "cl_community_detail_viewed",
        {
          community_id: selected.community.id,
          privacy: selected.community.privacy,
          members_count: selected.community.membersCount,
          source,
        },
        { sid: trackCtx.sid, story: STORY },
      );
    }
  }, [value, rows.length, search, source, selected, trackCtx.sid, emit]);

  const lastSync = useRef<string | null>(null);
  useEffect(() => {
    const syncKey = `${step}:${ctxId}`;
    if (lastSync.current === syncKey) return;
    lastSync.current = syncKey;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        params.set("panel", "communities");
        if (params.get("step") === step && (params.get("select") ?? "") === (value === "browsing" ? "" : ctxId)) {
          return params;
        }
        params.set("step", step);
        if (value === "browsing") params.delete("select");
        else if (ctxId) params.set("select", ctxId);
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, ctxId, value, setSearchParams]);

  const detailById = (id: string) => rows.find((r) => r.id === id) ?? null;
  const activeRow =
    selected?.community ?? (ctxId ? detailById(ctxId) : null) ?? null;

  return (
    <ExploreChrome active="communities" onTab={() => {}} onClose={() => {}}>
      <div className="cm" data-step={step} data-source={source}>
        {value === "browsing" && (
          <BrowseGrid
            rows={rows}
          total={total}
            search={search}
            onSearch={(q) =>
              setSearchParams(
                (prev) => {
                  const p = new URLSearchParams(prev);
                  if (q) p.set("q", q);
                  else p.delete("q");
                  return p;
                },
                { preventScrollReset: true },
              )
            }
            onSelect={(row) =>
              send({ type: "SELECT", communityId: row.id, action: actionFor(row) })
            }
          />
        )}

        {value === "detail" && (
          <DetailView
            detail={selected}
            fallbackRow={activeRow}
            onStart={() => send({ type: "START" })}
            onBack={() => send({ type: "BACK" })}
          />
        )}

        {(value === "joining" || value === "requesting") && (
          <ConfirmStep
            community={activeRow}
            action={value === "requesting" ? "request" : "join"}
            real={!!commit}
            onConfirm={() => send({ type: "CONFIRM" })}
            onBack={() => send({ type: "BACK" })}
          />
        )}

        {value === "confirming" && (
          <CommitProgress community={activeRow} action={action} />
        )}

        {value === "joined" && (
          <DoneView
            community={activeRow}
            pending={state.context.result?.pending ?? action === "request"}
            onBrowseMore={() => send({ type: "BROWSE_MORE" })}
          />
        )}

        {value === "error" && (
          <ErrorView
            error={state.context.error}
            onRetry={() => send({ type: "RETRY" })}
            onBack={() => send({ type: "BACK" })}
          />
        )}
      </div>
    </ExploreChrome>
  );
}

/** An unreported size renders as an em dash -- "0 Members" would be a headcount
 *  the API never gave. */
function memberCountLabel(count: number | null): string {
  return count === null ? "\u2014" : count.toLocaleString();
}

function BrowseGrid({
  rows,
  total,
  search,
  onSearch,
  onSelect,
}: {
  rows: CommunityRow[];
  total?: number;
  search: string;
  onSearch: (q: string) => void;
  onSelect: (row: CommunityRow) => void;
}) {
  const totalCount = total && total > rows.length ? total : rows.length;
  return (
    <section className="cm__main" style={{ width: "100%" }}>
      <div className="cm__header">
        <h1 className="cm__title">Communities</h1>
        <div className="cm__searchwrap">
          <SearchField placeholder="Search" value={search} onChange={onSearch} />
        </div>
      </div>
      <div className="cm__section">
        <div className="cm__seclabel">
          Browse Communities ({totalCount})
          {totalCount > rows.length ? ` \u00b7 showing first ${rows.length}` : ""}
        </div>
        {rows.length === 0 ? (
          <p style={{ color: "rgba(255,255,255,0.8)" }}>No communities found.</p>
        ) : (
          <div className="cm__grid">
            {rows.map((c, i) => (
              <CommunityCard key={c.id} c={c} hue={(i * 53) % 360} onSelect={onSelect} />
            ))}
          </div>
        )}
      </div>
    </section>
  );
}

function CommunityCard({
  c,
  hue,
  onSelect,
}: {
  c: CommunityRow;
  hue: number;
  onSelect: (row: CommunityRow) => void;
}) {
  const isPublic = c.privacy !== "private";
  const isMember = c.role !== "none" && c.role !== "";
  return (
    <article className="cm__card">
      <button
        type="button"
        className="cm__banner"
        style={{
          "--hue": hue,
          border: "none",
          width: "100%",
          cursor: "pointer",
          ...(c.thumbnailUrl && c.thumbnailUrl.startsWith("http")
            ? { backgroundImage: `url(${c.thumbnailUrl})`, backgroundSize: "cover", backgroundPosition: "center" }
            : null),
        } as React.CSSProperties}
        onClick={() => onSelect(c)}
        aria-label={`Open ${c.name}`}
      >
        <span className="cm__bannermark" aria-hidden="true">{c.name}</span>
      </button>
      <div className="cm__cardbody">
        <div className="cm__name u-truncate" title={c.name}>{c.name}</div>
        <div className="cm__meta">
          <span className="cm__vispill">
            <span className="cm__visicon" aria-hidden="true">{isPublic ? "\u{1F310}" : "\u{1F512}"}</span>
            <span className={"cm__vis" + (isPublic ? " is-public" : "")}>
              {isPublic ? "Public" : "Private"}
            </span>
          </span>
          <span className="cm__memberchip">
            <span className="cm__membicon" aria-hidden="true">&#x1F464;</span>
            {memberCountLabel(c.membersCount)} Members
          </span>
        </div>
        <div className="cm__actions">
          {isMember ? (
            <button className="cm__joined" type="button" onClick={() => onSelect(c)}>
              Joined<span className="cm__btncoin" aria-hidden="true">&#x25C6;</span>
            </button>
          ) : (
            <button className="cm__join" type="button" onClick={() => onSelect(c)}>
              {isPublic ? "Join" : "Request to join"}
              <span className="cm__btncoin" aria-hidden="true">&#x25C6;</span>
            </button>
          )}
        </div>
      </div>
    </article>
  );
}

function DetailView({
  detail,
  fallbackRow,
  onStart,
  onBack,
}: {
  detail: CommunityJoinDetail | null;
  fallbackRow: CommunityRow | null;
  onStart: () => void;
  onBack: () => void;
}) {
  const community = detail?.community ?? fallbackRow;
  if (!community) {
    return (
      <section className="cm__main" style={{ width: "100%" }}>
        <p style={{ color: "#fff" }}>Community not found.</p>
        <WizardControls>
          <button type="button" className="cmj__btn" onClick={onBack}>Back to browse</button>
        </WizardControls>
      </section>
    );
  }
  const isPublic = community.privacy !== "private";
  const isLive = community.isLive;
  const members = detail?.members ?? fallbackRow?.members ?? [];
  return (
    <section className="cm__main" style={{ width: "100%" }}>
      <header style={DETAIL_HEAD}>
        <div>
          <h1 className="cm__title" style={{ marginBottom: 4 }}>{community.name}</h1>
          <div style={{ color: "rgba(255,255,255,.85)", fontWeight: 600 }}>
            {isPublic ? "\u{1F310} Public" : "\u{1F512} Private"} &#xB7; {memberCountLabel(community.membersCount)} Members
            {isLive ? "  \u{B7}  \u{1F534} Live" : ""}
          </div>
          {community.ownerName ? (
            <div style={{ color: "rgba(255,255,255,.6)", fontSize: 13, marginTop: 2 }}>
              by {community.ownerName}
            </div>
          ) : null}
        </div>
        <button
          type="button"
          className="cmj__btn cmj__btn--primary"
          onClick={onStart}
        >
          {isPublic ? "Join" : "Request to join"}
        </button>
      </header>

      <p style={{ color: "rgba(255,255,255,.85)", maxWidth: 720, lineHeight: 1.5 }}>
        {community.description}
      </p>

      <div className="cm__section">
        <div className="cm__seclabel">Members ({members.length})</div>
        <ul style={MEMBERS_LIST}>
          {members.map((m) => (
            <li key={m.memberAddress} style={MEMBER_ROW}>
              <span
                style={{
                  ...MEMBER_AV,
                  ...(m.profilePictureUrl && m.profilePictureUrl.startsWith("http")
                    ? { backgroundImage: `url(${m.profilePictureUrl})`, backgroundSize: "cover", backgroundPosition: "center" }
                    : null),
                }}
                aria-hidden="true"
              />
              <span style={{ fontWeight: 600 }}>{m.name || m.memberAddress}</span>
              {m.role !== "member" ? (
                <span style={MEMBER_ROLE}>{m.role}</span>
              ) : null}
            </li>
          ))}
        </ul>
      </div>

      {isLive ? (
        <p style={{ color: "rgba(255,255,255,.7)", fontSize: 13 }}>
          &#x1F534; A live community voice stream is in progress.
        </p>
      ) : null}

      <WizardControls>
        <button type="button" className="cmj__btn" onClick={onBack}>Back to browse</button>
        <button type="button" className="cmj__btn cmj__btn--primary" onClick={onStart}>
          {isPublic ? "Join this community" : "Request to join"}
        </button>
      </WizardControls>
    </section>
  );
}

function ConfirmStep({
  community,
  action,
  real,
  onConfirm,
  onBack,
}: {
  community: CommunityRow | null;
  action: JoinAction;
  real: boolean;
  onConfirm: () => void;
  onBack: () => void;
}) {
  const name = community?.name ?? "this community";
  const isRequest = action === "request";
  return (
    <section className="cm__main" style={{ width: "100%" }}>
      <h1 className="cm__title">{isRequest ? "Request to join" : "Join"} {name}</h1>
      <p style={{ color: "rgba(255,255,255,.85)", maxWidth: 640, lineHeight: 1.5 }}>
        {isRequest
          ? `${name} is a private community. Your request will be sent to its moderators for approval.`
          : `You're about to join ${name}. You'll appear in the member roster and can join its chat and voice streams.`}
        {real ? "" : " (Simulated \u{2014} sign in to write.)"}
      </p>
      {real ? null : (
        <p style={SIM_NOTE}>
          SIMULATED commit &#x2014; the communities write routes require a DCL auth-chain
          signature. Sign in to perform a real write.
        </p>
      )}
      <WizardControls>
        <button type="button" className="cmj__btn" onClick={onBack}>Back</button>
        <button type="button" className="cmj__btn cmj__btn--primary" onClick={onConfirm}>
          {isRequest ? "Send request" : "Confirm join"}
        </button>
      </WizardControls>
    </section>
  );
}

function CommitProgress({
  community,
  action,
}: {
  community: CommunityRow | null;
  action: JoinAction;
}) {
  const name = community?.name ?? "community";
  return (
    <section className="cm__main" style={{ width: "100%" }}>
      <h1 className="cm__title">{action === "request" ? "Sending request\u{2026}" : "Joining\u{2026}"}</h1>
      <p style={{ color: "rgba(255,255,255,.85)" }}>
        {action === "request"
          ? `Submitting your request to join ${name}.`
          : `Adding you to ${name}.`}
      </p>
      <div className="cmj__spinner" aria-hidden="true" />
    </section>
  );
}

function DoneView({
  community,
  pending,
  onBrowseMore,
}: {
  community: CommunityRow | null;
  pending: boolean;
  onBrowseMore: () => void;
}) {
  const name = community?.name ?? "the community";
  return (
    <section className="cm__main" style={{ width: "100%" }}>
      <h1 className="cm__title">{pending ? "Request sent \u{2713}" : "Joined \u{2713}"}</h1>
      <p style={{ color: "rgba(255,255,255,.85)", maxWidth: 640, lineHeight: 1.5 }}>
        {pending
          ? `Your request to join ${name} has been submitted. A moderator will review it. (Simulated.)`
          : `You're now a member of ${name}. Jump into its chat and live streams. (Simulated.)`}
      </p>
      <WizardControls>
        <button type="button" className="cmj__btn cmj__btn--primary" onClick={onBrowseMore}>
          Browse more communities
        </button>
        <Link to="/discover" className="cmj__btn" style={{ textDecoration: "none" }}>
          Back to Discover
        </Link>
      </WizardControls>
    </section>
  );
}

function ErrorView({
  error,
  onRetry,
  onBack,
}: {
  error?: string;
  onRetry: () => void;
  onBack: () => void;
}) {
  return (
    <section className="cm__main" style={{ width: "100%" }}>
      <h1 className="cm__title">Something went wrong</h1>
      <p style={{ color: "rgba(255,255,255,.85)" }}>{error ?? "The commit failed."}</p>
      <WizardControls>
        <button type="button" className="cmj__btn" onClick={onBack}>Back</button>
        <button type="button" className="cmj__btn cmj__btn--primary" onClick={onRetry}>
          Retry
        </button>
      </WizardControls>
    </section>
  );
}

function WizardControls({ children }: { children: React.ReactNode }) {
  return (
    <div
      role="group"
      style={{ display: "flex", gap: 10, marginTop: 18, flexWrap: "wrap" }}
    >
      {children}
    </div>
  );
}

const DETAIL_HEAD: React.CSSProperties = {
  display: "flex",
  alignItems: "flex-start",
  justifyContent: "space-between",
  gap: 16,
  marginBottom: 12,
};
const MEMBERS_LIST: React.CSSProperties = {
  listStyle: "none",
  margin: 0,
  padding: 0,
  display: "grid",
  gridTemplateColumns: "repeat(auto-fill, minmax(180px, 1fr))",
  gap: 8,
};
const MEMBER_ROW: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 8,
  color: "#fff",
  background: "rgba(255,255,255,.06)",
  borderRadius: 8,
  padding: "8px 10px",
};
const MEMBER_AV: React.CSSProperties = {
  width: 28,
  height: 28,
  borderRadius: "50%",
  background: "linear-gradient(135deg,#ff2d55,#7a2bff)",
  flex: "0 0 auto",
};
const MEMBER_ROLE: React.CSSProperties = {
  marginLeft: "auto",
  fontSize: 11,
  fontWeight: 700,
  textTransform: "uppercase",
  color: "#ffd54a",
};
const SIM_NOTE: React.CSSProperties = {
  color: "rgba(255,255,255,.6)",
  fontSize: 12,
  fontStyle: "italic",
  maxWidth: 640,
};

export const UI3_SURFACES = { CommunityMembers, CommunityStream };
