import { useCallback, useEffect, useRef, useState } from "react";
import { CreatorHubChromeMaybe } from "../frames/CreatorHubChrome";
import { ChevronLeft, Close } from "../../atoms/icons";
import { useDialogKeys } from "../../components/useDialogKeys";
import { asset } from "../../asset";
import "./chtemplates.css";

const ThreeDots = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
    <circle cx="12" cy="5" r="1.7" fill="currentColor" />
    <circle cx="12" cy="12" r="1.7" fill="currentColor" />
    <circle cx="12" cy="19" r="1.7" fill="currentColor" />
  </svg>
);

type DropdownOption = { text: string; href?: string; handler?: () => void };

type TemplateProjectCardProps = {
  title: string;
  description?: string;
  thumb?: string;
  tags?: string[];
  dropdownOptions?: DropdownOption[];
  onClick?: () => void;
  isNewScene?: boolean;
};

function ProjectCard({ title, description, thumb, tags, dropdownOptions, onClick, isNewScene }: TemplateProjectCardProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const menuWrapRef = useRef<HTMLDivElement>(null);
  const menuBtnRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!menuOpen) return undefined;
    const items = () =>
      [...(menuWrapRef.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];
    items()[0]?.focus();
    const onPointerDown = (e: PointerEvent) => {
      if (menuWrapRef.current && !menuWrapRef.current.contains(e.target as Node)) setMenuOpen(false);
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        setMenuOpen(false);
        menuBtnRef.current?.focus();
        return;
      }
      if (e.key === "ArrowDown" || e.key === "ArrowUp" || e.key === "Home" || e.key === "End") {
        const list = items();
        if (!list.length) return;
        e.preventDefault();
        const idx = list.indexOf(document.activeElement as HTMLElement);
        const next =
          e.key === "Home" ? 0 :
          e.key === "End" ? list.length - 1 :
          e.key === "ArrowDown" ? (idx + 1) % list.length :
          (idx - 1 + list.length) % list.length;
        list[next]?.focus();
      }
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [menuOpen]);

  return (
    <div className="chtpl__card">
      <button type="button" className="chtpl__open" aria-label={`Open ${title}`} onClick={onClick}>
        <div
          className={"chtpl__thumb" + (isNewScene ? " chtpl__thumb--newscene" : "")}
          style={thumb ? { backgroundImage: `url(${thumb})` } : undefined}
        />
        <div className="chtpl__info">
          <div className="chtpl__cardtitle">
            <span className="chtpl__cardtitletext u-truncate">{title}</span>
          </div>
          {description ? <p className="chtpl__desc">{description}</p> : null}
          {tags?.length ? (
            <div className="chtpl__tags">
              {tags.map((tag) => (
                <span key={tag} className="chtpl__chip chtpl__chip--tag">
                  {tag}
                </span>
              ))}
            </div>
          ) : null}
        </div>
      </button>
      {dropdownOptions?.length ? (
        <div className="chtpl__menu" ref={menuWrapRef}>
          <button
            ref={menuBtnRef}
            type="button"
            className="chtpl__iconbtn"
            aria-label="options"
            aria-haspopup="menu"
            aria-expanded={menuOpen}
            onClick={(e) => {
              e.stopPropagation();
              setMenuOpen((o) => !o);
            }}
          >
            <ThreeDots />
          </button>
          {menuOpen && (
            <div className="chtpl__dropdown" role="menu">
              {dropdownOptions.map((opt) =>
                opt.href ? (
                  <a
                    key={opt.text}
                    role="menuitem"
                    className="chtpl__dropitem"
                    href={opt.href}
                    target="_blank"
                    rel="noreferrer"
                    onClick={(e) => {
                      e.stopPropagation();
                      setMenuOpen(false);
                      opt.handler?.();
                    }}
                  >
                    {opt.text}
                  </a>
                ) : (
                  <button
                    key={opt.text}
                    type="button"
                    role="menuitem"
                    className="chtpl__dropitem"
                    onClick={(e) => {
                      e.stopPropagation();
                      setMenuOpen(false);
                      opt.handler?.();
                    }}
                  >
                    {opt.text}
                  </button>
                ),
              )}
            </div>
          )}
        </div>
      ) : null}
    </div>
  );
}

