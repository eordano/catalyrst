import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import LegalDocPageLayout from "./LegalDocPageLayout";
import type { LegalDoc, TocItem } from "./LegalDocPageLayout";
import { LEGAL_DOCS } from "../../data/legalPageConfig";

const termsDemo: LegalDoc = {
  title: "Terms of Use",
  activeSlug: "/terms",
  tableOfContents: [
    { id: "acceptance-of-terms", label: "1. Acceptance of Terms" },
    { id: "section-1-1", label: "1.1 Introduction", depth: 1 },
    { id: "section-1-2", label: "1.2 Services", depth: 1 },
    { id: "eligibility", label: "2. Eligibility" },
    { id: "representations-and-risks", label: "3. Representations and Risks" },
  ],
  sections: [
    {
      id: "acceptance-of-terms",
      heading: "1. Acceptance of Terms",
      body: [
        { type: "h3", id: "section-1-1", content: "1.1 Introduction" },
        "The Decentraland Platform is a community-driven virtual space supported by the Decentraland Foundation and guided by its users through transparent governance.",
        { type: "h3", id: "section-1-2", content: "1.2 Services" },
        "The Foundation makes the following available for the benefit of the Decentraland community:",
        {
          type: "ul",
          items: [
            "the Clients \u{2014} the applications through which users access Decentraland;",
            "the Tools \u{2014} additional features such as the Marketplace, Builder and DAO interface;",
            "the Site \u{2014} the website located at decentraland.org.",
          ],
        },
      ],
    },
    {
      id: "eligibility",
      heading: "2. Eligibility",
      body: [
        "You represent and warrant that you are of the legal age of majority in your jurisdiction and are otherwise legally permitted to use the Services where you live.",
      ],
    },
    {
      id: "representations-and-risks",
      heading: "3. Representations and Risks",
      body: [
        'Your use of the Services is at your sole risk. The Services are provided on an "AS IS" and "AS AVAILABLE" basis, without warranties of any kind, either express or implied.',
      ],
    },
  ],
};

const DOCS = { termsDemo, ...LEGAL_DOCS };
type DocKey = keyof typeof DOCS;
const DOC_KEYS = Object.keys(DOCS) as DocKey[];

/** `undefined` falls through to the doc's own table of contents; `empty` hides it. */
const TOC: Record<string, TocItem[] | undefined> = { fromDoc: undefined, empty: [] };

/** The story args: the doc and the TOC override are picked by name, the rest are real props. */
type LegalDocStoryArgs = Omit<
  ComponentProps<typeof LegalDocPageLayout>,
  "doc" | "tableOfContents"
> & { docName: DocKey; tocOverride: keyof typeof TOC };

const meta = {
  title: "Web/Frames/LegalDocPageLayout",
  component: LegalDocPageLayout,
  parameters: { layout: "fullscreen" },
  argTypes: {
    docName: {
      control: "select",
      options: DOC_KEYS,
      description: "Which descriptor from `LEGAL_DOCS` (plus a trimmed terms demo) is rendered.",
    },
    tocOverride: {
      control: "inline-radio",
      options: Object.keys(TOC),
      description: "`fromDoc` uses the doc's own TOC; `empty` passes `[]` to override it away.",
    },
    title: { control: "text" },
    activeSlug: { control: "text" },
  },
  args: { docName: "termsDemo", tocOverride: "fromDoc" },
  render: ({ docName, tocOverride, ...rest }) => (
    <LegalDocPageLayout
      doc={DOCS[docName]}
      tableOfContents={TOC[tocOverride]}
      onNavClick={(e) => e.preventDefault()}
      {...rest}
    />
  ),
} satisfies Meta<LegalDocStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/*
 * These three are reachable from the `docName` control like every other doc, but their story
 * ids are hardcoded external consumers: tools/screen-tour/add-story-links.mts keeps
 * `web-frames-legaldocpagelayout--{terms,privacy,ethics}` in its MAPS and only console.warns
 * when an id stops resolving, so dropping the exports would silently break those deep links.
 */
export const Terms: Story = { args: { docName: "termsDemo" } };

export const Privacy: Story = { args: { docName: "privacy" } };

export const Ethics: Story = { args: { docName: "ethics" } };

export const Content: Story = { args: { docName: "content" } };

export const Rewards: Story = { args: { docName: "rewards" } };

export const Referral: Story = { args: { docName: "referral" } };

export const Security: Story = { args: { docName: "security" } };

export const Brand: Story = { args: { docName: "brand" } };

/** `tableOfContents={[]}` overrides the doc's own table of contents away. */
export const PropOverride: Story = { args: { docName: "privacy", tocOverride: "empty" } };
