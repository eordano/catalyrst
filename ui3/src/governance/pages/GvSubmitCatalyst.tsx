import type { CSSProperties, ReactNode } from "react";
import { useMemo, useState } from "react";
import { GovernanceChromeMaybe } from "../frames/GovernanceChrome";
import { asset } from "../../asset";
import "./gvsubmitcatalyst.css";
import { ChevronLeft } from "../../atoms/icons";

type CatalystType = "add" | "remove";

type CatalystCopy = {
  title: string;
  description: string;
  description_2: string;
  description_detail: string;
};

const COPY: Record<CatalystType, CatalystCopy> = {
  add: {
    title: "Add a catalyst node",
    description:
      "Instead of using central servers, Decentraland is run on a network of community-operated nodes. These nodes store copies of all scenes deployed to Decentraland, and they handle the messaging and interactions by establishing peer-to-peer connections between users.",
    description_2: "To propose the addition of a new node, please provide the following details.",
    description_detail:
      "Explain why this node should be added to the Catalyst network. Why would this addition be beneficial to the network? For example, adding nodes in new geographic areas helps to provide a better experience to more users.",
  },
  remove: {
    title: "Remove a catalyst node",
    description:
      "Instead of using central servers, Decentraland is run on a network of community-operated nodes. These nodes store copies of all scenes deployed to Decentraland, and they handle the messaging and interactions by establishing peer-to-peer connections between users.",
    description_2: "To propose the removal of a node, please provide the following details.",
    description_detail:
      "Explain why this node should be removed from the Catalyst network. Please, be as descriptive and objective as possible.",
  },
};

const DESCRIPTION_LIMIT = 7000;

const ErrorNotice = () => (
  <svg viewBox="0 0 20 20" width="18" height="18" aria-hidden="true">
    <circle cx="10" cy="10" r="9" fill="currentColor" />
    <rect x="9" y="5" width="2" height="6" rx="1" fill="#fff" />
    <rect x="9" y="13" width="2" height="2" rx="1" fill="#fff" />
  </svg>
);

const StatusOk = ({ children }: { children?: ReactNode }) => (
  <span className="gsc__statusline gsc__statusline--ok">{children}</span>
);

function Field({ id, label, value, onChange, placeholder, error, message }: { id: string; label: ReactNode; value: string; onChange: (value: string) => void; placeholder?: string; error?: boolean; message?: ReactNode }) {
  return (
    <div className="gsc__section">
      <label className="gsc__label" htmlFor={id}>{label}</label>
      <div className={"gsc__field" + (error ? " is-error" : "")}>
        <input
          id={id}
          className="gsc__input"
          type="text"
          value={value}
          placeholder={placeholder}
          onChange={(e) => onChange(e.target.value)}
        />
        {message ? <div className="gsc__fieldmsg">{message}</div> : null}
      </div>
    </div>
  );
}

const MD_COMMANDS = ["B", "I", "\u{201F}", "\u{2022}", "1.", "\u{1F517}", "\u{1F5BC}"];

function ErrorPanel({ label }: { label: ReactNode }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="gsc__section">
      <div className="gsc__error">
        <div className="gsc__errorhead">
          <div className="gsc__errornotice">
            <ErrorNotice />
            <span className="gsc__errorlabel">{label}</span>
          </div>
          <button type="button" className="gsc__errortoggle" onClick={() => setOpen((o) => !o)}>
            {open ? "Hide" : "Show"}
          </button>
        </div>
      </div>
    </div>
  );
}

