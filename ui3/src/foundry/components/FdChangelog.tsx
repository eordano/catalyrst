import "./fdchangelog.css";

export type FdChangelogEntryVM = {
  at: string;
  ask: string;
  change: string;
  origin: "imported" | "visitor";
};

type FdChangelogProps = {
  entries: readonly FdChangelogEntryVM[];
  className?: string;
};

export default function FdChangelog({ entries, className = "" }: FdChangelogProps) {
  return (
    <ol className={"fd-changelog" + (className ? " " + className : "")}>
      {entries.map((entry, i) => (
        <li key={entry.at + i} className="fd-changelog__item">
          <span className="fd-changelog__at fd-mono">{entry.at}</span>
          <div className="fd-changelog__pair">
            <p className="fd-changelog__ask">
              <span className="fd-changelog__lead">You asked</span>
              {entry.ask}
            </p>
            <p className="fd-changelog__change">
              <span className="fd-changelog__lead">We changed</span>
              {entry.change}
            </p>
          </div>
          <div className="fd-changelog__origin">
            <span className="fd-chip">
              {entry.origin === "visitor" ? "shipped from a session" : "imported"}
            </span>
          </div>
        </li>
      ))}
    </ol>
  );
}
