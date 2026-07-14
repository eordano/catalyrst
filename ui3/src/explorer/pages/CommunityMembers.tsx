import type { CSSProperties } from "react";
import { useEffect, useRef, useState } from "react";
import SearchField from "../../atoms/SearchField";
import { Avatar, fmt } from "../../atoms/primitives";
import UpcomingEventCard from "../../web/components/UpcomingEventCard";
import ContextMenu from "../../components/ContextMenu";
import type { ContextMenuItem } from "../../components/ContextMenu";
import "./communitymembers.css";

const TABS = [
  { id: "announcements", label: "Announcements" },
  { id: "members", label: "Members" },
  { id: "places", label: "Places" },
  { id: "photos", label: "Photos" },
];

type Role = "Owner" | "Moderator" | "Member";
type Member = { name: string; role: Role; online: boolean; friend: boolean; verified: boolean; hue: number };

const MEMBERS: Member[] = [
  { name: "DCLDfuse", role: "Owner", online: true, friend: false, verified: true, hue: 320 },
  { name: "Kimbo", role: "Moderator", online: true, friend: false, verified: true, hue: 270 },
  { name: "KaijuWolfson", role: "Moderator", online: true, friend: false, verified: false, hue: 285 },
  { name: "Sephiroten", role: "Moderator", online: false, friend: false, verified: false, hue: 200 },
  { name: "MET.kAEZ.f", role: "Member", online: false, friend: true, verified: true, hue: 95 },
  { name: "Rashiyo", role: "Member", online: true, friend: false, verified: false, hue: 35 },
  { name: "izumimusa", role: "Member", online: true, friend: false, verified: false, hue: 190 },
  { name: "Souldarsoa", role: "Member", online: false, friend: false, verified: false, hue: 255 },
  { name: "Eve.0", role: "Member", online: true, friend: false, verified: false, hue: 330 },
  { name: "KaldhiyaShae", role: "Member", online: false, friend: false, verified: false, hue: 145 },
  { name: "Thoot.d", role: "Member", online: false, friend: false, verified: false, hue: 215 },
  { name: "siorra", role: "Member", online: true, friend: false, verified: false, hue: 120 },
];

type Post = { author: string; when: string; likes: number; text: string; verified: boolean; hue: number };

const POSTS: Post[] = [
  {
    author: "DCLDfuse",
    when: "Jun 21",
    likes: 42,
    verified: true,
    hue: 320,
    text: "Pride month is here! Join us all week for watch parties, DJ sets, and community meet-ups across Genesis Plaza.",
  },
  {
    author: "Kimbo",
    when: "Jun 18",
    likes: 17,
    verified: true,
    hue: 270,
    text: "Reminder: Career Mondays kicks off at 8PM \u{2014} this week we're talking AI & automation in virtual production.",
  },
];

type PlaceItem = { name: string; addedBy: string; hue: number };

const PLACES: PlaceItem[] = [
  { name: "Genesis Plaza", addedBy: "DCLDfuse", hue: 280 },
  { name: "Fashion Street", addedBy: "Kimbo", hue: 200 },
  { name: "Wonderzone", addedBy: "Sephiroten", hue: 40 },
];

type EventItem = { title: string; when: string; started?: string; live?: boolean; hue: number };

const EVENTS: EventItem[] = [
  { title: "Celebrate Pride in Decentra\u{2026}", when: "", started: "Started 10 hour ago", live: true, hue: 330 },
  { title: "Monthly Community Meet-Up", when: "MON, JUN 23 @ 6:00PM", hue: 280 },
  { title: "Career Mondays: AI, Autom\u{2026}", when: "MON, JUN 23 @ 8:00PM", hue: 210 },
  { title: "PRIDE: Join Watch Party We\u{2026}", when: "WED, JUN 24 @ 8:00PM", hue: 350 },
  { title: "PRIDE: Join Watch Party We\u{2026}", when: "WED, JUN 24 @ 10:00PM", hue: 300 },
  { title: "PRIDE EDITION: Watch scar\u{2026}", when: "FRI, JUN 26 @ 7:00AM", hue: 40 },
];

const ROLE_CLASS: Record<Role, string> = { Owner: "is-owner", Moderator: "is-mod", Member: "" };

function ChatIcon() {
  return (
    <svg viewBox="0 0 24 24" width="16" height="16" fill="none" aria-hidden="true">
      <path d="M20 2H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h4l4 4 4-4h4c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2z" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" />
    </svg>
  );
}

function CopyIcon() {
  return (
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" aria-hidden="true">
      <rect x="9" y="9" width="11" height="11" rx="2" stroke="currentColor" strokeWidth="1.7" />
      <path d="M5 15V5a2 2 0 0 1 2-2h8" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
    </svg>
  );
}

