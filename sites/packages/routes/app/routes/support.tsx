import { useEffect, useRef } from "react";

import StHelpSupportCenter, { SERVICES, Status } from "@ui/web/pages/StHelpSupportCenter";
import type { Service } from "@ui/web/pages/StHelpSupportCenter";
import "@ui/web/pages/sthelpsupportcenter.css";

import { catalystBase } from "@data/lib/catalyst/client";
import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/support";

const STORY = "support";

const SUPPORT_EMAIL =
  (typeof process !== "undefined" ? process.env?.SUPPORT_EMAIL : undefined) ?? "hello@catalyst.example.com";

const PROBE_TIMEOUT_MS = 3000;
const SLOW_THRESHOLD_MS = 1500;
const CACHE_TTL_MS = 60_000;

let cachedServices: { at: number; services: Service[] } | null = null;

async function probeService(svc: Service): Promise<Service> {
  const url = svc.url.startsWith("/") ? `${catalystBase()}${svc.url}` : svc.url;
  const started = Date.now();
  try {
    const res = await fetch(url, {
      signal: AbortSignal.timeout(PROBE_TIMEOUT_MS),
      headers: { accept: "application/json" },
    });
    if (!res.ok) return { ...svc, status: Status.DOWN };
    const elapsed = Date.now() - started;
    return { ...svc, status: elapsed > SLOW_THRESHOLD_MS ? Status.SLOW : Status.OK };
  } catch {
    return { ...svc, status: Status.DOWN };
  }
}

async function probeServices(): Promise<Service[]> {
  if (cachedServices && Date.now() - cachedServices.at < CACHE_TTL_MS) {
    return cachedServices.services;
  }
  const services = await Promise.all(SERVICES.map(probeService));
  cachedServices = { at: Date.now(), services };
  return services;
}

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);
  const services = await probeServices();
  const payload = { sid, services, supportEmail: SUPPORT_EMAIL };
  return wrap(payload);
}

type LoaderData = { sid: string; services: Service[]; supportEmail: string };

export default function SupportRoute({ loaderData }: Route.ComponentProps) {
  const { sid, services, supportEmail } = loaderData as LoaderData;

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track("lp_support_viewed", {}, { sid, story: STORY });
  }, [sid]);

  return <StHelpSupportCenter services={services} supportEmail={supportEmail} />;
}
