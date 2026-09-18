import { expect, waitFor } from "storybook/test";

export const LIVE_CATALYST = "https://catalyst.example.com";

export async function expectLiveAvatar(canvasElement: HTMLElement) {
  await waitFor(() => {
    const preview = canvasElement.querySelector('[data-status="ready"] canvas');
    expect(preview).toBeVisible();
    expect(preview?.getBoundingClientRect().width).toBeGreaterThan(0);
  }, { timeout: 15000 });
}