function Verified() {
  return (
    <svg className="cmb__verified" viewBox="0 0 24 24" width="13" height="13" aria-label="verified">
      <path d="M12 2l2.4 1.8 3-.3 1 2.8 2.6 1.5-.9 2.9.9 2.9-2.6 1.5-1 2.8-3-.3L12 22l-2.4-1.8-3 .3-1-2.8L3 16.2l.9-2.9L3 10.4l2.6-1.5 1-2.8 3 .3L12 2z" fill="#4d8dff" />
      <path d="M8.5 12l2.2 2.2 4.5-4.5" stroke="#fff" strokeWidth="1.8" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function HeaderMenu() {
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return undefined;
    const onDown = (e: PointerEvent) => {
      if (wrapRef.current && e.target instanceof Node && !wrapRef.current.contains(e.target)) setOpen(false);
    };
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [open]);

  const items: ContextMenuItem[] = [
    {
      kind: "button",
      label: copied ? "Copied!" : "Copy link",
      icon: <CopyIcon />,
      onClick: () => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1200);
      },
    },
    { kind: "separator" },
    { kind: "button", label: "Leave community", danger: true, onClick: () => setOpen(false) },
  ];

  return (
    <div className="cmb__kebabwrap" ref={wrapRef}>
      <button type="button" className="cmb__kebab" aria-label="More options" aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        &#x22EE;
      </button>
      {open && (
        <div className="cmb__menuanchor">
          <ContextMenu items={items} onClose={() => setOpen(false)} autoFocus />
        </div>
      )}
    </div>
  );
}

export default function CommunityMembers() {
  const [tab, setTab] = useState("members");

  return (
    <div className="cmb__backdrop">
      <div className="cmb">
        <button className="cmb__close" aria-label="Close" data-sb-linkto="Explorer/Pages/Communities">&#xD7;</button>

        <div className="cmb__main">
          <header className="cmb__header">
            <span className="cmb__thumb" />
            <div className="cmb__headinfo">
              <h2 className="cmb__cname">Decentraland Foundation</h2>
              <div className="cmb__meta">
                <svg className="cmb__globe" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="2">
                  <circle cx="12" cy="12" r="9" /><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18" />
                </svg>
                Public &#xB7; {fmt(1200)} Members
              </div>
              <p className="cmb__about">
                Your hub for official Decentraland updates and events
                <br />
                Keep up with All Hands, Art Week, Music Festival, Fashion Week, workshops, and more&#x2014;all in one place.
              </p>
            </div>
            <div className="cmb__headactions">
              <button className="cmb__join" type="button">&#x2713; JOINED</button>
              <button type="button" className="cmb__chatbtn" aria-label="Open chat" title="Open chat">
                <ChatIcon />
              </button>
              <HeaderMenu />
            </div>
          </header>

          <nav className="cmb__tabs" role="tablist">
            {TABS.map((t) => (
              <button
                key={t.id}
                role="tab"
                aria-selected={t.id === tab}
                className={"cmb__tab" + (t.id === tab ? " is-active" : "")}
                onClick={() => setTab(t.id)}
              >
                {t.label}
              </button>
            ))}
          </nav>

          <div className="cmb__body">
            {tab === "members" ? (
              <ul className="cmb__grid">
                {MEMBERS.map((m) => (
                  <li className="cmb__row" key={m.name} data-sb-linkto="Explorer/Pages/Passport">
                    <Avatar hue={m.hue} size={44} status={m.online ? "online" : "offline"} />
                    <div className="cmb__info">
                      <div className="cmb__name">
                        <span className="u-truncate">{m.name}</span>
                        {m.verified && <Verified />}
                      </div>
                      {m.role !== "Member" && (
                        <span className={"cmb__role " + ROLE_CLASS[m.role]}>{m.role}</span>
                      )}
                    </div>
                    {m.friend ? (
                      <span className="cmb__add is-friend">Already Friend</span>
                    ) : (
                      <button className="cmb__add">
                        <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
                          <path d="M12 4a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM4 20a8 8 0 0 1 16 0z" />
                        </svg>
                        ADD FRIEND
                      </button>
                    )}
                  </li>
                ))}
              </ul>
            ) : tab === "announcements" ? (
              <ul className="cmb__posts">
                {POSTS.map((p, i) => (
                  <li className="cmb__post" key={i}>
                    <Avatar hue={p.hue} size={36} />
                    <div className="cmb__postbody">
                      <div className="cmb__posthead">
                        <span className="cmb__postauthor">{p.author}</span>
                        {p.verified && <Verified />}
                        <span className="cmb__postdate">&#xB7; {p.when}</span>
                        <span className="cmb__postlikes">&#x2665; {p.likes}</span>
                      </div>
                      <p className="cmb__posttext">{p.text}</p>
                    </div>
                  </li>
                ))}
              </ul>
            ) : tab === "places" ? (
              <div className="cmb__placegrid">
                {PLACES.map((p) => (
                  <div className="cmb__place" key={p.name}>
                    <div className="cmb__placecover" style={{ "--hue": p.hue } as CSSProperties & { "--hue": number }} />
                    <span className="cmb__placename u-truncate" title={p.name}>{p.name}</span>
                    <span className="cmb__placemeta">Added by {p.addedBy}</span>
                  </div>
                ))}
              </div>
            ) : (
              <div className="cmb__empty">{TABS.find((t) => t.id === tab)?.label}</div>
            )}
          </div>

          {tab === "members" && (
            <div className="cmb__search">
              <SearchField placeholder="Search member or name" />
            </div>
          )}
        </div>

        <aside className="cmb__events">
          <h3 className="cmb__eventstitle">Upcoming Events</h3>
          <div className="cmb__eventlist">
            {EVENTS.map((e, i) => (
              <UpcomingEventCard key={i} event={e} />
            ))}
          </div>
        </aside>
      </div>
    </div>
  );
}
