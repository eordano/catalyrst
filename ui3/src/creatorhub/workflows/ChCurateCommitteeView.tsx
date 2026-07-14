import { useEffect, useId, useState } from "react";

import type { MouseEvent } from "react";

import ChCuration from "../pages/ChCuration";
import Button from "../../atoms/Button";
import "./chcuratecommitteeview.css";

type BdDisplayState =
  | "to_review"
  | "under_review"
  | "approved"
  | "rejected"
  | "disabled";

type BdCurationComment = {
  id: string;
  collection_id: string;
  author: string;
  authorName: string;
  decision: "approved" | "rejected" | null;
  raw: string;
  topic_id: number | null;
  created_at: string;
};

type BdCommitteeRow = {
  id: string;
  name: string;
  type: "standard" | "third_party";
  isProgrammatic?: boolean;
  status: string | null;
  count: number;
  owner: string | null;
  curationStatus: BdDisplayState;
  assignee: string | null;
  assigneeName?: string;
  you?: boolean;
  date: string;
  ago: string;
  createdAtMs?: number | null;
  forumLink: string | null;
  forumTopicId: number | null;
  thumbs: string[];
  comments: BdCurationComment[];
};

type BdPostedComment = { postId: number; link: string; raw: string };

type ChCurateCommitteeViewProps = {
  view?: string;
  step?: string;
  isCommittee?: boolean;
  collections?: BdCommitteeRow[];
  collectionsKey?: string;
  filters?: { status: string; type: string; assignee: string };
  onDashboardClick?: (e: MouseEvent<HTMLDivElement>) => void;
  activeRow?: BdCommitteeRow;
  activeId?: string;
  builderHref?: string;
  decision?: "approved" | "rejected";
  draftComment?: string;
  authorName?: string;
  error?: string;
  postedComment?: BdPostedComment;
  onBack?: () => void;
  onDraftReject?: () => void;
  onDraftApprove?: () => void;
  onChangeComment?: (comment: string) => void;
  onSubmit?: () => void;
  onBackToQueue?: () => void;
};

export default function ChCurateCommitteeView({
  view = "dashboard",
  step = "dashboard",
  isCommittee = true,
  collections = [],
  collectionsKey = "",
  filters = undefined,
  onDashboardClick = undefined,
  activeRow = undefined,
  activeId = undefined,
  builderHref = "#",
  decision = undefined,
  draftComment = "",
  authorName = "",
  error = undefined,
  postedComment = undefined,
  onBack = undefined,
  onDraftReject = undefined,
  onDraftApprove = undefined,
  onChangeComment = undefined,
  onSubmit = undefined,
  onBackToQueue = undefined,
}: ChCurateCommitteeViewProps) {
  const reviewTitleId = useId();
  const commentTitleId = useId();
  const doneTitleId = useId();

  if (view === "dashboard" || view === "assigning") {
    return (
      <div
        className="curw"
        data-step={step}
        onClick={view === "dashboard" ? onDashboardClick : undefined}
      >
        {view === "dashboard" && error ? (
          <p className="curw__error curw__error--banner" role="alert">
            Couldn&apos;t assign this collection to you &#x2014; try again.
          </p>
        ) : null}
        <ChCuration
          key={collectionsKey}
          embedded
          collections={isCommittee ? collections : []}
          loading={false}
          initialStatus={filters?.status}
          initialType={filters?.type}
          initialAssignee={filters?.assignee}
        />
        {view === "assigning" && (
          <p className="curw__hint" role="status" aria-live="polite">
            Saving your assignment&#x2026; (simulated &#x2014; recorded locally only, not on
            the Builder server)
          </p>
        )}
      </div>
    );
  }

  if (view === "reviewing") {
    const count = activeRow?.count ?? 0;
    return (
      <div className="curw" data-step={step}>
        <section className="curw__panel" aria-labelledby={reviewTitleId}>
          <p className="curw__eyebrow">Curation review</p>
          <h1 id={reviewTitleId} className="curw__title">
            Review &#x201C;{activeRow?.name ?? activeId}&#x201D;
          </h1>
          <p className="curw__lead">
            {count} item{count === 1 ? "" : "s"} &#xB7;{" "}
            {activeRow ? DISPLAY_LABEL[activeRow.curationStatus] : "To review"}. Open{" "}
            <a className="curw__deeplink" href={builderHref}>
              the collection's items
            </a>{" "}
            for review.
          </p>

          <CommentThread comments={activeRow?.comments ?? []} />

          <div className="curw__controls" role="group" aria-label="Decision">
            <Button variant="secondary" onClick={onBack}>
              Back to dashboard
            </Button>
            <Button
              variant="secondary"
              className="curw__btn--reject"
              onClick={onDraftReject}
            >
              Reject with comment&#x2026;
            </Button>
            <Button
              variant="secondary"
              className="curw__btn--approve"
              onClick={onDraftApprove}
            >
              Approve with comment&#x2026;
            </Button>
          </div>
        </section>
      </div>
    );
  }

  if (view === "commenting") {
    const dec = decision ?? "approved";
    return (
      <div className="curw" data-step={step}>
        <section className="curw__panel" aria-labelledby={commentTitleId}>
          <p className="curw__eyebrow">
            {dec === "approved" ? "Approving" : "Rejecting"} &#x201C;{activeRow?.name ?? activeId}&#x201D;
          </p>
          <h1 id={commentTitleId} className="curw__title">
            Add a {dec === "approved" ? "comment" : "reason"}
          </h1>
          <p className="curw__lead">
            On a real committee decision the comment would go to the
            collection's forum topic
            {activeRow?.forumTopicId ? ` (#${activeRow.forumTopicId})` : ""}.
            This is a preview &#x2014; submitting here posts nothing to the forum.
          </p>

          <CommentComposer
            decision={dec}
            value={draftComment}
            onChange={onChangeComment}
            authorName={authorName}
          />

          {error ? (
            <p className="curw__error" role="alert">
              {error} &#x2014; could not save your decision; your draft was kept,
              try again.
            </p>
          ) : null}

          <div className="curw__controls" role="group" aria-label="Submit decision">
            <Button variant="secondary" onClick={onBack}>
              Back to review
            </Button>
            <Button
              variant="secondary"
              className={dec === "approved" ? "curw__btn--approve" : "curw__btn--reject"}
              onClick={onSubmit}
            >
              {draftComment.trim()
                ? `Submit ${dec} + comment`
                : `Submit ${dec} without comment`}
            </Button>
          </div>
        </section>
      </div>
    );
  }

  if (view === "deciding") {
    return (
      <div className="curw" data-step={step}>
        <section className="curw__status" aria-live="polite">
          <div className="curw__spinner" aria-hidden="true" />
          <h1 className="curw__title">
            {decision === "rejected" ? "Rejecting" : "Approving"} items&#x2026;
          </h1>
          <p className="curw__lead">
            Recording the curation decision
            {draftComment.trim() ? " and your comment" : ""} locally &#x2014; this is
            a preview, nothing is posted to the forum.
          </p>
        </section>
      </div>
    );
  }

  const decided = decision ?? "approved";
  return (
    <div className="curw" data-step={step}>
      <section className="curw__status" aria-labelledby={doneTitleId}>
        <svg
          className={`curw__bigmark curw__bigmark--${decided}`}
          viewBox="0 0 64 64"
          width="64"
          height="64"
          aria-hidden="true"
        >
          <circle cx="32" cy="32" r="29" fill="none" stroke="currentColor" strokeWidth="4" />
          {decided === "approved" ? (
            <path
              d="M20 33l8 8 16-18"
              fill="none"
              stroke="currentColor"
              strokeWidth="4"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          ) : (
            <path d="M22 22l20 20M42 22L22 42" fill="none" stroke="currentColor" strokeWidth="4" strokeLinecap="round" />
          )}
        </svg>
        <h1 id={doneTitleId} className="curw__title">
          {decided === "approved" ? "Approved" : "Rejected"} &#x201C;{activeRow?.name ?? activeId}&#x201D;
        </h1>
        <p className="curw__lead">
          This is a preview &#x2014; your <strong>{decided}</strong> decision
          {postedComment ? " and comment were" : " was"} recorded locally
          only. Nothing was posted to the collection's forum topic.
        </p>
        {postedComment ? (
          <blockquote className="curw__quote">{postedComment.raw}</blockquote>
        ) : null}
        <div className="curw__controls">
          <Button variant="primary" onClick={onBackToQueue}>
            Back to the curation queue
          </Button>
        </div>
      </section>
    </div>
  );
}

