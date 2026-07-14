import type { ReactNode } from "react";
import type { SitesNavId } from "../web/frames/SitesChrome";

type TocEntry = { id: string; label: string; depth?: number };

type BodyBlock =
  | string
  | {
      type?: string;
      id?: string;
      heading?: string;
      content?: ReactNode;
      items?: ReactNode[];
    };

type LegalSection = { id: string; heading: string; body: BodyBlock[] };

type LegalDoc = {
  title: string;
  activeSlug: string;
  chromeActive?: SitesNavId;
  intro?: string;
  tableOfContents: TocEntry[];
  sections: LegalSection[];
};

export const SIDEBAR_PAGES = [
  { label: "Logo and Name", slug: "/brand", icon: "favorite" },
  { label: "Content Policy", slug: "/content", icon: "park" },
  { label: "Code of Ethics", slug: "/ethics", icon: "balance" },
  { label: "Rewards Program", slug: "/rewards-terms", icon: "cardGiftcard" },
  { label: "Security", slug: "/security", icon: "shield" },
  { label: "Privacy Policy", slug: "/privacy", icon: "vpnKey" },
  { label: "Referral", slug: "/referral-terms", icon: "redeem" },
  { label: "Terms of Use", slug: "/terms", icon: "explore" },
];

export const LEGAL_ICON_PATHS: Record<string, string> = {
  favorite:
    "M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z",
  park: "M17 12h2L12 2 5.05 12H7l-3.9 6h6.92v4h3.96v-4H21l-4-6z",
  balance:
    "M12 3c-1.27 0-2.4.8-2.82 2H3v2h2.95L2 14c0 2 4 2 4 2s4 0 4-2L7.05 7h2.13c.4.69.99 1.21 1.82 1.5V19H2v2h20v-2h-9V8.5c.83-.29 1.42-.81 1.82-1.5h2.13L14 14c0 2 4 2 4 2s4 0 4-2l-3.95-7H21V5h-6.18C14.4 3.8 13.27 3 12 3zm0 2.5c.55 0 1 .45 1 1s-.45 1-1 1-1-.45-1-1 .45-1 1-1zM4 14l2-3.66L8 14H4zm12 0l2-3.66L20 14h-4z",
  cardGiftcard:
    "M20 6h-2.18c.11-.31.18-.65.18-1a2.996 2.996 0 0 0-5.5-1.65l-.5.67-.5-.68C10.96 2.54 10.05 2 9 2 7.34 2 6 3.34 6 5c0 .35.07.69.18 1H4c-1.11 0-1.99.89-1.99 2L2 19c0 1.11.89 2 2 2h16c1.11 0 2-.89 2-2V8c0-1.11-.89-2-2-2zm-5-2c.55 0 1 .45 1 1s-.45 1-1 1-1-.45-1-1 .45-1 1-1zM9 4c.55 0 1 .45 1 1s-.45 1-1 1-1-.45-1-1 .45-1 1-1zm11 15H4v-2h16v2zm0-5H4V8h5.08L7 10.83 8.62 12 11 8.76l1-1.36 1 1.36L15.38 12 17 10.83 14.92 8H20v6z",
  shield: "M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4z",
  vpnKey:
    "M12.65 10C11.83 7.67 9.61 6 7 6c-3.31 0-6 2.69-6 6s2.69 6 6 6c2.61 0 4.83-1.67 5.65-4H17v4h4v-4h2v-4H12.65zM7 14c-1.1 0-2-.9-2-2s.9-2 2-2 2 .9 2 2-.9 2-2 2z",
  redeem:
    "M20 6h-2.18c.11-.31.18-.65.18-1a2.996 2.996 0 0 0-5.5-1.65l-.5.67-.5-.68C10.96 2.54 10.05 2 9 2 7.34 2 6 3.34 6 5c0 .35.07.69.18 1H4c-1.11 0-1.99.89-1.99 2L2 19c0 1.11.89 2 2 2h16c1.11 0 2-.89 2-2V8c0-1.11-.89-2-2-2zm-5-2c.55 0 1 .45 1 1s-.45 1-1 1-1-.45-1-1 .45-1 1-1zM9 4c.55 0 1 .45 1 1s-.45 1-1 1-1-.45-1-1 .45-1 1-1zm3 15.5L7 17v-5l5 2.5 5-2.5v5l-5 2.5z",
  explore:
    "M12 10.9c-.61 0-1.1.49-1.1 1.1s.49 1.1 1.1 1.1c.61 0 1.1-.49 1.1-1.1s-.49-1.1-1.1-1.1zM12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm2.19 12.19L6 18l3.81-8.19L18 6l-3.81 8.19z",
};

function Ext({ href, children }: { href: string; children: ReactNode }) {
  return (
    <a href={href} target="_blank" rel="noopener noreferrer">
      {children}
    </a>
  );
}

