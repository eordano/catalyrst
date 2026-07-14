import type { ReactNode } from "react";

type IntroPart = string | { text: string; href: string };

type FieldOption = string | { value: string; label: string; hue?: number };

type FormField = {
  type?: string;
  name?: string;
  label?: string;
  sublabel?: string;
  postlabel?: string;
  placeholder?: string;
  maxLength?: number;
  minLength?: number;
  min?: number;
  max?: number;
  markdown?: boolean;
  optional?: boolean;
  required?: boolean;
  readOnly?: boolean;
  inline?: boolean;
  tall?: boolean;
  stepper?: boolean;
  counterInBar?: boolean;
  numbered?: boolean;
  addLabel?: string;
  emptyText?: string;
  help?: string;
  shortError?: string;
  unit?: string;
  unitLabel?: string;
  rows?: number;
  value?: string | number;
  hue?: number;
  options?: FieldOption[];
  lines?: { text: string }[];
  fields?: FormField[];
  when?: (v: Record<string, unknown>) => boolean;
};

type FieldGroup = {
  section: string;
  validated?: boolean;
  isNew?: boolean;
  fields: FormField[];
};

type FormDescriptor = {
  title: string;
  showBack?: boolean;
  description?: string | ReactNode[];
  sections: Array<FormField | FieldGroup>;
  vpNotice?: string;
  errorLabel?: string;
  errorCollapsible?: boolean;
  numbered?: boolean;
  submitLabel?: string;
  secondaryLabel?: string;
};

const COAUTHOR_DESC =
  "If you co-authored this proposal with someone else, you can add their wallet addresses to acknowledge their work. After you publish the proposal, co-authors will be asked to confirm or reject the request. Only if they confirm, they will be listed publicly on the proposal page.";

const coAuthorsField = (overrides: Partial<FormField> = {}): FormField => ({
  type: "coauthors",
  name: "coAuthors",
  label: "Co-authors",
  optional: true,
  sublabel: COAUTHOR_DESC,
  placeholder: "Add co-authors",
  ...overrides,
});

const vpLink = (pre: string): IntroPart[] => [
  pre,
  { text: "Buy MANA", href: "https://account.decentraland.org/" },
  " to get VP, or ",
  { text: "run for delegate", href: "https://forum.decentraland.org/t/open-call-for-delegates-apply-now/5840/5" },
  ".",
];
const intro = (parts: IntroPart[]): ReactNode[] =>
  parts.map((p, i) =>
    typeof p === "string" ? p : (
      <a key={i} href={p.href}>{p.text}</a>
    )
  );

const vpGate = (threshold: number) => {
  const formatted = threshold.toLocaleString("en-US");
  return {
    intro: intro(vpLink(`This action requires at least ${formatted} VP. `)),
    vpNotice: `This action requires at least ${formatted} VP.`,
  };
};

const poll = {
  title: "Create a community poll",
  description: [
    "The purpose of the Poll is to introduce a governance issue to the community, gauge community sentiment, and determine if there is enough support to move forward with the drafting of an initial proposal. Polls can only pass to the Draft stage if they have accumulated a threshold of at least 500K VP",
    vpGate(100).intro,
  ],
  sections: [
    {
      type: "text",
      name: "title",
      label: "Title",
      sublabel: "The question you would like to ask the community.",
      placeholder: "Enter your question here",
      maxLength: 80,
    },
    {
      type: "markdown",
      name: "description",
      label: "Description",
      markdown: true,
      sublabel:
        "A brief description of your question. Feel free to explain your motivation for polling the community, elaborate on the optional responses, or link to any relevant resources that might help inform voters.",
      placeholder: "A brief description of your question.",
      maxLength: 7000,
    },
    {
      type: "list",
      name: "choices",
      label: "Options",
      placeholder: "choice",
      addLabel: "Add option",
      max: 100,
    },
    coAuthorsField(),
  ],
};

const banName = {
  title: "Ban a name",
  description: "Banning a name prevents the use of abusive or offensive names for avatars and scenes.",
  sections: [
    {
      type: "text",
      name: "name",
      label: "The name you want to ban",
      placeholder: "Name",
      maxLength: 15,
    },
    {
      type: "markdown",
      name: "description",
      label: "Description",
      markdown: true,
      sublabel:
        "Provide a brief explanation of why this name should be banned. For example: why or to whom might it be offensive?",
      placeholder: "A brief description of your question.",
      maxLength: 250,
      minLength: 20,
    },
    coAuthorsField(),
  ],
};