function CommentThread({ comments = [] }: { comments?: BdCurationComment[] }) {
  if (!comments.length) {
    return (
      <p className="curw__nocomments" aria-live="polite">
        No committee comments yet &#x2014; yours will be the first.
      </p>
    );
  }
  return (
    <ul className="curw__thread" aria-label="Committee comments">
      {comments.map((c) => (
        <li key={c.id} className="curw__comment">
          <div className="curw__commenthead">
            <span className="curw__commentauthor">{c.authorName}</span>
            {c.decision ? (
              <span className={`curw__tag curw__tag--${c.decision}`}>{c.decision}</span>
            ) : null}
          </div>
          <p className="curw__commentbody">{c.raw}</p>
        </li>
      ))}
    </ul>
  );
}

const COMMENT_MAX_LENGTH = 2000;

function CommentComposer({
  decision,
  value,
  onChange,
  authorName,
}: {
  decision: "approved" | "rejected";
  value: string;
  onChange?: (comment: string) => void;
  authorName: string;
}) {
  const [local, setLocal] = useState(value);
  const commentId = useId();
  useEffect(() => {
    setLocal(value);
  }, [value]);
  const remaining = COMMENT_MAX_LENGTH - local.length;
  return (
    <div className="curw__composer">
      <label className="curw__label" htmlFor={commentId}>
        Comment as {authorName}
      </label>
      <textarea
        id={commentId}
        className="curw__textarea"
        rows={5}
        maxLength={COMMENT_MAX_LENGTH}
        placeholder={
          decision === "approved"
            ? "Optional: note anything the creator should know (mesh, rig, theme)\u{2026}"
            : "Explain what needs to change so the creator can resubmit\u{2026}"
        }
        value={local}
        onChange={(e) => {
          setLocal(e.target.value);
          onChange?.(e.target.value);
        }}
      />
      <p className="curw__count">{remaining} characters remaining</p>
    </div>
  );
}

const DISPLAY_LABEL: Record<BdDisplayState, string> = {
  to_review: "To review",
  under_review: "Under Review",
  approved: "Approved",
  rejected: "Rejected",
  disabled: "Disabled",
};
