import { CatalystError, sendSignedJSON, serviceBase, type RequestOpts } from "./client";
import { signedFetchHeaders } from "../auth/signedFetchLocal";

export type {
  Community,
  CommunityMember,
  CommunityEvent,
  CommunityDetail,
  CommunityPost,
  CommunityPlace,
} from "./communitiesSchema";

export type CreatedCommunity = { id: string; name: string; role?: string };

export async function createCommunity(input: {
  name: string;
  description: string;
  privacy?: string;
  visibility?: string;
}): Promise<CreatedCommunity> {
  const path = "/v1/communities";
  const url = `${serviceBase("communities")}${path}`;
  const headers = await signedFetchHeaders("post", path);

  const body = new FormData();
  body.set("name", input.name);
  body.set("description", input.description);
  body.set("privacy", input.privacy ?? "public");
  body.set("visibility", input.visibility ?? "all");

  let res: Response;
  try {
    res = await fetch(url, { method: "POST", headers, body });
  } catch (err) {
    throw new CatalystError(
      `Catalyst request failed: ${(err as { message?: string } | null)?.message ?? "network error"}`,
      url,
    );
  }

  let text: string;
  try {
    text = await res.text();
  } catch (err) {
    throw new CatalystError(
      `Catalyst response unreadable: ${(err as { message?: string } | null)?.message ?? "body read failed"}`,
      url,
      res.status,
    );
  }
  if (!res.ok) {
    let serverMsg = "";
    try {
      serverMsg = (JSON.parse(text) as { message?: string } | null)?.message ?? "";
    } catch {
      serverMsg = text;
    }
    throw new CatalystError(serverMsg || `Catalyst returned ${res.status}`, url, res.status);
  }

  try {
    const parsed = JSON.parse(text) as { data?: CreatedCommunity } | null;
    if (parsed?.data?.id) return parsed.data;
  } catch {
  }
  return { id: "", name: input.name };
}

export async function joinCommunity(
  id: string,
  opts: RequestOpts & { privacy?: string } = {},
): Promise<{ id: string; status: string }> {
  if (!id) throw new CatalystError("Missing community id", "", 0);
  const { privacy, ...rest } = opts;
  if (privacy === "private") {
    await sendSignedJSON(`/v1/communities/${encodeURIComponent(id)}/requests`, {
      service: "communities",
      ...rest,
      method: "POST",
      body: { type: "request_to_join" },
    });
    return { id, status: "requested" };
  }
  await sendSignedJSON(`/v1/communities/${encodeURIComponent(id)}/members`, {
    service: "communities",
    ...rest,
    method: "POST",
  });
  return { id, status: "joined" };
}

export async function leaveCommunity(
  id: string,
  opts: RequestOpts & { address?: string } = {},
): Promise<{ id: string; status: string }> {
  if (!id) throw new CatalystError("Missing community id", "", 0);
  const { address, ...rest } = opts;
  if (!address) {
    throw new CatalystError("Sign in to manage this community", "", 401);
  }
  await sendSignedJSON(
    `/v1/communities/${encodeURIComponent(id)}/members/${encodeURIComponent(address)}`,
    { service: "communities", ...rest, method: "DELETE" },
  );
  return { id, status: "left" };
}
