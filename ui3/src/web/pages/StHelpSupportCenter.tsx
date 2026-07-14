import { siteHost } from "../../data/site";
import { useState } from "react";
import { SitesChromeMaybe } from "../frames/SitesChrome";
import { asset } from "../../asset";
import "./sthelpsupportcenter.css";


const SITE_HOST = siteHost();
const HelpTab = { FAQ: "faq", SUPPORT_UPDATES: "support updates" } as const;
type HelpTabValue = (typeof HelpTab)[keyof typeof HelpTab];

const Status = { OK: "ok", DOWN: "down", SLOW: "slow", UNKNOWN: "unknown" } as const;
type StatusValue = (typeof Status)[keyof typeof Status];

type Service = { name: string; url: string; status: StatusValue };

const SERVICES: Service[] = [
  { name: "Content Server", url: "/about", status: Status.OK },
  { name: "Lambdas", url: "/lambdas/status", status: Status.OK },
  { name: "Marketplace", url: "/market/ping", status: Status.OK },
  { name: "Places", url: "/places/api/status", status: Status.OK },
  { name: "Events", url: "/events/api/events?limit=1", status: Status.OK },
  { name: "Comms", url: "/comms/status", status: Status.OK },
  { name: "Communities", url: "/v1/communities?limit=1", status: Status.OK },
  { name: "Governance", url: "/governance-api/health", status: Status.OK },
  { name: "Credits", url: "/credits/health", status: Status.OK },
];

type HelpArticle = { question: string; answer: string };

const FAQ_ITEMS: HelpArticle[] = [
  {
    question: "How do I enter Decentraland?",
    answer:
      `The first step is downloading the Decentraland app onto your computer at ${SITE_HOST}/download. Once installed, log into your Decentraland account by connecting your Google, Discord, or another social profile, or a digital wallet such as MetaMask or Coinbase. Once online verification is complete, open the Decentraland app and click 'Jump Into Decentraland'. From here you can customize your avatar, explore the community-built world, or attend an event and make some friends!`,
  },
  {
    question: "Do I need crypto or a digital wallet to use Decentraland?",
    answer:
      "No, you do not need to own crypto or already have a digital wallet to use Decentraland. Decentraland is free to use, and if you'd like to purchase something from the Marketplace, you can use a credit/debit card in addition to cryptocurrency. If you sign into Decentraland with a social account such as Google or Discord, a digital wallet will be made for your account behind the scenes, so you don't have to worry about anything.",
  },
  {
    question: "What hardware do I need to run Decentraland?",
    answer:
      `Currently, Decentraland is available for PC on Windows and Mac. For Windows, you'll need at least Windows 10 64-bit, an Intel i5 7th gen or AMD Ryzen 5 CPU, an Nvidia RTX 20 Series or AMD Radeon RX 5000 Series GPU, 6 GB VRAM, 16 GB RAM, and 8 GB storage. For Mac, you'll need at least macOS 11 Big Sur with an Apple M1 chip, 16 GB RAM, and 8 GB storage. Decentraland currently does not run on mobile devices. For full hardware specs, visit ${SITE_HOST}/docs/player/FAQs/decentraland-101.`,
  },
  {
    question: "How do I explore and navigate Decentraland?",
    answer:
      "Decentraland consists of Genesis City, an open traversable world made up of community parcels referenced by coordinates, as well as individual Worlds, more intimate 3D spaces that can be teleported into. To explore Genesis City, use your arrow or WASD keys to move around, jump to locations from the map, or type '/goto x,y' in the chat to teleport to specific coordinates. To visit a World, type '/goto WorldName' in the chat.",
  },
  {
    question: "How do I get Wearables, Emotes, LAND, and NAMEs in the Marketplace?",
    answer:
      `Browse the Marketplace at ${SITE_HOST}/marketplace for hundreds of community-made Wearables and Emotes to customize your digital identity, buy or rent Genesis City LAND parcels, or claim your unique NAME which comes with its own World. You can pay by card or with a variety of cryptocurrencies. Anything you purchase is truly owned by you, with ownership registered on the blockchain. You can resell items whenever you wish.`,
  },
  {
    question: "How can I meet people in Decentraland?",
    answer:
      `The best way to meet new people in Decentraland is to attend events and start chatting! Browse the event page at ${SITE_HOST}/whats-on for current and upcoming events and don't be shy \u{2014} Decentraland's community is known for being welcoming. Keep an eye out for weekly tours and meetups, such as ABC DCL - Adventures of Decentraland, which are specifically targeted at new community members!`,
  },
  {
    question: "How can I learn about in-world events?",
    answer:
      `Decentraland's Event Page at ${SITE_HOST}/whats-on is the official place where anyone in the community can post their in-world events. To stay on top of events, check the event page regularly, follow Decentraland on social channels such as X, or subscribe to the weekly newsletter at decentraland.beehiiv.com to learn about upcoming activities!`,
  },
  {
    question: "How do I take pictures in-world?",
    answer:
      "Press C on your keyboard to open the Camera. A helpful guide with camera controls will appear on the bottom right of your screen. Your Gallery has space for up to 500 photos, from which you can easily share your images with a link or download them onto your computer. You can feature your favorite photos on your Decentraland Profile by toggling the 'Set as Public' option.",
  },
  {
    question: "How can I get involved in Decentraland governance?",
    answer:
      `Decentraland's DAO is the heart of the community-driven world's governance. To get started, read through the DAO proposals at ${SITE_HOST}/governance to learn about current issues, see what the community consensus is, and add your own comments. Learn more on the DAO's official page at ${SITE_HOST}/dao.`,
  },
];

