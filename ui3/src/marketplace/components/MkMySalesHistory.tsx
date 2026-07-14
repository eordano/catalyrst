import EmptyState from "../../components/EmptyState";

export type MkMySalesEntry = {
  id: string;
  rawType: string;
  to: string;
  price: string | null;
};

export type MkMySalesHistoryProps = {
  me?: string;
  mine?: MkMySalesEntry[];
  manaEarned?: string;
};

export default function MkMySalesHistory({
  me = "",
  mine = [],
  manaEarned = "0",
}: MkMySalesHistoryProps) {
  if (!me) {
    return (
      <section className="mksales" aria-label="My sales history">
        <h2 className="mksales__title">My sales</h2>
        <EmptyState variant="inline" title="Sign in to see your sales here." />
      </section>
    );
  }

  return (
    <section className="mksales" aria-label="My sales history">
      <div className="mksales__head">
        <h2 className="mksales__title">My sales</h2>
        <span className="mksales__note" title="Wallet connection is simulated on this surface">
          showing {me} (simulated account)
        </span>
      </div>

      <div className="mksales__stats">
        <Stat label="Sales on this page" value={String(mine.length)} />
        <Stat label="MANA earned" value={manaEarned} />
      </div>

      {mine.length > 0 ? (
        <ul className="mksales__list">
          {mine.map((e) => (
            <li key={e.id} className="mksales__item">
              <span className="mksales__kind">{e.rawType}</span>
              <span className="mksales__to">to {e.to || "\u{2014}"}</span>
              <span className="mksales__amt">
                {e.price ? (
                  <>
                    <span className="mksales__mana">&#x25C7;</span> {e.price}
                  </>
                ) : (
                  "\u{2014}"
                )}
              </span>
            </li>
          ))}
        </ul>
      ) : (
        <p className="mksales__empty">No sales for this account on this page.</p>
      )}
    </section>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="mksales__stat">
      <span className="mksales__statval">{value}</span>
      <span className="mksales__statlabel">{label}</span>
    </div>
  );
}
