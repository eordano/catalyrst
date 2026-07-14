import AccountChrome from "./AccountChrome";

export default {
  title: "Account/Frames/AccountChrome",
  component: AccountChrome,
  parameters: { layout: "fullscreen" },
};

const Body = () => (
  <div style={{ padding: 40, color: "rgba(255,255,255,.55)", fontSize: 14 }}>
    Page body renders here.
  </div>
);

export const Default = {
  render: () => (
    <AccountChrome>
      <Body />
    </AccountChrome>
  ),
};

export const SignedIn = {
  render: () => (
    <AccountChrome signedIn>
      <Body />
    </AccountChrome>
  ),
};
