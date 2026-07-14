import { useState } from "react";
import type { ReactNode } from "react";
import { Avatar } from "../../atoms/primitives";
import "./chitemeditor.css";

type Collection = {
  id: string;
  name: string;
  itemCount?: number;
  status?: string;
};

type Item = {
  id: string;
  name: string;
  type: string;
  rarity: string;
};

export type ChItemEditorCollection = {
  id?: string;
  name: string;
  itemCount?: number;
  status?: string;
};

export type ChItemEditorItem = {
  id: string;
  name: string;
  type: string;
  rarity?: string;
};

const RARITY_VAR: Record<string, string> = {
  common: "--rar-common",
  uncommon: "--rar-uncommon",
  rare: "--rar-rare",
  epic: "--rar-epic",
  legendary: "--rar-legendary",
  mythic: "--rar-mythic",
  unique: "--rar-unique",
  exotic: "--rar-exotic",
};

const CloseGlyph = () => (
  <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
    <path d="M1 1l10 10M11 1L1 11" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
  </svg>
);
const Chevron = ({ open }: { open: boolean }) => (
  <svg viewBox="0 0 12 12" width="11" height="11" aria-hidden="true"
    className={"bdie__chev" + (open ? " is-open" : "")}>
    <path d="M3 2l5 4-5 4" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);

function StatusPip({ status }: { status: string }) {
  const draft = { c: "var(--bdie-secondary-text)", t: "Draft" };
  const map: Record<string, { c: string; t: string }> = {
    published: { c: "var(--online)", t: "Published" },
    draft,
    under_review: { c: "var(--gold)", t: "Under review" },
  };
  const s = map[status] || draft;
  return <span className="bdie__pip" style={{ background: s.c }} title={s.t} />;
}

function SidebarCollection({
  collection,
  selected,
}: {
  collection: Collection;
  selected: boolean;
}) {
  return (
    <div className={"bdie__col is-static" + (selected ? " is-selected" : "")}>
      <span className="bdie__col-img" />
      <div className="bdie__col-wrap">
        <div className="bdie__col-name u-truncate">
          {collection.status ? <StatusPip status={collection.status} /> : null}
          {collection.name}
        </div>
        {collection.itemCount != null && (
          <div className="bdie__col-count">{collection.itemCount} items</div>
        )}
      </div>
    </div>
  );
}

function SidebarItem({
  item,
  selected,
  onClick,
}: {
  item: Item;
  selected: boolean;
  onClick: () => void;
}) {
  const imgStyle = {
    "--rb": "var(" + (RARITY_VAR[item.rarity] || "--rar-common") + ")",
  } as React.CSSProperties;
  return (
    <div className={"bdie__item" + (selected ? " is-selected" : "")}>
      <button type="button" className="bdie__item-link" onClick={onClick}>
        <span className="bdie__item-img u-rar-bg" style={imgStyle} />
        <span className="bdie__item-name u-truncate">{item.name}</span>
      </button>
    </div>
  );
}

function Collapsable({
  label,
  children,
  defaultOpen = true,
}: {
  label: string;
  children: ReactNode;
  defaultOpen?: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className={"bdie__collapsable" + (open ? " is-open" : "")}>
      <button type="button" className="bdie__collapsable-head" onClick={() => setOpen((o) => !o)}>
        <Chevron open={open} />
        <span className="bdie__collapsable-label">{label}</span>
      </button>
      {open && <div className="bdie__collapsable-body">{children}</div>}
    </div>
  );
}

function Field({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="bdie__field">
      <div className="bdie__field-label">{label}</div>
      <div className="bdie__field-value">{value}</div>
    </div>
  );
}