const DRAFT_BODY = [
  { name: "summary", label: "Summary", sublabel: "One sentence summarizing the proposal.", maxLength: 250 },
  {
    name: "abstract",
    label: "Abstract",
    sublabel: "Two to three sentence overview of the proposal, specifying its motivation and outcomes.",
    maxLength: 3500,
  },
  {
    name: "motivation",
    label: "Motivation",
    sublabel: "Detailed description of the reason why the proposal is necessary/relevant, i.e. what is the problem?",
    maxLength: 3500,
  },
  {
    name: "specification",
    label: "Specification",
    sublabel: "Detailed description of the proposed policy",
    maxLength: 3500,
  },
  {
    name: "conclusion",
    label: "Conclusion",
    sublabel: "Closing statement encompassing the motivation or problem, proposed solution, and its intended impact/outcome.",
    maxLength: 3500,
  },
].map((f) => ({ type: "markdown", markdown: true, minLength: 20, placeholder: "Start typing...", ...f }));

const draft = {
  showBack: true,
  title: "Draft proposal",
  description: [
    "The purpose of the Draft Proposal is to present a potential policy to the community in a structured format and to formalize discussion about a proposal\u{2019}s potential impacts and implementation pathways. Draft Proposals must be structured in a particular form (see Annex 1), and can only pass to the binding Governance Proposal stage with a simple majority (51%) of participating voting power with a threshold of at least 1M VP. A Draft Proposal that fails or does not reach this threshold can be amended and resubmitted one time.",
    vpGate(1000).intro,
  ],
  sections: [
    {
      type: "select",
      name: "linked_proposal_id",
      label: "Linked Poll",
      readOnly: true,
      value: "poll-1",
      options: [
        { value: "poll-1", label: "[DAO:Poll] Should the DAO fund a community-run events calendar?" },
      ],
    },
    {
      type: "text",
      name: "title",
      label: "Title",
      placeholder: "Enter a descriptive title for your proposal here",
      maxLength: 80,
    },
    ...DRAFT_BODY,
    coAuthorsField(),
  ],
  vpNotice: vpGate(1000).vpNotice,
};

const GOVERNANCE_BODY = [
  { name: "summary", label: "Summary", sublabel: "One sentence summarizing the proposal.", maxLength: 250 },
  {
    name: "abstract",
    label: "Abstract",
    sublabel: "Two to three sentence overview of the proposal, specifying its motivation and outcomes.",
    maxLength: 3500,
  },
  {
    name: "motivation",
    label: "Motivation",
    sublabel: "Detailed description of the reason why the proposal is necessary/relevant, i.e. what is the problem?",
    maxLength: 3500,
  },
  {
    name: "specification",
    label: "Specification",
    sublabel: "Detailed description of the proposed policy",
    maxLength: 3500,
  },
  {
    name: "impacts",
    label: "Impacts",
    sublabel:
      "Detailed assessment of potential impacts, citing your methods, data sources (if relevant), or line of reasoning used in your assessment. This could include, for example, a scenarios assessment outlining preferred (e.g. best case scenario), possible, and undesirable (what could go wrong with the policy, e.g. worst-case scenario) outcomes.",
    maxLength: 3500,
  },
  {
    name: "implementation_pathways",
    label: "Implementation Pathways",
    sublabel:
      "Detailed description of concrete steps that can be taken to implement the proposal. This section should demonstrate consultation and communication with community members, and take into consideration DAO Committee feedback on overall technical feasibility and/or constraints.",
    maxLength: 3500,
  },
  {
    name: "conclusion",
    label: "Conclusion",
    sublabel: "Closing statement encompassing the motivation or problem, proposed solution, and its intended impact/outcome.",
    maxLength: 3500,
  },
].map((f) => ({ type: "markdown", markdown: true, minLength: 20, counterInBar: false, ...f }));

