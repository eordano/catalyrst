type IconBase = { size?: number; className?: string };
type StrokeIcon = IconBase & { strokeWidth?: number };

export function Coin({ size = 18, ring = true, className }: IconBase & { ring?: boolean }) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <circle cx="12" cy="12" r={ring ? 10 : 11} fill="var(--gold)" stroke="#e0a429" strokeWidth="1.5" />
      {ring && <circle cx="12" cy="12" r="6.5" fill="none" stroke="#e0a429" strokeWidth="1.2" opacity=".6" />}
      <path
        d={
          ring
            ? "M8.9 15.1 V8.9 L12 12.8 L15.1 8.9 V15.1"
            : "M8.6 15.5 V8.5 L12 12.8 L15.4 8.5 V15.5"
        }
        fill="none"
        stroke="#8a5a00"
        strokeWidth={ring ? 1.7 : 1.9}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function PersonIcon({ size = 11 }: { size?: number }) {
  return (
    <svg viewBox="0 0 12 12" width={size} height={size} aria-hidden="true">
      <circle cx="6" cy="3.6" r="2.2" fill="currentColor" />
      <path d="M1.6 10.5c0-2.4 2-3.8 4.4-3.8s4.4 1.4 4.4 3.8z" fill="currentColor" />
    </svg>
  );
}

export function ManaIcon({ size = 18, className }: IconBase) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <circle cx="12" cy="12" r="11" fill="var(--brand)" />
      <path d="M6 16l3.2-7 2.8 5 1.5-3 1.3 5z" fill="#fff" opacity=".92" />
    </svg>
  );
}

export function Mute({ size = 16, className }: IconBase) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <path d="M4 9v6h4l5 4V5L8 9H4z" fill="currentColor" />
      <path d="M16 9a3.5 3.5 0 0 1 0 6M18.5 7a6.5 6.5 0 0 1 0 10" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  );
}

export function Check({ size = 16, className, stroke = "currentColor", strokeWidth = 2.2 }: StrokeIcon & { stroke?: string }) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <path d="M7 12.5l3 3 7-7" fill="none" stroke={stroke} strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

export function Bag({ size = 16, className }: IconBase) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <path d="M7 7V6a5 5 0 0 1 10 0v1h2.2l.8 12.5a2 2 0 0 1-2 2.1H6a2 2 0 0 1-2-2.1L4.8 7H7Zm2 0h6V6a3 3 0 0 0-6 0v1Z" fill="currentColor" />
    </svg>
  );
}

export function Pin({ size = 16, className }: IconBase) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <path d="M12 2a7 7 0 0 0-7 7c0 5 7 13 7 13s7-8 7-13a7 7 0 0 0-7-7Zm0 9.5A2.5 2.5 0 1 1 12 6.5a2.5 2.5 0 0 1 0 5Z" fill="currentColor" />
    </svg>
  );
}

export function CameraIcon({ size = 16, className }: IconBase) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <path d="M9 4 7.5 6H5a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-2.5L15 4H9Zm3 5a4.2 4.2 0 1 1 0 8.4A4.2 4.2 0 0 1 12 9Zm0 2a2.2 2.2 0 1 0 0 4.4A2.2 2.2 0 0 0 12 11Z" fill="currentColor" />
    </svg>
  );
}

export function People({ size = 16, className }: IconBase) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <path d="M8.5 11a3 3 0 1 0 0-6 3 3 0 0 0 0 6Zm7 0a3 3 0 1 0 0-6 3 3 0 0 0 0 6Zm-7 1.5C5.5 12.5 2 14 2 16.5V19h8v-2.5c0-1 .5-2 1.4-2.7a7 7 0 0 0-2.9-.8Zm7 0c-.6 0-1.2.1-1.8.2 1 .8 1.8 1.8 1.8 3.3V19h6v-2.5c0-2.5-3.5-4-6-4Z" fill="currentColor" />
    </svg>
  );
}

