import type { ReactNode } from "react";
import PlaceCard from "./PlaceCard";
import "../explorer/pages/places.css";

export default {
  tags: ["autodocs"],
  title: "Components/PlaceCard",
  component: PlaceCard,
  parameters: { layout: "centered" },
};

const Grid = ({ children }: { children: ReactNode }) => (
  <div className="pl" style={{ background: "#1c0a2e", padding: 24 }}>
    <div className="pl__grid" style={{ width: 280, gridTemplateColumns: "1fr" }}>{children}</div>
  </div>
);

export const Featured = {
  render: () => (
    <Grid>
      <PlaceCard
        title="Genesis Plaza"
        creator="Decentraland Foundation"
        live={14}
        players={142}
        rating={100}
        coords="-3,-2"
        featured
        hue={0}
      />
    </Grid>
  ),
};

export const Standard = {
  render: () => (
    <Grid>
      <PlaceCard
        title="Bloom Garden"
        creator="limmagarden.dcl.eth"
        players={0}
        rating={100}
        coords="limmagarden.dcl.eth"
        hue={141}
      />
    </Grid>
  ),
};

export const Skeleton = {
  render: () => (
    <Grid>
      <PlaceCard skeleton />
    </Grid>
  ),
};