const governance = {
  showBack: true,
  title: "Governance proposal",
  description: [
    "The purpose of the Governance Proposal is to formalize the passed version of a Draft into a binding governance outcome. Only established or recognized community members can submit Governance Proposals, which are only passed if they reach the needed acceptance criteria for their category. In the interim period before new voting categories have been established (and for proposals that do not have a pre-set category) a Governance Proposal must receive a simple majority (51%) of participating voting power and at least 6M VP to pass as a binding decision.",
    "Processes and thresholds for established categories will not be changed as part of this proposal. Meaning, the process for grants, POIs, etc\u{2026} will remain unchanged. Additional categories for specific types of issues, e,g, \u{201C}fee structures,\u{201D} will be proposed, and relevant processes and thresholds developed.",
    vpGate(2500).intro,
  ],
  sections: [
    {
      type: "select",
      name: "linked_proposal_id",
      label: "Linked Draft",
      readOnly: true,
      value: "draft-1",
      options: [
        { value: "draft-1", label: "Draft #b6b1c0 \u{2014} Establish a standing Community Grants review committee" },
      ],
    },
    {
      type: "text",
      name: "title",
      label: "Title",
      placeholder: "Enter a descriptive title for your proposal here",
      maxLength: 80,
    },
    ...GOVERNANCE_BODY,
    coAuthorsField({ label: "Add co-author", sublabel: "Co-authors must accept the request to be displayed as such on the proposal." }),
  ],
  vpNotice: vpGate(2500).vpNotice,
  errorLabel: "There was an error while trying to create the proposal, please try again later.",
  errorCollapsible: true,
};

const pitch = {
  title: "Pitch proposal",
  description: [
    "Pitch proposals are the first step towards validating whether an idea is worth pursuing as a Project Tender and securing funds and bids from external teams to execute it.",
    vpGate(100).intro,
  ],
  sections: [
    {
      type: "text",
      name: "initiative_name",
      label: "Initiative name",
      maxLength: 80,
      postlabel: "This is what will be displayed on the Proposals Feed",
    },
    {
      type: "markdown",
      name: "problem_statement",
      label: "Problem statement",
      markdown: true,
      sublabel: "Please explain the problem this initiative would solve in using simple words",
      placeholder: "Describe the problem this initiative would solve\u{2026}",
      maxLength: 3500,
      minLength: 20,
      counterInBar: true,
    },
    {
      type: "markdown",
      name: "proposed_solution",
      label: "Proposed solution",
      markdown: true,
      sublabel: "Please explain how you would think this problem should be approached using simple words",
      placeholder: "Describe how this problem should be approached\u{2026}",
      maxLength: 3500,
      minLength: 20,
      counterInBar: true,
    },
    {
      type: "markdown",
      name: "target_audience",
      label: "Target audience",
      markdown: true,
      sublabel: "Please describe the intended user of the proposed solution: their needs, concerns, motivations, etc.",
      placeholder: "Describe the intended user of the proposed solution\u{2026}",
      maxLength: 3500,
      minLength: 20,
      counterInBar: true,
    },
    {
      type: "markdown",
      name: "relevance",
      label: "Why is this relevant now?",
      markdown: true,
      sublabel:
        "Please explain why the Decentraland DAO should be spending money on this project. Specify market conditions, competitors, new technical developments, and any other relevant factors you consider",
      placeholder: "Explain why the DAO should spend money on this now\u{2026}",
      maxLength: 3500,
      minLength: 20,
      counterInBar: true,
    },
    coAuthorsField({
      sublabel: "Add the addresses of any co-authors who contributed to this proposal.",
      placeholder: "Add a co-author address",
    }),
  ],
};

