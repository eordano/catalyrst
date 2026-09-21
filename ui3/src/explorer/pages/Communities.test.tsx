import { expect, test } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import Communities, { relativeDate, type SocialDetail } from "./Communities";

const NOW = Date.parse("2026-09-21T12:00:00Z");
const club = { id: "club", name: "Music Club", description: "Live sets every Friday.", membersCount: 4, privacy: "public", role: "member" };
const author = "0x1111111111111111111111111111111111111111";
const detail: SocialDetail = {
  id: "club",
  community: club,
  members: [{ memberAddress: author, name: "Nyx", role: "owner" }],
  places: [{ id: "plaza", title: "Club Plaza", addedBy: author, addedAt: "2026-09-02T12:00:00Z" }],
  posts: [
    { id: "new", authorAddress: author, authorName: "Nyx", content: "Newest word", createdAt: "2026-09-18T17:40:00Z", likesCount: 0 },
    { id: "old", authorAddress: author, authorName: "Nyx", content: "First word", createdAt: "2024-06-01T10:00:00Z", likesCount: 0 },
  ],
};

test("dates older than a month read as months or years, newer ones as the date", () => {
  expect(relativeDate("2026-08-21T12:00:00Z", NOW)).toBe("1 month ago");
  expect(relativeDate("2026-04-11T20:05:00Z", NOW)).toBe("5 months ago");
  expect(relativeDate("2025-09-01T00:00:00Z", NOW)).toBe("1 year ago");
  expect(relativeDate("2024-06-01T10:00:00Z", NOW)).toBe("2 years ago");
  expect(relativeDate("2026-09-18T17:40:00Z", NOW)).not.toMatch(/ago/);
});

test("the rail offers Home, Explore communities, Friends and Nearby, and a community opens on its summary", async () => {
  const user = userEvent.setup();
  render(<Communities communities={[club]} detail={detail} now={NOW} />);
  const rail = within(screen.getByRole("navigation", { name: "Sections and communities" }));
  expect(rail.getByRole("button", { name: "Home" })).toHaveAttribute("aria-current", "page");
  for (const name of ["Explore communities", "Friends", "Nearby"]) expect(rail.getByRole("button", { name })).not.toHaveAttribute("aria-current");
  expect(rail.queryByRole("button", { name: /Events|Worlds/ })).toBeNull();

  await user.click(rail.getByRole("button", { name: "Explore communities" }));
  expect(screen.getByRole("heading", { name: "Your communities" })).toBeInTheDocument();

  await user.click(rail.getByRole("button", { name: "Music Club" }));
  expect(screen.getByRole("heading", { level: 1, name: "Music Club" })).toBeInTheDocument();
  expect(screen.getByText("Your role").nextElementSibling).toHaveTextContent("Member");
  const side = within(screen.getByRole("complementary", { name: "Music Club" }));
  expect(side.getAllByRole("button").map((button) => button.textContent?.replace(/\d+$/, ""))).toEqual(["Music Club", "General", "Announcements", "Hangouts", "Members"]);

  await user.click(side.getByRole("button", { name: "Hangouts" }));
  expect(screen.getByText("Club Plaza")).toBeInTheDocument();
  expect(screen.queryByRole("dialog", { name: /Hangouts/ })).toBeNull();
});

test("announcements read oldest first and carry the full date on hover", async () => {
  const user = userEvent.setup();
  render(<Communities communities={[club]} detail={detail} initialCommunityId="club" now={NOW} />);
  await user.click(screen.getByRole("button", { name: "Announcements" }));
  const log = within(screen.getByRole("log", { name: "Announcements" }));
  const posts = log.getAllByRole("article");
  expect(posts.map((post) => within(post).getByRole("paragraph").textContent)).toEqual(["First word", "Newest word"]);
  expect(within(posts[0]!).getByText("2 years ago")).toBeInTheDocument();
  expect(posts[0]).toHaveAttribute("title", expect.stringContaining("2024"));
  expect(screen.getByText("Only moderators can post here.")).toBeInTheDocument();
});