const SUPPORT_ITEMS: HelpArticle[] = [
  {
    question: "Explorer Release - v0.141.0 (22/4/2026)",
    answer:
      'Client:\n\nFeatures:\n- Disable "switch account" button for new users\n- Extend AVPro to support YouTube and public Google Drive videos (v2)\n- Lens flare for sun and moon\n\nFixes:\n- Enable only the current scene stream setting\n- Error when removing members from a community\n- Multiple LSD and asset load issues\n- Overlapping login screen on new account\n- Remove the mic button when outgoing call\n\nFor more details, please check the PR.',
  },
  {
    question: "New Creator Hub (v0.35.0) Release",
    answer:
      'Features:\n- Spawn Areas: You can now see a visual representation of spawn areas directly in the scene editor, and adjust spawn areas visually, including random offset ranges.\n- Dynamic Asset Catalog with SDK Compatibility: The asset catalog now loads the latest version from the CDN at runtime, so new assets appear automatically without waiting for a full app update.\n- World Access Permissions: The "Access" tab is now available in Worlds settings, letting you manage who can enter your world directly from the Creator Hub.\n\nFixes:\n- Scene Layout in Advanced Mode: The scene layout now updates correctly when editing in advanced mode.\n- Duplicated Ground in Scenes: Opening a scene no longer renders a duplicated ground plane.\n- Fetch Managed Projects: Fixed a failure when loading managed projects.\n- Windows Code Signing: The Windows build pipeline now properly fails on code-signing errors instead of silently producing unsigned executables.\n- Removed Legacy Dependency: Replaced the deprecated decentraland-connect library with a lighter vendored implementation using viem, reducing bundle size.\n\nImprovements:\n- UI Polish: Updated colors, component styles, transform alignment, entity inspector header, entity renaming, and asset tab styling for a better editing experience.\n- Collaborators Tab Copy: Updated the text in the collaborators tab with clearer guidance.',
  },
  {
    question: "Attention Creators: Backface Culling Updates",
    answer:
      "Backface Culling is now set to On on the engine by default on all model materials.\n\nWhy: Significant Performance Optimization.\nHow: Because it is an engine optimization, it also affects materials that are already published.\n\nAction Points:\nWe encourage you to check your scenes following the Troubleshooting section in the documentation.\n\nSome scenarios worth checking first:\n- Flags, curtains, walls made of Planes: If a wall made of a plane with a single-sided material, it won't be seen from the other side.\n- Grass / Leaves of a tree made of Planes with single-sided materials: Same case as above.\n- Interior of Cubes and other shapes that act as rooms: If the cube material is facing out, it won't be seen from the inside.",
  },
  {
    question: "Visual Studio Extension - Support Ending",
    answer:
      "With the focus on the Creator Hub, the Visual Studio Extension is no longer supported.\n\nThe Extension has been removed from the VS Marketplace.",
  },
  {
    question: "Player.logs - How to get them",
    answer:
      "Below is a guide on how to find and generate the Player.log file. These files can be helpful when it comes to troubleshooting issues reported.\n\nOpen the chat in the Decentraland Desktop app and type: /logs\n\nOr:\n\nFor Windows:\nNavigate to: C:\\Users\\{YourUserName}\\AppData\\LocalLow\\Decentraland\\Explorer\nFind the file named Player.log\n\nFor Mac:\nOpen Finder and press Cmd + Shift + G to open the \"Go to Folder\" dialog.\nEnter the path: /Users/UserName/Library/Logs/Decentraland/Explorer/\nFind the file named Player.log\n\nPlayer.log file would show issues with the current/active session.\nPlayer-prev.log file would show issues with the previous session.\n\nLauncher Logs (output.log):\nWindows: C:\\Users\\username\\AppData\\Roaming\\DecentralandLauncherLight\nMac: /Users/UserName/Library/Logs/DecentralandLauncherLight",
  },
];

