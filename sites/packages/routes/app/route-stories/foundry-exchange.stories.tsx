import FoundryExchange from "../routes/foundry.exchange";
import foundry from "@data/fixtures/foundry.json";
import { routeStory } from "./lib";

// The board holds nothing but visitor writes, so the fixture holds nothing: the
// empty state IS the shipped state until somebody asks for something. The row
// below is a storybook stand-in, labeled as one — there is no real request to
// import.
const base = foundry.exchange;
const standIn = {
  id: "storybook-request",
  title: "Storybook stand-in — a visitor request lands here",
  body: "Requests are written by visitors from this page. This row exists so the card, the pledge control and the count have something to render in storybook.",
  source: "storybook",
  status: "open",
  pledges: 1,
  pledgedByMe: false,
  createdAt: "2026-08-15T00:00:00.000Z",
};

export default {
  title: "Routes/FoundryExchange",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const EmptyBoard = {
  render: routeStory({
    Component: FoundryExchange,
    path: "/foundry/exchange",
    loaderData: base,
  }),
};

export const WithARequest = {
  render: routeStory({
    Component: FoundryExchange,
    path: "/foundry/exchange",
    loaderData: {
      ...base,
      requests: [standIn],
      stats: { openRequests: 1, totalPledges: 1 },
    },
  }),
};

// Pledges are per-visitor shared state: the same board read by the session that
// placed the pledge.
export const PledgedByMe = {
  render: routeStory({
    Component: FoundryExchange,
    path: "/foundry/exchange",
    loaderData: {
      ...base,
      requests: [{ ...standIn, pledgedByMe: true }],
      stats: { openRequests: 1, totalPledges: 1 },
    },
  }),
};

// The board as the prioritized demand surface: one card read into a cell (the
// dated chip), one honestly not yet read. Both readings are storybook
// stand-ins shaped like the DB rows the loader serves.
export const ReadAndUnread = {
  render: routeStory({
    Component: FoundryExchange,
    path: "/foundry/exchange",
    loaderData: {
      ...base,
      requests: [
        {
          ...standIn,
          id: "storybook-request-read",
          title: "Storybook stand-in — an ask read into a cell",
          reading: { cell: "creator-led-social-competition", readAt: "2026-08-17" },
        },
        {
          ...standIn,
          id: "storybook-request-unread",
          title: "Storybook stand-in — an ask not yet read",
          reading: null,
        },
      ],
      stats: { openRequests: 2, totalPledges: 2 },
    },
  }),
};
