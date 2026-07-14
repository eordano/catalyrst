import { siteUrl } from "../data/site";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, waitFor, within } from "storybook/test";

import SignInModalView from "./SignInModalView";

const noop = async () => undefined;
const shellHandle = () => ({ identity: new Promise<unknown>(() => {}), cancel: () => {} });

const fakePairSession = async () => ({
  uri: siteUrl("/auth/pair/u0FbYq3XN1zJc8dJx2wLpA"),
  qrDataUrl:
    "data:image/svg+xml;utf8," +
    encodeURIComponent(
      "<svg xmlns='http://www.w3.org/2000/svg' width='200' height='200'><rect width='200' height='200' fill='#fff'/><text x='100' y='105' text-anchor='middle' font-size='14'>QR</text></svg>",
    ),
  connected: new Promise<unknown>(() => {}),
  cancel: () => {},
});

const meta = {
  tags: ["autodocs"],
  title: "Components/SignInModalView",
  component: SignInModalView,
  parameters: { layout: "centered" },
  args: {
    onClose: () => {},
    inAppAvailable: true,
    walletAvailable: false,
    wallets: [],
    desktopShell: false,
    authError: null,
    onStartEmailSignIn: noop,
    onVerifyEmailSignIn: noop,
    onSocialSignIn: () => {},
    onConnectWallet: noop,
    onPhonePair: fakePairSession,
    onBeginShellSignIn: shellHandle,
  },
} satisfies Meta<typeof SignInModalView>;

export default meta;
type Story = StoryObj<typeof meta>;

export const EmailAndSocial: Story = {};

export const WalletStep: Story = {
  args: {
    walletAvailable: true,
    wallets: [
      {
        rdns: "io.metamask",
        name: "MetaMask",
        icon:
          "data:image/svg+xml;utf8," +
          encodeURIComponent(
            '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect width="24" height="24" rx="6" fill="#f6851b"/><path d="M6 6l5 4-1.5-4z" fill="#fff"/></svg>',
          ),
      },
      { rdns: "com.coinbase.wallet", name: "Coinbase Wallet" },
      { rdns: "io.rabby", name: "Rabby" },
    ],
  },
};

export const LegacyInjectedWallet: Story = {
  args: { walletAvailable: true, wallets: [] },
};

export const DesktopShell: Story = {
  args: { desktopShell: true },
};

export const WithError: Story = {
  args: { authError: "That email isn't recognized. Check the address and try again." },
};

export const PhoneQrPane: Story = {
  play: async ({ canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    const walletRow = await body.findByRole("button", {
      name: "Continue with wallet",
    });
    walletRow.click();
    await waitFor(async () => {
      await expect(
        body.getByAltText("QR code linking to this site's phone sign-in page"),
      ).toBeVisible();
    });
    await expect(
      body.getByText(siteUrl("/auth/pair/u0FbYq3XN1zJc8dJx2wLpA")),
    ).toBeVisible();
  },
};