const FaqIcon = ({ dark }: { dark: boolean }) => (
  <svg width="20" height="20" viewBox="0 0 20 20" fill="none" aria-hidden="true">
    <path
      fillRule="evenodd"
      d="M10 0C4.48 0 0 4.48 0 10C0 15.52 4.48 20 10 20C15.52 20 20 15.52 20 10C20 4.48 15.52 0 10 0ZM11 17H9V15H11V17ZM13.07 9.25L12.17 10.17C11.45 10.9 11 11.5 11 13H9V12.5C9 11.4 9.45 10.4 10.17 9.67L11.41 8.41C11.78 8.05 12 7.55 12 7C12 5.9 11.1 5 10 5C8.9 5 8 5.9 8 7H6C6 4.79 7.79 3 10 3C12.21 3 14 4.79 14 7C14 7.88 13.64 8.68 13.07 9.25Z"
      fill={dark ? "#161518" : "#FCFCFC"}
    />
  </svg>
);

const SupportIcon = ({ dark }: { dark: boolean }) => (
  <svg width="20" height="20" viewBox="0 0 20 20" fill="none" aria-hidden="true">
    <path
      d="M10 0C4.48 0 0 4.48 0 10C0 15.52 4.48 20 10 20C15.52 20 20 15.52 20 10C20 4.48 15.52 0 10 0ZM17.46 7.12L14.68 8.27C14.17 6.91 13.1 5.83 11.73 5.33L12.88 2.55C14.98 3.35 16.65 5.02 17.46 7.12ZM10 13C8.34 13 7 11.66 7 10C7 8.34 8.34 7 10 7C11.66 7 13 8.34 13 10C13 11.66 11.66 13 10 13ZM7.13 2.54L8.3 5.32C6.92 5.82 5.83 6.91 5.32 8.29L2.54 7.13C3.35 5.02 5.02 3.35 7.13 2.54ZM2.54 12.87L5.32 11.72C5.83 13.1 6.91 14.18 8.29 14.68L7.12 17.46C5.02 16.65 3.35 14.98 2.54 12.87ZM12.88 17.46L11.73 14.68C13.1 14.17 14.18 13.09 14.68 11.71L17.46 12.88C16.65 14.98 14.98 16.65 12.88 17.46Z"
      fill={dark ? "#161518" : "#FCFCFC"}
    />
  </svg>
);

const CircleAndArrowIcon = ({ isOpen }: { isOpen: boolean }) => (
  <svg className="shsc__accicon" viewBox="0 0 72 72" fill="none" aria-hidden="true">
    <circle
      className="shsc__accring"
      cx="36"
      cy="36"
      r="35"
      stroke="white"
      strokeWidth="2"
      opacity={isOpen ? "1" : "0.2"}
      fill={isOpen ? "white" : "none"}
    />
    <path
      d="M45 33.0022L42.885 30.8872L36 37.7572L29.115 30.8872L27 33.0022L36 42.0022L45 33.0022Z"
      fill={isOpen ? "#242129" : "white"}
    />
  </svg>
);

