import GovernanceChrome from "../frames/GovernanceChrome";
import "./gvnotfound.css";

const TITLE = "Not found";
const DESCRIPTION = "You just hit a route that doesn't exist...";

type GvNotFoundProps = {
  title?: string;
  description?: string;
};

export default function GvNotFound({
  title = TITLE,
  description = DESCRIPTION,
}: GvNotFoundProps) {
  return (
    <GovernanceChrome active={null}>
      <div className="gvnf__container">
        <div className="gvnf">
          <div className="gvnf__glyph" aria-hidden="true">
            <svg viewBox="0 0 240 96" width="240" height="96">
              <text
                x="120"
                y="78"
                textAnchor="middle"
                className="gvnf__glyphtext"
              >
                404
              </text>
            </svg>
          </div>

          <p className="gvnf__title">{title}</p>

          <p className="gvnf__desc">{description}</p>
        </div>
      </div>
    </GovernanceChrome>
  );
}
