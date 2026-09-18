import { useState } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import View, { type WorldSettingsValues } from "./ChWorldSettingsTabbedSections";

const settings: WorldSettingsValues = { title: "Existing World", description: "Existing description", categories: ["art"], spawnCoordinates: "1,2", skyboxTime: null, singlePlayer: false, showInPlaces: true, thumbnailUrl: null };
function Form() {
  const [value, setValue] = useState(settings);
  return <View settings={value} onSettingsChange={(change) => setValue((current) => ({ ...current, ...change }))} />;
}

describe("World settings form values", () => {
  it("keeps actual edits across tabs and controls spawn, skybox and toggles", () => {
    render(<Form />);
    const title = screen.getByLabelText("World Title") as HTMLInputElement;
    expect(title.value).toBe("Existing World");
    fireEvent.change(title, { target: { value: "Updated World" } });
    fireEvent.change(screen.getByLabelText("Description"), { target: { value: "Updated description" } });
    fireEvent.click(screen.getByRole("tab", { name: "Misc." }));
    fireEvent.change(screen.getByLabelText("X"), { target: { value: "3" } });
    fireEvent.click(screen.getByLabelText("Auto (decentraland time)"));
    fireEvent.change(screen.getByLabelText("Max Offset time"), { target: { value: "09:30" } });
    fireEvent.click(screen.getByLabelText("Single Player"));
    fireEvent.click(screen.getByRole("tab", { name: "Details" }));
    expect((screen.getByLabelText("World Title") as HTMLInputElement).value).toBe("Updated World");
    expect((screen.getByLabelText("Description") as HTMLTextAreaElement).value).toBe("Updated description");
    fireEvent.click(screen.getByRole("tab", { name: "Misc." }));
    expect((screen.getByLabelText("X") as HTMLInputElement).value).toBe("3");
    expect((screen.getByLabelText("Y") as HTMLInputElement).value).toBe("2");
    expect((screen.getByLabelText("Max Offset time") as HTMLInputElement).value).toBe("09:30");
    expect((screen.getByLabelText("Single Player") as HTMLInputElement).checked).toBe(true);
  });
});