const StatusHealthyIcon = () => (
  <svg width="16" height="17" viewBox="0 0 16 17" fill="none" aria-hidden="true">
    <circle cx="8" cy="8.5" r="8" fill="#34CE76" />
    <path
      fillRule="evenodd"
      d="M11.913 5.04734C12.1879 5.31166 12.1965 5.7488 11.9322 6.0237L6.73377 11.43C6.42328 11.7529 5.90817 11.758 5.59142 11.4412L3.6267 9.47652C3.35704 9.20685 3.35704 8.76963 3.6267 8.49997C3.89637 8.2303 4.33359 8.2303 4.60326 8.49997L6.14897 10.0457L10.9367 5.06648C11.201 4.79158 11.6381 4.78301 11.913 5.04734Z"
      fill="white"
    />
  </svg>
);
const StatusWarningIcon = () => (
  <svg width="16" height="17" viewBox="0 0 16 17" fill="none" aria-hidden="true">
    <circle cx="8" cy="8.5" r="8" fill="#FEA217" />
    <path
      fillRule="evenodd"
      d="M7.40745 5.995C7.40745 5.66789 7.67293 5.40242 8.00003 5.40242C8.32714 5.40242 8.59261 5.66789 8.59261 5.995V8.36531C8.59261 8.69242 8.32714 8.95789 8.00003 8.95789C7.67293 8.95789 7.40745 8.69242 7.40745 8.36531V5.995ZM7.40746 10.1431C7.40746 9.81595 7.67294 9.55047 8.00004 9.55047C8.32715 9.55047 8.59262 9.81595 8.59262 10.1431C8.59262 10.4702 8.32715 10.7356 8.00004 10.7356C7.67294 10.7356 7.40746 10.4702 7.40746 10.1431ZM13.2531 11.5374C13.1174 11.7774 12.8531 11.9208 12.5461 11.9208H3.45358C3.14662 11.9208 2.88233 11.7774 2.74722 11.5374C2.68323 11.4248 2.59908 11.1955 2.7573 10.9347L7.30297 3.39831C7.57971 2.93966 8.41999 2.93966 8.69672 3.39831L13.243 10.9347C13.4006 11.1955 13.3171 11.4248 13.2531 11.5374Z"
      fill="white"
    />
  </svg>
);
const StatusUnhealthyIcon = () => (
  <svg width="16" height="17" viewBox="0 0 16 17" fill="none" aria-hidden="true">
    <circle cx="8" cy="8.5" r="8" fill="#FF2D55" />
    <path
      fillRule="evenodd"
      d="M7.40745 5.995C7.40745 5.66789 7.67293 5.40242 8.00003 5.40242C8.32714 5.40242 8.59261 5.66789 8.59261 5.995V8.36531C8.59261 8.69242 8.32714 8.95789 8.00003 8.95789C7.67293 8.95789 7.40745 8.69242 7.40745 8.36531V5.995ZM7.40746 10.1431C7.40746 9.81595 7.67294 9.55047 8.00004 9.55047C8.32715 9.55047 8.59262 9.81595 8.59262 10.1431C8.59262 10.4702 8.32715 10.7356 8.00004 10.7356C7.67294 10.7356 7.40746 10.4702 7.40746 10.1431ZM13.2531 11.5374C13.1174 11.7774 12.8531 11.9208 12.5461 11.9208H3.45358C3.14662 11.9208 2.88233 11.7774 2.74722 11.5374C2.68323 11.4248 2.59908 11.1955 2.7573 10.9347L7.30297 3.39831C7.57971 2.93966 8.41999 2.93966 8.69672 3.39831L13.243 10.9347C13.4006 11.1955 13.3171 11.4248 13.2531 11.5374Z"
      fill="white"
    />
  </svg>
);

const StatusResult = ({ status }: { status: StatusValue }) => {
  if (status === Status.OK) return <StatusHealthyIcon />;
  if (status === Status.DOWN) return <StatusUnhealthyIcon />;
  if (status === Status.SLOW) return <StatusWarningIcon />;
  return <span>&#10068;</span>;
};

type GlobalStatus = { status: StatusValue; color: string; text: string };

function determineGlobalStatus(services: Service[]): GlobalStatus {
  const vals = services.map((s) => s.status);
  if (vals.every((s) => s === Status.OK)) {
    return { status: Status.OK, color: "green", text: "All Operational" };
  }
  if (vals.every((s) => s === Status.DOWN)) {
    return { status: Status.DOWN, color: "red", text: "Services Shutdown" };
  }
  return { status: Status.SLOW, color: "orange", text: "Experiencing Problems" };
}

function StatusDropdown({ services, loading }: { services: Service[]; loading: boolean }) {
  const [open, setOpen] = useState(false);
  const global = determineGlobalStatus(services);

  if (loading) {
    return (
      <div className="shsc__status">
        <div className="shsc__spinner" role="status" aria-label="Loading status" />
      </div>
    );
  }

  if (services.length === 0) return null;

  return (
    <div className="shsc__status">
      <span className="shsc__statusicon">
        <StatusResult status={global.status} />
      </span>
      <button type="button" className="shsc__statusbtn" onClick={() => setOpen((v) => !v)} aria-expanded={open}>
        System Status
      </button>
      {open && (
        <div className="shsc__menu" role="menu">
          <div className={"shsc__menubar is-" + global.color}>{global.text}</div>
          {services.map((svc) => (
            <div key={svc.name} className="shsc__menuitem" role="menuitem">
              <StatusResult status={svc.status} />
              <p className="shsc__menuname">{svc.name}</p>
            </div>
          ))}
          <div className="shsc__menudetails">
            <a href="/about" target="_blank" rel="noopener noreferrer">
              View Details
            </a>
          </div>
        </div>
      )}
    </div>
  );
}