export function Help({ size = 16, className }: IconBase) {
  return (
    <svg viewBox="0 0 20 20" width={size} height={size} aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.6" className={className}>
      <circle cx="10" cy="10" r="7" />
      <path d="M8 8a2 2 0 1 1 3 1.7c-.7.5-1 .9-1 1.8" />
      <circle cx="10" cy="14" r=".5" fill="currentColor" stroke="none" />
    </svg>
  );
}

export function ChevronDown({ size = 20, className, strokeWidth = 2 }: StrokeIcon) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" aria-hidden="true" className={className}>
      <path d="M6 9l6 6 6-6" stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

export function ChevronUp({ size = 20, className, strokeWidth = 2 }: StrokeIcon) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" aria-hidden="true" className={className}>
      <path d="M18 15l-6-6-6 6" stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

export function ChevronLeft({ size = 18, className, strokeWidth = 2 }: StrokeIcon) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" aria-hidden="true" className={className}>
      <path d="M15 6l-6 6 6 6" stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

export function ChevronRight({ size = 18, className, strokeWidth = 2 }: StrokeIcon) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" aria-hidden="true" className={className}>
      <path d="M9 6l6 6-6 6" stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

export function ChevronDownAlt({ size = 18, className, strokeWidth = 2 }: StrokeIcon) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <path d="M7 10l5 5 5-5" stroke="currentColor" strokeWidth={strokeWidth} fill="none" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

export function Heart({ size = 13, className }: IconBase) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <path d="M12 2 3 12l9 10 9-10L12 2Z" fill="currentColor" />
    </svg>
  );
}

export function Close({ size = 20, className, strokeWidth = 2 }: StrokeIcon) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <path d="M6 6l12 12M18 6L6 18" stroke="currentColor" strokeWidth={strokeWidth} fill="none" strokeLinecap="round" />
    </svg>
  );
}

export function Caret({ size = 13, className = "", open = false, strokeWidth = 1.7 }: StrokeIcon & { open?: boolean }) {
  return (
    <svg
      className={className + (open ? " is-open" : "")}
      viewBox="0 0 16 16"
      width={size}
      height={size}
      aria-hidden="true"
    >
      <path d="M4 6l4 4 4-4" stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" fill="none" />
    </svg>
  );
}

export function Section({ size = 14, className, open = false }: IconBase & { open?: boolean }) {
  return (
    <svg viewBox="0 0 512 512" width={size} height={size} fill="currentColor" aria-hidden="true" className={className}>
      {open ? (
        <path d="M98.9 184.7l1.8 2.1 136 156.5c4.6 5.3 11.5 8.6 19.2 8.6 7.7 0 14.6-3.4 19.2-8.6L411 187.1l2.3-2.6c1.7-2.5 2.7-5.5 2.7-8.7 0-8.7-7.4-15.8-16.6-15.8H112.6c-9.2 0-16.6 7.1-16.6 15.8 0 3.3 1.1 6.4 2.9 8.9z" />
      ) : (
        <path d="M190.5 66.9l22.2-22.2c9.4-9.4 24.6-9.4 33.9 0L441 239c9.4 9.4 9.4 24.6 0 33.9L246.6 467.3c-9.4 9.4-24.6 9.4-33.9 0l-22.2-22.2c-9.5-9.5-9.3-25 .4-34.3L311.4 296H24c-13.3 0-24-10.7-24-24v-32c0-13.3 10.7-24 24-24h287.4L168.9 101.2c-9.8-9.3-10-24.8-.4-34.3z" />
      )}
    </svg>
  );
}

export function Search({ size = 18, className, strokeWidth = 2, handle = "M21 21l-4.3-4.3" }: StrokeIcon & { handle?: string }) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" aria-hidden="true" className={className}>
      <circle cx="11" cy="11" r="7" stroke="currentColor" strokeWidth={strokeWidth} />
      <path d={handle} stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" />
    </svg>
  );
}

export function Plus({ size = 18, className, strokeWidth = 2 }: StrokeIcon) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" aria-hidden="true" className={className}>
      <path d="M12 5v14M5 12h14" stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" />
    </svg>
  );
}

