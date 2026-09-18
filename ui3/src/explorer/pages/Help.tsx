import { useState } from "react";
import { siteUrl } from "../../data/site";
import Icon from "../frames/SidebarDesignIcon";
import { SIDEBAR_GUIDE } from "../frames/sidebarDesignGuide";
import { HELP_SHORTCUT_GROUPS, type Shortcut } from "./shortcuts";
import "./help.css";

export const HELP_LINKS = [
  { id: "shortcuts", label: "Shortcuts & chat commands", path: "/docs/player/decentraland-in-world/shortcuts-and-chat-commands/" },
  { id: "docs", label: "Documentation", path: "/docs/" },
  { id: "support", label: "Support center", path: "/support" },
] as const;

function Keys({ keys }: { keys: string[] }) {
  return (
    <span className="hp__keys">
      {keys.map((k, i) =>
        k === "/" ? (
          <span key={i} className="hp__keysep">/</span>
        ) : (
          <kbd key={i} className={"hp__key" + (k.length > 2 ? " hp__key--wide" : "")}>{k}</kbd>
        ),
      )}
    </span>
  );
}

function ShortcutRows({ rows }: { rows: Shortcut[] }) {
  return (
    <div className="hp__rows">
      {rows.map((s) => (
        <div className="hp__row" key={s.action}>
          <span className="hp__action">{s.action}</span>
          <Keys keys={s.keys} />
        </div>
      ))}
    </div>
  );
}

export default function Help({ initialSection = "controls" }: { initialSection?: string }) {
  const [section, setSection] = useState(initialSection);
  return (
    <section className="hp" aria-labelledby="help-title">
      <header className="hp__head">
        <h1 id="help-title" className="hp__title">Help &amp; Support</h1>
        <p className="hp__lede">
          Controls, shortcuts and what every sidebar button does. Press <kbd className="hp__key">Esc</kbd> to go back to the world.
        </p>
      </header>

      <nav className="hp__sections" aria-label="Help sections"><button type="button" aria-pressed={section === "controls"} onClick={() => setSection("controls")}>Mouse / Key Controls</button><button type="button" aria-pressed={section === "sidebar"} onClick={() => setSection("sidebar")}>Sidebar guide</button></nav>
      {section === "controls" && <div className="hp__grid">
        {HELP_SHORTCUT_GROUPS.map((g) => (
          <section className="hp__card" key={g.id} aria-labelledby={"hp-" + g.id}>
            <h2 className="hp__cardtitle" id={"hp-" + g.id}>{g.title}</h2>
            <ShortcutRows rows={g.rows} />
          </section>
        ))}
      </div>}

      {section === "sidebar" && <section className="hp__guide" aria-labelledby="hp-guide">
        <h2 className="hp__sectiontitle" id="hp-guide">What&apos;s in the sidebar</h2>
        <ul className="hp__guidelist">
          {SIDEBAR_GUIDE.map((g) => (
            <li className="hp__guideitem" key={g.label}>
              <span className="hp__glyph" aria-hidden="true">
                {g.icon ? <Icon name={g.icon} /> : <span className="hp__avatar" />}
              </span>
              <div className="hp__guidetext">
                <div className="hp__guidelabel">
                  {g.label}
                  {g.shortcut ? <kbd className="hp__key hp__key--inline">{g.shortcut}</kbd> : null}
                </div>
                <p className="hp__guidehelp">{g.help}</p>
              </div>
            </li>
          ))}
        </ul>
      </section>}

      <section className="hp__support" aria-labelledby="hp-support">
        <h2 className="hp__sectiontitle" id="hp-support">Need more help?</h2>
        <div className="hp__links">
          {HELP_LINKS.map((l) => (
            <a
              key={l.id}
              className="hp__link"
              href={siteUrl(l.path)}
              target="_blank"
              rel="noopener noreferrer"
            >
              {l.label}
            </a>
          ))}
        </div>
      </section>
    </section>
  );
}