type ModalValue = { template: ChTemplate | null; name: string };

type CreateProjectModalProps = {
  value: ModalValue;
  onClose?: () => void;
  onCreate?: (values: { template: ChTemplate | null }) => void;
};

function CreateProjectModal({ value, onClose, onCreate }: CreateProjectModalProps) {
  const submit = () => onCreate?.({ template: value.template ?? null });
  const modalRef = useRef<HTMLDivElement>(null);
  useDialogKeys(modalRef, onClose);
  return (
    <div className="chtpl__backdrop" onClick={onClose}>
      <div
        className="chtpl__modal"
        role="dialog"
        aria-modal="true"
        aria-label="Create Scene"
        tabIndex={-1}
        ref={modalRef}
        onClick={(e) => e.stopPropagation()}
      >
        <header className="chtpl__modalhead">
          <h2 className="chtpl__modaltitle">Create Scene</h2>
          <button type="button" className="chtpl__iconbtn chtpl__modalclose" aria-label="close" onClick={onClose}>
            <Close size={20} />
          </button>
        </header>
        <div className="chtpl__modalbody">
          <p className="chtpl__desc">
            You&apos;re starting a new scene from <strong>{value.name}</strong>.
          </p>
          <p className="chtpl__desc">
            {value.template
              ? "The editor opens with this template's scene content already placed \u{2014} press Play there to run its starter game, rename the scene any time and use Save to keep it in your library."
              : "The editor opens on an empty scene \u{2014} use Save to keep it in your library."}
          </p>
        </div>
        <footer className="chtpl__modalactions">
          <button type="button" className="chtpl__btn chtpl__btn--secondary" onClick={onClose}>
            Cancel
          </button>
          <button type="button" className="chtpl__btn chtpl__btn--primary" onClick={submit}>
            Create
          </button>
        </footer>
      </div>
    </div>
  );
}

type ChTemplate = {
  id: string;
  title: string;
  description?: string;
  tags?: string[];
  difficulty_level?: string;
  date_created?: string;
  play_link?: string | null;
  github_link?: string;
  thumb?: string;
};

const TEMPLATES: ChTemplate[] = [
  {
    id: "nft-art-wall",
    title: "Art Wall",
    description: "A curated gallery layout \u{2014} white walls, pillars and three placeholder canvases \u{2014} plus a small SDK7 starter script that cycles each canvas and shows how to swap one for a real NFT frame. Press Play in the editor to run it.",
    tags: ["Showcase"],
    difficulty_level: "Easy",
    date_created: "2024-08-22",
    play_link: null,
    github_link: "https://github.com/decentraland-scenes/nft-wall-example-scene",
    thumb: asset("assets/templates/nft-art-wall.jpg"),
  },
  {
    id: "castaway-2048",
    title: "Castaway 2048",
    description: "A beach-scene starter with a 2048-style board and four tiles, plus a small SDK7 starter script that labels tiles and doubles their value on click \u{2014} a seed for building the full sliding puzzle \u{2014} press Play in the editor to run it (original SDK6 scene linked below).",
    tags: ["Game", "Puzzle"],
    difficulty_level: "Intermediate",
    date_created: "2025-01-09",
    play_link: null,
    github_link: "https://github.com/decentraland-scenes/Castaway-2048",
    thumb: asset("assets/templates/castaway-2048.jpg"),
  },
  {
    id: "escape-room",
    title: "Escape Room",
    description: "An escape-room starter layout \u{2014} locked door, lever, hidden key and candle-lit props \u{2014} with a small SDK7 script wiring the find-the-key, pull-the-lever, open-the-door sequence \u{2014} press Play in the editor to run it (not the original 9-room game, which is linked below).",
    tags: ["Game", "Puzzle"],
    difficulty_level: "Hard",
    date_created: "2025-04-30",
    play_link: null,
    github_link: "https://github.com/decentraland-scenes/Escape-Room",
    thumb: asset("assets/templates/escape-room.jpg"),
  },
  {
    id: "memory-game",
    title: "Memory Game",
    description: "An arcade starter with four Simon-style colour pads and a small SDK7 script that flashes a growing sequence and checks your clicks \u{2014} a compact example of click events and game state. Press Play in the editor to run it.",
    tags: ["Game"],
    difficulty_level: "Easy",
    date_created: "2025-03-18",
    play_link: null,
    github_link: "https://github.com/decentraland-scenes/Memory-game",
    thumb: asset("assets/templates/memory-game.jpg"),
  },
  {
    id: "tower-defense",
    title: "Tower Defense",
    description: "A tower-defense starter layout \u{2014} cobbled path, spawn gate, watch posts and creep spiders \u{2014} with a small SDK7 script that marches the creeps down the path and lets you click to repel them \u{2014} press Play in the editor to run it (not the full original game, which is linked below).",
    tags: ["Game"],
    difficulty_level: "Hard",
    date_created: "2024-11-02",
    play_link: null,
    github_link: "https://github.com/decentraland-scenes/Tower-defense",
    thumb: asset("assets/templates/tower-defense.jpg"),
  },
];

