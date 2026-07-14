import type { CSSProperties, ReactNode } from "react";
import "./viewport.css";
import "./layout.css";

export type FigmaBoxMargins = {
  top?: number;
  right?: number;
  bottom?: number;
  left?: number;
};

export const FIGMA_BOX_MODAL = {
  portraitFrameHeight: 1600,
  portraitMargins: { top: 64, right: 56, bottom: 60, left: 56 },
  landscapeFrameHeight: 720,
  landscapeMargins: { top: 80, right: 80, bottom: 70, left: 80 },
} as const;

export type FigmaBoxProps = {
  children?: ReactNode;
  portraitFrameHeight?: number;
  landscapeFrameHeight?: number;
  portraitMargins?: FigmaBoxMargins;
  landscapeMargins?: FigmaBoxMargins;
  className?: string;
  style?: CSSProperties;
};

type FigmaBoxVars = CSSProperties & {
  "--mfm-pt": string;
  "--mfm-pr": string;
  "--mfm-pb": string;
  "--mfm-pl": string;
  "--mfm-lt": string;
  "--mfm-lr": string;
  "--mfm-lb": string;
  "--mfm-ll": string;
};

const DEFAULT_MARGIN = 20;

function frameRatio(value: number | undefined, frameHeight: number): string {
  const margin = value ?? DEFAULT_MARGIN;
  if (!Number.isFinite(frameHeight) || frameHeight === 0) return "0px";
  return `${Math.round((margin / frameHeight) * 1e6) / 1e4}dvh`;
}

export default function FigmaBox({
  children,
  portraitFrameHeight = 720,
  landscapeFrameHeight = 720,
  portraitMargins,
  landscapeMargins,
  className,
  style,
}: FigmaBoxProps) {
  const vars: FigmaBoxVars = {
    ...style,
    "--mfm-pt": frameRatio(portraitMargins?.top, portraitFrameHeight),
    "--mfm-pr": frameRatio(portraitMargins?.right, portraitFrameHeight),
    "--mfm-pb": frameRatio(portraitMargins?.bottom, portraitFrameHeight),
    "--mfm-pl": frameRatio(portraitMargins?.left, portraitFrameHeight),
    "--mfm-lt": frameRatio(landscapeMargins?.top, landscapeFrameHeight),
    "--mfm-lr": frameRatio(landscapeMargins?.right, landscapeFrameHeight),
    "--mfm-lb": frameRatio(landscapeMargins?.bottom, landscapeFrameHeight),
    "--mfm-ll": frameRatio(landscapeMargins?.left, landscapeFrameHeight),
  };

  return (
    <div className={className ? `mfm ${className}` : "mfm"} style={vars}>
      {children}
    </div>
  );
}