const TERMS: LegalDoc = {
  title: "Terms of Use",
  activeSlug: "/terms",
  chromeActive: undefined,
  tableOfContents: [
    { id: "acceptance-of-terms", label: "1. Acceptance of Terms" },
    { id: "section-1-1", label: "1.1 Introduction", depth: 1 },
    { id: "section-1-2", label: "1.2 Services", depth: 1 },
    { id: "section-1-3", label: "1.3 Agreement to these Terms", depth: 1 },
    { id: "section-1-4", label: "1.4 Free access", depth: 1 },
    { id: "section-1-5", label: "1.5 Open Source Software", depth: 1 },
    { id: "disclaimer-and-modification", label: "2. Disclaimer and Modification of Terms of Use" },
    { id: "eligibility", label: "3. Eligibility" },
    { id: "account-access-and-security", label: "4. Account Access and Security" },
    { id: "representations-and-risks", label: "5. Representations and Risks" },
    { id: "section-5-1", label: "5.1 Disclaimer", depth: 1 },
    { id: "section-5-2", label: "5.2 Sophistication and Risk of Cryptographic Systems", depth: 1 },
    { id: "section-5-3", label: "5.3 Risk of Regulatory Actions in One or More Jurisdictions", depth: 1 },
    { id: "section-5-4", label: "5.4 Risk of Weaknesses or Exploits in the Field of Cryptography", depth: 1 },
    { id: "section-5-5", label: "5.5 Use of Crypto Assets", depth: 1 },
    { id: "section-5-6", label: "5.6 Application Security", depth: 1 },
    { id: "section-5-7", label: "5.7 Third Party Providers", depth: 1 },
    { id: "section-5-8", label: "5.8 Taxes", depth: 1 },
    { id: "section-5-9", label: "5.9 Uses of the Tools", depth: 1 },
    { id: "section-5-10", label: "5.10 Risks of Changes to Ethereum Platform", depth: 1 },
    { id: "section-5-11", label: "5.11 Wearables Curation Committee", depth: 1 },
    { id: "transactions-and-fees", label: "6. Transactions and Fees" },
    { id: "changes", label: "7. Changes" },
    { id: "children", label: "8. Age Eligibility" },
    { id: "indemnity", label: "9. Indemnity" },
    { id: "disclaimers", label: "10. Disclaimers" },
    { id: "limitation-on-liability", label: "11. Limitation on Liability" },
    { id: "proprietary-rights", label: "12. Proprietary Rights" },
    { id: "open-source-license", label: "13. Open Source License" },
    { id: "section-13-1", label: "13.1 Grant of Copyright License", depth: 1 },
    { id: "section-13-2", label: "13.2 Grant of Patent License", depth: 1 },
    { id: "section-13-3", label: "13.3 Redistribution", depth: 1 },
    { id: "section-13-4", label: "13.4 Submission of Contributions", depth: 1 },
    { id: "section-13-5", label: "13.5 Trademarks", depth: 1 },
    { id: "section-13-6", label: "13.6 Disclaimer of Warranty", depth: 1 },
    { id: "section-13-7", label: "13.7 Limitation of Liability", depth: 1 },
    { id: "section-13-8", label: "13.8 Accepting Warranty or Additional Liability", depth: 1 },
    { id: "links", label: "14. Links" },
    { id: "termination-and-suspension", label: "15. Termination and Suspension" },
    { id: "no-third-party-beneficiaries", label: "16. No Third-Party Beneficiaries" },
    { id: "copyright-infringement", label: "17. Notice and Procedure for Making Claims of Copyright Infringement" },
    { id: "binding-arbitration", label: "18. Binding Arbitration and Class Action Waiver" },
    { id: "section-18-1", label: "18.1 Initial Dispute Resolution", depth: 1 },
    { id: "section-18-2", label: "18.2 Binding Arbitration", depth: 1 },
    { id: "section-18-3", label: "18.3 Class Action Waiver", depth: 1 },
    { id: "section-18-4", label: "18.4 Exception - Litigation of Intellectual Property and Small Court Claims", depth: 1 },
    { id: "section-18-5", label: "18.5 30-day Right to Opt-Out", depth: 1 },
    { id: "section-18-6", label: "18.6 Changes to this Section", depth: 1 },
    { id: "general-information", label: "19. General Information" },
    { id: "section-19-1", label: "19.1 Entire Agreement", depth: 1 },
    { id: "section-19-2", label: "19.2 Waiver and Severability of Terms", depth: 1 },
    { id: "section-19-3", label: "19.3 Statute of Limitations", depth: 1 },
    { id: "section-19-4", label: "19.4 Section Titles", depth: 1 },
    { id: "section-19-5", label: "19.5 Communications", depth: 1 },
    { id: "definitions", label: "20. Definitions" },
  ],
  sections: [
    {
      id: "acceptance-of-terms",
      heading: "1. Acceptance of Terms",
      body: [
        { type: "h3", id: "section-1-1", content: "1.1 Introduction" },
        'The Decentraland Platform ("Decentraland Platform") is a community-driven virtual space supported by Decentraland Foundation (the "Foundation") and guided by its users through transparent governance.',
        "The Foundation does not own or control Decentraland platform. Ownership and governance are decentralized and rest with the community through a decentralized autonomous organization (the \"DAO\").",
        {
          type: "p",
          content: (
            <>
              For more information about the DAO, please visit{" "}
              <Ext href="https://dao.decentraland.org">https://dao.decentraland.org</Ext>.
            </>
          ),
        },
        'The Foundation, the DAO and any other entity related to each of the above shall be referred herein jointly as "Decentraland".',
        { type: "h3", id: "section-1-2", content: "1.2 Services" },
        "The Foundation, acting for the benefit of the Decentraland community as a whole, holds the intellectual property rights over, and makes available, the following:",
        {
          type: "p",
          content: (
            <>
              (a) <strong>Clients</strong>, -the software applications through which users access Decentraland, including:
            </>
          ),
        },
        {
          type: "ul",
          items: [
            "the DCL Client (original Web Client) and Desktop Client;",
            "The SDK 7.0, together with versions distributed through third party stores (such as the Epic Game Store, Google Play Store, Apple App Store, among others);",
            <>
              The Bevy Client (*) accessible via browser at{" "}
              <Ext href="https://decentraland.zone/bevy-web">decentraland.zone/bevy-web</Ext>;
            </>,
            "the Mobile Android Client (*) available via Google Play Store;",
            "the Mobile iOS Client (*), available via Apple App Store, and,",
            "any future Client or update developed by Decentraland.",
          ],
        },
        'All of the above, individually and collectively, are referred as the "Clients".',
        {
          type: "p",
          content: (
            <>
              (b) <strong>Tools</strong>, -additional features and services made available by Decentraland, including the Marketplace (
              <Ext href="https://market.decentraland.org">https://market.decentraland.org</Ext>), the Builder, the Blog, Events, Agora, Forum,
              the Land Manager, the Command Line Interface, the DAO interface, the Developers&apos; Hub, the Rewards tool, and any other features,
              tools and/or materials made available from time to time. These are collectively referred as the &quot;Tools&quot;.
            </>
          ),
        },
        {
          type: "p",
          content: (
            <>
              (c) <strong>the Site</strong>, the website located at{" "}
              <Ext href="https://decentraland.org">https://decentraland.org</Ext> (the &quot;Site&quot;).
            </>
          ),
        },
        'The Clients, the Tools and the Site are collectively referred as the "Services".',
        { type: "h3", id: "section-1-3", content: "1.3 Agreement to these Terms" },
        'Please read these Terms of Use (the "Terms" or "Terms of Use") carefully before using the Services.',
        "By accessing or using any of the Services, you confirm that you:",
        "1. Accept and agree to be bound by these Terms;",
        "2. Acknowledge that the Clients and the Tools are still in testing phase and that you use at your own risk, as further explained in Section 2 below;",
        "3. Represent that you are old enough to use the Clients, the Tools and the Site pursuant to Sections 3 and 8 below;",
        {
          type: "p",
          content: (
            <>
              4. Consent to the collection, use, disclosure, handling of information as described in the Privacy Policy, available here{" "}
              <Ext href="https://decentraland.org/privacy">https://decentraland.org/privacy</Ext>;
            </>
          ),
        },
        {
          type: "p",
          content: (
            <>
              5. Accept and agree, the Content Policy, available here{" "}
              <Ext href="https://decentraland.org/content">https://decentraland.org/content</Ext>, and any additional terms and conditions of
              participation issued by the Foundation from time to time.
            </>
          ),
        },
        "If you do not agree to the Terms, then you must not access or use the Services.",
        { type: "h3", id: "section-1-4", content: "1.4 Free access" },
        "Funded through its endowment, the Foundation makes available the Services free of charge to enable interactions with the Decentraland platform.",
        "Decentraland has no continuing obligation to operate the Services and may cease to operate one or more of the Clients, the Tools and/or the Site in the future, at its exclusive discretion, with no liability whatsoever in connection thereto.",
        { type: "h3", id: "section-1-5", content: "1.5 Open Source Software" },
        "With respect to the source code of the software of the Clients and the Tools that has been released under an open source license, such software code must be used in accordance with the applicable open source license terms and conditions as described in Section 13 below. Other similar tools might be developed in the future by the community or third parties.",
      ],
    },
    {
      id: "disclaimer-and-modification",
      heading: "2. Disclaimer and Modification of Terms of Use",
      body: [
        'The Services are provided on an "as is" and "as available" basis and may contain defects and software bugs. You are advised to safeguard important data, property and content, to use caution, and not to rely in any way on the correct or secure functionality or performance of the Tools.',
        "Except for Section 18, providing for binding arbitration and waiver of class action rights, as detailed in Section 7, the Foundation reserves the right, at the sole discretion of the DAO, to modify or replace the Terms of Use at any time. The most current version of these Terms will be posted on the Site. You shall be responsible for reviewing and becoming familiar with any such modifications. Use of the Tools by you after any modification to the Terms constitutes your acceptance of the Terms of Use as modified.",
      ],
    },
    {
      id: "eligibility",
      heading: "3. Eligibility",
      body: [
        "You hereby represent and warrant that you are fully able and competent to enter into the terms, conditions, obligations, affirmations, representations and warranties set forth in these Terms and to abide by and comply with these Terms. Decentraland Platform is a global platform and by accessing the Services, you are representing and warranting that you are of the legal age of majority in your jurisdiction as is required to access such Services and Content and enter into arrangements as provided by the Tools. You further represent that you are otherwise legally permitted to use the Services in your jurisdiction including owning cryptographic tokens, and interacting with the Services or Content in any way. You further represent that you are responsible for ensuring compliance with the laws of your jurisdiction and acknowledge that Decentraland is not liable for your compliance or failure to comply with such laws. You further represent and warrant that all funds or assets used by you have been lawfully obtained by you in compliance with all applicable laws.",
      ],
    },
    {
      id: "account-access-and-security",
      heading: "4. Account Access and Security",
      body: [
        'Access to the Services is provided via a third party private key manager selected by you (e.g., a Web3 Provider, Metamask, a USB interface for Ledger Wallet, the Mist browser, or other). Security and secure access to each account in the Services is provided solely by the third party private key manager you select to administer your private key. You and the third party private key manager you select are entirely responsible for security related to access of the Services and all information provided by you to such third party provider (including without limitation, email or phone number). Decentraland bear no responsibility for any breach of security or unauthorized access to your account (the "Account"). You are advised to: (a) avoid any use of the same password with your selected third party private key manager that you have ever used outside of the third party private key manager; and (b) keep your password and any related secret information secure and confidential and do not share them with anyone else.',
        "You are solely responsible for all activities conducted through your Account whether or not you authorize the activity. In the event that fraud, illegality or other conduct that violates this Agreement is discovered or reported (whether by you or someone else) that is connected with your Account, Decentraland may suspend or block your Account (or Accounts) as described in Section 15.",
        "You are solely responsible for maintaining the confidentiality of your password and for restricting access to your devices. You are solely responsible for any harm resulting from your disclosure, or authorization of the disclosure, of your password or from any person's use of your password to gain access to your Account. You acknowledge that in the event of any unauthorized use of or access to your Account, password or other breach of security, you accept that due to the nature of the Services and the platform itself, Decentraland will be unable to remedy any issues that arise.",
        "Decentraland will not be liable for any loss or damage (of any kind and under any legal theory) to you or any third party arising from your inability or failure for any reason to comply with any of the foregoing obligations, or for any reason whatsoever, except fraud on our part.",
        "Transactions that take place using the Tools are confirmed and managed via the Ethereum blockchain. You understand that your Ethereum public address will be made publicly visible whenever you engage in a transaction using the Services.",
      ],
    },
    {
      id: "representations-and-risks",
      heading: "5. Representations and Risks",
      body: [
        { type: "h3", id: "section-5-1", content: "5.1 Disclaimer" },
        'You acknowledge and agree that your use of the Services is at your sole risk. The Services are provided on an "AS IS" and "AS AVAILABLE" basis, without warranties of any kind, either express or implied, including, without limitation, implied warranties of merchantability, fitness for a particular purpose or non-infringement. You acknowledge and agree that Decentraland has no obligation to take any action regarding: which users gain access to or use the Services; what effects the Services may have on you; the LAND you own; how you may interpret or use the Tools; or what actions you may take or fail to take as a result of having been exposed to the Tools. You release Decentraland from all liability for your inability to access to the Services or any Content therein.',
        { type: "h3", id: "section-5-2", content: "5.2 Sophistication and Risk of Cryptographic Systems" },
        {
          type: "p",
          content: (
            <>
              By utilizing the Services or interacting with the Tools or platform or anything contained or provided therein in any way, you
              represent that you understand the inherent risks associated with cryptographic systems; and warrant that you have an understanding
              of the usage, risks, potential bugs based on novel technology (where applicable), and intricacies of native cryptographic tokens,
              like Ether (ETH) and Bitcoin (BTC), smart contract based tokens such as those that follow the Ethereum Token Standard (
              <Ext href="https://github.com/ethereum/EIPs/issues/20">https://github.com/ethereum/EIPs/issues/20</Ext>), MANA (the ERC-20 token
              that allows users to claim parcels of LAND and trade with each other within Decentraland), LAND (the ERC-721 token, associating
              each LAND parcel&apos;s x and y coordinates with a definition of a parcel&apos;s 3D scene that makes up the larger metaverse) and
              blockchain-based software systems.
            </>
          ),
        },
        { type: "h3", id: "section-5-3", content: "5.3 Risk of Regulatory Actions in One or More Jurisdictions" },
        "The Foundation, MANA, LAND and ETH could be impacted by one or more regulatory inquiries or regulatory action, which could impede or limit your ability to access or use the Tools or Ethereum blockchain.",
        { type: "h3", id: "section-5-4", content: "5.4 Risk of Weaknesses or Exploits in the Field of Cryptography" },
        "You acknowledge and agree that cryptography is a progressing field. Advances in code cracking or technical advances such as the development of quantum computers may present risks to smart contracts, cryptocurrencies and the Tools, which could result in the theft or loss of your cryptographic tokens or property, among other potential consequences. By using the Tools you acknowledge and agree to undertake these risks.",
        { type: "h3", id: "section-5-5", content: "5.5 Use of Crypto Assets" },
        "Some Tools allow the use of MANA, ETH or other similar blockchain technologies. You acknowledge and agree that MANA, Ether and blockchain technologies and associated assets, and other assets are highly volatile due to many factors including but not limited to popularity, adoption, speculation, regulation, technology and security risks. You also acknowledge and agree that the cost of transacting on such technologies is variable and may increase at any time causing impact to any activities taking place on the Ethereum blockchain.",
      ],
    },
  ],
};

const MAIL_PRIVACY = (
  <a href="mailto:privacy@decentraland.org">privacy@decentraland.org</a>
);

const PRIVACY: LegalDoc = {
  title: "Privacy Policy",
  activeSlug: "/privacy",
  chromeActive: "play",
  tableOfContents: [
    { id: "introduction", label: "1. Introduction" },
    { id: "information-collected", label: "2. Information Collected" },
    { id: "way-information-used", label: "3. How Your Personal Information Is Used" },
    { id: "what-is-done", label: "4. What Is Done with Your Information" },
    { id: "your-choice", label: "5. Your Choices" },
    { id: "cookies", label: "6. Cookies and Tracking Technologies" },
    { id: "information-not-collected", label: "7. Information Not Collected" },
    { id: "information-security", label: "8. Information Security" },
    { id: "privacy-rights", label: "9. Privacy Rights" },
    { id: "changes-and-updates", label: "10. Changes and Updates" },
  ],
  sections: [
    {
      id: "introduction",
      heading: "1. Introduction",
      body: [
        "This Privacy Policy should be read in conjunction with the Terms of Use.",
        "1.2. The aim of this Privacy Policy is to help you understand how your personal information is used and your choices regarding said use.",
        "1.3. By accessing and using the Services, you agree and consent that Decentraland can collect, use, disclose, and process your information as described in this Privacy Policy.",
        "1.4 Your use of the Services and any personal information you provide through them remains subject to the terms of this Privacy Policy and the Terms of Use, as each may be updated from time to time.",
        "1.5. This Privacy Policy only applies to the Services, and not to any other websites, products or services you may be able to access or link to via the Services. We encourage you to read the privacy policies of any other websites you visit before providing your information to them.",
        "1.6. The Services may evolve over time, and this Privacy Policy will change to reflect that evolution. If changes are made, you will be notified by revising the date at the top of this Privacy Policy. In some cases, if significant changes are made, an statement may be placed in the homepage. We encourage you to review this Privacy Policy periodically to stay informed.",
        { type: "p", content: <>1.7 Any questions, comments or complaints should be emailed to {MAIL_PRIVACY}.</> },
      ],
    },
    {
      id: "information-collected",
      heading: "2. Information Collected",
      body: [
        "The personal information collected from you generally may include:",
        "2.1 Network information regarding transactions, including the type of device you use, access times, hardware model, operating system and version, and other unique device identifiers.",
        "2.2 Information about plugins you might be using, including but not limited to those related to the management of cryptocurrency assets and any information provided by them.",
        "2.3 Your email and Ether address.",
        "2.4 The Services require the highest level of browser permissions that could potentially lead to procurement of more personal information. Information on these permissions is used for a limited purpose, and why this is necessary, can be found in paragraph 3 below.",
        "2.5 Your interactions with the Services are documented via third party analytics providers such as Segment.io (Twilio) and/or Firebase, and that information is processed by Google and any related company or service.",
      ],
    },
    {
      id: "way-information-used",
      heading: "3. How Your Personal Information Is Used",
      body: [
        "3.1 As with nearly all interactions that take place on the World Wide Web, the servers may receive information by virtue of your interaction with them, including but not limited to IP addresses.",
        "3.2 The Services require full browser permissions that could potentially be used to access additional personal information. Such browser permissions are used for an extremely limited technical purpose for allowing the Services to properly interact with your browser. No additional information is obtained beyond what is necessary to provide the Services. No information received is shared with any third party except as required for provision of the Services.",
        "3.3 Third party providers such as Segment.io (Twilio) and Firebase are used for purposes of monitoring web traffic, usage, engagement and technical performance. Any identifying information collected via such third-party providers is collected, processed and controlled by them.",
        "3.4 Public blockchains provide transparency into transactions and the Foundation is not responsible for preventing or managing information broadcasted on a blockchain.",
      ],
    },
    {
      id: "what-is-done",
      heading: "4. What Is Done with Your Information",
      body: [
        "4.1 Your information may be used in the following ways: To analyze trends for how the Services are being used; To improve the Services; To help personalize your experience of the Services; and if you provide your contact information, you may receive technical notices, updates, confirmations, security alerts, to provide information about events and governance initiatives, to provide support to you, to tell you about other products and services that might interest you, or to respond to your comments or questions.",
        "4.2 Your information may be shared with third parties who need to access it in order to do work related to the Services, including helping make the Services available, or providing analytics services. These third parties only access and use your information as necessary to perform their functions.",
        "4.3 Aggregations and anonymizations that contain your information may be created in a way that does not directly identify you. Those aggregations and anonymizations may be used or shared for a variety of purposes related to the Services.",
        "4.4 Your personal information may be disclosed to agents, businesses, or service providers who process it for providing the Services to you. The Agreements with these service providers limit the kinds of information they can use and ensure they use reasonable efforts to keep your personal information secure.",
        "4.5 The Foundation also reserves the right to disclose personal information that it believes, in good faith, is appropriate or necessary to enforce the Terms of Use, take precautions against liability or harm, to investigate and respond to third-party claims or allegations, to respond to court orders or official requests, to protect the security or integrity of the Services, and to protect the rights, property, or safety of the Decentraland, the Decentraland community of users and LAND owners, or others.",
        "4.6 In the event that the Foundation is involved in a merger, acquisition, sale, bankruptcy, insolvency, reorganization, receivership, assignment for the benefit of creditors, or the application of laws or equitable principles affecting creditors’ rights generally, or other change of control, there may be a disclosure of your information to another entity related to such event.",
      ],
    },
    {
      id: "your-choice",
      heading: "5. Your Choices",
      body: [
        "5.1 Your personal information will be processed in accordance with this Privacy Policy, unless you exercise any of your privacy rights stated in clause 9 of this privacy policy.",
      ],
    },
    {
      id: "cookies",
      heading: "6. Cookies and Tracking Technologies",
      body: [
        "The Services do not use first-party cookies at this time. However, third-party service providers may place cookies or tracking pixels on your browser in connection with your use of these Services. Most web browsers allow you to manage or reject cookies via browser settings. Disabling cookies may affect certain features of the Services.",
      ],
    },
    {
      id: "information-not-collected",
      heading: "7. Information Not Collected",
      body: [
        "Any other personally identifiable information about you shall not be collected, unless you give it to the Foundation directly: by log in, filling out a form, giving written feedback, communicating via third party social media sites, or otherwise communicating via the Site, the Tools, or any other means.",
      ],
    },
    {
      id: "information-security",
      heading: "8. Information Security",
      body: [
        "Whilst neither the Foundation nor any other organization can guarantee the security of information processed online, the Foundation has appropriate security measures in place to protect your personal information including storage on computer systems with limited access, encryption, or both.",
      ],
    },
    {
      id: "privacy-rights",
      heading: "9. Privacy Rights",
      body: [
        "9.1 Subject to applicable law, you may have some or all of the following rights in respect of your personal information: (i) to obtain a copy of your personal information together with information about how and on what basis that personal information is processed; (ii) to rectify inaccurate personal information (including the right to have incomplete personal information completed); (iii) to erase your personal information (in limited circumstances, where it is no longer necessary in relation to the purposes for which it was collected or processed); (iv) to restrict processing of your personal information where: a. the accuracy of the personal information is contested; b. the processing is unlawful but you object to the erasure of the personal information; or c. we no longer require the personal information but it is still required for the establishment, exercise or defense of a legal claim; (v) to challenge processing which we have justified on the basis of a legitimate interest (as opposed to your consent, or to perform a contract with you); (vi) to prevent us from sending you direct marketing; (vii) to withdraw your consent to our processing of your personal information (where that processing is based on your consent); (viii) to object to decisions which are based solely on automated processing or profiling; (ix) in addition to the above, you have the right to file a complaint with the supervisory authority.",
        "9.2 If you reside in California, you may request certain general information regarding our disclosure of personal information to third parties for their direct marketing purposes.",
        { type: "p", content: <>9.3 To exercise any of these rights, please contact us at: {MAIL_PRIVACY}.</> },
      ],
    },
    {
      id: "changes-and-updates",
      heading: "10. Changes and Updates",
      body: [
        "10.1 This Privacy Policy may be revised periodically, reflected by the “Last update posted” date above. In the case of significant changes, a notice will be placed on the homepage. Your continued use of the Services constitutes your agreement to this Privacy Policy and any future revisions.",
        { type: "p", content: <>10.2 Contact Information: {MAIL_PRIVACY}.</> },
      ],
    },
  ],
};

const CONTENT: LegalDoc = {
  title: "Content Policy",
  activeSlug: "/content",
  chromeActive: "docs",
  tableOfContents: [
    { id: "definitions", label: "1. Definitions" },
    { id: "prohibited-content", label: "2. Prohibited Content" },
    { id: "gambling", label: "3. Gambling" },
    { id: "breaches-of-this-policy", label: "4. Breaches of this Policy" },
    { id: "age-restricted-content", label: "5. Restricted Content" },
    { id: "user-representations-and-warranties", label: "6. User Representations and Warranties" },
    { id: "storage", label: "7. Storage" },
    { id: "limitations-to-liability", label: "8. Limitations to Liability" },
    { id: "changes-to-this-policy", label: "9. Changes to this Policy" },
  ],
  sections: [
    {
      id: "definitions",
      heading: "1. Definitions",
      body: [
        '"Content" shall mean any work of authorship, creative works, graphics, images, textures, photos, logos, video, audio, text and interactive features, including without limitation NFTs, submitted by the Users of Decentraland.',
        '"Intellectual Property Rights" shall mean rights in, arising out of, or associated with intellectual property in any jurisdiction, including without limitation rights in or arising out of, or associated with (1) copyrights, mask work rights, and other rights in published and unpublished works of authorship, including without limitation computer programs, databases, graphics, user interfaces, and similar works; (2) patents, design rights, and other rights in inventions and discoveries, including without limitation articles of manufacture, business methods, compositions of matter, improvements, machines, methods, and processes; (3) trademarks, service marks, trade dress and other logos and similar indications of origin of, or association with, a group, business, good, product, or service; (4) trade secrets and other information that is not generally known or readily ascertainable by third parties through proper means, whether tangible or intangible, including without limitation algorithms, customer lists, ideas, designs, formulas, know-how, source code, methods, processes, programs, prototypes, systems, and techniques; (5) a person\'s name, voice, signature, photograph, or likeness, including without limitation rights of personality, privacy, and publicity; (6) attribution and integrity and other so-called moral rights of an author; (7) internet domain names; (8) data and databases; and (9) similar proprietary rights arising under the laws of any jurisdiction',
        '"NFT" means non-fungible token, including, LAND, Wearables and any other kind of NFTs available in Decentraland. All NFTs must comply with this Content Policy in accordance with section 12.4 of the Terms of Use.',
      ],
    },
    {
      id: "prohibited-content",
      heading: "2. Prohibited Content",
      body: [
        "All Content uploaded, posted, created, displayed, transmitted or made available by the User through the Services must not include:",
        "2.1. Content involving illegality, such as piracy, criminal activity, terrorism, obscenity, child pornography, gambling (subject to Section 3 below), and illegal drug use.",
        "2.2. Content infringing third party Intellectual Property Rights.",
        "2.3. Cruel or hateful Content that could harm, harass, promote or condone violence against, or that is primarily intended to incite hatred of, animals, or individuals or groups based on race or ethnic origin, religion, nationality, disability, gender, age, veteran status, or sexual orientation/gender identity.",
        "2.4. Content that is libelous, false, inaccurate, misleading, or invades another person's privacy.",
        "2.5. Content that breaches the Privacy Policy or applicable data privacy laws.",
        "2.6. Content that promotes or could be construed as primarily intended to evade the limitations above.",
      ],
    },
    {
      id: "gambling",
      heading: "3. Gambling",
      body: [
        "If your Content involves gambling, the following shall apply: (i) if you reside in a jurisdiction which requires a license for online gambling, you must obtain such license prior to making your Content available; (ii) you must be in full compliance with the regulations of your country of residence; (iii) you must geo-block your Content for IPs from jurisdictions where online gambling is banned (including, without limitation, the United States of America, South Korea and China) and (iv) you must include in the terms and conditions of use of your Content (if any) a release from liability in favor of the Foundation and the DAO to the fullest extent allowed by applicable law vis a vis you and the users of your Content.",
      ],
    },
    {
      id: "breaches-of-this-policy",
      heading: "4. Breaches of this Policy",
      body: [
        "Any Content in infringement of Section 2, may be blocked and upon blocking shall not be available to other users of the Services. Moreover, infringing Content may result in suspension of the Account, court orders, civil actions, injunctions, criminal prosecutions and other legal consequences brought by the Foundation, the DAO or third parties against the creator, distributor and/or user of said infringing Content. The User's Account may also be terminated in accordance with Section 15 of the Terms of Use.",
      ],
    },
    {
      id: "age-restricted-content",
      heading: "5. Restricted Content",
      body: [
        "Any Content which qualifies as (i) intensely violent or graphic, (ii) gambling or (ii) sexually explicit, shall only be available to people aged 18 or older. If you upload, post, create, display, transmit or make available such Content on the Tools, you must provide sufficient warning as to this restriction. Failure to do so may result in termination of your Account pursuant to Section 15 of the Terms.",
      ],
    },
    {
      id: "user-representations-and-warranties",
      heading: "6. User Representations and Warranties",
      body: [
        "You represent and warrant that at any time you submit Content, you are at least the age of majority in the jurisdiction in which you reside and are the parent or legal guardian, or have all proper consents from the parent or legal guardian, of any minor who contributed to any Content you submit, and that, as to that Content, (a) you are the sole author and owner of the Intellectual Property Rights to such Content, or you have a lawful right to submit the Content, all without any obligation to obtain consent of any third party and without creating any obligation or liability for the Foundation; (b) the Content is accurate; (c) the Content does not and will not infringe any Intellectual Property Right of any third party; and (d) the User Content will not violate the Terms or this Content Policy, or cause injury or harm to any person.",
        "You expressly acknowledge and accept that the Content you submit will be accessible to and viewable by other users and waive any claim with regards to the Foundation, its directors, officers, employees and affiliates in connection with said third party access. You can withdraw your Content at any time you wish.",
        "Please remember that the Content that you submit will be accessible to and viewable by other users. Except as may be required to register and/or maintain your Account, do not submit personally identifiable information (e.g. first and last name together, password, phone number, address, credit or debit card number, medical information, e-mail address, or other contact information) on the Tools.",
        "By submitting, posting or displaying content, and or through Decentraland Platform, the Services and/or the Marketplace, you grant us a worldwide, non-exclusive, royalty-free perpetual, irrevocable, transferable right and license (with the right to sublicense) to use, copy, reproduce, process, adapt, modify, publish, transmit, display, develop, improve, distribute such Content, promote Decentraland, activities, Events, in any and all media or distribution methods (now known or later developed). You further grant Decentraland, the DAO and/or the Foundation, the right to use your name and trademarks, if any, in connection with our use of your Content available at the Platform and/or the Marketplace, from time to time.",
      ],
    },
    {
      id: "storage",
      heading: "7. Storage",
      body: [
        'You acknowledge that due to the decentralized nature of Decentraland and of the blockchain technology, all Content and information submitted by you is not stored in a centralized server, but in several decentralized nodes (the "Nodes"). Thus, the Foundation or the DAO are not liable for any loss of content or information.',
        "The Nodes recognize and accept that in the event of any court order relating to the blocking, suspension or deletion of any Content, they will abide by any such court order.",
      ],
    },
    {
      id: "limitations-to-liability",
      heading: "8. Limitations to Liability",
      body: [
        "The Foundation, its officers, employees, and the DAO are not responsible or liable for the Content, conduct, or services of users or third parties. The Foundation, its officers, employees and the DAO do not control or endorse the Content of communications between users or users' interactions with each other or the Tools.",
        "You acknowledge that you will be exposed to various aspects of the Services involving the conduct, Content, and services of users, and that the Foundation does not control and is not responsible or liable for the quality, safety, legality, truthfulness or accuracy of any such user conduct, Content or user services. You acknowledge that Decentraland does not guarantee the accuracy of information submitted by any user of the Services, nor any identity information about any user. Your interactions with other users and your use of Content are entirely at your own risk. The Foundation has no obligation to become involved in any dispute that you may have or claim to have with one or more users of the Services, or in any manner in any resolution thereof.",
        "THE TOOLS MAY CONTAIN LINKS TO OR OTHERWISE ALLOW CONNECTIONS TO THIRD-PARTY WEBSITES, SERVERS, AND ONLINE SERVICES OR ENVIRONMENTS THAT ARE NOT OWNED OR CONTROLLED BY THE FOUNDATION. DECENTRALAND, ITS OFFICERS, EMPLOYEES AND THE DAO ARE NOT RESPONSIBLE OR LIABLE FOR THE CONTENT, POLICIES OR PRACTICES OF ANY THIRD-PARTY WEBSITES, SERVERS OR ONLINE SERVICES OR ENVIRONMENTS. Please consult any applicable terms of use and privacy policies provided by the third party for such websites, servers or online services or environments.",
        'You acknowledge that the Content of the Services is provided or made available to you under license from independent Content Providers, including other users of the Services ("Content Providers"). You acknowledge and agree that except as expressly provided in this Agreement, the Intellectual Property Rights of other Content Providers in their respective Content are not licensed to you by your mere use of the Services. You must obtain from the applicable Content Providers any necessary license rights in Content that you desire to use or access.',
        "You copy and use Content at your own risk. You are solely responsible and liable for your use, reproduction, distribution, modification, display, or performance of any Content in violation of any Intellectual Property Rights. You agree that Decentraland will have no liability for, and you agree to defend, indemnify, and hold Decentraland harmless from, any claims, losses or damages arising out of or in connection with your use, reproduction, distribution, modification, display, or performance of any Content.",
      ],
    },
    {
      id: "changes-to-this-policy",
      heading: "9. Changes to this Policy",
      body: [
        "The Foundation and/or the DAO may change this Content Policy from time to time. All users have the obligation to be aware of the updated versions of this Policy.",
      ],
    },
  ],
};

const ETHICS: LegalDoc = {
  title: "Code of Ethics",
  activeSlug: "/ethics",
  chromeActive: "docs",
  tableOfContents: [
    { id: "code-of-ethics", label: "1. Decentraland’s Code of Ethics" },
    { id: "policies-and-principles", label: "2. Policies and Principles" },
    { id: "standard-of-conduct", label: "2.1 Standard of Conduct", depth: 1 },
    { id: "compliance-with-law", label: "2.2 Compliance with Law", depth: 1 },
    { id: "financial-recordkeeping", label: "2.3 Financial Recordkeeping/Management", depth: 1 },
    { id: "reporting", label: "2.4 Reporting", depth: 1 },
    { id: "monitoring-and-controlling", label: "2.5 Monitoring and controlling", depth: 1 },
    { id: "employees", label: "2.6 Employees", depth: 1 },
    { id: "equal-opportunity-employment", label: "2.7 Equal Opportunity Employment", depth: 1 },
    { id: "health-and-safety", label: "2.8 Health and Safety in the Workplace", depth: 1 },
    { id: "no-violence", label: "2.9 No Violence", depth: 1 },
    { id: "drugs-and-alcohol", label: "2.10 Drugs and Alcohol", depth: 1 },
    { id: "the-environment", label: "2.11 The Environment", depth: 1 },
    { id: "records-and-reports", label: "2.12 Records and Reports", depth: 1 },
    { id: "confidentiality", label: "2.13 Confidentiality", depth: 1 },
    { id: "compliance-team", label: "3. The Compliance Team" },
    { id: "implementation", label: "4. Implementation – Training Procedures" },
    { id: "business-partners", label: "5. Treatment of Business Partners and Third Parties/Conflicts of Interest" },
    { id: "shareholders", label: "5.1 Shareholders", depth: 1 },
    { id: "competition", label: "5.2 Competition", depth: 1 },
    { id: "business-integrity-gifts", label: "5.3 Business Integrity – Gifts", depth: 1 },
    { id: "conflicts-of-interests", label: "5.4 Conflicts of Interests", depth: 1 },
    { id: "public-activities", label: "5.5 Public Activities", depth: 1 },
    { id: "contracts", label: "5.6 Contracts", depth: 1 },
    { id: "know-your-client", label: "5.7 Know your client", depth: 1 },
    { id: "basic-rules-of-conduct", label: "6. Basic Rules of Conducts on Risk Matters" },
    { id: "breaches-to-the-code", label: "7. Breaches to the Code" },
  ],
  sections: [
    {
      id: "code-of-ethics",
      heading: "1. Decentraland's Code of Ethics",
      body: [
        "This Code of Ethics applies to Decentraland and all of its affiliates or subsidiaries (the \"Decentraland Group\"). Our Values of integrity, responsibility, respect and pioneering are the simplest statement of who we are. They govern everything we do. Our reputation as a company that the users can trust is our most valuable asset, and it is up to all of us to make sure that we continually earn that trust. All of our communications and other interactions with our users should increase their trust in us, that's why we publish this externally and expect all others who work with us to set themselves equally high principles.",
      ],
    },
    {
      id: "policies-and-principles",
      heading: "2. Policies and Principles",
      body: [
        "These Policies and Principles we are talking about the ethical behaviors and guides that we all need to follow when working for Decentraland. They are mandatory for us, but we also publish them externally on our website in support of transparency. Compliance with these principles is an essential part of the way we conduct business. The Decentraland Compliance Team is responsible for guarantying the application of these Policies and Principles throughout the company. Keeping in mind the following Policies and Principles will help us to achieve the highest standards of compliance.",
        { type: "h3", id: "standard-of-conduct", content: "2.1 Standard of Conduct" },
        "We conduct all our operations and transactions with honesty, integrity and openness, and with respect for the human rights and interests of our employees and of all of those with whom we interact.",
        { type: "h3", id: "compliance-with-law", content: "2.2 Compliance with Law" },
        "Decentraland Group and its employees are required to know, respect and comply the laws and regulations of the countries in which we operate. Every employee shall act ethically and in compliance with applicable laws and regulations while carrying out the Decentraland's business. Decentraland has a zero tolerance policy for violations of applicable laws.",
        { type: "h3", id: "financial-recordkeeping", content: "2.3 Financial Recordkeeping/Management" },
        "All relevant transactions must be approved by the Compliance Team before being implemented. The accounting and cash handling procedures must be followed in order to avoid operational and legal risks.",
        { type: "h3", id: "reporting", content: "2.4 Reporting" },
        {
          type: "p",
          content: (
            <>
              Any breaches of this Code must be reported to the Compliance Team, to the email account{" "}
              <a href="mailto:compliance@decentraland.org">compliance@decentraland.org</a>. Provision has been made for
              employees to be able to report in confidence and no employee will suffer as a consequence of doing so no
              matter who he/she is reporting (even a superior). Decentraland aims to encourage employees to report
              potential breaches of the Codes of Ethics, not only once the act is committed.
            </>
          ),
        },
        { type: "h3", id: "monitoring-and-controlling", content: "2.5 Monitoring and controlling" },
        "Although we respect your privacy please be aware that for personal business you should use your personal devices. Decentraland will monitor and control the use of Decentraland's property, which includes, but is not limited to, computer, tablet, cell phone and email accounts provided to you. All devices provided by Decentraland have a monitoring software to prevent the commission of illegal acts. Random checks will be made on all Decentraland's devices",
        { type: "h3", id: "employees", content: "2.6 Employees" },
        "Decentraland is committed to a working environment that promotes diversity and equal opportunity and where there is mutual trust, respect for human rights and no discrimination. We are committed to working with employees to develop and enhance each individual's skills and capabilities, respect them, and maintain good communication with them. This Code of Ethics will be annexed to all employments agreements.",
        { type: "h3", id: "equal-opportunity-employment", content: "2.7 Equal Opportunity Employment" },
        "Employment here is based exclusively upon individual merit and qualifications directly related to professional competence in the area where each employee is specialized. We strictly prohibit unlawful discrimination, harassment, bullying in any form – verbal, physical, or visual or any other characteristics protected by law (such as race, sex, marital status, medical condition, etc.). We also make all reasonable accommodations to meet our obligations under laws protecting the rights of the disabled.",
        { type: "h3", id: "health-and-safety", content: "2.8 Health and Safety in the Workplace" },
        "We are committed to a safe work environment, and we strongly procure full compliance with health and safety regulation. All employees will be receive training concerning safety procedures and fire drills. Furthermore, we encourage a healthy diet for our employees and we make available fruit and vegetables for snacking in the workplace.",
        { type: "h3", id: "no-violence", content: "2.9 No Violence" },
        "We are committed to a violence-free work environment, and we have zero tolerance for any level of violence, harassment or any other inappropriate behavior in the workplace.",
        { type: "h3", id: "drugs-and-alcohol", content: "2.10 Drugs and Alcohol" },
        "Substance abuse is incompatible with the health and safety of our employees, and we don't permit it. Consumption of alcohol is banned at our offices, except for special events, where all employees should use a good judgment and never drink in a way that leads to impaired performance or inappropriate behavior, endangers the safety of others, or violates the law. Illegal drugs in our offices are strictly prohibited.",
        { type: "h3", id: "the-environment", content: "2.11 The Environment" },
        "Decentraland is committed to promote environmental care, increase understanding of environmental issues and disseminate good practice inside the company with recycling procedures.",
        { type: "h3", id: "records-and-reports", content: "2.12 Records and Reports" },
        "Open and effective cooperation requires correct and truthful reporting. This applies equally to the relationship with shareholders, employees, customers and the Decentraland Group as well as with the public and any governmental offices such as, for instance, supervisory authorities.",
        { type: "h3", id: "confidentiality", content: "2.13 Confidentiality" },
        "Confidentiality must be maintained with regard to internal corporate matters which have not been made known to the public. We respect and protect the data privacy and security of the information that we received from any third party.",
      ],
    },
    {
      id: "compliance-team",
      heading: "3. The Compliance Team",
      body: [
        "The Compliance Team has a duty of supervision. The members of the Compliance Team must be diligent, proactive and ethical individuals whose role is to make sure that the Company is conducting its business in full compliance with this Code of Ethics and the applicable law.",
      ],
    },
    {
      id: "implementation",
      heading: "4. Implementation – Training Procedures",
      body: [
        "Decentraland shall conduct regular training procedures to make sure that everyone knows and understands the Code of Ethics. Our employees are the face of our Company and we train them to respect the Company's principles and standards not only while working but also in their own life.",
      ],
    },
    {
      id: "business-partners",
      heading: "5. Treatment of Business Partners and Third Parties/Conflicts of Interest",
      body: [
        { type: "h3", id: "shareholders", content: "5.1 Shareholders" },
        "Decentraland will conduct its operations in accordance with internationally accepted principles of good corporate governance. We will provide timely, regular and reliable information on our activities, structure, financial situation and performance to all shareholders anytime they need it and also be in accordance between all the companies in the Decentraland Group.",
        { type: "h3", id: "competition", content: "5.2 Competition" },
        "Decentraland companies and employees will conduct their operations in accordance with the principles of fair competition and all applicable regulations. Every employee must comply with the laws of fair competition. Employees shall seek guidance from the legal department of their particular company within the Decentraland Group when in doubt.",
        { type: "h3", id: "business-integrity-gifts", content: "5.3 Business Integrity – Gifts" },
        "Decentraland does not give or receive, whether directly or indirectly, bribes or other improper payments or advantages for business or financial gain. One of our principles is to avoid corruption, that's why no employee may offer, give or receive any gift or payment which is, or may be construed as being, a bribe. Any demand for, or offer of, a bribe must be rejected immediately and reported to the Compliance Team. In cases of doubt, the recipient should be asked to obtain prior permission from the Compliance Team.",
        { type: "h3", id: "conflicts-of-interests", content: "5.4 Conflicts of Interests" },
        "All employees and service providers working for Decentraland are expected to avoid personal activities and financial interests which could conflict with their responsibilities to the Company. No employee may directly or indirectly, neither in his/her country nor abroad, offer or grant unlawful benefits in connection with his/her business dealings.",
        { type: "h3", id: "public-activities", content: "5.5 Public Activities" },
        "Decentraland will co-operate with governments and other organizations, both directly and through bodies such as trade associations, in the development of proposed legislation and other regulations which may affect legitimate business interests.",
        { type: "h3", id: "contracts", content: "5.6 Contracts" },
        "Inclusion of the Code of Ethics as an annex to all contracts of the company will be mandatory. All new contractors and partners will need to sign statement acknowledging and accepting the contents of the Code of Ethics to be sure that they know and respect our standards.",
        { type: "h3", id: "know-your-client", content: "5.7 Know your client" },
        "Before entering into any contract a Know Your Client form should be completed by the relevant party. This should provide for the identification of directors, shareholders and final economic beneficiaries and this allows the company to have a real record of who are we dealing with.",
      ],
    },
    {
      id: "basic-rules-of-conduct",
      heading: "6. Basic Rules of Conducts on Risk Matters",
      body: [
        "Decentraland and its employees will ensure that Decentraland does not receive the proceeds of criminal activities. All employees must be alert to the suspicious transactions such as when third parties (i) make or ask for payments in a form outside the ordinary course of business; (ii) split payments from several companies to our company; (iii) make or ask for payments in cash when they are usually made by check or wire transfer; or (iv) make or ask for payments in advance when are not customary or required by contract.",
        "Employees involved in engaging or contracting with third parties such as new clients or investors must:",
        {
          type: "ul",
          items: [
            "Ensure that the third parties in question are subject to screening to assess their identity and legitimacy before contracts are signed or transactions occur;",
            "Carefully consider if it is necessary to consult with the Company's Compliance Team before deciding whether to do business with the third party.",
            "Certain decisions that could involve risks pursuant to the risk matters mentioned should be backed by legal opinions issued by attorneys of the relevant jurisdiction.",
          ],
        },
      ],
    },
    {
      id: "breaches-to-the-code",
      heading: "7. Breaches to the Code",
      body: [
        "Breaching the Code of Ethics could have very serious consequences for Decentraland and for individuals involved. Where illegal conduct is involved, these could include significant fines for Decentraland, imprisonment for individuals and significant damage to our reputation.",
        "Regardless of the sanctions imposed by the law, any employee guilty of a violation of the law or of this Code of Ethics while carrying out the Decentraland Group's business will be subject to disciplinary measures up to and including termination when applicable.",
      ],
    },
  ],
};

const REWARDS_SEASONS = [
  {
    id: "first-season",
    title: "First Season (2025)",
    bullets: [
      "The First Season starts on May 19, 2025 and finishes July 14, 2025.",
      "The First Season will run for 8 weeks, with each week defined as Monday 00:00 UTC to Sunday 23:59 UTC.",
      "The maximum total reward budget for the first Season is set at 640,000 Credits.",
      "The maximum a user can earn during the 8-week trial Season is 64 Marketplace Credits.",
      "Rewards will be valid for a period of ten (10) weeks from the Season launch date (the “validity period”).",
      "Each Marketplace Credit is valued at 1 MANA.",
    ],
    goalsLabel: "Weekly Goals and Credit Values:",
    goals: [
      "Log into DCL on at least 3 separate days - Value: 4 Credits",
      "Attend at least 2 events - Value: 2 Credits",
      "View 3 new Profiles - Value: 1 Credit",
      "Visit 3 new locations - Value: 1 Credit",
    ],
  },
  {
    id: "second-season",
    title: "Second Season (2025)",
    bullets: [
      "Starts August 11, 2025, finishes October 12, 2025.",
      "9 weeks, Monday 00:00 UTC to Sunday 23:59 UTC.",
      "Budget: 640,000 Credits. Max per user: 360 Credits.",
      "Validity: eleven (11) weeks from launch (October 26, 2025 at 23:59).",
      "Each Credit valued at 1 MANA.",
    ],
    goalsLabel: "Weekly Goals:",
    goals: [
      "Log into DCL on at least 3 separate days - 20 Credits (Must remain in-world 10+ min each day)",
      "Attend at least 2 events - 10 Credits (Must remain 10+ min each event)",
      "View 3 new Profiles - 5 Credits",
      "Visit 3 new locations - 5 Credits",
    ],
  },
  {
    id: "third-season",
    title: "Third Season (2025)",
    bullets: [
      "Starts October 27, 2025, finishes December 21, 2025.",
      "8 weeks. Budget: 640,000 Credits. Max per user: 320 Credits.",
      "Validity: ten (10) weeks (January 4, 2026 at 23:59:59).",
      "Each Credit valued at 1 MANA.",
    ],
    goalsLabel: "Weekly Goals:",
    goals: [
      "Log into DCL on at least 3 separate days - 20 Credits",
      "Attend at least 2 events - 10 Credits (Must remain 5+ min)",
      "Explore a weekly location - 5 Credits",
      "Snap A Photo Emoting With Someone - 5 Credits",
    ],
  },
  {
    id: "fourth-season",
    title: "Fourth Season (2026)",
    bullets: [
      "Starts January 12, 2026, finishes March 01, 2026.",
      "7 weeks. Budget: 640,000 Credits. Max per user: 280 Credits.",
      "Validity: nine (9) weeks (March 15, 2026 at 23:59:59).",
      "Each Credit valued at 1 MANA.",
    ],
    goalsLabel: "Weekly Goals:",
    goals: [
      "Log into DCL on at least 3 separate days - 20 Credits",
      "Attend at least 2 events - 10 Credits (Must remain 5+ min)",
      "Explore a weekly location - 5 Credits",
      "Snap A Photo Emoting With Someone - 5 Credits",
    ],
  },
  {
    id: "fifth-season",
    title: "Fifth Season (2026)",
    bullets: [
      "Starts March 16, 2026, finishes April 19, 2026.",
      "5 weeks. Budget: 640,000 Credits. Max per user: 200 Credits.",
      "Validity: six (6) weeks (April 26, 2026 at 23:59:59).",
      "Each Credit valued at 1 MANA.",
    ],
    goalsLabel: "Weekly Goals:",
    goals: [
      "Log into DCL on at least 3 separate days - 20 Credits",
      "Attend at least 2 events - 10 Credits (Must remain 5+ min)",
      "Explore a weekly location - 5 Credits",
      "Snap A Photo Emoting With Someone - 5 Credits",
    ],
  },
  {
    id: "sixth-season",
    title: "Sixth Season (2026)",
    bullets: [
      "Starts April 27, 2026, finishes May 24, 2026.",
      "4 weeks. Budget: 640,000 Credits. Max per user: 160 Credits.",
      "Validity: five (5) weeks (May 31, 2026 at 23:59:59).",
      "Each Credit valued at 1 MANA.",
    ],
    goalsLabel: "Weekly Goals:",
    goals: [
      "Log into DCL on at least 3 separate days - 20 Credits",
      "Attend at least 2 events - 10 Credits (Must remain 5+ min)",
      "Explore a weekly location - 5 Credits",
      "Snap A Photo Emoting With Someone - 5 Credits",
    ],
  },
  {
    id: "seventh-season",
    title: "Seventh Season (2026)",
    bullets: [
      "Starts June 15, 2026, finishes July 19, 2026.",
      "5 weeks. Budget: 640,000 Credits. Max per user: 200 Credits.",
      "Validity: six (6) weeks (July 26, 2026 at 23:59:59).",
      "Each Credit valued at 1 MANA.",
    ],
    goalsLabel: "Weekly Goals:",
    goals: [
      "Log into DCL on at least 3 separate days - 20 Credits",
      "Attend at least 2 events - 10 Credits (Must remain 5+ min)",
      "Explore a weekly location - 5 Credits",
      "Snap A Photo Emoting With Someone - 5 Credits",
    ],
  },
];

const REWARDS_SEASON_BLOCKS = REWARDS_SEASONS.flatMap((s) => [
  { type: "h3", id: s.id, content: s.title },
  { type: "ul", items: s.bullets },
  { type: "p", content: s.goalsLabel },
  { type: "ul", items: s.goals },
]);

const REWARDS: LegalDoc = {
  title: "Rewards Program",
  activeSlug: "/rewards-terms",
  chromeActive: "docs",
  intro: "Terms and Conditions of the Rewards Program",
  tableOfContents: [
    { id: "general-provisions", label: "1. General Provisions" },
    { id: "program-duration", label: "2. Program Duration and Structure" },
    { id: "credits-and-prize-allocation", label: "3. Credits and Prize Allocation" },
    { id: "eligibility-and-participation", label: "4. Eligibility and Participation" },
    { id: "earnings-and-credit-allocation", label: "5. Earnings and Credit Allocation" },
    { id: "reward-validity-and-usage", label: "6. Reward Validity and Usage" },
    { id: "non-transferability", label: "7. Non-Transferability and Restrictions" },
    { id: "email-notifications", label: "8. Email Notifications" },
    { id: "miscellaneous", label: "9. Miscellaneous" },
    { id: "seasons", label: "10. Seasons" },
    { id: "first-season", label: "First Season (2025)", depth: 1 },
    { id: "second-season", label: "Second Season (2025)", depth: 1 },
    { id: "third-season", label: "Third Season (2025)", depth: 1 },
    { id: "fourth-season", label: "Fourth Season (2026)", depth: 1 },
    { id: "fifth-season", label: "Fifth Season (2026)", depth: 1 },
    { id: "sixth-season", label: "Sixth Season (2026)", depth: 1 },
    { id: "seventh-season", label: "Seventh Season (2026)", depth: 1 },
  ],
  sections: [
    {
      id: "general-provisions",
      heading: "1. General Provisions",
      body: [
        '1.1. The Rewards Program (the "Program") is offered by the Decentraland Foundation and is conducted in specific Seasons, the dates of which will be announced by the Decentraland Foundation.',
        "1.2. Participation in the Program constitutes acceptance of these Terms and Conditions, as well as the Decentraland Terms of Use, Privacy Policy, and Content Policy.",
        "1.3. The Decentraland Foundation reserves the right to modify, suspend, or terminate the Program at its discretion.",
      ],
    },
    {
      id: "program-duration",
      heading: "2. Program Duration and Structure",
      body: [
        '2.1. The Program is organized in Seasons (the "Season"). The Decentraland Foundation will announce the start and end dates for each Season.',
      ],
    },
    {
      id: "credits-and-prize-allocation",
      heading: "3. Credits and Prize Allocation",
      body: [
        "3.1. Credits issued under the Program may only be used on primary sales of Polygon Wearables & Emotes (L1 sales are excluded).",
        "3.2. Each Marketplace Credit is valued in each Season; however this value may be changed (lowered or increased) in the future at the discretion of the Decentraland Foundation.",
        "3.3. Credits do not represent voting power (VP).",
        "3.4. A maximum total reward budget is allocated per Season. Once this budget is exhausted, no further rewards will be granted during that Season.",
      ],
    },
    {
      id: "eligibility-and-participation",
      heading: "4. Eligibility and Participation",
      body: [
        "4.1. To be eligible for rewards, participants must complete specified actions, attend designated events, or fulfill other tasks as determined by the Decentraland Foundation.",
        "4.2. The Decentraland Foundation reserves the right to verify the completion of required actions before awarding any rewards.",
      ],
    },
    {
      id: "earnings-and-credit-allocation",
      heading: "5. Earnings and Credit Allocation",
      body: [
        "5.1. Participants who fully complete all designated weekly goals will earn the maximum Marketplace Credits per week mentioned in the corresponding Season.",
        "5.2. The maximum Marketplace Credits a user can earn during the Season shall be determined in each Season.",
        "5.3. A weekly goal must be fully completed to receive its associated credit value.",
        "5.4. Participants will earn credits only for the goals they fully complete; it is not necessary to complete every goal in order to receive some credit.",
      ],
    },
    {
      id: "reward-validity-and-usage",
      heading: "6. Reward Validity and Usage",
      body: [
        '6.1. Rewards will be valid for a period of weeks to be announced in each Season (the "validity period").',
        "6.2. Any rewards not redeemed within the validity period will expire and become non-redeemable.",
      ],
    },
    {
      id: "non-transferability",
      heading: "7. Non-Transferability and Restrictions",
      body: [
        "7.1. Rewards are non-transferable and may only be used by the designated recipient.",
        "7.2. Credits must be issued to a specific beneficiary (a valid EVM Address) and can only be utilized by that beneficiary.",
        "7.3. Rewards cannot be exchanged for cash, MANA, or any other alternative benefits.",
        "7.4. Credits cannot be used to publish a collection, used on Ethereum for this iteration, or transferred between users.",
        "7.5. The Decentraland Foundation reserves the right to revoke or cancel rewards if they are misused or if participants are found to be in violation of the Program's terms.",
      ],
    },
    {
      id: "email-notifications",
      heading: "8. Email Notifications",
      body: [
        '8.1. By participating in the Program, you consent to receive email notifications related to "Weekly Rewards."',
        "8.2. The Decentraland Foundation may add additional email notification categories in the future.",
      ],
    },
    {
      id: "miscellaneous",
      heading: "9. Miscellaneous",
      body: [
        "9.1. The Decentraland Foundation is not responsible for any technical malfunctions, errors, or issues that may affect participation in the Program.",
        "9.2. Any disputes regarding the Program shall be resolved at the sole discretion of the Decentraland Foundation.",
        {
          type: "p",
          content: (
            <>
              For further inquiries, please contact{" "}
              <a href="mailto:legal@decentraland.org">legal@decentraland.org</a>.
            </>
          ),
        },
      ],
    },
    {
      id: "seasons",
      heading: "10. Seasons",
      body: REWARDS_SEASON_BLOCKS,
    },
  ],
};

const REFERRAL_TIERS = [
  "5 Friends Joined: EPIC Bottoms Wearable + Starter Community Recruiter Badge",
  "10 Friends Joined: EPIC Jacket Wearable + Bronze Community Recruiter Badge",
  "20 Friends Joined: LEGENDARY Handwear Wearable",
  "25 Friends Joined: LEGENDARY Emote + Silver Community Recruiter Badge",
  "30 Friends Joined: EXOTIC Shoes Wearable",
  "50 Friends Joined: EXOTIC Hair Wearable + Gold Community Recruiter Badge",
  "60 Friends Joined: MYTHIC “Companion-style” Wearable",
  "75 Friends Joined: MYTHIC Looping Emote+ Platinum Community Recruiter Badge",
  "100 Friends Joined: IRL Swag Pack + Digital Twin Wearable + Diamond Community Recruiter Badge",
];

const REFERRAL: LegalDoc = {
  title: "Referral",
  activeSlug: "/referral-terms",
  chromeActive: undefined,
  intro: "Referral Program – Terms and Conditions",
  tableOfContents: [
    { id: "general-provisions", label: "1. General Provisions" },
    { id: "how-it-works", label: "2. How It Works" },
    { id: "rewards-structure", label: "3. Rewards Structure" },
    { id: "reward-distribution", label: "4. Reward Distribution and Delivery" },
    { id: "program-abuse", label: "5. Program Abuse and Disqualification" },
    { id: "email-notifications", label: "6. Email Notifications" },
    { id: "changes-and-termination", label: "7. Changes and Termination" },
    { id: "miscellaneous", label: "8. Miscellaneous" },
  ],
  sections: [
    {
      id: "general-provisions",
      heading: "1. General Provisions",
      body: [
        '1.1. This Referral Program ("Program") is open to all registered users of Decentraland platform who are in good standing and comply with our general Terms of Service.',
        "1.2. Participation in the Program constitutes acceptance of these Terms and Conditions, as well as the Decentraland Terms of Use, Privacy Policy, and Content Policy.",
        "1.3. The Decentraland Foundation reserves the right to modify, suspend, or terminate the Program at its discretion.",
      ],
    },
    {
      id: "how-it-works",
      heading: "2. How It Works",
      body: [
        '2.1. To refer a new user, share your unique referral link. A referred user ("Referee") will be considered valid if the following conditions are met:',
        {
          type: "ul",
          items: [
            "The Referee successfully creates an account on our platform using a referral link.",
            "The Referee downloads and installs Decentraland's desktop client.",
            "The Referee jumps into Genesis City via the desktop client three (3) different days.",
          ],
        },
        "2.2. Only referrals meeting those criteria will count towards your reward tally.",
        '2.3. Every successful referral should count and be displayed in a counter under the "Referral Rewards" section.',
      ],
    },
    {
      id: "rewards-structure",
      heading: "3. Rewards Structure",
      body: [
        'Rewards are granted based on the number of valid referrals you accumulate in the "Referral Rewards" section. Each reward tier is cumulative and unlocks sequentially:',
        { type: "ul", items: REFERRAL_TIERS },
      ],
    },
    {
      id: "reward-distribution",
      heading: "4. Reward Distribution and Delivery",
      body: [
        "4.1. Digital rewards (Wearables, Emotes, badges, and Credits) will be automatically credited to your account within 3 business days of reaching a reward tier.",
        "4.2. IRL rewards require contact through the designated email or the email address you share for such purpose.",
        "4.3. We commit to delivering the Rewards in accordance with the rarity levels of the Wearables as stated in the Program. However, we do not guarantee the delivery of a specific Wearable within a given rarity category, as each Wearable is subject to availability and are traded on the Marketplace.",
      ],
    },
    {
      id: "program-abuse",
      heading: "5. Program Abuse and Disqualification",
      body: [
        "We reserve the right to disqualify participants who attempt to game, abuse, or manipulate the referral system, including but not limited to creating fake accounts, using bots, or violating the platform's Terms of Service.",
      ],
    },
    {
      id: "email-notifications",
      heading: "6. Email Notifications",
      body: [
        '6.1. By participating in the Program, you consent to receive email notifications related to "Referral Program."',
        "6.2. The Decentraland Foundation may add additional email notification categories in the future.",
      ],
    },
    {
      id: "changes-and-termination",
      heading: "7. Changes and Termination",
      body: [
        "We reserve the right to modify or terminate this Program at any time without prior notice. Any changes will be effective immediately upon posting an updated version of these Terms and Conditions.",
      ],
    },
    {
      id: "miscellaneous",
      heading: "8. Miscellaneous",
      body: [
        "7.1. The Decentraland Foundation is not responsible for any technical malfunctions, errors, or issues that may affect participation in the Program.",
        "7.2. Any disputes regarding the Program shall be resolved at the sole discretion of the Decentraland Foundation.",
        {
          type: "p",
          content: (
            <>
              7.3. For any questions regarding the Referral Program, please contact our support team via the platform or at{" "}
              <a href="mailto:support@decentraland.org">support@decentraland.org</a>.
            </>
          ),
        },
        "Updated",
      ],
    },
  ],
};

const SECURITY: LegalDoc = {
  title: "Vulnerability Disclosure Procedure",
  activeSlug: "/security",
  chromeActive: undefined,
  tableOfContents: [],
  sections: [
    {
      id: "disclosure",
      heading: "",
      body: [
        "At Decentraland, we take every measure necessary to ensure the security of the platform. If you are a security researcher and took a look at some of our code, contracts, or websites and found a vulnerability, you're eligible for a bounty for doing a responsible disclosure of that bug.",
        {
          type: "node",
          content: (
            <a
              className="legaldoc__button"
              href="https://immunefi.com/bounty/decentraland/"
              target="_blank"
              rel="noopener noreferrer"
            >
              Submit a report on Immunefi
            </a>
          ),
        },
      ],
    },
  ],
};

const BRAND: LegalDoc = {
  title: "Terms of Use for Decentraland's Logo and Name",
  activeSlug: "/brand",
  chromeActive: undefined,
  intro:
    "Decentraland is a decentralized virtual world governed by its users and as such strongly supports user generated content (UGC). People may use the Decentraland logo and name in their creations within Decentraland, but must follow the following terms.",
  tableOfContents: [
    { id: "allowable-usage", label: "1. Allowable Usage" },
    { id: "logo-and-name-ownership", label: "2. Logo and name ownership" },
    { id: "wearables-and-user-names", label: "3. Wearables and user names in Decentraland" },
    { id: "nfts-and-other-uses", label: "4. NFTs and other uses outside the Decentraland platform" },
    { id: "disclaimer", label: "5. Disclaimer" },
    { id: "press-releases", label: "6. Press Releases" },
    { id: "advertisements", label: "7. Advertisements" },
    { id: "the-name-decentraland", label: '8. The Name "Decentraland"' },
    { id: "the-logo-decentraland", label: '9. The Logo "Decentraland"' },
    { id: "additional-rules", label: "10. Additional Rules" },
  ],
  sections: [
    {
      id: "allowable-usage",
      heading: "1. Allowable Usage",
      body: [
        {
          type: "p",
          content: (
            <>
              Users shall comply with these terms as well as Decentraland&apos;s{" "}
              <Ext href="https://decentraland.org/terms/">Terms of Use</Ext> and{" "}
              <Ext href="https://decentraland.org/content/">Content Policy</Ext> in order to be allowed to use the
              Decentraland name and logo within Decentraland.
            </>
          ),
        },
      ],
    },
    {
      id: "logo-and-name-ownership",
      heading: "2. Logo and name ownership",
      body: [
        "The Decentraland Foundation is the registered owner of the Decentraland name and logo, acting for the benefit of the DAO. This is because currently the DAO lacks formal legal status.",
      ],
    },
    {
      id: "wearables-and-user-names",
      heading: "3. Wearables and user names in Decentraland",
      body: [
        "Users may create wearables and names using the Decentraland logo and name on the Decentraland platform, provided that they don't violate the platform's Content Policy and Terms of Use. As far as the Decentraland Foundation is concerned, you are not prevented from monetizing or obtaining profits from the sale of said wearables or user names. The NFT must be unique in design.",
        "The Decentraland Foundation has the right to deny the use of the Decentraland name and logo at its sole discretion in any case and at any time.",
      ],
    },
    {
      id: "nfts-and-other-uses",
      heading: "4. NFTs and other uses outside the Decentraland platform",
      body: [
        "The Decentraland Foundation does not authorize the creation of NFTs using the Decentraland name or logo outside the Decentraland platform. The use of the Decentraland name or logo is not allowed in other virtual platforms different from Decentraland or in the real world.",
      ],
    },
    {
      id: "disclaimer",
      heading: "5. Disclaimer",
      body: [
        "In all written materials relating to your product, including websites, publications, etc. it must be made clear that you do not have any kind of affiliation, business partnerships or other official association with Decentraland, the DAO or the Decentraland Foundation.",
      ],
    },
    {
      id: "press-releases",
      heading: "6. Press Releases",
      body: [
        "Any press releases that you distribute through social networks, media or news services should clearly state that you do not have any kind of affiliation, business partnerships or other official association with Decentraland, the DAO or the Decentraland Foundation.",
      ],
    },
    {
      id: "advertisements",
      heading: "7. Advertisements",
      body: [
        {
          type: "p",
          content: (
            <>
              Any paid advertisement that uses the Decentraland name or logo is generally not allowed and must first be
              approved by the Decentraland Foundation or the DAO. To seek such approval, you can send an email to:{" "}
              <a href="mailto:legal@decentraland.org">legal@decentraland.org</a> or through a DAO proposal.
            </>
          ),
        },
      ],
    },
    {
      id: "the-name-decentraland",
      heading: '8. The Name "Decentraland"',
      body: [
        'Subject to these terms you can use the name "Decentraland" to promote your game(s) or wearables as long as it doesn\'t confuse consumers and as long as it is not used as the name of an app or any kind of merchandise or product. In other words, you can refer to "Decentraland" as the name of the platform, and you can use the name to show your interest in or affection for the Decentraland platform.',
      ],
    },
    {
      id: "the-logo-decentraland",
      heading: '9. The Logo "Decentraland"',
      body: [
        {
          type: "p",
          content: (
            <>
              Subject to these terms, you are only allowed to use the logo design that is available at the following link{" "}
              <Ext href="https://decentraland.org/press/">https://decentraland.org/press/</Ext>. The Decentraland
              Foundation or the DAO may alter the form of this design from time to time. You are allowed to resize the logo
              as long as you do not modify the proportions. But otherwise, you cannot modify the logo design in any way and
              must abide by these terms.
            </>
          ),
        },
      ],
    },
    {
      id: "additional-rules",
      heading: "10. Additional Rules",
      body: [
        "THE USE OF DECENTRALAND LOGO AND NAME CANNOT BE ASSOCIATED WITH ANYTHING THAT VIOLATES THE RIGHTS OF OTHER THIRD PARTIES IP, CREATES BRAND CONFUSION, HAS HARMFUL OR OBJECTIONABLE ASPECTS OR DOES NOT COMPLY WITH APPLICABLE LAWS OR ANY OF DECENTRALAND GUIDELINES, TERMS OR RULES. THE DAO AND THE DECENTRALAND FOUNDATION HAVE THE RIGHT TO DECIDE (IN THEIR SOLE DISCRETION) WHETHER THE USE IS ACCEPTABLE.",
        "YOU MUST NOT USE THE DECENTRALAND NAME OR LOGO IN ANY MANNER THAT IS LIKELY TO HAVE AN ADVERSE EFFECT ON THE REPUTATION OF DECENTRALAND (AS THE DAO OR DECENTRALAND FOUNDATION MAY DETERMINE IN THEIR SOLE DISCRETION). YOU MAY NOT USE THE DECENTRALAND NAME OR LOGO IN ANY WAY THAT SUGGESTS THAT YOU ARE AFFILIATED WITH DECENTRALAND, THE DAO OR THE DECENTRALAND FOUNDATION, OR IN ANY WAY THAT SUGGESTS THAT DECENTRALAND, THE DAO OR THE DECENTRALAND FOUNDATION SPONSORS OR ENDORSES YOUR USAGE.",
        "These terms grant you permission to use the Decentraland trademarks only in the ways described above. The Decentraland Foundation or the DAO may withdraw or change this permission at any time for any reason.",
        "Other uses of the Decentraland logo and name require the express approval of the Decentraland Foundation or the DAO.",
      ],
    },
  ],
};

export const LEGAL_DOCS = {
  terms: TERMS,
  privacy: PRIVACY,
  content: CONTENT,
  ethics: ETHICS,
  rewards: REWARDS,
  referral: REFERRAL,
  security: SECURITY,
  brand: BRAND,
} satisfies Record<string, LegalDoc>;