function CatalystForm({ catalystType, showError }: { catalystType: CatalystType; showError: boolean }) {
  const c = COPY[catalystType] || COPY.add;
  const [owner, setOwner] = useState("");
  const [domain, setDomain] = useState("");
  const [committedDomain, setCommittedDomain] = useState("catalyst.example.com");
  const [description, setDescription] = useState("");
  const [coAuthors, setCoAuthors] = useState<string[]>(["0x06012c\u{2026}a266d"]);
  const [coAuthorDraft, setCoAuthorDraft] = useState("");

  const addCoAuthor = () => {
    const v = coAuthorDraft.trim();
    if (v && coAuthors.length < 5) {
      setCoAuthors([...coAuthors, v]);
      setCoAuthorDraft("");
    }
  };

  return (
    <div className="gsc__layout">
      <div className="gsc__back">
        <button type="button" className="gsc__backbtn" aria-label="Back to submit">
          <ChevronLeft size={15} />
        </button>
      </div>

      <div className="gsc__col">
        <form onSubmit={(e) => e.preventDefault()}>
          <div className="gsc__section">
            <h1 className="gsc__title">{c.title}</h1>
          </div>

          <div className="gsc__section">
            <p className="gsc__lead">{c.description}</p>
            <p className="gsc__lead">{c.description_2}</p>
          </div>

          <Field
            id="gsc-owner"
            label="Ethereum address of the owner of the Catalyst Node"
            value={owner}
            onChange={setOwner}
            placeholder="Example: 0x06012c8cf97bead5deae237070f9587f8e7a266d"
          />

          <div className="gsc__section">
            <label className="gsc__label" htmlFor="gsc-domain">Domain for the Catalyst Node</label>
            <div className="gsc__field">
              <input
                id="gsc-domain"
                className="gsc__input"
                type="text"
                value={domain}
                placeholder="Example: catalyst.yourdomainname.com"
                onChange={(e) => setDomain(e.target.value)}
                onBlur={(e) => setCommittedDomain(e.target.value)}
              />
            </div>
            {committedDomain ? (
              <div className="gsc__status">
                <StatusOk>&#x2714; Content server is ready.</StatusOk>
                <StatusOk>&#x2714; Lambda server is ready.</StatusOk>
              </div>
            ) : null}
          </div>

          <div className="gsc__section">
            <label className="gsc__label" htmlFor="gsc-description">
              Description
              <sup className="gsc__notice" title="You can format your proposal using markdown! Toggle the preview switch to see how your post will be displayed.">
                {" "}(markdown)
              </sup>
            </label>
            <p className="gsc__sublabel">{c.description_detail}</p>
            <div className="gsc__md">
              <div className="gsc__mdtoolbar">
                {MD_COMMANDS.map((cmd, i) => (
                  <button key={i} type="button" className="gsc__mdbtn" tabIndex={-1} aria-hidden="true">
                    {cmd}
                  </button>
                ))}
                <span className="gsc__mdspacer" />
                <span className="gsc__mdpreview">Preview</span>
              </div>
              <textarea
                id="gsc-description"
                className="gsc__mdarea"
                value={description}
                placeholder={"Write your description using markdown\u{2026}"}
                onChange={(e) => setDescription(e.target.value)}
              />
            </div>
            <div className="gsc__counter">
              ({description.length} out of {DESCRIPTION_LIMIT} characters)
            </div>
          </div>

          <div className="gsc__section">
            <div className="gsc__coauthorhead">
              <label className="gsc__label" style={{ marginBottom: 0 }} htmlFor="gsc-coauthor">
                Co-authors
              </label>
              <sup className="gsc__optional"> (optional)</sup>
            </div>
            <p className="gsc__sublabel">
              If you co-authored this proposal with someone else, you can add their wallet addresses to acknowledge their
              work. After you publish the proposal, co-authors will be asked to confirm or reject the request. Only if
              they confirm, they will be listed publicly on the proposal page.
            </p>
            <div className="gsc__select">
              {coAuthors.map((addr, i) => (
                <span className="gsc__chip" key={addr + i}>
                  <span className="u-avatar" style={{ "--sz": "20px", "--hue": 200 + i * 40 } as CSSProperties & { "--sz": string; "--hue": number }} aria-hidden="true" />
                  {addr}
                  <button
                    type="button"
                    className="gsc__chipx"
                    aria-label={"Remove " + addr}
                    onClick={() => setCoAuthors(coAuthors.filter((_, j) => j !== i))}
                  >
                    &#xD7;
                  </button>
                </span>
              ))}
              <input
                id="gsc-coauthor"
                className="gsc__selectinput"
                type="text"
                value={coAuthorDraft}
                placeholder={coAuthors.length === 0 ? "Co-authors" : ""}
                onChange={(e) => setCoAuthorDraft(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    addCoAuthor();
                  }
                }}
              />
            </div>
          </div>

          <div className="gsc__section">
            <button type="submit" className="gsc__submit">
              Submit proposal
            </button>
          </div>

          {showError ? (
            <ErrorPanel label="There was an error while trying to create the proposal, please try again later." />
          ) : null}
        </form>
      </div>
    </div>
  );
}

function LogInGate({ catalystType }: { catalystType: CatalystType }) {
  const c = COPY[catalystType] || COPY.add;
  return (
    <div className="gsc__login">
      <img className="gsc__loginlogo" src={asset("assets/dcl-logo.png")} alt="" />
      <h1 className="gsc__logintitle">{c.title}</h1>
      <p className="gsc__logindesc">{c.description}</p>
      <button type="button" className="gsc__submit">
        Sign in
      </button>
    </div>
  );
}

function NotFound() {
  return (
    <div className="gsc__notfound">
      <h1>404</h1>
      <p>Page not found.</p>
    </div>
  );
}

type CatalystState = "form" | "login" | "notfound";

export default function GvSubmitCatalyst({ chrome = true, catalystType = "add", state = "form", showError = false }: { chrome?: boolean; catalystType?: CatalystType; state?: CatalystState; showError?: boolean }) {
  const body = useMemo(() => {
    if (state === "notfound") return <NotFound />;
    if (state === "login") return <LogInGate catalystType={catalystType} />;
    return <CatalystForm catalystType={catalystType} showError={showError} />;
  }, [state, catalystType, showError]);

  return (
    <GovernanceChromeMaybe chrome={chrome} active="proposals">
      <div className="gsc">{body}</div>
    </GovernanceChromeMaybe>
  );
}
