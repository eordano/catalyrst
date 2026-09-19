import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test } from "vitest";
import { fireEvent, render } from "@testing-library/react";

import { Avatar } from "./primitives";

const FACE = "data:image/png;base64,iVBORw0KGgo=";
const initials = (c: HTMLElement) => c.querySelector(".u-avatar__initials");
const img = (c: HTMLElement) => c.querySelector<HTMLImageElement>(".u-avatar__img");

describe("Avatar initials fallback", () => {
  test("initials show without a source, until the face loads, again on a new source, and back after a broken image", () => {
    const bare = render(<Avatar name="Brown" />);
    expect(initials(bare.container)?.textContent).toBe("BR");
    expect(img(bare.container)).toBeNull();

    const { container, rerender } = render(<Avatar name="Brown" src={FACE} />);
    expect(initials(container)?.textContent).toBe("BR");
    fireEvent.load(img(container)!);
    expect(initials(container)).toBeNull();
    expect(img(container)?.getAttribute("src")).toBe(FACE);
    rerender(<Avatar name="Brown" src={FACE + "AA"} />);
    expect(initials(container)?.textContent).toBe("BR");
    fireEvent.load(img(container)!);
    expect(initials(container)).toBeNull();

    const { container: broken } = render(<Avatar name="Brown" src={FACE} />);
    fireEvent.error(img(broken)!);
    expect(img(broken)).toBeNull();
    expect(initials(broken)?.textContent).toBe("BR");
  });

  test("while the face is still loading the initials paint above the image box, which carries no background of its own", () => {
    const { container } = render(<Avatar name="Brown" src={FACE} />);
    const face = img(container)!;
    const letters = initials(container)!;
    expect(face.compareDocumentPosition(letters) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    const css = readFileSync(join(__dirname, "primitives.css"), "utf8");
    const rule = css.match(/\.u-avatar__img\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(rule).not.toMatch(/background/);
    fireEvent.load(face);
    expect(initials(container)).toBeNull();
  });

  test("explicit initials follow the same rule and children replace the initials", () => {
    const { container } = render(<Avatar initials="xy" src={FACE} />);
    expect(initials(container)?.textContent).toBe("XY");
    fireEvent.load(img(container)!);
    expect(initials(container)).toBeNull();
    const { container: custom } = render(
      <Avatar name="Brown">
        <span className="custom" />
      </Avatar>,
    );
    expect(initials(custom)).toBeNull();
    expect(custom.querySelector(".custom")).not.toBeNull();
  });
});

test("Bevy full-body thumbnails are framed as face portraits, while profile photos stay intact", () => {
  const { container, rerender } = render(<Avatar src={FACE} name="Brown" />);
  const fullBody = container.querySelector('img')!;
  Object.defineProperties(fullBody, { naturalWidth: { value: 320 }, naturalHeight: { value: 512 } });
  fireEvent.load(fullBody);
  expect(container.querySelector('svg')?.getAttribute('viewBox')).toBe('100 80 120 120');
  expect(container.querySelector('svg image')?.getAttribute('href')).toBe(FACE);
  rerender(<Avatar src="https://example.org/face.png" name="Brown" />);
  fireEvent.load(container.querySelector('img')!);
  expect(container.querySelector('svg')).toBeNull();
});
