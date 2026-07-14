import { useId } from "react";
import "./fdideacard.css";

type FdIdeaCardProps = {
  num: string;
  name: string;
  oneLiner: string;
  surfaceLabel?: string;
  surfaceHref?: string;
  expanded: boolean;
  onToggle: () => void;
};

export default function FdIdeaCard({
  num,
  name,
  oneLiner,
  surfaceLabel,
  surfaceHref,
  expanded,
  onToggle,
}: FdIdeaCardProps) {
  const bodyId = useId();
  return (
    <article className={"fd-idea" + (expanded ? " is-expanded" : "")}>
      <button
        type="button"
        className="fd-idea__head"
        aria-expanded={expanded}
        aria-controls={bodyId}
        onClick={onToggle}
      >
        <span className="fd-idea__num">{num}</span>
        <span className="fd-idea__name">{name}</span>
        <span className="fd-idea__caret" aria-hidden="true">
          {expanded ? "−" : "+"}
        </span>
      </button>

      <div className="fd-idea__body" id={bodyId} hidden={!expanded}>
        <p className="fd-idea__line">{oneLiner}</p>
        {surfaceHref && surfaceLabel ? (
          <a className="fd-idea__link" href={surfaceHref}>
            {surfaceLabel} →
          </a>
        ) : null}
      </div>
    </article>
  );
}
