import { siteUrl } from "../../data/site";
import type { ReactNode } from "react";
import { useState } from "react";
import SitesChrome from "../frames/SitesChrome";
import "./stinvite.css";

export type Referrer = { name: string; ethAddress: string };

const ENVELOPE_SRC = "/assets/twemoji-1f48c.svg";

type Faq = { q: string; a: string };

const FAQS: Faq[] = [
  {
    q: "What is Decentraland?",
    a: "Decentraland is a community-driven virtual world where you can connect, explore, and create. Built on the Unity engine, Decentraland streams and renders a vast, continuous environment filled with diverse, user-generated content. As you explore this immersive landscape, you encounter new creations daily, from games and puzzles to scenes and artworks, all crafted by Decentraland's vibrant community of creators.\n\nDecentraland goes beyond traditional gaming experiences. In Decentraland, you can attend live music events, conferences, exhibitions, dance parties, and more.\n\nYour New Friends Are Waiting -- open Decentraland in your browser and jump straight in!",
  },
  {
    q: "How do I enter Decentraland?",
    a: "Step 1: Open Decentraland in Your Browser\nNothing to install \u{2014} go to catalyst.example.com/play and the world loads right in your browser.\n\nStep 2: Create your Account\nWhen you enter Decentraland, you can use a standard web3 wallet such as Metamask, or log in with a Social account (Google, Discord, X, or Apple).\n\nStep 3: Customize Your Avatar\nOnce you enter Decentraland you'll first be asked to customize your avatar.\n\nStep 4: Explore!\nAfter completing a quick tutorial, Decentraland is yours to explore!\n\nStep 5: Have fun!\nWalk around, talk to others via the chat box, and pop into some events!",
  },
  {
    q: "Do I need cryptocurrency or a digital wallet to use Decentraland?",
    a: "You do not need to own cryptocurrency to enjoy Decentraland as it is free to use. If you decide to purchase a community-made creation from the Marketplace, there are multiple payment options available, such as credit/debit card and bank transfer in addition to various cryptocurrencies.\n\nAs for owning a digital wallet, if you don't already have one, you don't need to get one yourself. When you sign in to Decentraland for the first time, a digital wallet will be created for you behind the scenes if you choose to use a Social account.",
  },
  {
    q: "How do I become a Decentraland Creator?",
    a: "Decentraland is a world created by its users, meaning you can be a creator of everything from Wearables, Emotes, entire scenes with buildings and/or landscapes, interactive experiences, and games.\n\nMost Wearable and Emote designers use Blender to design their creations. Once complete, items are submitted for publishing in the Marketplace.\n\nYou can also download the Creator Hub which combines all of the ease of use of the Web Editor with the power of writing code.",
  },
  {
    q: "What is a NAME?",
    a: "Costing just 100 MANA, the currency of Decentraland, a Decentraland NAME is essentially a unique and personalized username for your avatar that represents your virtual identity within the Decentraland ecosystem. Getting a Decentraland NAME comes with the added bonus of granting the owner their own World.",
  },
  {
    q: "What is the difference between LANDs and Worlds?",
    a: "Currently, Worlds can only admit up to 100 visitors at a time and scenes are limited to 100MB whereas LAND parcels have no such limitations. If you would like to build large scenes or host big events, buying or renting LAND will be a better option.\n\nScenes deployed on LANDs in Genesis City are also more easily discovered by users as they don't need to know your NAME to visit.",
  },
  {
    q: "How can I get help and contact the Support Team?",
    a: "You can contact the Decentraland Support Team via live chat or you can join the official Decentraland Discord server. Please remember that Decentraland staff will never ask you for the private key (seed phrase) to your crypto wallet.",
  },
];

function GradientTitle({ children }: { children: ReactNode }) {
  const text = String(children);
  const idx = text.toLowerCase().indexOf("decentraland");
  if (idx === -1) return <>{text}</>;
  return (
    <>
      {text.slice(0, idx)}
      <span className="stinvite__grad">Decentraland</span>
      {text.slice(idx + "Decentraland".length)}
    </>
  );
}