export const STARTER_TEMPLATES = TEMPLATES;

type ChTemplatesProps = {
  templates?: ChTemplate[];
  signedIn?: boolean;
  chrome?: boolean;
  account?: string;
  name?: string;
  onSignIn?: () => void;
  modalOpen?: boolean;
  embedded?: boolean;
  onBack?: () => void;
  onSelectTemplate?: (template: ChTemplate | null) => void;
  onCreate?: (values: { template: ChTemplate | null }) => void;
  onPreview?: (template: ChTemplate) => void;
  onViewCode?: (template: ChTemplate) => void;
};

export default function ChTemplates({
  templates: allTemplates = TEMPLATES,
  signedIn = false,
  account = "",
  name = "",
  onSignIn,
  modalOpen: initialModalOpen = false,
  embedded = false,
  onBack,
  onSelectTemplate,
  onCreate,
  onPreview,
  onViewCode,
  chrome = true,
}: ChTemplatesProps) {
  const [modal, setModal] = useState<ModalValue | null>(
    initialModalOpen ? { template: null, name: "Tower Defense" } : null,
  );

  const openCreate = useCallback(
    (template: ChTemplate | null, name: string) => () => {
      onSelectTemplate?.(template);
      if (onCreate) setModal({ template, name });
    },
    [onSelectTemplate, onCreate],
  );

  const body = (
    <section className="chtpl">
        <div className="chtpl__container">
          <h1 className="chtpl__title">
            {onBack ? (
              <button type="button" className="chtpl__back" aria-label="Back" onClick={() => onBack()}>
                <i className="chtpl__backicon">
                  <ChevronLeft size={22} />
                </i>
              </button>
            ) : null}
            <span className="chtpl__titletext">Choose a Template</span>
          </h1>

          <div className="chtpl__list">
            <ProjectCard
              title="Empty Scene"
              description="Start your own scene from scratch"
              isNewScene
              onClick={openCreate(null, "Empty Scene")}
            />
            {allTemplates.map((tpl) => (
              <ProjectCard
                key={tpl.id}
                title={tpl.title}
                description={tpl.description}
                thumb={tpl.thumb}
                tags={tpl.tags}
                onClick={openCreate(tpl, tpl.title)}
                dropdownOptions={[
                  ...(tpl.play_link
                    ? [{ text: "Preview Template", href: tpl.play_link, handler: () => onPreview?.(tpl) }]
                    : []),
                  { text: "View Code", href: tpl.github_link, handler: () => onViewCode?.(tpl) },
                ]}
              />
            ))}
          </div>
        </div>

        {modal && (
          <CreateProjectModal
            value={modal}
            onClose={() => setModal(null)}
            onCreate={(values) => {
              onCreate?.(values);
              setModal(null);
            }}
          />
        )}
    </section>
  );

  if (embedded) return body;

  return (
    <CreatorHubChromeMaybe chrome={chrome} active="templates" signedIn={signedIn} account={account} name={name} onSignIn={onSignIn}>
      {body}
    </CreatorHubChromeMaybe>
  );
}
