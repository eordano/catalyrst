import "./smartwearablespanel.css";

type SmartWearablesPanelProps = {
  floating?: boolean;
  onClose?: () => void;
};

export default function SmartWearablesPanel(_props: SmartWearablesPanelProps = {}) {
  return (
    <div className="swpanel">
      <h2 className="swpanel__title">Portable experiences</h2>
      <p className="swpanel__empty">Nothing is running right now.</p>
      <p className="swpanel__hint">
        Smart wearables and world apps can run alongside the scene you’re in.
        When one asks for a sensitive capability — like your wallet or opening
        a link — the permission prompt appears automatically.
      </p>
    </div>
  );
}