const tender = {
  showBack: true,
  title: "Tender proposal",
  description: [
    "Tender proposals are the second step in the Bidding & Tendering process. The aim of this stage is to refine the problems outlined in the Pitch proposal, providing a clearer vision of what the execution teams should propose in their Bid proposals.",
    vpGate(1000).intro,
  ],
  sections: [
    {
      type: "select",
      name: "linked_proposal_id",
      label: "Linked Pitch Proposal",
      readOnly: true,
      value: "pitch-1",
      options: [{ value: "pitch-1", label: "Pitch: A unified content-moderation pipeline for Worlds" }],
    },
    { type: "text", name: "project_name", label: "Project name", maxLength: 80 },
    {
      type: "markdown",
      name: "summary",
      label: "Summary",
      markdown: true,
      sublabel: "Provide a high-level overview of the idea behind this tender and its connection to the original Pitch proposal.",
      maxLength: 3500,
    },
    {
      type: "markdown",
      name: "problem_statement",
      label: "Problem statement",
      markdown: true,
      sublabel: "Describe the specific problem that this solution aims to solve for the Decentraland community.",
      maxLength: 3500,
    },
    {
      type: "markdown",
      name: "technical_specification",
      label: "Technical specification",
      markdown: true,
      sublabel:
        "Outline how the execution team could implement this solution. Please note that this is not binding, and execution teams may propose a new technical pathway in their bid proposals.",
      maxLength: 3500,
    },
    {
      type: "markdown",
      name: "use_cases",
      label: "Use cases",
      markdown: true,
      tall: true,
      sublabel: "Explain how the target audience will benefit from this project, how they will use it, and the value they will derive from it.",
      maxLength: 3500,
    },
    {
      type: "markdown",
      name: "deliverables",
      label: "Deliverables",
      markdown: true,
      tall: true,
      sublabel:
        "Detail all the artifacts that the execution teams should deliver as part of this Tender. For example, working software, documentation, marketing plans, social media strategies, etc.",
      maxLength: 3500,
    },
    {
      type: "date",
      name: "target_release_quarter",
      label: "Target release quarter",
      placeholder: "Select a target release quarter",
      sublabel:
        "State the anticipated release date for the community. Please note that this is not binding, and execution teams may propose a new date in their bid proposals.",
      options: ["2026 Q3", "2026 Q4", "2027 Q1", "2027 Q2", "2027 Q3"],
    },
    coAuthorsField(),
  ],
  vpNotice: vpGate(1000).vpNotice,
};

const councilDecisionVeto = {
  showBack: true,
  title: "Council Decision Veto",
  description:
    "Allows the community to challenge and veto recent decisions made by the DAO Council through a governance vote.",
  sections: [
    {
      type: "text",
      name: "decision_url",
      label: "DAO Council Decision (URL)",
      sublabel:
        "This field requires the URL of the DAO Council decision from the Council Snapshot Space. It validates two conditions: 1. The URL must belong to the Council Snapshot Space. 2. The proposal must have been closed no more than 14 days before this veto proposal is created.",
      placeholder:
        "URL of the Council decision you want to veto (must be from the Council Snapshot Space).",
    },
    {
      type: "markdown",
      name: "reasons",
      label: "Reasons to Veto",
      markdown: true,
      required: true,
      sublabel:
        "Explain why you believe this decision should be vetoed. Consider including potential issues, inconsistencies, or negative impacts.",
      maxLength: 3500,
      minLength: 20,
      counterInBar: true,
    },
    {
      type: "markdown",
      name: "suggestions",
      label: "Suggestions to the Council",
      markdown: true,
      optional: true,
      sublabel:
        "Share your suggestions or alternative recommendations for the Council regarding this decision.",
      maxLength: 3500,
      minLength: 20,
      counterInBar: true,
    },
    coAuthorsField(),
  ],
  vpNotice: vpGate(2500).vpNotice,
  errorLabel: "There was an error while trying to create the proposal, please try again later.",
  errorCollapsible: true,
};

const catalystForm = (variant: "add" | "remove") => {
  const detail =
    variant === "remove"
      ? "Explain why this node should be removed from the Catalyst network. Please, be as descriptive and objective as possible."
      : "Explain why this node should be added to the Catalyst network. Why would this addition be beneficial to the network? For example, adding nodes in new geographic areas helps to provide a better experience to more users.";
  const intro2 =
    variant === "remove"
      ? "To propose the removal of a node, please provide the following details."
      : "To propose the addition of a new node, please provide the following details.";
  return {
    showBack: true,
    title: variant === "remove" ? "Remove a catalyst node" : "Add a catalyst node",
    description: [
      "Instead of using central servers, Decentraland is run on a network of community-operated nodes. These nodes store copies of all scenes deployed to Decentraland, and they handle the messaging and interactions by establishing peer-to-peer connections between users.",
      intro2,
    ],
    sections: [
      {
        type: "text",
        name: "owner",
        label: "Ethereum address of the owner of the Catalyst Node",
        placeholder: "Example: 0x06012c8cf97bead5deae237070f9587f8e7a266d",
      },
      {
        type: "text",
        name: "domain",
        label: "Domain for the Catalyst Node",
        placeholder: "Example: catalyst.yourdomainname.com",
      },
      {
        type: "status",
        name: "domain_status",
        lines: [
          { text: "\u{2714} Content server is ready." },
          { text: "\u{2714} Lambda server is ready." },
        ],
      },
      {
        type: "markdown",
        name: "description",
        label: "Description",
        markdown: true,
        sublabel: detail,
        placeholder: "Write your description using markdown\u{2026}",
        maxLength: 7000,
      },
      coAuthorsField(),
    ],
    errorLabel: "There was an error while trying to create the proposal, please try again later.",
    errorCollapsible: true,
  };
};

