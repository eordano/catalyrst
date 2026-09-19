export type ScreenSection<T> =
  | { status: "ready"; data: T; updatedAt: number }
  | { status: "unavailable"; data: null; updatedAt: null };

export function sectionData<T>(section: ScreenSection<T>): T {
  if (section.status !== "ready") throw new Error("This section is temporarily unavailable");
  return section.data;
}
