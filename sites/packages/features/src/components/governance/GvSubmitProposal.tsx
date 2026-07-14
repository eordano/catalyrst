import { useEffect, useRef } from "react";
import { Link, useSearchParams } from "react-router";
import { href } from "@core/lib/router/routes";

import GovernanceChrome from "@ui/governance/frames/GovernanceChrome";
import { ChevronLeft } from "@ui/atoms/icons";
import "@ui/governance/pages/gvsubmitproposal.css";

import type {
  SubmitGroup,
  SubmitChooser,
} from "@data/lib/catalyst/governance/submit";
import { track } from "@core/lib/telemetry/track";
import CategoryBanner from "./CategoryBanner";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/submit-proposal";

const noop = () => {};

export type GvSubmitProposalProps = {
  sid: string;
  pageTitle: string;
  pageLead: string;
  group: string;
  groups: SubmitGroup[];
  totalCategories: number;
  chooser: SubmitChooser | null;
  chooserType: string;
  request: string;
};

export default function GvSubmitProposal({
  sid,
  pageTitle,
  pageLead,
  group,
  groups,
  totalCategories,
  chooser,
  chooserType,
  request,
}: GvSubmitProposalProps) {
  const [, setSearchParams] = useSearchParams();

  const lastGroup = useRef<string | null>(null);
  useEffect(() => {
    if (lastGroup.current === group) return;
    const firstView = lastGroup.current === null;
    lastGroup.current = group;
    track(
      "gv_submit_hub_viewed",
      { group: group || null, categories: totalCategories },
      { sid, story: STORY },
    );
    if (!firstView && group) {
      track(
        "gv_submit_group_filtered",
        { group },
        { sid, story: STORY },
      );
    }
  }, [sid, group, totalCategories]);

  const lastChooser = useRef<string | null>(null);
  useEffect(() => {
    const key = chooserType || "";
    if (lastChooser.current === key) return;
    lastChooser.current = key;
    if (chooserType) {
      track(
        "gv_submit_chooser_opened",
        { category: chooserType, request: request || null },
        { sid, story: STORY },
      );
    }
  }, [sid, chooserType, request]);

  function openChooser(type: string) {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.set("category", type);
        if (next.get("request") !== "remove") next.set("request", "add");
        return next;
      },
      { preventScrollReset: true },
    );
  }

  function closeChooser() {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.delete("category");
        next.delete("request");
        return next;
      },
      { preventScrollReset: true },
    );
  }

  function onCategorySelected(type: string, route: string) {
    track(
      "gv_submit_category_selected",
      { category: type, route, group: group || null },
      { sid, story: STORY },
    );
  }

  return (
    <GovernanceChrome active="proposals" onTab={noop}>
      <div className="gsp">
        <div className="gsp__back">
          <Link to={href("/governance/proposals")} className="gsp__backbtn" aria-label="Back to proposals">
            <ChevronLeft size={14} className="" />
          </Link>
        </div>

        <div className="gsp__container">
          <section className="gsp__section">
            <h1 className="gsp__h1">{pageTitle}</h1>
            <p className="gsp__lead">{pageLead}</p>
          </section>

          {groups.map((g) => (
            <section key={g.id} id={`group-${g.id}`} className="gsp__section">
              <p className="gsp__shead">{g.heading}</p>
              {g.categories.map((c) => {
                if (c.behavior === "chooser") {
                  return (
                    <CategoryBanner
                      key={c.type}
                      type={c.type}
                      title={c.title}
                      description={c.description}
                      active={c.active}
                      isNew={c.isNew ?? undefined}
                      paused={c.paused}
                      onActivate={() => openChooser(c.type)}
                      onSelect={() =>
                        onCategorySelected(c.type, `/governance/submit/${c.type}?request=add`)
                      }
                    />
                  );
                }
                return (
                  <CategoryBanner
                    key={c.type}
                    type={c.type}
                    title={c.title}
                    description={c.description}
                    active={c.active}
                    isNew={c.isNew ?? undefined}
                    paused={c.paused}
                    to={c.route ?? undefined}
                    onSelect={() => onCategorySelected(c.type, c.route ?? "")}
                  />
                );
              })}
            </section>
          ))}
        </div>
      </div>

      {chooser && (
        <div
          className="gsp__scrim"
          role="dialog"
          aria-modal="true"
          aria-label={chooser.title}
          onClick={closeChooser}
        >
          <div className="gsp__modal" onClick={(e) => e.stopPropagation()}>
            <button
              type="button"
              className="gsp__modalclose"
              aria-label="Close"
              onClick={closeChooser}
            >
              <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
                <path
                  d="M4 4l8 8M12 4l-8 8"
                  stroke="currentColor"
                  strokeWidth="1.6"
                  strokeLinecap="round"
                />
              </svg>
            </button>
            <div className="gsp__modalcontent">
              <h2 className="gsp__modaltitle">{chooser.title}</h2>
              <p className="gsp__modaldesc">{chooser.prompt}</p>
            </div>
            <div className="gsp__modalactions">
              {chooser.options.map((o) => (
                <CategoryBanner
                  key={o.type}
                  type={o.type}
                  title={o.title}
                  description={o.description}
                  active={o.active}
                  paused={o.paused}
                  to={o.active ? o.route : undefined}
                  onSelect={() =>
                    onCategorySelected(`${chooserType}_${o.request}`, o.route)
                  }
                />
              ))}
            </div>
          </div>
        </div>
      )}
    </GovernanceChrome>
  );
}
