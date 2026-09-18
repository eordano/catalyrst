import { sendSignedJSON } from "./client";
import { field, isRecord, listOf } from "./rows";

export type ConversationKind = "direct" | "community";
export type Conversation = { id: string; name: string };
export type ConversationMessage = { id: string; author: string; body: string; date: string };

export async function loadConversations(kind: ConversationKind): Promise<Conversation[]> {
  const raw = await sendSignedJSON(kind === "direct" ? "/v1/friends" : "/v1/communities", {
    method: "GET", query: kind === "community" ? { onlyMemberOf: true, limit: 100 } : undefined,
  });
  const rows = kind === "direct" ? field(raw, "friends") : field(field(raw, "data") ?? raw, "results");
  return listOf<unknown>(rows).flatMap((row) => {
    if (!isRecord(row)) return [];
    const id = kind === "direct" ? row.address : row.id;
    if (typeof id !== "string" || !id) return [];
    return [{ id, name: typeof row.name === "string" && row.name.trim() ? row.name : id }];
  });
}

function path(kind: ConversationKind, id: string): string {
  return kind === "direct" ? `/v1/friends/${encodeURIComponent(id)}/messages` : `/v1/communities/${encodeURIComponent(id)}/posts`;
}

export async function loadConversation(kind: ConversationKind, id: string): Promise<ConversationMessage[]> {
  const raw = await sendSignedJSON(path(kind, id), { method: "GET", query: { limit: 100 } });
  const rows = kind === "direct" ? field(raw, "messages") : field(field(raw, "data") ?? raw, "posts");
  if (!Array.isArray(rows)) throw new Error("The conversation returned an unexpected response.");
  const messages = rows.flatMap((row) => {
    if (!isRecord(row)) return [];
    const body = kind === "direct" ? row.body : row.content;
    if (typeof row.id !== "string" || typeof body !== "string") return [];
    const author = kind === "direct" ? row.from : row.authorName || row.authorAddress;
    const date = kind === "direct" ? row.sentAt : row.createdAt;
    return [{ id: row.id, body, author: typeof author === "string" ? author : "", date: typeof date === "string" ? date : "" }];
  });
  return kind === "community" ? messages.reverse() : messages;
}

export async function sendConversationMessage(kind: ConversationKind, id: string, body: string) {
  const raw = await sendSignedJSON(path(kind, id), { body: kind === "direct" ? { body } : { content: body } });
  const result = kind === "direct" ? field(raw, "message") ?? raw : field(raw, "data") ?? raw;
  if (typeof field(result, "id") !== "string") throw new Error("The server did not confirm the message. Refresh before retrying.");
}
