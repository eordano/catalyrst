import { siteUrl } from "../../data/site";
import { useRef, useState } from "react";
import Communities from "../pages/Communities";
import { createCommunity } from "../../data/catalyst/communities";
import type { CatalystError } from "../../data/catalyst/client";
import Dropdown from "../../components/Dropdown";
import Toggle from "../../atoms/Toggle";
import Button from "../../atoms/Button";
import { useDialogKeys } from "../../components/useDialogKeys";
import "./communitycreate.css";

const MEMBERSHIP_OPTIONS = [
  { value: "public", label: "Public", note: "Anyone can become a member, view Community details, and join your Voice Streams" },
  { value: "private", label: "Private", note: "Members must be approved by an owner or moderator before they can join" },
];

const membershipDisplay = (o: (typeof MEMBERSHIP_OPTIONS)[number]) => `${o.label}  ${o.note}`;

type CommunityCreateProps = {
  onDone?: (created: boolean) => void;
  standalone?: boolean;
};

export default function CommunityCreate({ onDone, standalone = false }: CommunityCreateProps = {}) {
  const [gated, setGated] = useState(false);
  const [name, setName] = useState("");
  const [desc, setDesc] = useState("");
  const [membership, setMembership] = useState("public");
  const [discoverable, setDiscoverable] = useState(true);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const cardRef = useRef<HTMLDivElement>(null);
  useDialogKeys(cardRef, gated ? undefined : () => onDone?.(false));

  const submit = async () => {
    if (!name.trim() || busy) return;
    setBusy(true);
    setErr("");
    try {
      await createCommunity({
        name: name.trim(),
        description: desc.trim() || name.trim(),
        privacy: membership,
        visibility: discoverable ? "all" : "unlisted",
      });
      onDone?.(true);
    } catch (e) {
      const failure = e as CatalystError;
      if (failure.status === 401 && /doesn't have any names/i.test(failure.message ?? "")) {
        setGated(true);
      } else {
        setErr(failure.message || "Couldn't create the community.");
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      {standalone && (
        <div className="ccr__behind" aria-hidden="true" inert>
          <Communities />
        </div>
      )}

      {gated ? (
        <div className="ep__backdrop ccr__backdrop">
          <div className="ccr__gate">
            <div className="ccr__gateart" aria-hidden="true">
              <svg viewBox="0 0 48 48" width="48" height="48">
                <path d="M14 20v-4a10 10 0 0 1 20 0v4M10 20h28v18H10z" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </div>
            <h1 className="ccr__gatetitle">Get a NAME to Unlock Community Creation</h1>
            <p className="ccr__gatebody">
              NAMEs are unique Decentraland usernames that come with a{" "}
              <a className="ccr__link" href="https://decentraland.org/blog/about-decentraland/decentraland-worlds-your-own-virtual-space" target="_blank" rel="noopener noreferrer">World</a>, and unlock community creation.
            </p>
            <div className="ccr__gatebtns">
              <Button variant="primary" size="lg" className="ccr__primary" onClick={() => setGated(false)}>Get a NAME</Button>
              <Button variant="ghost" size="lg" className="ccr__ghost" onClick={() => setGated(false)}>Maybe later</Button>
            </div>
          </div>
        </div>
      ) : (
        <div className="ep__backdrop ccr__backdrop">
          <div className="ccr" role="dialog" aria-modal="true" aria-label="Create a Community" tabIndex={-1} ref={cardRef}>
            <h1 className="ccr__title">Create a Community</h1>

            <div className="ccr__scroll">
              <div className="ccr__group">
                <div className="ccr__seclabel">PROFILE PICTURE</div>
                <div className="ccr__hint">PNG or JPG | 512x512 px | 500KB max</div>
                <div className="ccr__pfp">
                  <span className="ccr__pfpimg" aria-hidden="true">
                    <svg viewBox="0 0 24 24" width="34" height="34">
                      <rect x="3" y="4" width="18" height="16" rx="2" fill="none" stroke="currentColor" strokeWidth="1.6" />
                      <circle cx="8.5" cy="9.5" r="1.8" fill="currentColor" />
                      <path d="M5 18l4.5-5 3.5 4 2.5-2.5L21 17" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
                    </svg>
                  </span>
                  <button className="ccr__pfpedit" aria-label="Edit profile picture">
                    <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
                      <path d="M14.06 6.19l3.75 3.75M3 17.25V21h3.75L17.81 9.94a1.5 1.5 0 0 0 0-2.12l-1.63-1.63a1.5 1.5 0 0 0-2.12 0L3 17.25z" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
                    </svg>
                  </button>
                </div>
              </div>

              <div className="ccr__group">
                <label className="ccr__seclabel" htmlFor="ccr-name">COMMUNITY NAME <span className="ccr__req">*</span></label>
                <input
                  id="ccr-name" className="ccr__input" maxLength={30} placeholder="Write here"
                  value={name} onChange={(e) => setName(e.target.value)}
                />
              </div>

              <div className="ccr__group">
                <label className="ccr__seclabel" htmlFor="ccr-desc">DESCRIPTION</label>
                <input
                  id="ccr-desc" className="ccr__input" maxLength={240} placeholder="What's your community about?"
                  value={desc} onChange={(e) => setDesc(e.target.value)}
                />
              </div>

              <div className="ccr__group">
                <div className="ccr__seclabel" id="ccr-membership">MEMBERSHIP</div>
                <Dropdown
                  ariaLabel="Membership"
                  options={MEMBERSHIP_OPTIONS.map(membershipDisplay)}
                  value={membershipDisplay(
                    MEMBERSHIP_OPTIONS.find((o) => o.value === membership) ?? MEMBERSHIP_OPTIONS[0]!
                  )}
                  onChange={(display) => {
                    const found = MEMBERSHIP_OPTIONS.find((o) => membershipDisplay(o) === display);
                    if (found) setMembership(found.value);
                  }}
                />
              </div>

              <div className="ccr__group">
                <div className="ccr__seclabel">DISCOVERABILITY</div>
                <Toggle
                  checked={discoverable}
                  onChange={setDiscoverable}
                  ariaLabel={discoverable ? "Visible in the directory" : "Hidden from the directory"}
                />
              </div>
            </div>

            {err && <p className="ccr__policy" style={{ color: "#ff6b6b" }}>{err}</p>}
            <div className="ccr__actions">
              <Button variant="ghost" size="lg" className="ccr__ghost ccr__cancel" onClick={() => onDone?.(false)}>CANCEL</Button>
              <Button
                variant="primary"
                size="lg"
                className="ccr__primary ccr__create"
                aria-disabled={!name.trim() || busy}
                onClick={submit}
              >
                {busy ? "CREATING\u{2026}" : "CREATE"}
              </Button>
            </div>

            <p className="ccr__policy">Please ensure Community content follows Decentraland's <a className="ccr__link" href={siteUrl("/content")} target="_blank" rel="noopener noreferrer">Content Policy</a>.</p>
          </div>
        </div>
      )}
    </>
  );
}