const POI_DESCRIPTION =
  "Points of interest (POIs) are scenes that have been highlighted on Decentraland\u{2019}s map in a \u{2018}pin drop\u{2019} form, helping other users to quickly find and access especially interesting areas.";
const poiForm = (variant: "add" | "remove") => {
  const isRemove = variant === "remove";
  return {
    showBack: true,
    title: isRemove ? "Remove a Point of Interest" : "Add a Point of Interest",
    description: POI_DESCRIPTION,
    sections: [
      {
        type: "coords",
        name: "coordinates",
        label: isRemove
          ? "What location do you want to remove as a POI?"
          : "What location do you want to add as a POI?",
        sublabel: "Enter the X and Y coordinates of the location.",
        fields: [
          { name: "x", placeholder: "-150 through 150", min: -150, max: 163, label: "X coordinate" },
          { name: "y", placeholder: "-150 through 150", min: -150, max: 159, label: "Y coordinate" },
        ],
      },
      {
        type: "markdown",
        name: "description",
        label: "Description",
        markdown: true,
        sublabel: isRemove
          ? "Why do you think that this location should be removed as a featured point of interest within Decentraland? Please describe the scene located at these coordinates, explaining why it is no longer an interesting or helpful place for other Decentraland users."
          : "Why do you think that this location should be a featured point of interest within Decentraland? Please describe the scene located at these coordinates, explaining how it would be interesting or helpful for other Decentraland users.",
        placeholder: isRemove
          ? "The scene located at 0,0 is no longer visually stunning for users. The location has been abandoned for a long time."
          : "The scene located at 0,0 is both visually stunning and informative for new users. The interactive tutorial hosted here would provide people with a fun and enjoyable way to learn more about Decentraland.",
        maxLength: 250,
        minLength: 20,
        shortError: "This description is too short.",
      },
      coAuthorsField(),
    ],
  };
};

const ADD_COMMITTEES = ["DAO Council", "Wearable Curation Committee"];
const ALL_COMMITTEES = ["Security Advisory Board", "DAO Council", "Wearable Curation Committee"];
const COMMITTEE_MEMBERS = [
  { value: "0xab33\u{2026}77d3", label: "governance.dcl", hue: 130 },
  { value: "0x55f1\u{2026}1b2a", label: "0x55f1\u{2026}1b2a", hue: 0 },
  { value: "0x3e90\u{2026}c901", label: "elena.dcl", hue: 305 },
];
const hiringForm = (variant: "add" | "remove") => {
  const isRemove = variant === "remove";
  return {
    showBack: true,
    title: isRemove ? "Remove Committee Member" : "Add Committee Member",
    description: isRemove
      ? "Use this type of Proposal wisely. Before going through this way, talk with the person first, present your case at the Town Halls, and use public communication channels to discuss it. Only after then, publish this proposal."
      : "Being part of a Committee is great responsibility inside the DAO. Check with whom you are proposing that they agree to be postulated before creating this proposal.",
    sections: [
      {
        type: "dropdown",
        name: "committee",
        label: "Target Committee",
        placeholder: "Select a committee",
        options: isRemove ? ALL_COMMITTEES : ADD_COMMITTEES,
        help: isRemove ? undefined : "Only those with available positions are listed",
      },
      isRemove
        ? {
            type: "dropdown",
            name: "member",
            label: "Committee member",
            placeholder: "Select a member",
            emptyText: "Select a committee first",
            options: COMMITTEE_MEMBERS,
          }
        : {
            type: "text",
            name: "address",
            label: "Wallet address",
            sublabel: "Please copy the address of the proposed member. Check with them which address should you provide",
            placeholder: "Enter their address",
          },
      {
        type: "markdown",
        name: "reasons",
        label: isRemove ? "Reasons for removing" : "Reasons for adding",
        markdown: true,
        sublabel: isRemove
          ? "Explain why this person should be removed from the Committee. Be kind, descriptive, and objective as possible."
          : "Explain why you think this person should be added to the Committee. Have in mind the goal of the Committee and how this person aligns with it. Be as descriptive and objective as possible",
        maxLength: 3000,
        minLength: 20,
        counterInBar: true,
      },
      {
        type: "markdown",
        name: "evidence",
        label: "Evidence",
        markdown: true,
        sublabel: isRemove
          ? "Be as objective and detailed as possible. Provide only publicly available information."
          : "Be as objective and detailed as possible. List their qualifications and achievements. Provide only publicly available information",
        maxLength: 3000,
        minLength: 20,
        counterInBar: true,
      },
      coAuthorsField(),
    ],
    vpNotice: vpGate(2500).vpNotice,
  };
};

