import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { sceneRecipients } from "../../data/catalyst/sceneOwner";
import { catalystBase } from "../../data/catalyst/client";
import { detectWallets, selectWallet, type Eip1193Provider } from "../../data/auth/wallet";
import { FakeBridge, makeFriend, makeFriendRequest } from "../../test/fakeBridge";
import { encodeManaTransfer, PAYMENTS_CONFIG_PATH } from "./manaTip";
import { SceneFeedbackModal, SceneTipModal } from "./SceneOwnerActions";

const OWNER = "0x92de52247aeae00fcfb18072c8564f3549b64f9c";
const MANA = "0x0f5d2fb29fb7d3cfee444a200298f468908cc942";
const SIGNER = "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd";

type Json = { ok: boolean; status: number; statusText: string; json: () => Promise<unknown> };
const json = (body: unknown): Json => ({ ok: true, status: 200, statusText: "OK", json: async () => body });

let bridge: FakeBridge;

function mount(node: React.ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={qc}>{node}</QueryClientProvider>);
  return userEvent.setup();
}

beforeEach(() => {
  window.history.replaceState({}, "", "/");
  localStorage.removeItem("dcl.feature.2026-09-scene-feedback-tip");
  bridge = new FakeBridge();
  window.dclBridge = bridge;
  vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
    const url = String(input);
    if (url.endsWith(PAYMENTS_CONFIG_PATH)) return json({ chainId: 137, manaToken: MANA }) as unknown as Response;
    return json({}) as unknown as Response;
  });
});

afterEach(async () => {
  vi.restoreAllMocks();
  delete window.dclBridge;
  delete window.ethereum;
  selectWallet(null);
  await new Promise((r) => setTimeout(r, 0));
});

