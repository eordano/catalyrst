import Button from "../../atoms/Button";
import { suffixLabel, type LabelSuffixProps } from "../../components/labelSuffix";

const TABS = [
  { id: "whats_on", label: "What's On" },
  { id: "pending", label: "Pending Hangouts" },
  { id: "users", label: "Users" },
] as const;

export type WhatsOnAdminTab = (typeof TABS)[number]["id"];

export default function StWhatSOnAdminTabs({ active, labelSuffix }: LabelSuffixProps & { active: WhatsOnAdminTab }) {
  return (
    <nav className="adm-tabs" aria-label={suffixLabel("Admin", labelSuffix)}>
      <div className="adm-tabs__list" role="tablist">
        {TABS.map((tab) => (
          <button
            key={tab.id}
            type="button"
            role="tab"
            aria-selected={tab.id === active}
            className={"adm-tabs__tab" + (tab.id === active ? " is-active" : "")}
          >
            {tab.label}
          </button>
        ))}
      </div>
      <Button size="sm">+ Create Hangout</Button>
    </nav>
  );
}