const MAX_IMAGES = 10;
const linkedWearables = {
  showBack: true,
  title: "Linked Wearables Registry",
  description: [
    "Linked Wearables are a way to represent NFTs as Wearables in Decentraland. Third parties need to submit a proposal to be approved by the DAO in order to access the tool in the Builder and get slots to submit the 3D models. By using this tool, you will be able to submit NFTs as Wearables to be curated and made available for your NFT holders inside Decentraland.",
    "Note that after being approved, you will need to create an API with the endpoints described in this Document.",
  ],
  sections: [
    {
      type: "text",
      name: "name",
      label: "Name",
      sublabel: "Please enter the name that represents your Project, Company, or Community as a whole",
      placeholder: "Enter the name here",
      maxLength: 80,
    },
    {
      type: "text",
      name: "marketplace_link",
      label: "NFT Marketplace Listing",
      sublabel:
        "Provide an URL where users can see your NFT collection listed in an NFT marketplace like OpenSea, Rarible, or any other",
      placeholder: "Add a link here",
    },
    {
      type: "list",
      name: "links",
      label: "Links",
      sublabel: "Links for your project website, Discord server, social media, or any other relevant space for your Project",
      placeholder: "Add a link here",
      addLabel: "Add another link",
    },
    {
      type: "list",
      name: "images",
      label: "Collection Images",
      sublabel: `Provide up to ${MAX_IMAGES} images to show the community what your collection and wearables looks like. JPG, PNG, BMP & WEBP formats supported.`,
      placeholder: "Insert image URL",
      addLabel: "Add another image",
      max: MAX_IMAGES,
    },
    {
      type: "markdown",
      name: "nft_collections",
      label: "NFT Collections",
      markdown: true,
      sublabel: "Describe the NFT Collections you created. If it\u{2019}s only one, just describe that one",
      placeholder: "Describe the NFT Collections you created here",
      maxLength: 3500,
    },
    {
      type: "markdown",
      name: "motivation",
      label: "Motivation",
      markdown: true,
      sublabel: "Why do you want to have your NFTs represented in Decentraland?",
      placeholder: "A brief motivation",
      maxLength: 3500,
    },
    {
      type: "number",
      name: "items",
      label: "Items in Linked Wearables Collection",
      sublabel:
        "How many 3D models (Linked Wearables) will be uploaded. Note: This is not the number of NFTs in the original collection.",
      placeholder: "Add the number of items you will upload",
      min: 1,
      value: "1",
    },
    {
      type: "markdown",
      name: "intellectual_property",
      label: "Intellectual Property",
      markdown: true,
      sublabel:
        "Provide proof that you are the rightful owner or representative of the Project, Company, or Community. Please share any links with relevant information.",
      placeholder: "Provide proof here",
      maxLength: 3500,
    },
    {
      type: "list",
      name: "contracts",
      label: "Smart Contracts",
      sublabel: "Share the Addresses of the smart contracts of your NFT collections",
      placeholder: "Add Ethereum address",
      addLabel: "Add another address",
    },
    {
      type: "list",
      name: "managers",
      label: "Managers",
      sublabel:
        "Addresses of the representatives that will Manage the tool. Note: Managers are the only ones allowed to add item representations and manage the tool",
      placeholder: "Add Ethereum address",
      addLabel: "Add another address",
    },
    {
      type: "radio",
      name: "programmatic",
      label: "Is this collection programmatically generated?",
      inline: true,
      value: "no",
      sublabel:
        "The collection you will upload to Decentraland as Linked Wearables is programmatically generated. This means the 3D models you will submit to Decentraland were made this way.",
      options: [
        { value: "yes", label: "Yes" },
        { value: "no", label: "No" },
      ],
      postlabel:
        "In general, large collections of more than 5k NFTs are not created manually, they are systematically generated from individual traits that were designed individually. If your Linked Wearables Collection is made like this, share the details below.",
    },
    {
      type: "markdown",
      name: "method",
      label: "Method",
      markdown: true,
      when: (v: Record<string, unknown>) => v.programmatic === "yes",
      sublabel:
        "Describe the method used to create the programmatic collection. If possible, share proof and links to the repository.",
      placeholder: "Describe the method",
      maxLength: 3500,
    },
    {
      type: "address",
      name: "co_author",
      label: "Co-Authors",
      sublabel: "Add the Decentraland addresses of any co-authors of this proposal",
      placeholder: "Add Ethereum address",
    },
  ],
  submitLabel: "Submit",
};

