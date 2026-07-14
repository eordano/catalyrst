import type { ReactNode } from "react";
import "./auth-layout.css";
import { asset } from "../../asset";

type AuthLayoutProps = {
  children?: ReactNode;
  avatar?: ReactNode;
  centered?: boolean;
  topLeft?: ReactNode;
  bottomLeft?: ReactNode;
  hideBrand?: boolean;
  hideFooter?: boolean;
  brandGlyph?: boolean;
};

export default function AuthLayout({
  children,
  avatar,
  centered = false,
  topLeft,
  bottomLeft,
  hideBrand = false,
  hideFooter = false,
  brandGlyph = false,
}: AuthLayoutProps) {
  return (
    <div className={"auth" + (centered ? " auth--centered" : "")}>
      {!hideBrand && (
        <div className={"auth__logo" + (brandGlyph ? " auth__logo--glyph" : "")}>
          <img src={asset("assets/dcl-logo.png")} alt="" />
          {!brandGlyph && <span>Decentraland</span>}
        </div>
      )}
      {topLeft && <div className="auth__topleft">{topLeft}</div>}
      {avatar}
      <div className="auth__panel">{children}</div>
      {bottomLeft && <div className="auth__bottomleft">{bottomLeft}</div>}
      {!hideFooter && (
        <div className="auth__footer">
          <span>v0.146.0-alpha</span>
        </div>
      )}
    </div>
  );
}
