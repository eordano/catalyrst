import type { Meta, StoryObj } from "@storybook/react-vite";
import GvTransparency, {
  type GvBalance,
  type GvCommittee,
  type GvMonthlyData,
} from "./GvTransparency";

const BALANCES: GvBalance[] = [
  { symbol: "MANA", amount: 18432905 },
  { symbol: "USDC", amount: 1240500 },
  { symbol: "USDT", amount: 642118 },
  { symbol: "DAI", amount: 305744 },
  { symbol: "ETH", amount: 412 },
  { symbol: "MATIC", amount: 89320 },
  { symbol: "WETH", amount: 58 },
];

const INCOME: GvMonthlyData = {
  total: 1284530,
  previous: 12.4,
  details: [
    { name: "Vesting Contract", value: 962400, description: "MANA released from the 10-year DAO vesting contract." },
    { name: "Marketplace Fees", value: 214860, description: "2.5% secondary-market fee accrued to the DAO." },
    { name: "Names & LAND Auctions", value: 78420, description: "Primary sales of DCL names and LAND parcels." },
    { name: "Wearable Curation Fees", value: 22150, description: "Submission fees from collection curation." },
    { name: "Returned Grants", value: 6700, description: "Unspent grant funds returned to the treasury." },
    { name: "Misc Donations", value: 0.62, description: "Rounding / dust returns." },
  ],
};
const EXPENSES: GvMonthlyData = {
  total: 1098240,
  previous: -8.1,
  details: [
    { name: "Community Grants", value: 742300, description: "Disbursements to active grant projects." },
    { name: "Core Unit Operations", value: 196500, description: "Facilitation, SAB and DAO Committee operations." },
    { name: "Infrastructure & Catalysts", value: 88420, description: "Catalyst node operators and hosting." },
    { name: "Events & Marketing", value: 54120, description: "Community events and growth initiatives." },
    { name: "Audits & Legal", value: 16500, description: "Smart-contract audits and legal counsel." },
    { name: "Gas & Transaction Fees", value: 0.4, description: "On-chain settlement dust." },
  ],
};

const COMMITTEES: GvCommittee[] = [
  {
    name: "Security Advisory Board",
    description:
      "The Security Advisory Board (SAB) holds the keys that can pause or revoke malicious smart contracts in an emergency.",
    members: [
      { name: "Yemel Jardi", address: "0x71c…a4e1", hue: 268 },
      { name: "Esteban Ordano", address: "0x12c…9f0c", hue: 210 },
      { name: "Agustin Mendez", address: "0xab1…77d3", hue: 130 },
      { name: "Nicolas Santangelo", address: "0x55a…1b2a", hue: 30 },
      { name: "Pablo De Haro", address: "0x3e7…c901", hue: 305 },
    ],
  },
  {
    name: "DAO Council",
    description:
      "The DAO Council reviews grant requests, enacts passed proposals, and represents the community in operational matters.",
    members: [
      { name: "Marcos Nieto", address: "0x9f3…7a21", hue: 190 },
      { name: "Tobias Bordenave", address: "0x44e…0b58", hue: 48 },
      { name: "Lautaro Petaccio", address: "0x7d2…e3a9", hue: 340 },
    ],
  },
  {
    name: "Wearable Curation Committee",
    description:
      "The Wearable Curation Committee reviews and approves wearables and emotes submitted to the Decentraland marketplace.",
    members: [
      { name: "Hiro Lambert", address: "0xc0f…12ab", hue: 96 },
      { name: "Sara Khoury", address: "0x6b8…44cd", hue: 12 },
      { name: "Dévora Lin", address: "0x2a1…99ef", hue: 280 },
      { name: "Karl Renz", address: "0x8de…5577", hue: 160 },
    ],
  },
];

const meta = {
  title: "Governance/Pages/Transparency",
  component: GvTransparency,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof GvTransparency>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <GvTransparency
      balances={BALANCES}
      income={INCOME}
      expenses={EXPENSES}
      committees={COMMITTEES}
    />
  ),
};

export const Loading: Story = {
  render: () => <GvTransparency loading />,
};

export const Empty: Story = {
  render: () => (
    <GvTransparency
      balances={[]}
      income={{ total: 0, previous: 0, details: [] }}
      expenses={{ total: 0, previous: 0, details: [] }}
      committees={[]}
    />
  ),
};
