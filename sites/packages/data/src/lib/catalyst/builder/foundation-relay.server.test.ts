import { expect, it, vi } from "vitest";
import { generatePrivateKey } from "viem/accounts";
import { createIdentityFromPrivateKey } from "../../auth/identity";
import { signRequest } from "../../auth/signer";
import { relayFoundation } from "./foundation-relay.server";

const path="/items/11111111-1111-4111-8111-111111111111";
const relay="https://catalyst.example.com/api/builder-foundation";
it("only relays approved operations after verifying the signature against the upstream path",async()=>{
  const send=vi.fn<typeof fetch>();
  expect((await relayFoundation(new Request(relay+path),path,send)).status).toBe(401);
  expect((await relayFoundation(new Request(relay+"/anything"),"/anything",send)).status).toBe(404);
  expect((await relayFoundation(new Request(relay+path,{method:"DELETE"}),path,send)).status).toBe(404);
  const identity=await createIdentityFromPrivateKey(generatePrivateKey());
  const wrong=await signRequest(identity,"PUT",relay+path);
  expect((await relayFoundation(new Request(relay+path,{method:"PUT",headers:wrong.headers}),path,send)).status).toBe(401);
  expect(send).not.toHaveBeenCalled();
  const signed=await signRequest(identity,"PUT","/v1"+path);
  send.mockResolvedValue(Response.json({data:{id:"saved"}}));
  const body=JSON.stringify({item:{name:"Hat"}});
  const response=await relayFoundation(new Request(relay+path,{method:"PUT",headers:{...signed.headers,"content-type":"application/json",cookie:"private=secret",authorization:"private"},body}),path,send);
  expect(response.status).toBe(200);expect(response.headers.get("cache-control")).toBe("private, no-store");
  const [url,init]=send.mock.calls[0];
  expect(url).toBe("https://builder-api.decentraland.org/v1"+path);
  expect(init?.method).toBe("PUT");expect(init?.redirect).toBe("error");
  const headers=new Headers(init?.headers);
  expect(headers.get("cookie")).toBeNull();expect(headers.get("authorization")).toBeNull();
  expect(headers.get("x-identity-auth-chain-2")).toBe(signed.headers["x-identity-auth-chain-2"]);
  expect(new TextDecoder().decode(init?.body as Uint8Array)).toBe(body);
  send.mockRejectedValue(new Error("offline"));
  expect((await relayFoundation(new Request(relay+path,{method:"PUT",headers:signed.headers,body}),path,send)).status).toBe(503);
});
it("rejects oversized bodies and query/path tricks without forwarding",async()=>{
  const identity=await createIdentityFromPrivateKey(generatePrivateKey());
  const signed=await signRequest(identity,"PUT","/v1"+path);
  const send=vi.fn<typeof fetch>();
  expect((await relayFoundation(new Request(relay+path,{method:"PUT",headers:{...signed.headers,"content-length":String(23*1024*1024)}}),path,send)).status).toBe(413);
  expect((await relayFoundation(new Request(relay+path+"?url=https://other.test",{method:"PUT",headers:signed.headers}),path,send)).status).toBe(404);
  expect((await relayFoundation(new Request(relay+path),"/items/../collections",send)).status).toBe(404);
  expect(send).not.toHaveBeenCalled();
});

it("relays signed provider discovery without granting provider mutations", async () => {
  const identity = await createIdentityFromPrivateKey(generatePrivateKey());
  const signed = await signRequest(identity, "GET", "/v1/thirdParties");
  const send = vi.fn<typeof fetch>().mockResolvedValue(Response.json({ data: [] }));
  expect((await relayFoundation(new Request(relay + "/thirdParties", { headers: signed.headers }), "/thirdParties", send)).status).toBe(200);
  expect(send.mock.calls[0][0]).toBe("https://builder-api.decentraland.org/v1/thirdParties");
  expect((await relayFoundation(new Request(relay + "/thirdParties", { method: "PUT", headers: signed.headers }), "/thirdParties", send)).status).toBe(404);
});

it("only relays canonical, signed Polygon provider-slot reads", async () => {
  const identity = await createIdentityFromPrivateKey(generatePrivateKey());
  const slots = "/thirdParties/" + encodeURIComponent("urn:decentraland:matic:collections-thirdparty:provider") + "/slots";
  const signed = await signRequest(identity, "GET", "/v1" + slots);
  const send = vi.fn<typeof fetch>().mockResolvedValue(Response.json({ data: 7 }));
  const response = await relayFoundation(new Request(relay + slots, { headers: signed.headers }), slots, send);
  expect(response.status).toBe(200); expect(await response.json()).toEqual({ data: 7 });
  expect(send.mock.calls[0][0]).toBe("https://builder-api.decentraland.org/v1" + slots);
  send.mockClear();
  for (const invalid of [slots.replace("matic", "amoy"), slots.replace("provider", "%252e%252e%252fadmin"), slots.replace("provider", "%"), slots + "/admin", "/thirdParties/provider/slots"]) {
    expect((await relayFoundation(new Request(relay + invalid, { headers: signed.headers }), invalid, send)).status).not.toBe(200);
  }
  expect((await relayFoundation(new Request(relay + slots, { method: "POST", headers: signed.headers }), slots, send)).status).toBe(404);
  const wrong = await signRequest(identity, "GET", "/v1/thirdParties");
  expect((await relayFoundation(new Request(relay + slots, { headers: wrong.headers }), slots, send)).status).toBe(401);
  expect(send).not.toHaveBeenCalled();
});