function CircleAndArrow({ open }: { open: boolean }) {
  return (
    <svg
      className={"stinvite__arrow" + (open ? " is-open" : "")}
      width="57"
      height="57"
      viewBox="0 0 72 72"
      fill="none"
      aria-hidden="true"
    >
      <circle
        opacity={open ? 1 : 0.2}
        cx="36"
        cy="36"
        r="35"
        stroke="white"
        fill={open ? "white" : "none"}
        strokeWidth="2"
      />
      <path
        d="M45 33.0022L42.885 30.8872L36 37.7572L29.115 30.8872L27 33.0022L36 42.0022L45 33.0022Z"
        fill={open ? "#242129" : "white"}
      />
    </svg>
  );
}

type InviteHeroProps = {
  title: string;
  subtitle: string;
  buttonLabel: string;
  referrerName?: string;
  secondary?: boolean;
  loading?: boolean;
};

function InviteHero({ title, subtitle, buttonLabel, referrerName, secondary, loading }: InviteHeroProps) {
  return (
    <section className={"stinvite__hero" + (secondary ? " stinvite__hero--second" : "")}>
      <div className="stinvite__hero-inner">
        <div className="stinvite__rail">
          <div className="stinvite__rail-text">
            {!secondary && (
              <div className="stinvite__envelope">
                <span className="stinvite__envelope-glow" />
                <img className="stinvite__envelope-img" src={ENVELOPE_SRC} alt="" width={115} height={115} />
              </div>
            )}
            <h1 className="stinvite__title">
              {!secondary && !loading && referrerName ? referrerName + " " : ""}
              <GradientTitle>{title}</GradientTitle>
            </h1>
            <p className="stinvite__subtitle">{subtitle}</p>
          </div>
          <div className="stinvite__actions">
            <a className="stinvite__cta-btn" href={siteUrl("/download")} onClick={(e) => e.preventDefault()}>
              {buttonLabel}
            </a>
          </div>
        </div>

        <div className="stinvite__media">
          <div className="stinvite__anim-bg" aria-hidden="true" />
          {secondary && <div className="stinvite__overlay" aria-hidden="true" />}
          <div className="stinvite__video" aria-hidden="true" />
          {!secondary && (
            <div className="stinvite__avatar-slot" aria-hidden="true">
              {loading ? (
                <span className="u-skel__line stinvite__avatar-skel" />
              ) : (
                <div className="stinvite__avatar-figure" />
              )}
            </div>
          )}
        </div>
      </div>
    </section>
  );
}

function InviteFaqs() {
  const [expanded, setExpanded] = useState<number | false>(false);
  return (
    <section className="stinvite__faqs" aria-label="Frequently Asked Questions">
      <div className="stinvite__faqs-border">
        <div className="stinvite__faqs-inner">
          <p className="stinvite__faqs-eyebrow">Learn more about Decentraland</p>
          <h2 className="stinvite__faqs-title">
            Frequently Asked
            {"\n"}
            Questions
          </h2>
          {FAQS.map((faq, i) => {
            const open = expanded === i;
            return (
              <div
                key={i}
                className={"stinvite__faq" + (open ? " is-open" : "")}
                role="button"
                tabIndex={0}
                aria-expanded={open}
                onClick={() => setExpanded(open ? false : i)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    setExpanded(open ? false : i);
                  }
                }}
              >
                <div className="stinvite__faq-row">
                  <span className="stinvite__faq-q">{faq.q}</span>
                  <CircleAndArrow open={open} />
                </div>
                <div className={"stinvite__faq-ans" + (open ? " is-open" : "")} role="region" aria-hidden={!open}>
                  <p className="stinvite__faq-a">{faq.a}</p>
                </div>
              </div>
            );
          })}
          <a className="stinvite__faqs-cta" href="https://docs.decentraland.org/faqs/decentraland-101">
            see more
          </a>
        </div>
      </div>
    </section>
  );
}

type StInviteProps = {
  referrer?: Referrer | null;
  loading?: boolean;
};

export default function StInvite({ referrer = null, loading = false }: StInviteProps) {
  return (
    <SitesChrome hideNav>
      <div className="stinvite">
        <InviteHero
          title="invited you to join Decentraland"
          subtitle="A social virtual world where you can connect, create, and be yourself"
          buttonLabel="JOIN NOW"
          referrerName={referrer?.name}
          loading={loading}
        />
        <InviteHero
          secondary
          title="Hang out in Decentraland"
          subtitle="Meet people from around the world, attend live events, and customize your digital self -- all in a virtual world owned by its community."
          buttonLabel="JOIN FREE"
        />
        <InviteFaqs />
      </div>
    </SitesChrome>
  );
}
