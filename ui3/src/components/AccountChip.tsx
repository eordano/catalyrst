import type { CSSProperties, MouseEventHandler } from "react";
import { ChevronDown } from "../atoms/icons";
import "./accountchip.css";

type AccountChipProps = {
  account?: string;
  hue?: number;
  onClick?: MouseEventHandler<HTMLButtonElement>;
  className?: string;
};

function displayAccount(account: string): string {
  if (!account) return "Sign in";
  return /^0x[0-9a-fA-F]{40}$/.test(account)
    ? `${account.slice(0, 6)}…${account.slice(-4)}`
    : account;
}

export default function AccountChip({
  account = "",
  hue = 268,
  onClick,
  className = "",
}: AccountChipProps) {
  const avatarStyle: CSSProperties & { "--sz": string; "--hue": number } = {
    "--sz": "26px",
    "--hue": hue,
  };
  return (
    <button type="button" className={"u-chip" + (className ? " " + className : "")} onClick={onClick}>
      <span className="u-chip__avatar u-avatar" style={avatarStyle} />
      <span className="u-chip__addr">{displayAccount(account)}</span>
      <ChevronDown size={14} className="u-chip__caret" />
    </button>
  );
}