describe("feedback delivery", () => {
  test("shows profiles and sources for all four roles and sends only to the selected wallet", async () => {
    const tip = "0x1111111111111111111111111111111111111111";
    const author = "0x2222222222222222222222222222222222222222";
    const land = "0x3333333333333333333333333333333333333333";
    vi.mocked(fetch).mockImplementation(async input => {
      const address = String(input).split("/").at(-1);
      return json({ avatars: [{ userId: address, name: address === author ? "Scene Maker" : "Profile", avatar: { snapshots: { face256: "https://example.test/face.png" } } }] }) as unknown as Response;
    });
    const recipients = sceneRecipients({ entityId: "scene", title: "Plaza", tipAddress: tip, sceneAuthor: author, deployer: OWNER }, land);
    const user = mount(<SceneFeedbackModal recipients={recipients} sceneTitle="Plaza" coords="0,0" onClose={() => {}} />);
    act(() => {
      bridge.pushIdentity({ address: SIGNER, signerAddress: SIGNER });
      bridge.pushFriends({ friends: [] });
    });
    expect(await screen.findByText("Scene Maker")).toBeInTheDocument();
    expect(screen.getAllByRole("radio")).toHaveLength(4);
    expect(screen.queryByText("0x2222\u20262222")).toBeNull();
    expect(screen.getByRole("radio", { name: "Scene author" })).toBeChecked();
    for (const recipient of recipients) expect(screen.getByText(recipient.source)).toBeInTheDocument();
    expect(document.querySelector('img[src="https://example.test/face.png"]')).toBeNull();
    await user.click(screen.getByRole("radio", { name: "Land owner" }));
    await user.type(screen.getByRole("textbox", { name: "Feedback" }), "Hello land owner");
    await user.click(screen.getByRole("button", { name: "Send feedback" }));
    expect(bridge.expectSent("SignRequest")).toMatchObject({ address: land, message: "Feedback on Plaza (0,0): Hello land owner" });
  });

  test("hides unavailable roles and explains when no recipients exist", () => {
    mount(<SceneFeedbackModal recipients={sceneRecipients(null, null, true)} sceneTitle="World" coords="0,0" onClose={() => {}} />);
    expect(screen.queryAllByRole("radio")).toHaveLength(0);
    expect(screen.getByText("No recipients are available for this scene.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Send feedback" })).toBeDisabled();
  });

  test("to a friend it is a signed POST to the owner's messages endpoint with the scene header, confirmed by the signed-fetch result", async () => {
    const user = mount(<SceneFeedbackModal owner={OWNER} sceneTitle="CBD Plaza" coords="-143,102" onClose={() => {}} />);
    act(() => {
      bridge.pushIdentity({ address: SIGNER, signerAddress: SIGNER });
      bridge.pushFriends({ friends: [makeFriend({ address: OWNER })] });
    });
    expect(screen.queryByRole("note")).toBeNull();
    await user.type(screen.getByRole("textbox", { name: "Feedback" }), " the door is stuck ");
    await user.click(screen.getByRole("button", { name: "Send message" }));

    const sent = bridge.expectSent("SignedFetch");
    expect(sent.url).toBe(`${catalystBase()}/v1/friends/${OWNER}/messages`);
    expect(sent.method).toBe("POST");
    expect(JSON.parse(sent.body ?? "")).toEqual({ body: "Feedback on CBD Plaza (-143,102): the door is stuck" });
    expect(bridge.sentOf("SignRequest")).toEqual([]);

    act(() => {
      bridge.push({ kind: "signedFetchResult", id: sent.id, status: 200, body: "{}" });
    });
    expect(await screen.findByText(/Delivered\. Your note reached the recipient/)).toBeInTheDocument();
  });

  test("to a stranger it rides a friend request signed by the engine, and the sent list confirms it", async () => {
    const user = mount(<SceneFeedbackModal owner={OWNER} sceneTitle="" coords="0,0" onClose={() => {}} />);
    act(() => {
      bridge.pushIdentity({ address: SIGNER, signerAddress: SIGNER });
    });
    expect(screen.queryByRole("note")).toBeNull();
    act(() => {
      bridge.pushFriends({ friends: [] });
    });
    expect(screen.getByRole("note")).toHaveTextContent("your feedback will be sent as a friend request");
    await user.type(screen.getByRole("textbox", { name: "Feedback" }), "hi");
    await user.click(screen.getByRole("button", { name: "Send feedback" }));

    expect(bridge.expectSent("SignRequest")).toEqual({
      kind: "upsert_friendship",
      action: "request",
      address: OWNER,
      message: "Feedback on your scene (0,0): hi",
    });
    expect(bridge.sentOf("SignedFetch")).toEqual([]);
    expect(screen.getByRole("button", { name: "Confirming\u2026" })).toBeDisabled();

    act(() => {
      bridge.pushFriends({ friends: [], sent: [makeFriendRequest({ friend: { address: OWNER, name: "Owner", hasClaimedName: false, profilePictureUrl: "" } })] });
    });
    expect(await screen.findByText(/Your friend request is on its way/)).toBeInTheDocument();
  });
});

