import type { SVGProps } from "react";

const S = { width: 15, height: 15, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 1.8, strokeLinecap: "round", strokeLinejoin: "round", "aria-hidden": true } satisfies SVGProps<SVGSVGElement>;

export const IconSelect = () => (
  <svg {...S}><path d="m4 4 7.07 17 2.51-7.39L21 11.07 4 4z" /></svg>
);
export const IconMove = () => (
  <svg {...S}><path d="M5 9l-3 3 3 3M9 5l3-3 3 3M15 19l-3 3-3-3M19 9l3 3-3 3M2 12h20M12 2v20" /></svg>
);
export const IconRotate = () => (
  <svg {...S}><path d="M21 12a9 9 0 1 1-3-6.7" /><path d="M21 3v6h-6" /></svg>
);
export const IconScale = () => (
  <svg {...S}><path d="M21 3 9 15" /><path d="M12 3H3v18h18v-9" /><path d="M16 3h5v5" /></svg>
);
export const IconPlay = () => (
  <svg {...S}><path d="M6 4l14 8-14 8V4z" /></svg>
);
export const IconPause = () => (
  <svg {...S}><rect x="6" y="5" width="4" height="14" rx="1" /><rect x="14" y="5" width="4" height="14" rx="1" /></svg>
);
export const IconStep = () => (
  <svg {...S}><path d="M5 4l10 8-10 8V4z" /><path d="M19 5v14" /></svg>
);
export const IconStop = () => (
  <svg {...S}><rect x="5" y="5" width="14" height="14" rx="2" /></svg>
);
export const IconBug = () => (
  <svg {...S}>
    <circle cx="12" cy="13" r="5" />
    <path d="M12 8V6M9.5 6.5 8 4M14.5 6.5 16 4M7 11H3M7 15H4M17 11h4M17 15h3M12 18v3" />
  </svg>
);
export const IconDots = () => (
  <svg {...S}><circle cx="5" cy="12" r="1.4" /><circle cx="12" cy="12" r="1.4" /><circle cx="19" cy="12" r="1.4" /></svg>
);
export const IconPlus = () => (
  <svg {...S}><path d="M12 5v14M5 12h14" /></svg>
);
export const IconBolt = () => (
  <svg {...S}><path d="M13 2 4 14h7l-1 8 9-12h-7l1-8z" /></svg>
);
export const IconImport = () => (
  <svg {...S}><path d="M4 4h6l2 2h8v12a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V4z" /><path d="M12 11v6M9 14l3 3 3-3" /></svg>
);
export const IconTrash = () => (
  <svg {...S}><path d="M3 6h18M8 6V4h8v2M6 6l1 14a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-14" /><path d="M10 11v6M14 11v6" /></svg>
);
export const IconSidebarLeft = () => (
  <svg {...S}><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M9 4v16" /></svg>
);
export const IconSidebarRight = () => (
  <svg {...S}><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M15 4v16" /></svg>
);
export const IconCamera = () => (
  <svg {...S}><rect x="2" y="6" width="14" height="12" rx="2" /><path d="m16 10 6-3v10l-6-3" /></svg>
);
export const IconEdit = () => (
  <svg {...S}><path d="M12 20h9" /><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4 12.5-12.5z" /></svg>
);
export const IconUndo = () => (
  <svg {...S}><path d="M9 14 4 9l5-5" /><path d="M4 9h11a5 5 0 0 1 0 10h-1" /></svg>
);
export const IconRedo = () => (
  <svg {...S}><path d="m15 14 5-5-5-5" /><path d="M20 9H9a5 5 0 0 0 0 10h1" /></svg>
);
export const ModelGlyph = () => (
  <svg width="34" height="34" viewBox="0 0 24 24" fill="none" aria-hidden="true">
    <path d="M12 2.5 21 7v10l-9 4.5L3 17V7l9-4.5Z" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" />
    <path d="M3 7l9 4.5L21 7M12 11.5V21.5" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" />
  </svg>
);
