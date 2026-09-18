import { describe, expect, test } from "vitest";
import { render } from "@testing-library/react";

import Passport from "./Passport";

const editAvatar = (c: HTMLElement) => c.querySelector<HTMLButtonElement>(".ps__editavatar");

describe("Passport edit avatar", () => {
  test("the own profile links to the backpack; someone else's has no edit affordance", () => {
    const own = render(<Passport isSelf identity={{ name: "Brown" }} />);
    const btn = editAvatar(own.container);
    expect(btn?.getAttribute("data-sb-linkto")).toBe("Explorer/Pages/Backpack");
    expect(btn?.textContent).toContain("EDIT AVATAR");

    const other = render(<Passport isSelf={false} identity={{ name: "Lu" }} />);
    expect(editAvatar(other.container)).toBeNull();
  });

  test("the halo stage wraps only the avatar, so the edit pill sits outside it and cannot pull the halo off-centre", () => {
    const own = render(<Passport isSelf identity={{ name: "Brown" }} />);
    const stage = own.container.querySelector(".ps__stage")!;
    expect(stage.querySelector(".ps__avatar")).not.toBeNull();
    expect(stage.querySelector(".ps__editavatar")).toBeNull();
    expect(editAvatar(own.container)?.previousElementSibling).toBe(stage);

    const custom = render(
      <Passport isSelf identity={{ name: "Brown" }} avatarPreview={<canvas className="preview" />} />,
    );
    expect(custom.container.querySelector(".ps__stage > .preview")).not.toBeNull();
  });
});