export function Trash({ size = 18, className }: IconBase) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" aria-hidden="true" className={className}>
      <path d="M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z" />
    </svg>
  );
}

export function Pencil({ size = 18, className }: IconBase) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" aria-hidden="true" className={className}>
      <path d="M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04a1 1 0 0 0 0-1.41l-2.34-2.34a1 1 0 0 0-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z" />
    </svg>
  );
}

export function Gear({ size = 18, className }: IconBase) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" aria-hidden="true" className={className}>
      <path d="M19.14 12.94a7.49 7.49 0 0 0 .05-.94 7.49 7.49 0 0 0-.05-.94l2.03-1.58a.5.5 0 0 0 .12-.62l-1.92-3.32a.5.5 0 0 0-.6-.22l-2.39.96a7 7 0 0 0-1.62-.94l-.36-2.54a.49.49 0 0 0-.49-.42h-3.84a.49.49 0 0 0-.49.42l-.36 2.54a7 7 0 0 0-1.62.94l-2.39-.96a.5.5 0 0 0-.6.22L2.74 8.86a.5.5 0 0 0 .12.62l2.03 1.58a7.49 7.49 0 0 0 0 1.88l-2.03 1.58a.5.5 0 0 0-.12.62l1.92 3.32a.5.5 0 0 0 .6.22l2.39-.96a7 7 0 0 0 1.62.94l.36 2.54a.49.49 0 0 0 .49.42h3.84a.49.49 0 0 0 .49-.42l.36-2.54a7 7 0 0 0 1.62-.94l2.39.96a.5.5 0 0 0 .6-.22l1.92-3.32a.5.5 0 0 0-.12-.62l-2.03-1.58zM12 15.6A3.6 3.6 0 1 1 12 8.4a3.6 3.6 0 0 1 0 7.2z" />
    </svg>
  );
}

export function Copy({ size = 16, className, strokeWidth = 1.6 }: StrokeIcon) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" aria-hidden="true" className={className}>
      <rect x="9" y="9" width="11" height="11" rx="2" stroke="currentColor" strokeWidth={strokeWidth} />
      <path d="M5 15V5a2 2 0 0 1 2-2h8" stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" />
    </svg>
  );
}

export function Info({ size = 18, className, strokeWidth = 1.7 }: StrokeIcon) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth={strokeWidth} fill="none" />
      <path d="M12 11v5M12 7.6v.2" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

export function CheckFill({ size = 16, className }: IconBase) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" aria-hidden="true" className={className}>
      <path d="M9 16.17 4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z" />
    </svg>
  );
}

export function CheckBold({ size = 18, className, strokeWidth = 2, compact = false }: StrokeIcon & { compact?: boolean }) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" aria-hidden="true" className={className}>
      <path d={compact ? "M3 8.5l3 3 7-7" : "M20 6 9 17l-5-5"} stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

export function TriangleDown({ size = 16, className }: IconBase) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" aria-hidden="true" className={className}>
      <path d="M7 10l5 5 5-5z" />
    </svg>
  );
}

export function Kebab({ size = 18, className, r = 2 }: IconBase & { r?: number }) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" aria-hidden="true" className={className}>
      <circle cx="5" cy="12" r={r} />
      <circle cx="12" cy="12" r={r} />
      <circle cx="19" cy="12" r={r} />
    </svg>
  );
}

export function GridView({ size = 16, className }: IconBase) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" aria-hidden="true" className={className}>
      <rect x="3" y="3" width="7" height="7" rx="1.5" />
      <rect x="14" y="3" width="7" height="7" rx="1.5" />
      <rect x="3" y="14" width="7" height="7" rx="1.5" />
      <rect x="14" y="14" width="7" height="7" rx="1.5" />
    </svg>
  );
}

export function ListView({ size = 16, className, strokeWidth = 2 }: StrokeIcon) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" aria-hidden="true" className={className}>
      <path d="M8 6h13M8 12h13M8 18h13M3.5 6h.01M3.5 12h.01M3.5 18h.01" stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" />
    </svg>
  );
}