export default function ChItemEditor({
  onClose = undefined,
  collection = undefined,
  collections = undefined,
  items: realItems = undefined,
}: {
  onClose?: () => void;
  collection?: ChItemEditorCollection;
  collections?: ChItemEditorCollection[];
  items?: ChItemEditorItem[];
}) {
  const noCollection = !collection?.id;
  const itemsUnknown = !noCollection && realItems === undefined;
  const [tab, setTab] = useState("items");
  const [selectedItemId, setSelectedItemId] = useState("");

  const shownItems: Item[] = (realItems ?? []).map((i) => ({
    id: i.id,
    name: i.name,
    type: i.type,
    rarity: i.rarity ?? "common",
  }));

  const selectedCollection: Collection = {
    id: collection?.id ?? "",
    name: collection?.name ?? "No collection",
    itemCount: collection?.itemCount ?? realItems?.length,
    status: collection?.status,
  };

  const shownCollections: Collection[] = (collections ?? []).map((c) => ({
    id: c.id ?? c.name,
    name: c.name,
    itemCount: c.itemCount,
    status: c.status,
  }));

  const selectedItem: Item | undefined =
    shownItems.find((i) => i.id === selectedItemId) || shownItems[0];

  return (
    <div className="bdie">
      <div className="bdie__content">
        <aside className="bdie__left" aria-label="Collections and items">
          <div className="bdie__header">
            <button type="button" className="bdie__icon-btn" aria-label="Close" onClick={onClose}>
              <CloseGlyph />
            </button>
            <div className="bdie__header-title">
              <span className="bdie__header-name u-truncate">
                {selectedCollection.name}
              </span>
              {selectedCollection.status ? (
                <StatusPip status={selectedCollection.status} />
              ) : null}
            </div>
            <span className="bdie__icon-spacer" aria-hidden="true" />
          </div>

          <div className="bdie__tabs">
            <button type="button" className={"bdie__tab" + (tab === "collections" ? " is-active" : "")} onClick={() => setTab("collections")}>Collections</button>
            <button type="button" className={"bdie__tab" + (tab === "items" ? " is-active" : "")} onClick={() => setTab("items")}>Items</button>
          </div>

          <div className="bdie__left-scroll">
            {tab === "collections" ? (
              shownCollections.length ? (
                <div className="bdie__collections">
                  {shownCollections.map((c) => (
                    <SidebarCollection
                      key={c.id}
                      collection={c}
                      selected={c.id === selectedCollection.id}
                    />
                  ))}
                </div>
              ) : (
                <p className="bdie__empty">No collections yet.</p>
              )
            ) : shownItems.length ? (
              <div className="bdie__items">
                {shownItems.map((it) => (
                  <SidebarItem
                    key={it.id}
                    item={it}
                    selected={selectedItem?.id === it.id}
                    onClick={() => setSelectedItemId(it.id)}
                  />
                ))}
              </div>
            ) : noCollection ? (
              <p className="bdie__empty">
                No collection is loaded, so there are no items to show &#x2014; you
                can still draft a brand-new wearable with the wizard panel.
              </p>
            ) : itemsUnknown ? (
              <p className="bdie__empty">
                This collection&#x2019;s items can&#x2019;t be listed here right now (the
                builder item list needs a signed-in session). You can still
                draft a new wearable with the wizard panel.
              </p>
            ) : (
              <p className="bdie__empty">
                No items in this collection yet &#x2014; add your first one with the
                wizard panel.
              </p>
            )}
          </div>
        </aside>

        <section className="bdie__center">
          <div className="bdie__preview">
            <Avatar size={200} name="DCL Avatar" className="bdie__avatar" />
          </div>
        </section>

        <aside className="bdie__right" aria-label="Properties">
          <div className="bdie__rp-header">
            <div className="bdie__rp-title">PROPERTIES</div>
          </div>

          <div className="bdie__rp-container">
            {selectedItem ? (
              <>
                <Collapsable label="Basics">
                  <Field label="Name" value={selectedItem.name} />
                  <Field
                    label="Type"
                    value={selectedItem.type === "emote" ? "Emote" : "Wearable"}
                  />
                </Collapsable>
                <p className="bdie__note">
                  Edit the name, model, category, rarity and price with the
                  wizard panel.
                </p>
              </>
            ) : (
              <p className="bdie__empty">
                Item properties appear here once an item is selected.
              </p>
            )}
          </div>
        </aside>
      </div>
    </div>
  );
}
