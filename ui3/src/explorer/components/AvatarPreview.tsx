import { useEffect, useState } from "react";

export const SKIN = ["#f5d6c0", "#e8b48c", "#c98c63", "#8d5a3c", "#5c3824"];
export const HAIRC = ["#1a1a1a", "#5c3824", "#b06a2c", "#d9a441", "#9b2d2d", "#3a6ea5"];

export function AvatarStage({
  className = "",
  capabilityGate = false,
}: {
  className?: string;
  capabilityGate?: boolean;
}) {
  const [noWebGpu, setNoWebGpu] = useState(false);
  useEffect(() => {
    if (capabilityGate) setNoWebGpu(!("gpu" in navigator));
  }, [capabilityGate]);
  return (
    <div
      className={"avatar-stage" + (className ? " " + className : "")}
      aria-hidden={noWebGpu ? undefined : true}
    >
      <svg className="avatar-stage__platform" viewBox="0 0 320 96" preserveAspectRatio="xMidYMax meet" aria-hidden="true">
        <defs>
          <radialGradient id="avstage-gold" cx="50%" cy="32%" r="70%">
            <stop offset="0%" stopColor="#ffe27a" />
            <stop offset="46%" stopColor="#f5b73c" />
            <stop offset="78%" stopColor="#d8902a" />
            <stop offset="100%" stopColor="#a8651b" />
          </radialGradient>
        </defs>
        <ellipse cx="160" cy="58" rx="150" ry="34" fill="url(#avstage-gold)" />
        <ellipse cx="160" cy="50" rx="118" ry="26" fill="#f7c14e" opacity=".9" />
        <ellipse cx="160" cy="44" rx="92" ry="20" fill="#ffd76a" />
      </svg>

      <svg className="avatar-stage__figure" viewBox="0 0 120 250" preserveAspectRatio="xMidYMax meet" aria-hidden="true">
        <rect x="44" y="176" width="13" height="64" rx="5" fill="#c98c63" />
        <rect x="63" y="176" width="13" height="64" rx="5" fill="#c98c63" />
        <rect x="44" y="214" width="13" height="30" rx="4" fill="#1c1c22" />
        <rect x="63" y="214" width="13" height="30" rx="4" fill="#1c1c22" />
        <path d="M40 150h40v22q0 8-8 8H68l-8-14-8 14H48q-8 0-8-8z" fill="#e23b4c" />
        <rect x="24" y="92" width="14" height="64" rx="7" fill="#c98c63" />
        <rect x="82" y="92" width="14" height="64" rx="7" fill="#c98c63" />
        <rect x="24" y="92" width="14" height="40" rx="7" fill="#fff" />
        <rect x="82" y="92" width="14" height="40" rx="7" fill="#fff" />
        <rect x="24" y="100" width="14" height="5" fill="#e23b4c" />
        <rect x="24" y="112" width="14" height="5" fill="#e23b4c" />
        <rect x="24" y="124" width="14" height="5" fill="#e23b4c" />
        <rect x="82" y="100" width="14" height="5" fill="#e23b4c" />
        <rect x="82" y="112" width="14" height="5" fill="#e23b4c" />
        <rect x="82" y="124" width="14" height="5" fill="#e23b4c" />
        <rect x="36" y="86" width="48" height="74" rx="12" fill="#fff" />
        <rect x="40" y="86" width="5" height="74" fill="#e23b4c" />
        <rect x="50" y="86" width="5" height="74" fill="#e23b4c" />
        <rect x="60" y="86" width="5" height="74" fill="#e23b4c" />
        <rect x="70" y="86" width="5" height="74" fill="#e23b4c" />
        <rect x="52" y="74" width="16" height="16" rx="5" fill="#c98c63" />
        <circle cx="60" cy="46" r="26" fill="#c98c63" />
        <path d="M34 44c0-17 12-28 26-28s26 11 26 28c0-9-7-12-12-13 2 4 1 7 1 7-3-6-9-8-15-8s-12 2-15 8c0 0-1-3 1-7-5 1-12 4-12 13z" fill="#5c3824" />
        <circle cx="50" cy="46" r="7" fill="none" stroke="#2a2a2a" strokeWidth="2.4" />
        <circle cx="70" cy="46" r="7" fill="none" stroke="#2a2a2a" strokeWidth="2.4" />
        <path d="M57 46h6" stroke="#2a2a2a" strokeWidth="2.4" />
        <path d="M46 58q14 12 28 0q-2 10-14 10t-14-10z" fill="#5c3824" />
      </svg>
      {noWebGpu && (
        <p className="avatar-stage__gate" role="status">
          Live 3D preview unavailable — this browser has no WebGPU, so a
          placeholder mannequin is shown. Use a recent desktop Chrome or Edge
          (or another WebGPU-enabled browser) for the real preview.
        </p>
      )}
    </div>
  );
}

export function Swatches({ colors }: { colors: readonly string[] }) {
  return (
    <div className="bp__swatches">
      {colors.map((c) => <span key={c} className="bp__sw" style={{ background: c }} />)}
    </div>
  );
}
