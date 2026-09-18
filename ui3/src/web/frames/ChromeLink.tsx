import type { ComponentPropsWithRef } from "react";
import { chromeLinkClick, chromeLinkProps, useChromeNav, type ChromeNavigate } from "./chrome-nav";

type ChromeLinkProps = ComponentPropsWithRef<"a"> & { onNavigate?: ChromeNavigate };

export default function ChromeLink({ href = "", onClick, onNavigate, ...props }: ChromeLinkProps) {
  const nav = useChromeNav();
  const { Link } = nav;
  const navigate = onNavigate ?? nav.navigate;
  if (Link && !onNavigate) return <Link href={href} {...props} onClick={onClick} />;
  return <a {...chromeLinkProps(href, navigate)} {...props} onClick={event => {
    onClick?.(event);
    chromeLinkClick(event, href, navigate);
  }} />;
}
