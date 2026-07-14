import type { MouseEvent } from "react";

import StCreatorHubDownload from "../../web/pages/StCreatorHubDownload";
import StCreatorHubDownloadSuccess from "../../web/pages/StCreatorHubDownloadSuccess";
import "./ldcreatorhubdownloadpage.css";

export type LdChdOsKey = "windows" | "macos";

export type LdChdOption = {
  os: string;
  osKey: LdChdOsKey;
  url: string;
  arch: string;
};

const OS_GLYPH: Record<LdChdOsKey, string> = {
  windows:
    "data:image/svg+xml;utf8," +
    encodeURIComponent(
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="%23fff" d="M3 5.6 10.3 4.6V11.4H3V5.6Zm0 12.8 7.3 1V12.6H3v5.8Zm8.4 1.2L21 21V12.6h-9.6v8Zm0-15.6V11.4H21V3l-9.6 1Z"/></svg>',
    ),
  macos:
    "data:image/svg+xml;utf8," +
    encodeURIComponent(
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="%23fff" d="M16.4 12.6c0-2.3 1.9-3.4 2-3.5-1.1-1.6-2.8-1.8-3.4-1.8-1.4-.1-2.8.8-3.5.8-.7 0-1.8-.8-3-.8-1.5 0-2.9.9-3.7 2.3-1.6 2.8-.4 6.8 1.1 9 .7 1.1 1.6 2.3 2.8 2.3 1.1 0 1.5-.7 2.9-.7 1.3 0 1.7.7 2.9.7 1.2 0 1.9-1.1 2.7-2.2.8-1.3 1.2-2.5 1.2-2.6-.1 0-2.3-.9-2.3-3.5Zm-2.3-6.4c.6-.8 1-1.8.9-2.9-.9 0-2 .6-2.6 1.4-.6.7-1.1 1.7-.9 2.8 1 0 2-.5 2.6-1.3Z"/></svg>',
    ),
};

function toUiOption(opt: LdChdOption) {
  return { text: opt.os, image: OS_GLYPH[opt.osKey], link: opt.url, arch: opt.arch };
}

type LdCreatorHubDownloadPageProps = {
  step?: "view" | "success";
  os?: LdChdOsKey;
  version?: string;
  primary?: LdChdOption;
  secondary?: LdChdOption[];
  successHref?: string;
  onSurfaceClick?: (e: MouseEvent<HTMLDivElement>) => void;
  onSuccessLinkClick?: (e: MouseEvent<HTMLAnchorElement>) => void;
};

export default function LdCreatorHubDownloadPage({
  step = "view",
  os = "macos",
  version = "",
  primary = undefined,
  secondary = [],
  successHref = "",
  onSurfaceClick = undefined,
  onSuccessLinkClick = undefined,
}: LdCreatorHubDownloadPageProps) {
  if (step === "success") {
    return (
      <div className="creatorhub-download-route" data-step="success" data-os={os}>
        <StCreatorHubDownloadSuccess os={os} />
      </div>
    );
  }

  if (!primary) return null;

  return (
    <div
      className="creatorhub-download-route"
      onClickCapture={onSurfaceClick}
      data-os={primary.osKey}
      data-arch={primary.arch}
      data-version={version}
    >
      <StCreatorHubDownload
        primaryOption={toUiOption(primary)}
        secondaryOptions={secondary.map(toUiOption)}
      />

      <noscript>
        <a href={primary.url}>Download Decentraland Creator Hub for {primary.os}</a>
      </noscript>

      <a
        className="creatorhub-download-route__success-link"
        href={successHref}
        onClick={onSuccessLinkClick}
      >
        I&apos;ve started my download
      </a>
    </div>
  );
}