const grant = {
  title: "Request a Grant",
  description: [
    "The Decentraland Grants program allocates MANA owned by the DAO to fund the creation of features or content beneficial to the Decentraland platform and its growth. Either individuals or teams may request grant funding through the DAO.",
    vpGate(2000000).intro,
  ],
  numbered: true,
  sections: [
    {
      section: "Funding",
      validated: false,
      fields: [
        {
          type: "number",
          name: "funding",
          label: "Desired Funding",
          unit: "USD",
          placeholder: "100-240000",
          min: 100,
          max: 240000,
          help: "More about our Funding Tiers",
        },
        {
          type: "number",
          name: "duration",
          label: "Estimated Project Duration",
          stepper: true,
          unitLabel: "months",
          value: 3,
          min: 1,
          max: 12,
        },
        {
          type: "radio",
          name: "vesting",
          label: "What time of the month would you like for the funding to happen?",
          options: [
            { value: "first", label: "1st day of month" },
            { value: "fifteenth", label: "15th day of month" },
          ],
        },
        {
          type: "token",
          name: "token",
          label: "What is your preferred payment token?",
          inline: true,
          options: ["MANA", "DAI"],
        },
      ],
    },
    { section: "General Information", fields: [] },
    { section: "Team", fields: [] },
    { section: "Due Diligence", fields: [] },
    { section: "Category-Specific Assessment", fields: [] },
    { section: "Final Consent", fields: [] },
    { section: "Co-authors", fields: [coAuthorsField()] },
  ],
  submitLabel: "Submit",
};

