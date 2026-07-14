import type { Meta, StoryObj } from "@storybook/react-vite";
import Places from "./Places";

const CARDS = [
  { title: "Genesis Plaza", creator: "Decentraland Foundation", live: 14, players: 142, rating: 100, coords: "-3,-2", featured: true, to: "Explorer/Pages/PlaceDetail" },
  { title: "Antrom RPG (Dice Masters)", creator: "Matt", players: 6, rating: 86, coords: "144,-7", featured: true, to: "Explorer/Pages/PlaceDetail" },
  { title: "Festive Trail 7 - RAGE Parkour", creator: "myKMFT & cartoonia", players: 0, rating: 100, coords: "-148,-142", featured: true, to: "Explorer/Pages/PlaceDetail" },
  { title: "Pudgy Penguins!", creator: "Coreook", players: 0, rating: 100, coords: "-89,-86", featured: true, to: "Explorer/Pages/PlaceDetail" },
  { title: "Flag Tag", creator: "to", players: 0, rating: 100, coords: "flagtag.dcl.eth", featured: true, to: "Explorer/Pages/PlaceDetail" },
  { title: "Shrunken Shenanigans", creator: "joybuzby.dcl.eth", players: 0, rating: 100, coords: "joybuzby.dcl.eth", to: "Explorer/Pages/PlaceDetail" },
  { title: "Bloom Garden", creator: "limmagarden.dcl.eth", players: 0, rating: 100, coords: "limmagarden.dcl.eth", to: "Explorer/Pages/PlaceDetail" },
  { title: "Slay The Steps!", creator: "WikiFnags + zhin", players: 0, rating: 100, coords: "-19,99", to: "Explorer/Pages/PlaceDetail" },
  { title: "Digital Fashion Week Boutique", creator: "Digital Fashion Week", players: 0, rating: 100, coords: "-2,150", to: "Explorer/Pages/PlaceDetail" },
];

const meta = {
  title: "Explorer/Pages/Places",
  component: Places,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Places>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <Places places={CARDS} />,
};