function AccordionList({ items }: { items: HelpArticle[] }) {
  const [expanded, setExpanded] = useState<number | false>(false);
  return (
    <div className="shsc__acclist" role="region" aria-label="Frequently asked questions" tabIndex={0}>
      {items.map((item, i) => {
        const isOpen = expanded === i;
        return (
          <div key={i} className={"shsc__acc" + (isOpen ? " is-open" : "")}>
            <button
              type="button"
              className="shsc__accsum"
              aria-expanded={isOpen}
              onClick={() => setExpanded(isOpen ? false : i)}
            >
              <span className="shsc__accq">{item.question}</span>
              <CircleAndArrowIcon isOpen={isOpen} />
            </button>
            {isOpen && <p className="shsc__accdet">{item.answer}</p>}
          </div>
        );
      })}
    </div>
  );
}

function ContactCTABanner({ supportEmail }: { supportEmail: string }) {
  return (
    <div className="shsc__banner">
      <div className="shsc__bannerbg" aria-hidden="true" />
      <div className="shsc__bannercontent">
        <div className="shsc__bannertexts">
          <h2 className="shsc__bannertitle">Need Help?</h2>
          <p className="shsc__bannersub">Our support team is here to answer your questions!</p>
          <a
            className="shsc__chatbtn"
            href={`mailto:${supportEmail}?subject=${encodeURIComponent("Support request")}`}
          >
            EMAIL SUPPORT
          </a>
        </div>
      </div>
      <img className="shsc__bannerart" src={asset("assets/help-girl.png")} alt="" aria-hidden="true" />
    </div>
  );
}

type StHelpSupportCenterProps = {
  chrome?: boolean;
  activeTab?: HelpTabValue;
  statusLoading?: boolean;
  services?: Service[];
  supportEmail?: string;
};

export default function StHelpSupportCenter({
  chrome = true,
  activeTab: initialTab = HelpTab.FAQ,
  statusLoading = false,
  services = [],
  supportEmail = `hello@${SITE_HOST}`,
}: StHelpSupportCenterProps) {
  const [activeTab, setActiveTab] = useState<HelpTabValue>(initialTab);

  return (
    <SitesChromeMaybe chrome={chrome} active="legal" overlayNav>
      <div className="shsc">
        <div className="shsc__wrapper">
          <div className="shsc__container">
            <aside className="shsc__sidebar">
              <div className="shsc__sidetexts">
                <div className="shsc__mobhead">
                  <h2 className="shsc__title">Help Center</h2>
                </div>
                <p className="shsc__desc">
                  Browse through FAQs and Support Updates or email the Support Team directly using the contact button below.
                </p>
              </div>
              <div className="shsc__tabs">
                <button
                  type="button"
                  className={"shsc__tab" + (activeTab === HelpTab.FAQ ? " is-active" : "")}
                  onClick={() => setActiveTab(HelpTab.FAQ)}
                >
                  <FaqIcon dark={activeTab === HelpTab.FAQ} />
                  FAQ
                </button>
                <button
                  type="button"
                  className={"shsc__tab" + (activeTab === HelpTab.SUPPORT_UPDATES ? " is-active" : "")}
                  onClick={() => setActiveTab(HelpTab.SUPPORT_UPDATES)}
                >
                  <SupportIcon dark={activeTab === HelpTab.SUPPORT_UPDATES} />
                  SUPPORT UPDATES
                </button>
              </div>
            </aside>

            <div className="shsc__content">
              {activeTab === HelpTab.FAQ && (
                <div>
                  <div className="shsc__sectiontexts">
                    <h3 className="shsc__sectitle">Frequently Asked Questions</h3>
                    <p className="shsc__secdesc">
                      Don't see your question below? Check out our full FAQ resources{" "}
                      <a href="/docs/player/FAQs/decentraland-101/">here.</a>
                    </p>
                  </div>
                  <AccordionList items={FAQ_ITEMS} />
                </div>
              )}
              {activeTab === HelpTab.SUPPORT_UPDATES && (
                <div>
                  <div className="shsc__sectiontexts">
                    <h3 className="shsc__sectitle">Support Updates</h3>
                  </div>
                  <AccordionList items={SUPPORT_ITEMS} />
                </div>
              )}
            </div>

            <div className="shsc__statuswrap">
              <StatusDropdown services={services} loading={statusLoading} />
            </div>
          </div>

          <ContactCTABanner supportEmail={supportEmail} />
        </div>
      </div>
    </SitesChromeMaybe>
  );
}

export { HelpTab, Status, SERVICES };
export type { Service, StatusValue };