const bid = {
  title: "Submit Bid proposal",
  description:
    "Part of the Bidding & Tendering process, bid proposals are meant for professional teams to scope and propose a project out of their own understanding of one given issue or desire outlined by the Community based on the two preceding instances that would have passed. Only one Bid Proposal per a given Tender Proposal will be funded.",
  numbered: true,
  sections: [
    {
      section: "Funding",
      fields: [
        {
          type: "number",
          name: "funding",
          label: "Budget",
          unit: "USD",
          placeholder: "100-240000",
          min: 100,
          max: 240000,
        },
        {
          type: "number",
          name: "duration",
          label: "Project duration",
          stepper: true,
          unitLabel: "months",
          value: 1,
          min: 1,
          max: 12,
        },
        { type: "date", name: "delivery_date", label: "Delivery date", options: [] },
        {
          type: "text",
          name: "beneficiary",
          label: "Beneficiary address",
          sublabel:
            "The address that will receive the grant funds. This must be an Ethereum address! Entering a non-Ethereum address that cannot receive MANA may result in a permanent loss of funds.",
          placeholder: "0x\u{2026}",
        },
        {
          type: "email",
          name: "email",
          label: "Contact Email Address",
          sublabel:
            "This email address will be used by the Grant Support teams to contact you to check the progress of the grant, set up meetings, and maintain an open communication channel.",
          placeholder: "Enter your email address",
          postlabel:
            "Note: The address will be published in the proposal and publicly visible. If you want to keep your anonymity consider using an email address without personally identifiable information.",
        },
      ],
    },
    {
      section: "General information",
      fields: [
        { type: "text", name: "team_name", label: "Team Name", maxLength: 80 },
        {
          type: "textarea",
          name: "deliverables",
          label: "Deliverables",
          sublabel: "Be as specific as possible. Describe the entire scope of the project and the actual work you are going to deliver to the DAO.",
          maxLength: 1500,
          rows: 5,
        },
        {
          type: "textarea",
          name: "roadmap",
          label: "Roadmap",
          sublabel: "Describe the main phases or steps your project will follow to reach its goal.",
          placeholder:
            "Your estimated timeline and key milestones. Include your plan for reporting progress to the community.",
          maxLength: 1500,
          rows: 5,
        },
      ],
    },
    { section: "Team", fields: [] },
    { section: "Due-diligence", fields: [] },
    {
      section: "Final Consent",
      fields: [
        {
          type: "checkbox",
          name: "consent",
          label: "Review and check the following",
          options: [
            { value: "contentPolicy", label: "I\u{2019}ve read and understood Decentraland\u{2019}s Content Policy" },
            { value: "termsOfUse", label: "I\u{2019}ve read Decentraland\u{2019}s Terms of Use and agree with them" },
            { value: "codeOfEthics", label: "I\u{2019}ve read Decentraland\u{2019}s Code of Ethics and hereby commit to honoring it" },
          ],
        },
      ],
    },
  ],
};

const projectUpdate = {
  title: "Publish New Grant Update",
  description:
    "Share your grant updates with the Decentraland Community. Use this space to talk about the progress but also to raise issues or blockers you might have with your project. Feel free to add any relevant information or links to demo what you\u{2019}ve been up to.",
  numbered: true,
  sections: [
    {
      section: "General",
      fields: [
        {
          type: "pills",
          name: "health",
          label: "Project Health",
          value: "onTrack",
          options: [
            { value: "onTrack", label: "On Track" },
            { value: "atRisk", label: "At Risk" },
            { value: "offTrack", label: "Off Track" },
          ],
        },
        {
          type: "markdown",
          name: "introduction",
          label: "Introduction",
          placeholder: "Give a short introduction summarizing the period this update covers.",
          maxLength: 500,
        },
        {
          type: "markdown",
          name: "highlights",
          label: "Highlights",
          placeholder: "What have you shipped? Where have you made progress? What are you proud of?",
          maxLength: 3500,
        },
        {
          type: "markdown",
          name: "blockers",
          label: "Blockers",
          placeholder:
            "What is being difficult? Why is the project delayed? When sharing a blocker, share the mitigation strategy you\u{2019}re planning to remove that blocker.",
          maxLength: 3500,
        },
        {
          type: "markdown",
          name: "next_steps",
          label: "Next Steps",
          placeholder: "What are your upcoming tasks? Where is your focus going to be next?",
          maxLength: 3500,
        },
        {
          type: "markdown",
          name: "additional_notes",
          label: "Additional notes and links",
          placeholder: "Feel free to share any additional information or resources to showcase your project.",
          maxLength: 3500,
        },
      ],
    },
    {
      section: "Financials",
      isNew: true,
      fields: [
        {
          type: "textarea",
          name: "reporting",
          label: "Reporting",
          sublabel: "Bring some light onto how this project is utilizing funds, CSV syntax.",
          value: "category,description,token,amount,receiver,link",
          rows: 4,
        },
      ],
    },
  ],
  submitLabel: "Publish update",
  secondaryLabel: "Preview update",
};

export const GOVERNANCE_FORMS = {
  poll,
  banName,
  draft,
  governance,
  pitch,
  tender,
  councilDecisionVeto,
  catalystAdd: catalystForm("add"),
  catalystRemove: catalystForm("remove"),
  poiAdd: poiForm("add"),
  poiRemove: poiForm("remove"),
  hiringAdd: hiringForm("add"),
  hiringRemove: hiringForm("remove"),
  linkedWearables,
  grant,
  bid,
  projectUpdate,
} satisfies Record<string, FormDescriptor>;

export default GOVERNANCE_FORMS;
