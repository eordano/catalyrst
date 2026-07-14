import type { ReactNode } from "react";
import ManaMark from "../../atoms/ManaMark";
import { Coin } from "../../atoms/icons";
import { creditsSrLabel } from "../credits-unit";
import "./newshopranktable.css";

const Verified = () => (
  <svg className="nsrank__verified" viewBox="0 0 24 24" aria-label="Verified" role="img">
    <path
      fill="var(--info)"
      d="M12 1l2.4 1.8 3-.1 1 2.8 2.5 1.6-.9 2.9.9 2.9-2.5 1.6-1 2.8-3-.1L12 23l-2.4-1.8-3 .1-1-2.8-2.5-1.6.9-2.9-.9-2.9 2.5-1.6 1-2.8 3 .1z"
    />
    <path fill="#fff" d="M10.6 14.6l-2.2-2.2-1.2 1.2 3.4 3.4 6-6-1.2-1.2z" />
  </svg>
);

export type RankRow = {
  id: string;
  name: ReactNode;
  image?: string;
  verified?: boolean;
  floor?: ReactNode;
  volume?: ReactNode;
  unit?: "mana" | "credits";
  network?: "polygon" | "ethereum";
};

type NewShopRankTableProps = {
  title?: ReactNode;
  rows: RankRow[];
  onRow?: (id: string) => void;
};

export default function NewShopRankTable({ title = "Top Assets", rows, onRow }: NewShopRankTableProps) {
  const showVolume = rows.some((r) => r.volume != null);
  const cls = "nsrank__table" + (showVolume ? "" : " nsrank__table--novol");
  return (
    <section className="nsrank">
      <h3 className="nsrank__title">{title}</h3>
      <div className={cls} role="table">
        <div className="nsrank__head" role="row">
          <span className="nsrank__c nsrank__c--rank" role="columnheader">#</span>
          <span className="nsrank__c nsrank__c--item" role="columnheader">Item</span>
          <span className="nsrank__c nsrank__c--num" role="columnheader">Floor price</span>
          {showVolume ? (
            <span className="nsrank__c nsrank__c--num" role="columnheader">Volume</span>
          ) : null}
        </div>
        {rows.map((r, i) => (
          <div key={r.id} className="nsrank__row" role="row">
            <span className="nsrank__cellwrap" role="cell">
              <button
                type="button"
                className="nsrank__rowbtn"
                onClick={() => onRow?.(r.id)}
              >
                <span className="nsrank__c nsrank__c--rank">{i + 1}</span>
                <span className="nsrank__c nsrank__c--item">
                  <span className="nsrank__thumb" aria-hidden="true">
                    {r.image ? <img src={r.image} alt="" /> : null}
                  </span>
                  <span className="nsrank__name u-truncate">{r.name}</span>
                  {r.verified ? <Verified /> : null}
                </span>
                <span className="nsrank__c nsrank__c--num">
                  {r.floor != null ? (
                    <>
                      {r.unit === "credits" ? (
                        <Coin size={12} />
                      ) : (
                        <ManaMark size={12} network={r.network || "polygon"} />
                      )}
                      {r.floor}
                      <span className="u-sr-only">{r.unit === "credits" ? creditsSrLabel(r.floor) : " MANA"}</span>
                    </>
                  ) : (
                    "—"
                  )}
                </span>
                {showVolume ? (
                  <span className="nsrank__c nsrank__c--num">
                    {r.volume != null ? (
                      <>
                        {r.unit === "credits" ? (
                          <Coin size={12} />
                        ) : (
                          <ManaMark size={12} network={r.network || "polygon"} />
                        )}
                        {r.volume}
                        <span className="u-sr-only">{r.unit === "credits" ? creditsSrLabel(r.volume) : " MANA"}</span>
                      </>
                    ) : (
                      "—"
                    )}
                  </span>
                ) : null}
              </button>
            </span>
          </div>
        ))}
      </div>
    </section>
  );
}
