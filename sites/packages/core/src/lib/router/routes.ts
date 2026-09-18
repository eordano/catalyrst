import { href, type Register } from "react-router";

type Pages = Register["pages"];

type RoutePath = keyof Pages;

type StaticRoutePath = {
  [P in RoutePath]: keyof Pages[P]["params"] extends never ? P : never;
}[RoutePath];

export { href };

export function searchHref(path: StaticRoutePath, params: Record<string, string>): string {
  const q = new URLSearchParams(params).toString();
  return q ? `${href(path)}?${q}` : href(path);
}

