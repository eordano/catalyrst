import { afterEach, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import FeatureFlags from "./FeatureFlags";
import { FEATURE_FLAGS, flagStorageKey } from "../../data/featureFlags";
import { renderHud } from "../../test/harness";

afterEach(() => { vi.restoreAllMocks(); for (const flag of FEATURE_FLAGS) localStorage.removeItem(flagStorageKey(flag.id)); history.replaceState(null, "", "/"); });

test("settings overrides persist, reset together, and expose when a reload is needed", async () => {
  const user = userEvent.setup();
  render(<FeatureFlags />);
  expect(screen.getAllByRole("combobox")).toHaveLength(FEATURE_FLAGS.length);
  expect(screen.getByRole("button", { name: "Reload Explorer" })).toBeDisabled();
  await user.selectOptions(screen.getByLabelText("Unity-style shaders"), "disabled");
  expect(localStorage.getItem("dcl.feature.2026-09-unity-shaders")).toBe("0");
  expect(screen.getByRole("button", { name: "Reload Explorer" })).toBeEnabled();
  await user.type(screen.getByRole("searchbox"), "sidebar");
  expect(screen.getAllByRole("combobox")).toHaveLength(1);
  await user.click(screen.getByRole("button", { name: "Reset all to default" }));
  expect(localStorage.getItem("dcl.feature.2026-09-unity-shaders")).toBeNull();
  expect(screen.getByRole("button", { name: "Reload Explorer" })).toBeDisabled();
});

test("storage failure leaves the previous selection and an actionable error", async () => {
  render(<FeatureFlags />);
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw Error("blocked"); });
  await userEvent.selectOptions(screen.getByLabelText("Explorer lobby"), "disabled");
  expect(screen.getByRole("alert")).toHaveTextContent("Allow site storage");
  expect(screen.getByLabelText("Explorer lobby")).toHaveValue("default");
});

test("Explorer Settings links to flags even without an engine connection", async () => {
  const { user, router } = renderHud({ route: "/settings" });
  await user.click(await screen.findByRole("tab", { name: "Feature Flags" }));
  expect(screen.getByRole("region", { name: "Feature flags" })).toBeInTheDocument();
  expect(router.state.location.search).toBe("?section=flags");
  expect(screen.queryByText(/Not connected to the engine/)).toBeNull();
});
