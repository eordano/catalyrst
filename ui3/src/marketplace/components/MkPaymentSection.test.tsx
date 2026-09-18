import { test, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import MkPaymentSection, { MkPaymentCardPane, MkPaymentManaPane } from "./MkPaymentSection";

test("mana pane: an insufficient balance names both amounts and offers the card rail before any signing", async () => {
  const onPayWithCard = vi.fn();
  const onPay = vi.fn();
  render(
    <MkPaymentSection method="mana" shortfallCredits="29">
      <MkPaymentManaPane
        credits="29"
        phase={{ step: "insufficient", haveWei: "0", needWei: "39580378408756389366" }}
        onPay={onPay}
        onPayWithCard={onPayWithCard}
      />
    </MkPaymentSection>,
  );

  const alert = screen.getByRole("alert");
  expect(alert.textContent).toMatch(/39\.58 MANA/);
  expect(alert.textContent).toMatch(/holds 0\.00 MANA/);
  expect(screen.queryByRole("button", { name: "Continue" })).toBeNull();
  expect(document.querySelector(".paysec__insufficient")).not.toBeNull();
  expect(document.querySelector(".paysec__unavailable")).toBeNull();
  expect(document.querySelector(".paysec__quote")).toBeNull();

  await userEvent.click(screen.getByRole("button", { name: "Pay with card instead" }));
  expect(onPayWithCard).toHaveBeenCalledTimes(1);
  expect(onPay).not.toHaveBeenCalled();
  expect(screen.getByRole("link", { name: /Credits pack/ })).toHaveAttribute("href", "/marketplace/packs");
});

test("mana pane: a covered quote still shows the Continue action", () => {
  render(
    <MkPaymentSection method="mana" shortfallCredits="29">
      <MkPaymentManaPane
        credits="29"
        phase={{ step: "ready", quote: { weiSuggested: "39580378408756389366" } }}
      />
    </MkPaymentSection>,
  );
  expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled();
});

test("card pane renders the mock form with a Continue action", () => {
  render(
    <MkPaymentSection method="card" shortfallCredits="29">
      <MkPaymentCardPane />
    </MkPaymentSection>,
  );
  expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled();
  expect(screen.getByRole("button", { name: "Pay with card" })).toHaveAttribute("aria-pressed", "true");
});

test("re-clicking the already selected rail moves focus to that pane's primary action without re-picking the method", async () => {
  const onPickMethod = vi.fn();
  render(
    <MkPaymentSection method="card" shortfallCredits="29" onPickMethod={onPickMethod}>
      <MkPaymentCardPane />
    </MkPaymentSection>,
  );

  await userEvent.click(screen.getByRole("button", { name: "Pay with card" }));
  expect(screen.getByRole("button", { name: "Continue" })).toHaveFocus();
  expect(onPickMethod).not.toHaveBeenCalled();

  await userEvent.click(screen.getByRole("button", { name: "Pay with MANA" }));
  expect(onPickMethod).toHaveBeenCalledTimes(1);
  expect(onPickMethod).toHaveBeenLastCalledWith("mana");
  expect(screen.getByRole("button", { name: "Continue" })).not.toHaveFocus();
});

test("re-clicking the MANA rail on an insufficient balance focuses the card switch, and on a confirming transfer leaves the explorer link alone", async () => {
  const onPickMethod = vi.fn();
  const { rerender } = render(
    <MkPaymentSection method="mana" shortfallCredits="29" onPickMethod={onPickMethod}>
      <MkPaymentManaPane
        credits="29"
        phase={{ step: "insufficient", haveWei: "0", needWei: "39580378408756389366" }}
        onPayWithCard={() => {}}
      />
    </MkPaymentSection>,
  );
  await userEvent.click(screen.getByRole("button", { name: "Pay with MANA" }));
  expect(screen.getByRole("button", { name: "Pay with card instead" })).toHaveFocus();

  rerender(
    <MkPaymentSection method="mana" shortfallCredits="29" onPickMethod={onPickMethod}>
      <MkPaymentManaPane credits="29" phase={{ step: "confirming", txHash: "0xabc" }} />
    </MkPaymentSection>,
  );
  await userEvent.click(screen.getByRole("button", { name: "Pay with MANA" }));
  expect(screen.getByRole("link", { name: "view transaction" })).not.toHaveFocus();
  expect(onPickMethod).not.toHaveBeenCalled();
});

test("re-clicking the selected MANA rail focuses Continue on a ready quote", async () => {
  render(
    <MkPaymentSection method="mana" shortfallCredits="29" onPickMethod={() => {}}>
      <MkPaymentManaPane
        credits="29"
        phase={{ step: "ready", quote: { weiSuggested: "39580378408756389366" } }}
      />
    </MkPaymentSection>,
  );
  await userEvent.click(screen.getByRole("button", { name: "Pay with MANA" }));
  expect(screen.getByRole("button", { name: "Continue" })).toHaveFocus();
});
