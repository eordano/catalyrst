import { MarketplaceChromeMaybe } from "../frames/MarketplaceChrome";
import EnsCard from "../components/EnsCard";
import Button from "../../atoms/Button";
import ManaMark from "../../atoms/ManaMark";
import Spinner from "../../atoms/Spinner";
import "./mknamespage.css";

export type MkNameStatus =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "invalid"; message: string; warn?: boolean }
  | { kind: "claimable"; priceMana: string }
  | { kind: "listed"; name: string; priceMana: string }
  | { kind: "taken"; name: string }
  | { kind: "error"; message: string };

type MkNamesPageProps = {
  value?: string;
  status?: MkNameStatus;
  maxLength?: number;
  creditsNote?: string;
  chrome?: boolean;
  onChange?: (value: string) => void;
  onTab?: (id: string) => void;
  onClaim?: () => void;
  onBuy?: () => void;
};

const NAME_PERKS = [
  "A unique alias for your avatar",
  "Your own Decentraland World",
  "+100 Voting Power in the DAO",
];

function StatusLine({ status }: { status: MkNameStatus }) {
  switch (status.kind) {
    case "idle":
      return (
        <span className="mknamespage__hint">
          Every NAME is unique &#x2014; type one to check availability.
        </span>
      );
    case "checking":
      return (
        <span className="mknamespage__hint">
          <Spinner size={16} /> Checking availability&#x2026;
        </span>
      );
    case "invalid":
      return (
        <span className="mknamespage__verdict">
          <span className={status.warn ? "mknamespage__warn" : "mknamespage__no"}>
            {status.warn ? "\u{26A0}" : "\u{2715}"}
          </span>{" "}
          {status.message}
        </span>
      );
    case "claimable":
      return (
        <span className="mknamespage__verdict">
          <span className="mknamespage__ok">&#x2713;</span> Available &#x2014; nobody has claimed this
          NAME yet.
        </span>
      );
    case "listed":
      return (
        <span className="mknamespage__verdict">
          <span className="mknamespage__warn">&#x25C6;</span> Taken, but its owner listed it for
          sale.
        </span>
      );
    case "taken":
      return (
        <span className="mknamespage__verdict">
          <span className="mknamespage__no">&#x2715;</span> Taken and not for sale &#x2014; try another
          name.
        </span>
      );
    case "error":
      return (
        <span className="mknamespage__verdict" role="alert">
          <span className="mknamespage__no">&#x2715;</span> {status.message}
        </span>
      );
  }
}

function CreditsRow({ note }: { note?: string }) {
  if (!note) return null;
  return (
    <div className="mknamespage__credits">
      <Button variant="secondary" size="sm" disabled>
        Buy with Credits
      </Button>
      <span className="mknamespage__creditsnote">{note}</span>
    </div>
  );
}

function ResultPanel({
  status,
  value,
  creditsNote,
  onClaim,
  onBuy,
}: {
  status: MkNameStatus;
  value: string;
  creditsNote?: string;
  onClaim?: () => void;
  onBuy?: () => void;
}) {
  if (status.kind === "claimable") {
    return (
      <div className="mknamespage__result">
        <div className="mknamespage__card">
          <EnsCard name={value} />
        </div>
        <div className="mknamespage__offer">
          <span className="mknamespage__offerlabel">Claim it fresh</span>
          <span className="mknamespage__price">
            <ManaMark size={22} /> {status.priceMana} MANA
            <span className="mknamespage__network">Ethereum Mainnet</span>
          </span>
          <Button variant="primary" className="mknamespage__cta" onClick={onClaim}>
            Claim NAME
          </Button>
          <CreditsRow note={creditsNote} />
        </div>
      </div>
    );
  }

  if (status.kind === "listed") {
    return (
      <div className="mknamespage__result">
        <div className="mknamespage__card">
          <EnsCard name={status.name} price={status.priceMana} />
        </div>
        <div className="mknamespage__offer">
          <span className="mknamespage__offerlabel">Buy it from its owner</span>
          <span className="mknamespage__price">
            <ManaMark size={22} /> {status.priceMana} MANA
            <span className="mknamespage__network">Ethereum Mainnet</span>
          </span>
          <Button variant="primary" className="mknamespage__cta" onClick={onBuy}>
            Buy NAME
          </Button>
          <CreditsRow note={creditsNote} />
        </div>
      </div>
    );
  }

  if (status.kind === "taken") {
    return (
      <div className="mknamespage__result mknamespage__result--taken">
        <div className="mknamespage__card">
          <EnsCard name={status.name} />
        </div>
        <div className="mknamespage__offer">
          <span className="mknamespage__offerlabel">
            Already owned &#x2014; and not listed for sale
          </span>
          <span className="mknamespage__takencopy">
            The owner hasn&apos;t put this NAME on the market. Try a variation instead.
          </span>
        </div>
      </div>
    );
  }

  return null;
}

export default function MkNamesPage({
  value = "",
  status = { kind: "idle" },
  maxLength = 15,
  creditsNote,
  chrome = true,
  onChange,
  onTab,
  onClaim,
  onBuy,
}: MkNamesPageProps) {
  function onEnter() {
    if (status.kind === "claimable") onClaim?.();
    else if (status.kind === "listed") onBuy?.();
  }

  return (
    <MarketplaceChromeMaybe chrome={chrome} active="names" onTab={onTab}>
      <div className="mknamespage">
        <section className="mknamespage__hero">
          <h1 className="mknamespage__title">What NAME do you want?</h1>
          <p className="mknamespage__subtitle">
            Claim an unclaimed NAME, or buy one straight from its current owner if
            it&apos;s listed for sale.
          </p>

          <ul className="mknamespage__perks">
            {NAME_PERKS.map((perk) => (
              <li key={perk} className="mknamespage__perk">
                {perk}
              </li>
            ))}
          </ul>

          <div className="mknamespage__field">
            <input
              className="mknamespage__input"
              value={value}
              placeholder="yourname"
              maxLength={maxLength}
              spellCheck={false}
              autoComplete="off"
              autoCapitalize="off"
              aria-label="NAME to check"
              onChange={(e) => onChange?.(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && onEnter()}
            />
            <span className="mknamespage__suffix">.dcl.eth</span>
            <span className="mknamespage__counter">
              {value.length}/{maxLength}
            </span>
          </div>

          <div className="mknamespage__status" aria-live="polite">
            <StatusLine status={status} />
          </div>
        </section>

        <ResultPanel
          status={status}
          value={value}
          creditsNote={creditsNote}
          onClaim={onClaim}
          onBuy={onBuy}
        />
      </div>
    </MarketplaceChromeMaybe>
  );
}
