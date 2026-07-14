import EmptyState from "./EmptyState";
import "./mobileeditorgate.css";

const DesktopIcon = (
  <svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
    <rect
      x="2.5"
      y="3.5"
      width="19"
      height="13"
      rx="2"
      stroke="currentColor"
      strokeWidth="1.6"
    />
    <path d="M8.5 20.5h7M12 16.5v4" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
  </svg>
);

export type MobileEditorGateProps = {
  title?: string;
  message?: string;
  backHref?: string;
  backLabel?: string;
};

export default function MobileEditorGate({
  title = "Open the editor on a desktop",
  message = "This editor needs a wider screen and a WebGPU-capable desktop browser. Come back on a laptop or desktop to keep building.",
  backHref = "/create",
  backLabel = "Back to Creator Hub",
}: MobileEditorGateProps) {
  return (
    <div className="mobile-editor-gate" role="region" aria-label={title}>
      <EmptyState
        variant="screen"
        icon={DesktopIcon}
        title={title}
        subtitle={message}
        actions={[{ label: backLabel, href: backHref, variant: "outline" }]}
      />
    </div>
  );
}
