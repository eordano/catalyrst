import { vi, test, expect, beforeEach } from "vitest";
import { render } from "@testing-library/react";
import { composeStories } from "@storybook/react";

const { sendBridge } = vi.hoisted(() => ({ sendBridge: vi.fn() }));
vi.mock("../../overlay/bridge", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../overlay/bridge")>()),
  sendBridge,
}));

import * as FriendsStories from "./Friends.interactions.stories";
const { AcceptRequest } = composeStories(FriendsStories);

beforeEach(() => sendBridge.mockClear());

test("Friends: accepting a request fires the signed upsert_friendship mutation", async () => {
  const { container } = render(<AcceptRequest />);
  await AcceptRequest.play?.({ canvasElement: container });

  expect(sendBridge).toHaveBeenCalledWith(
    "SignRequest",
    expect.objectContaining({
      kind: "upsert_friendship",
      action: "accept",
      address: "0xReq0000000000000000000000000000000000aa",
    }),
  );
});
