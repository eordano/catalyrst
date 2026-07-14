import { data } from "react-router";

import { sidLoader } from "@core/lib/experiments/story-loader";

import { assertRate } from "@data/lib/foundry/db.server";
import {
  listRoomPresence,
  mintRoomTicket,
  roomsConfigured,
} from "@data/lib/foundry/room.server";

import { actionFailure, clientIp, requireCookie } from "../lib/foundry-action";

import type { Route } from "./+types/foundry.room-token";

// Resource route: the room dock POSTs the page path it is on and gets back a
// join ticket for that page's room. GET answers the dock's presence poll —
// the same live SFU reading People renders, null carrying its failure mode.
export async function loader() {
  return data({
    configured: roomsConfigured(),
    presence: await listRoomPresence(),
  });
}

export async function action({ request }: Route.ActionArgs) {
  const base = sidLoader(request);

  let form: FormData;
  try {
    form = await request.formData();
  } catch {
    return data(
      { ok: false, intent: "room", error: "Could not read the form." },
      { status: 400 },
    );
  }

  try {
    requireCookie(base.created);
    assertRate(base.sid, clientIp(request));
    const path = String(form.get("path") ?? "");
    const ticket = await mintRoomTicket({ path, sid: base.sid });
    return base.wrap({ ok: true, intent: "room", error: null, ...ticket });
  } catch (err) {
    return actionFailure("foundry.room-token", "room", err);
  }
}
