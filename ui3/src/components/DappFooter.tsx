import { siteBase, siteUrl } from "../data/site";
import "./dappfooter.css";
import SocialIcon from "../atoms/SocialIcons";

type FooterLink = { label: string; href: string };
type FooterSocial = { name: string; href: string; label: string };

const LINKS: FooterLink[] = [
  { label: "Home", href: siteBase() },
  { label: "Privacy Policy", href: siteUrl("/privacy") },
  { label: "Terms of Use", href: siteUrl("/terms") },
  { label: "Content Policy", href: siteUrl("/content") },
  { label: "Code of Ethics", href: siteUrl("/ethics") },
];

const SOCIALS: FooterSocial[] = [
  { name: "discord", href: "https://dcl.gg/discord", label: "Discord" },
  { name: "reddit", href: "https://reddit.com/r/decentraland", label: "Reddit" },
  { name: "github", href: "https://github.com/decentraland", label: "GitHub" },
  { name: "twitter", href: "https://x.com/decentraland", label: "X" },
];

export default function DappFooter() {
  return (
    <footer className="dappfooter">
      <div className="dappfooter__main">
        <span className="dappfooter__lang">
          <span className="dappfooter__flag" aria-hidden="true">
            <svg viewBox="0 0 60 30" width="20" height="14">
              <clipPath id="mkf-uk">
                <path d="M30,15 h30 v15 z v15 h-30 z h-30 v-15 z v-15 h30 z" />
              </clipPath>
              <rect width="60" height="30" fill="#00247d" />
              <path d="M0,0 L60,30 M60,0 L0,30" stroke="#fff" strokeWidth="6" />
              <path d="M0,0 L60,30 M60,0 L0,30" clipPath="url(#mkf-uk)" stroke="#cf142b" strokeWidth="4" />
              <path d="M30,0 v30 M0,15 h60" stroke="#fff" strokeWidth="10" />
              <path d="M30,0 v30 M0,15 h60" stroke="#cf142b" strokeWidth="6" />
            </svg>
          </span>
          English
        </span>
        <nav className="dappfooter__links">
          {LINKS.map((l) => (
            <a key={l.label} href={l.href}>{l.label}</a>
          ))}
        </nav>
      </div>

      <div className="dappfooter__secondary">
        <div className="dappfooter__social">
          {SOCIALS.map((s) => (
            <a key={s.name} href={s.href} aria-label={s.label} target="_blank" rel="noreferrer">
              <SocialIcon name={s.name} size={20} />
            </a>
          ))}
        </div>
        <div className="dappfooter__copyright">© 2026 Decentraland</div>
      </div>
    </footer>
  );
}
