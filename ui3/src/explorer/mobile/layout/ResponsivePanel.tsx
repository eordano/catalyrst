import type { CSSProperties, ReactNode } from "react";
import "./viewport.css";
import "./layout.css";

export type ResponsivePanelSizing = {
  portraitWidth: number;
  portraitMaxHeight: number;
  portraitMinHeight: number;
  landscapeWidth: number;
  landscapeMaxHeight: number;
  landscapeMinHeight: number;
};

export const RESPONSIVE_PANEL_DEFAULTS: ResponsivePanelSizing = {
  portraitWidth: 0.9,
  portraitMaxHeight: 0.8,
  portraitMinHeight: 0,
  landscapeWidth: 0.45,
  landscapeMaxHeight: 0.8,
  landscapeMinHeight: 0,
};

export const RESPONSIVE_PANEL_PRESETS = {
  modal: {
    portraitWidth: 0.72,
    portraitMaxHeight: 0.8,
    portraitMinHeight: 0.1,
    landscapeWidth: 0.45,
    landscapeMaxHeight: 0.8,
    landscapeMinHeight: 0.1,
  },
  codeModal: {
    portraitWidth: 0.85,
    portraitMaxHeight: 0.8,
    portraitMinHeight: 0.1,
    landscapeWidth: 0.45,
    landscapeMaxHeight: 0.8,
    landscapeMinHeight: 0.1,
  },
  inputModal: {
    portraitWidth: 0.85,
    portraitMaxHeight: 0.8,
    portraitMinHeight: 0.1,
    landscapeWidth: 0.45,
    landscapeMaxHeight: 0.8,
    landscapeMinHeight: 0.1,
  },
  travelModal: {
    portraitWidth: 0.85,
    portraitMaxHeight: 0.4,
    portraitMinHeight: 0.1,
    landscapeWidth: 0.55,
    landscapeMaxHeight: 0.5,
    landscapeMinHeight: 0.1,
  },
} as const satisfies Record<string, ResponsivePanelSizing>;

export type ResponsivePanelPreset = keyof typeof RESPONSIVE_PANEL_PRESETS;

export type ResponsivePanelProps = Partial<ResponsivePanelSizing> & {
  children?: ReactNode;
  preset?: ResponsivePanelPreset;
  centerHorizontal?: boolean;
  centerVertical?: boolean;
  verticalOffset?: number;
  className?: string;
  style?: CSSProperties;
};

type ResponsivePanelVars = CSSProperties & {
  "--mrp-pw": number;
  "--mrp-pmaxh": number;
  "--mrp-pminh": number;
  "--mrp-lw": number;
  "--mrp-lmaxh": number;
  "--mrp-lminh": number;
  "--mrp-voff": number;
};

export default function ResponsivePanel({
  children,
  preset,
  portraitWidth,
  portraitMaxHeight,
  portraitMinHeight,
  landscapeWidth,
  landscapeMaxHeight,
  landscapeMinHeight,
  centerHorizontal = true,
  centerVertical = true,
  verticalOffset = 0,
  className,
  style,
}: ResponsivePanelProps) {
  const base = preset ? RESPONSIVE_PANEL_PRESETS[preset] : RESPONSIVE_PANEL_DEFAULTS;

  const classes = ["mrp"];
  if (centerHorizontal) classes.push("mrp--center-h");
  if (centerVertical) classes.push("mrp--center-v");
  if (className) classes.push(className);

  const vars: ResponsivePanelVars = {
    ...style,
    "--mrp-pw": portraitWidth ?? base.portraitWidth,
    "--mrp-pmaxh": portraitMaxHeight ?? base.portraitMaxHeight,
    "--mrp-pminh": portraitMinHeight ?? base.portraitMinHeight,
    "--mrp-lw": landscapeWidth ?? base.landscapeWidth,
    "--mrp-lmaxh": landscapeMaxHeight ?? base.landscapeMaxHeight,
    "--mrp-lminh": landscapeMinHeight ?? base.landscapeMinHeight,
    "--mrp-voff": verticalOffset,
  };

  return (
    <div className={classes.join(" ")} style={vars}>
      {children}
    </div>
  );
}