describe("tip delivery", () => {
  function fakeWallet(chainId = "0x89") {
    const calls: { method: string; params?: unknown[] }[] = [];
    const provider: Eip1193Provider = {
      request: async (args) => {
        calls.push(args);
        switch (args.method) {
          case "eth_requestAccounts":
            return [SIGNER.toUpperCase()];
          case "eth_chainId":
            return chainId;
          case "wallet_switchEthereumChain":
            return null;
          case "eth_sendTransaction":
            return "0xdeadbeef";
          default:
            throw new Error(`unexpected ${args.method}`);
        }
      },
    };
    return { provider, calls };
  }

  function announce(rdns: string, provider: Eip1193Provider) {
    window.addEventListener("eip6963:requestProvider", () => {
      window.dispatchEvent(
        new CustomEvent("eip6963:announceProvider", {
          detail: { info: { uuid: rdns, name: "Rabby", icon: "", rdns }, provider },
        }),
      );
    });
  }

  test("an EIP-6963 wallet with no window.ethereum is both ready and the one that signs the MANA transfer", async () => {
    const { provider, calls } = fakeWallet("0x1");
    announce("io.rabby", provider);
    expect(detectWallets().map((w) => w.rdns)).toEqual(["io.rabby"]);
    selectWallet("io.rabby");
    expect(window.ethereum).toBeUndefined();

    const user = mount(<SceneTipModal owner={OWNER} sceneTitle="CBD Plaza" coords="-143,102" onClose={() => {}} />);
    act(() => {
      bridge.pushIdentity({ address: SIGNER, signerAddress: SIGNER });
    });
    const send = await screen.findByRole("button", { name: "Send 5 MANA" });
    expect(send).toBeEnabled();
    expect(screen.queryByText(/No browser wallet found here/)).toBeNull();
    expect(screen.getByText(/Your wallet will ask you to confirm 5 MANA on Polygon/)).toBeInTheDocument();
    await user.click(send);

    expect(await screen.findByText("0xdeadbeef")).toBeInTheDocument();
    expect(calls.map((c) => c.method)).toEqual([
      "eth_requestAccounts",
      "eth_chainId",
      "wallet_switchEthereumChain",
      "eth_sendTransaction",
    ]);
    expect(calls[2]?.params).toEqual([{ chainId: "0x89" }]);
    expect(calls[3]?.params).toEqual([
      { from: SIGNER, to: MANA, data: encodeManaTransfer(OWNER, 5n * 10n ** 18n) },
    ]);
    expect(bridge.sentOf("SignedFetch")).toEqual([]);
  });

  test("combined variant retries only the message after the tip was submitted", async () => {
    window.history.replaceState({}, "", "/?2026-09-scene-feedback-tip=1");
    const { provider, calls } = fakeWallet();
    window.ethereum = provider;
    const user = mount(<SceneFeedbackModal owner={OWNER} sceneTitle="Plaza" coords="0,0" onClose={() => {}} />);
    act(() => {
      bridge.pushIdentity({ address: SIGNER, signerAddress: SIGNER });
      bridge.pushFriends({ friends: [makeFriend({ address: OWNER })] });
    });
    await user.type(screen.getByRole("textbox", { name: "Feedback" }), "Thanks!");
    await user.click(screen.getByLabelText("Add a MANA tip"));
    await user.click(await screen.findByRole("button", { name: "Send 5 MANA and message" }));
    const first = bridge.expectSent("SignedFetch");
    act(() => bridge.push({ kind: "signedFetchResult", id: first.id, status: 500, body: "unavailable" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Your tip was already submitted");
    expect(screen.getByRole("radio")).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Retry message" }));
    const retry = bridge.sentOf("SignedFetch").at(-1)!;
    act(() => bridge.push({ kind: "signedFetchResult", id: retry.id, status: 200, body: "{}" }));
    expect(await screen.findByText(/Delivered\. Your note/)).toBeInTheDocument();
    expect(calls.filter(call => call.method === "eth_sendTransaction")).toHaveLength(1);
  });

  test("a rejected wallet transfer does not send the accompanying message", async () => {
    const { provider } = fakeWallet();
    const request = provider.request;
    provider.request = async args => { if (args.method === "eth_sendTransaction") throw new Error("Transfer rejected"); return request(args); };
    window.ethereum = provider;
    const user = mount(<SceneTipModal owner={OWNER} sceneTitle="Plaza" coords="0,0" onClose={() => {}} />);
    act(() => { bridge.pushIdentity({ address: SIGNER, signerAddress: SIGNER }); bridge.pushFriends({ friends: [makeFriend({ address: OWNER })] }); });
    await user.type(screen.getByRole("textbox", { name: "Feedback" }), "Thanks!");
    await user.click(await screen.findByRole("button", { name: "Send 5 MANA and message" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Transfer rejected");
    expect(bridge.sentOf("SignedFetch")).toHaveLength(0);
  });

  test("without any provider the modal falls back to the copyable owner address", async () => {
    mount(<SceneTipModal owner={OWNER} sceneTitle="CBD Plaza" coords="-143,102" onClose={() => {}} />);
    expect(await screen.findByText(/No browser wallet found here/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Send 5 MANA" })).toBeDisabled();
    expect(screen.getByText(OWNER)).toBeInTheDocument();
  });
});
