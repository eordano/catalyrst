import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { PageHeader, FilterButton, SectionCard, SurfaceCard } from "./Surface";
import CoverImage from "./CoverImage";
import FreshnessNotice from "./FreshnessNotice";
import ContentStatus from "./ContentStatus";
import EmptyState from "./EmptyState";
import Button from "../atoms/Button";
import { asset } from "../asset";

const meta = { title: "Shared/Product surfaces", parameters: { layout: "fullscreen" } } satisfies Meta;
export default meta;
type Story = StoryObj<typeof meta>;

function Catalog({ state = "ready" }: { state?: "ready" | "loading" | "empty" | "error" | "stale" }) {
  const [filter, setFilter] = useState("All");
  return <main className="ui-surface" style={{ padding: "var(--ui-page-pad)", minHeight: "100vh" }}>
    <PageHeader title="Explore together" description="Shared UI3 components for Explorer, Social, Shop and Creator Hub." actions={<Button>Open Decentraland</Button>} />
    <div role="group" aria-label="Categories" style={{ display: "flex", gap: 8, marginBottom: 24 }}>{["All", "Friends", "Communities", "Events"].map(label => <FilterButton key={label} selected={filter === label} onClick={() => setFilter(label)}>{label}</FilterButton>)}</div>
    <FreshnessNotice failed={state === "stale"} onRetry={() => {}} />
    {state === "loading" ? <ContentStatus pending message="Finding places&#x2026;" />
      : state === "error" ? <ContentStatus message="Couldn't load places." onRetry={() => {}} />
      : state === "empty" ? <EmptyState title="No places found" subtitle="Try another category or search." />
      : <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(min(280px, 100%), 1fr))", gap: 18 }}>
        <SurfaceCard><CoverImage src={asset("assets/scene-thumb.png")} alt="Decentraland scene" style={{ aspectRatio: "16 / 9" }} /><div style={{ padding: 18 }}><h2 style={{ fontSize: 17 }}>A place to meet</h2><p style={{ color: "var(--ui-muted)", marginTop: 8 }}>Images retain their space while loading.</p></div></SurfaceCard>
        <SectionCard title="Communities" titleId="surface-communities" action={<a href="https://dcl.social/">Discover</a>} footer={<a href="https://dcl.social/#/mine">Your communities</a>}><EmptyState title="Find your people" subtitle="Shared sections keep content, actions and states consistent." /></SectionCard>
      </div>}
  </main>;
}
export const Ready: Story = { render: () => <Catalog /> };
export const Loading: Story = { render: () => <Catalog state="loading" /> };
export const Empty: Story = { render: () => <Catalog state="empty" /> };
export const Error: Story = { render: () => <Catalog state="error" /> };
export const SavedResults: Story = { render: () => <Catalog state="stale" /> };
