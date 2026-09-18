// Full app -> Rust API -> local Foundation fixture. Only disposable wallets;
// this suite never publishes anything to production Foundation services.
const { chromium } = require("playwright");
const { privateKeyToAccount } = require("viem/accounts");
const http = require("node:http");
const net = require("node:net");
const { spawn } = require("node:child_process");
const { mkdtempSync, rmSync, mkdirSync, copyFileSync } = require("node:fs");
const { tmpdir } = require("node:os");
const path = require("node:path");
const assert = require("node:assert/strict");
const root = path.resolve(__dirname, "..");
const community = "11111111-1111-4111-8111-111111111111";
const account = privateKeyToAccount("0x" + "1".padStart(64, "0"));
const communityData = {
  id: community,
  name: "Genesis Builders",
  description: "A place to build, explore, and meet.",
  active: true,
  membersCount: 24,
  role: "owner",
  privacy: "public",
};
let member = true;
let rejectNextMessage = false;
let joinRequested = false;
const pendingRequest = "22222222-2222-4222-8222-222222222222";
let pendingApproval = true;
const inviteAccept = "44444444-4444-4444-8444-444444444444", inviteDecline = "55555555-5555-4555-8555-555555555555", ownRequest = "66666666-6666-4666-8666-666666666666";
const inboxInvitations = new Map([[inviteAccept,"pending"],[inviteDecline,"pending"]]);
let communityUpdated = false, pictureUpdated = false, uploadedPicture;
const hangoutId="77777777-7777-4777-8777-777777777777"; let hangouts=[];
const managedWallet="0x0000000000000000000000000000000000000002"; let managedRole="member", banned=false,removed=false,invited=false;
const prefix = process.env.SOCIAL_TEST_PREFIX || "";
const posts = [];
const telemetry = [];
const fixture = http.createServer(async (req, res) => {
  const chunks=[];
  for await (const chunk of req) chunks.push(chunk);
  const rawBody=Buffer.concat(chunks), body=rawBody.toString();
  res.setHeader("content-type", "application/json");
  if (req.url === "/telemetry") {
    telemetry.push(JSON.parse(body));
    res.end(JSON.stringify({ id: telemetry.at(-1).event_id }));
    return;
  }
  if (req.url === "/v1/community-voice-chats/active") { assert.ok(req.headers["x-identity-auth-chain-1"]);res.end(JSON.stringify({data:{activeChats:[{communityId:community,communityName:"Builders live voice",participantCount:3,moderatorCount:1,isMember:true}],total:1}}));return; }
  if (req.url.startsWith(`/v1/communities/${community}/places?`) && req.method === "GET") {res.end(JSON.stringify({data:{results:hangouts.map(id=>id==="embedded.dcl.eth"?{id,title:"Embedded World",world:true,world_name:id,base_position:"0,0"}:{id}),total:hangouts.length}}));return;}
  if (req.url === `/v1/communities/${community}/places` && req.method === "POST") {assert.deepEqual(JSON.parse(body),{placeIds:[hangoutId]});hangouts=[hangoutId];res.writeHead(204);res.end();return;}
  if (req.url === `/v1/communities/${community}/places/${hangoutId}` && req.method === "DELETE") {hangouts=[];res.writeHead(204);res.end();return;}
  if (req.url.includes("search=telemetry-failure")) { res.writeHead(503); res.end("{}"); return; }
  if (req.url === `/v1/communities/${community}` && req.method === "PUT") { assert.ok(req.headers["content-type"].startsWith("multipart/form-data; boundary=dcl-social-")); assert.ok(body.includes('name="name"')); assert.ok(body.includes('Genesis Builders')); communityUpdated=true; if(body.includes('name="thumbnail"')) { assert.ok(rawBody.includes(uploadedPicture));pictureUpdated=true; } res.end(JSON.stringify({data:communityData})); return; }
  if (req.url.startsWith(`/v1/communities/${community}/requests?`)) { res.end(JSON.stringify({data:{results:pendingApproval?[{id:pendingRequest,memberAddress:account.address,status:"pending"}]:[],total:pendingApproval?1:0}})); return; }
  if (req.url === `/v1/communities/${community}/requests/${pendingRequest}` && req.method === "PATCH") { assert.equal(JSON.parse(body).intention,"accepted"); pendingApproval=false; res.writeHead(204); res.end(); return; }
  if (req.url.startsWith(`/v1/communities/${community}/members?`) && req.method === "GET") { res.end(JSON.stringify({data:{results:[{memberAddress:account.address,role:"owner"},...(!removed?[{address:managedWallet,role:managedRole}]:[])],total:removed?1:2}})); return; }
  if (req.url === `/v1/communities/${community}/members/${managedWallet}`) {if(req.method==="PATCH")managedRole=JSON.parse(body).role;if(req.method==="DELETE")removed=true;res.writeHead(204);res.end();return;}
  if (req.url === `/v1/communities/${community}/members/${managedWallet}/bans`) {banned=req.method==="POST";removed=banned;res.writeHead(204);res.end();return;}
  if (req.url.startsWith(`/v1/communities/${community}/bans?`)) {res.end(JSON.stringify({data:{results:banned?[{address:managedWallet}]:[],total:banned?1:0}}));return;}
  if (req.url === `/v1/communities/${community}/requests` && req.method==="POST" && JSON.parse(body).type==="invite") {assert.equal(JSON.parse(body).targetedAddress,managedWallet);invited=true;res.writeHead(204);res.end();return;}
  if (req.url === `/v1/communities/${community}/members` && req.method === "POST") { assert.ok(req.headers["x-identity-auth-chain-1"]); member = true; res.writeHead(204); res.end(); return; }
  if (req.url === `/v1/communities/${community}/requests` && req.method === "POST") { assert.equal(JSON.parse(body).type, "request_to_join"); assert.equal(JSON.parse(body).targetedAddress.toLowerCase(), account.address.toLowerCase()); joinRequested = true; res.end(JSON.stringify({data:{status:"pending"}})); return; }
  if (req.url === `/v1/communities/${community}/requests/${ownRequest}` && req.method === "PATCH") { assert.equal(JSON.parse(body).intention,"cancelled"); joinRequested=false; res.writeHead(204);res.end();return; }
  const invitationId = req.url.split('/').at(-1);
  if (inboxInvitations.has(invitationId) && req.method === "PATCH") { const intention=JSON.parse(body).intention; assert.ok(["accepted","rejected"].includes(intention)); inboxInvitations.set(invitationId,intention); if(intention==="accepted")member=true; res.writeHead(204);res.end();return; }
  if (req.url.startsWith(`/v1/members/`)) { const rows=req.url.includes("type=invite") ? [...inboxInvitations].map(([id,status])=>({...communityData,id,communityId:community,name:id===inviteAccept?"Builders invitation":"Another invitation",status})) : joinRequested?[{...communityData,id:ownRequest,communityId:community,status:"pending"}]:[]; res.end(JSON.stringify({data:{results:rows,total:rows.length}})); return; }
  if (req.url.startsWith("/v1/communities/" + community + "/posts")) {
    if (req.method === "POST") {
      assert.ok(req.headers["x-identity-auth-chain-1"]);
      const post = {
        id: "post-" + posts.length,
        content: JSON.parse(body).content,
        authorAddress: account.address,
        authorName: "Builder",
        createdAt: new Date().toISOString(),
      };
      posts.unshift(post);
      res.end(JSON.stringify({ data: post }));
    } else res.end(JSON.stringify({ data: { posts, total: posts.length } }));
  } else if (req.url.startsWith("/v1/communities/" + community))
    res.end(
      JSON.stringify({
        data: { ...communityData, role: member ? "owner" : "none" },
      }),
    );
  else if (req.url.startsWith("/v1/communities"))
    res.end(JSON.stringify({ data: { results: req.url.includes("onlyMemberOf=true") ? (member ? [communityData] : []) : [communityData,{...communityData,id:"33333333-3333-4333-8333-333333333333",name:"Popular public community",role:"none"}], total: req.url.includes("onlyMemberOf=true") ? (member?1:0) : 2 } }));
  else {
    res.statusCode = 404;
    res.end("{}");
  }
});
const listen = (server) =>
  new Promise((resolve) =>
    server.listen(0, "127.0.0.1", () => resolve(server.address().port)),
  );
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
(async () => {
  const dir = mkdtempSync(path.join(tmpdir(), "dcl-social-browser-"));
  let browser, backend, proxy;
  try {
    const upstreamPort = await listen(fixture);
    const portServer = net.createServer();
    const apiPort = await listen(portServer);
    await new Promise((resolve) => portServer.close(resolve));
    backend = spawn(
      process.env.SOCIAL_TEST_BINARY ||
        path.resolve(root, "../target/debug/dcl-social-api"),
      [],
      {
        env: {
          ...process.env,
          SOCIAL_BIND: `127.0.0.1:${apiPort}`,
          SOCIAL_UPSTREAM: `http://127.0.0.1:${upstreamPort}`,
          SOCIAL_ALLOW_LOCAL_UPSTREAM: "1",
          SOCIAL_TELEMETRY_URL: `http://127.0.0.1:${upstreamPort}/telemetry`,
          SOCIAL_DATABASE: path.join(dir, "test.sqlite"),
          SOCIAL_ASSETS: path.join(root, "dist"),
        },
        stdio: ["ignore", "pipe", "pipe"],
      },
    );
    let logs = "";
    backend.stderr.on("data", (d) => (logs += d));
    backend.stdout.on("data", (d) => (logs += d));
    let base = `http://127.0.0.1:${apiPort}`;
    let ready = false;
    for (let i = 0; i < 100; i++) {
      try {
        if ((await fetch(base + "/api/health")).ok) {
          ready = true;
          break;
        }
      } catch {}
      await delay(100);
    }
    assert.ok(ready, logs);
    if (prefix) {
      proxy = http.createServer((req, res) => {
        if (req.url === prefix) {
          res.writeHead(308, { location: prefix + "/" });
          res.end();
          return;
        }
        if (!req.url.startsWith(prefix + "/")) {
          res.writeHead(404);
          res.end();
          return;
        }
        const request = http.request(
          {
            hostname: "127.0.0.1",
            port: apiPort,
            path: req.url.slice(prefix.length),
            method: req.method,
            headers: req.headers,
          },
          (r) => {
            res.writeHead(r.statusCode, r.headers);
            r.pipe(res);
          },
        );
        request.on("error", () => {
          res.writeHead(502);
          res.end();
        });
        req.pipe(request);
      });
      base = `http://127.0.0.1:${await listen(proxy)}${prefix}/`;
    }

    browser = await chromium.launch({
      headless: true,
      executablePath:
        process.env.CHROMIUM_PATH || chromium.executablePath(),
      args: ["--no-sandbox"],
    });
    const errors = [];
    const walletPage = await browser.newPage();
    await walletPage.routeWebSocket("wss://rpc-social-service-ea.decentraland.org/**", ws => ws.close());
    await walletPage.route("**/api/events?*", route => route.fulfill({json:{data:[]}}));
    await walletPage.route("**/api/worlds?*", route => route.fulfill({json:{data:[],total:0}}));
    await walletPage.route("**/api/image?*", route => route.fulfill({ contentType: "image/png", body: Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aD1sAAAAASUVORK5CYII=", "base64") }));
    await walletPage.exposeFunction("signWalletPayload", (payload) =>
      account.signMessage({ message: { raw: payload } }),
    );
    await walletPage.addInitScript((address) => {
      window.walletCalls = [];
      window.walletConnected = sessionStorage.getItem("test-wallet-connected") === "yes";
      window.rejectConnection = true;
      window.walletListeners = [];
      window.ethereum = {
        on(event, listener) { if (event === "accountsChanged") window.walletListeners.push(listener); },
        removeListener(event, listener) { window.walletListeners = window.walletListeners.filter(x => x !== listener); },
        async request({ method, params }) {
          window.walletCalls.push(method);
          if (method === "eth_accounts") return window.walletConnected ? [address] : [];
          if (method === "eth_requestAccounts") {
            if (window.rejectConnection) { window.rejectConnection = false; throw { code: 4001 }; }
            await new Promise(resolve => setTimeout(resolve, 150));
            window.walletConnected = true;
            sessionStorage.setItem("test-wallet-connected", "yes");
            window.walletListeners.forEach(listener => listener());
            return [address];
          }
          if (method === "personal_sign") return window.signWalletPayload(params[0]);
          throw new Error("Unexpected wallet method: " + method);
        },
      };
    }, account.address);
    await walletPage.goto(base);
    const connectButton = walletPage.getByRole("button", { name: "Connect wallet", exact: true }).first();
    await connectButton.waitFor();
    assert.equal(await walletPage.evaluate(() => window.walletCalls.includes("eth_requestAccounts")), false);
    await connectButton.click();
    await walletPage.getByRole("alert").filter({ hasText: "Connection cancelled" }).waitFor();
    await connectButton.click();
    await walletPage.locator(".account .avatar").waitFor();
    await walletPage.locator(".rail").getByRole("button", { name: "Genesis Builders", exact: true }).click();
    await walletPage.getByRole("textbox", { name: "Message", exact: true }).waitFor();
    for (const text of ["delegated message one", "delegated message two"]) {
      await walletPage.getByRole("textbox", { name: "Message", exact: true }).fill(text);
      await walletPage.getByRole("textbox", { name: "Message", exact: true }).press("Enter");
      await walletPage.locator(".message p").filter({ hasText: text }).waitFor();
    }
    assert.equal(await walletPage.evaluate(() => window.walletCalls.filter(x => x === "personal_sign").length), 1);
    await walletPage.reload();
    await walletPage.getByRole("textbox", { name: "Message", exact: true }).waitFor();
    assert.equal(await walletPage.evaluate(() => window.walletCalls.filter(x => x === "personal_sign").length), 0);
    // A fresh browser context restored from persistent storage has no sessionStorage.
    const restoredContext = await browser.newContext({ storageState: await walletPage.context().storageState() });
    const restoredPage = await restoredContext.newPage();
    await restoredPage.routeWebSocket("wss://rpc-social-service-ea.decentraland.org/**", ws => ws.close());
    await restoredPage.addInitScript(address => {
      window.ethereum = { async request({method}) { if(method === "eth_accounts") return window.walletLocked ? [] : [address]; if(method === "eth_requestAccounts") return [address]; throw new Error("Unexpected wallet prompt after restart"); } };
    }, account.address);
    await restoredPage.route("**/api/events?*", route => route.fulfill({json:{data:[]}}));
    await restoredPage.route("**/api/worlds?*", route => route.fulfill({json:{data:[],total:0}}));
    await restoredPage.route("**/api/image?*", route => route.fulfill({ status: 404 }));
    await restoredPage.goto(`${base.replace(/\/$/, "")}/#/c/${community}/general`);
    await restoredPage.getByRole("textbox", { name: "Message", exact: true }).waitFor();
    await restoredPage.evaluate(() => { window.walletLocked = true; window.dispatchEvent(new Event("dcl:identity-changed")); });
    await restoredPage.getByRole("button", { name: "Connect wallet", exact: true }).first().waitFor();
    assert.ok(await restoredPage.evaluate(() => localStorage.getItem("dcl.social.wallet-session.v1")));
    await restoredPage.evaluate(() => { window.walletLocked = false; });
    await restoredPage.getByRole("button", { name: "Connect wallet", exact: true }).first().click();
    await restoredPage.getByRole("textbox", { name: "Message", exact: true }).waitFor();
    await restoredPage.getByRole("button", { name: "Preferences", exact: true }).first().click();
    await restoredPage.getByRole("button", { name: "Forget wallet session", exact: true }).last().click();
    await restoredPage.getByRole("button", { name: "Connect wallet", exact: true }).first().waitFor();
    assert.equal(await restoredPage.evaluate(() => localStorage.getItem("dcl.social.wallet-session.v1")), null);
    await restoredContext.close();
    // Force renewal of the 90-second read capability inside a valid session.
    await walletPage.evaluate(() => { const now = Date.now; Date.now = () => now() + 100000; });
    await walletPage.waitForFunction(() => window.walletCalls.filter(x => x === "eth_accounts").length >= 3, { timeout: 12000 });
    assert.equal(await walletPage.evaluate(() => window.walletCalls.filter(x => x === "personal_sign").length), 0);
    // Expired delegation must be discarded without prompting the wallet in background.
    await walletPage.evaluate(() => { const now = Date.now; Date.now = () => now() + 25 * 60 * 60 * 1000; window.dispatchEvent(new Event("dcl:identity-changed")); });
    await walletPage.getByRole("button", { name: "Connect wallet", exact: true }).first().waitFor();
    assert.equal(await walletPage.evaluate(() => localStorage.getItem("dcl.social.wallet-session.v1")), null);
    assert.equal(await walletPage.evaluate(() => window.walletCalls.filter(x => x === "personal_sign").length), 0);

    await walletPage.evaluate(() => { window.walletConnected = false; window.walletListeners.forEach(listener => listener()); });
    await walletPage.getByRole("button", { name: "Connect wallet", exact: true }).first().waitFor();
    await walletPage.evaluate(() => { delete window.ethereum; });
    await walletPage.getByRole("button", { name: "Connect wallet", exact: true }).first().click();
    await walletPage.getByRole("alert").filter({ hasText: "wallet browser" }).waitFor();
    await walletPage.evaluate(() => {
      console.error(new Error("Telemetry browser test token=secret-value"));
      console.error(new Error("Telemetry browser test token=secret-value"));
      window.dispatchEvent(new ErrorEvent("error", { error: new Error("Telemetry uncaught test") }));
      window.dispatchEvent(new PromiseRejectionEvent("unhandledrejection", { promise: Promise.resolve(), reason: new Error("Telemetry rejection test") }));
    });
    const failure = await fetch(`${base.replace(/\/$/, "")}/api/communities?search=telemetry-failure`);
    assert.equal(failure.status, 502);
    for (let i = 0; i < 50 && !(telemetry.some(e => e.tags.component === "server" && e.message.includes("502")) && telemetry.some(e => e.exception?.values?.[0]?.type === "unhandledrejection")); i++) await delay(100);
    assert.ok(telemetry.some(e => e.tags.component === "server" && e.message.includes("502")), JSON.stringify(telemetry));
    assert.equal(telemetry.filter(e => e.message.includes("Telemetry browser test")).length, 1);
    assert.ok(telemetry.some(e => e.exception.values[0].type === "uncaught"));
    assert.ok(telemetry.some(e => e.exception.values[0].type === "unhandledrejection"));
    assert.ok(!JSON.stringify(telemetry).includes("secret-value"));
    assert.ok(telemetry.find(e => e.message.includes("Telemetry browser test")).extra.stack.includes("Error"));
    assert.equal((await fetch(`${base.replace(/\/$/, "")}/api/telemetry`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ kind: "test", message: "test", wallet: "unexpected" }) })).status, 422);
    assert.equal((await fetch(`${base.replace(/\/$/, "")}/api/image?url=${encodeURIComponent("http://127.0.0.1/private")}`)).status, 400);
    console.log("PASS: client console/uncaught/rejection and server failures reach telemetry; duplicates and secrets removed; image bridge rejects private origins.");
    const headers = (await fetch(base)).headers;
    assert.equal(headers.get("cross-origin-opener-policy"), "same-origin-allow-popups");
    assert.ok(headers.get("content-security-policy").includes("script-src 'self' 'wasm-unsafe-eval';"));
    await walletPage.route("https://security-check.livekit.cloud/probe", route => route.fulfill({body:"ok",headers:{"access-control-allow-origin":"*"}}));
    await walletPage.routeWebSocket("wss://security-check.livekit.cloud/probe", ws => ws.onMessage(() => {}));
    const policy = await walletPage.evaluate(async () => {
      window.cspProbeRan = false;
      const script = document.createElement("script"); script.textContent = "window.cspProbeRan = true"; document.head.append(script);
      let blocked = false; try { await fetch("https://blocked.invalid/probe"); } catch { blocked = true; }
      const livekit = await fetch("https://security-check.livekit.cloud/probe").then(r => r.text());
      const websocket = await new Promise(resolve => { const socket = new WebSocket("wss://security-check.livekit.cloud/probe"); socket.onopen = () => { socket.close(); resolve(true); }; socket.onerror = () => resolve(false); });
      const wasm = await WebAssembly.compile(new Uint8Array([0,97,115,109,1,0,0,0])).then(()=>true);
      let jsEvalBlocked=false; try { new Function("return 1")(); } catch {jsEvalBlocked=true;}
      return {wasm, jsEvalBlocked, inlineRan:window.cspProbeRan, blocked, livekit, websocket, microphone:document.featurePolicy.allowsFeature("microphone"), camera:document.featurePolicy.allowsFeature("camera")};
    });
    assert.deepEqual(policy,{wasm:true,jsEvalBlocked:true,inlineRan:false,blocked:true,livekit:"ok",websocket:true,microphone:true,camera:false});
    for(let i=0;i<50&&!telemetry.some(e=>e.exception?.values?.[0]?.type==="csp");i++)await delay(100);
    assert.ok(telemetry.some(e=>e.exception?.values?.[0]?.type==="csp"));
    console.log("PASS: CSP blocks inline scripts and unapproved connections, permits LiveKit HTTP/WebSocket, preserves microphone permissions, and reports violations.");
    await walletPage.close();
    console.log("PASS: fresh wallet connection, cancellation/retry, account-change race, one-time delegation, repeated sends, reload, silent read renewal, expiry, disconnect, and missing wallet.");
    const page = await browser.newPage({
      viewport: { width: 1440, height: 1000 },
    });
    await page.route("**/api/events?*", route => route.fulfill({json:{data:[]}}));
    await page.route("**/api/worlds?*", route => route.fulfill({json:{data:[],total:0}}));
    await page.route("**/api/image?*", route => route.fulfill({ contentType: "image/png", body: Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aD1sAAAAASUVORK5CYII=", "base64") }));
    await page.route("**/api/locations?*", route => route.fulfill({json:{locations:{[managedWallet]:{x:12,y:34,updatedAt:Date.now()}}}}));
    await page.routeWebSocket("wss://rpc-social-service-ea.decentraland.org/**", ws => ws.close());
    page.on("pageerror", (e) => errors.push(e.message));
    await page.exposeFunction("signTestRequest", async (prepared) => {
      if (rejectNextMessage && prepared.operation.type === "send_message") {
        rejectNextMessage = false;
        throw new Error("Signature declined");
      }
      return [
        { type: "SIGNER", payload: account.address, signature: "" },
        {
          type: "ECDSA_SIGNED_ENTITY",
          payload: prepared.payload,
          signature: await account.signMessage({ message: prepared.payload }),
        },
      ];
    });
    await page.addInitScript((address) => {
      window.dclSocialIdentity = {
        address,
        signRequest: (p) => window.signTestRequest(p),
        canSignSilently: true,
      };
    }, account.address);
    // Public enrichment is fixture-controlled too, so tests do not depend on WAN.
    await page.route("**/api/profiles/*", (route) =>
      route.fulfill({ json: { avatars: [{ name: "Builder" }] } }),
    );
    await page.route("**/api/places?*", (route) =>
      route.fulfill({
        json: {
          data: [
            { id: hangoutId, title: "Genesis Plaza", base_position: "0,0" },
          ],
        },
      }),
    );
    await page.route("**/api/place/*",route=>{const id=new URL(route.request().url()).pathname.split("/").pop(); if(id==="unlisted.dcl.eth")return route.fulfill({status:404,json:{error:"World details unavailable"}}); if(id==="embedded.dcl.eth")throw Error("Embedded metadata should not be fetched again");return route.fulfill({json:{data:id==="tophub.dcl.eth"?{id,title:"TOPHUB",world:true,world_name:id,base_position:"0,0"}:{id:hangoutId,title:"Builders Garden",base_position:"12,34",description:"Our community meeting place"}}});});
    await page.goto(`${base}#/discover`);
    await page.getByRole("heading",{name:"Live now",exact:true}).waitFor();
    await page.getByText("3 people in voice",{exact:true}).waitFor();
    await page.getByRole("button",{name:"Open voice chat",exact:true}).click();
    await page.waitForURL(`**/c/${community}/voice`);
    await page.getByRole("heading",{name:"Live now",exact:true}).waitFor({state:"hidden"});
    await page.evaluate(()=>{location.hash="#/home";});
    await page.locator(".social-home").waitFor();
    await page.getByRole("heading",{name:"Live now",exact:true}).waitFor({state:"hidden"});
    await page.evaluate(()=>{location.hash="#/discover";});

    await page.locator(".rail").getByRole("button",{name:"Genesis Builders",exact:true}).waitFor();
    assert.equal(await page.locator(".rail").getByRole("button",{name:"Popular public community",exact:true}).count(),0);
    await page.getByRole("heading", { name: "Genesis Builders" }).waitFor();
    await page
      .getByRole("button", { name: /Genesis Builders A place/ })
      .click();
    await page.getByRole("textbox", { name: "Message", exact: true }).waitFor();
    assert.ok(page.url().endsWith(`/c/${community}/general`));
    const composer = page.getByRole("textbox", {
      name: "Message",
      exact: true,
    });
    let actionCount = 0;
    page.on("request", (r) => {
      if (r.url().endsWith("/api/actions")) actionCount++;
    });
    await composer.press("Enter");
    await delay(150);
    assert.equal(actionCount, 0, "empty Enter must not request a signature");
    await composer.fill("keep this draft");
    await page.getByRole("button", { name: /announcements/ }).click();
    await page.getByRole("button", { name: "Publish announcement" }).waitFor();
    await composer.fill("announcement draft");
    await page.getByRole("button", { name: /# general/ }).click();
    await page.waitForFunction(
      () => document.querySelector("textarea")?.value === "keep this draft",
    );
    await page.goBack();
    await page.waitForFunction(
      () => document.querySelector("textarea")?.value === "announcement draft",
    );
    await page.goForward();
    await page.waitForFunction(
      () => document.querySelector("textarea")?.value === "keep this draft",
    );
    await page.getByRole("button", { name: "Refresh conversation" }).click();
    await page.waitForFunction(
      () => !document.querySelector("textarea")?.disabled,
    );
    assert.equal(await composer.inputValue(), "keep this draft");
    await page
      .getByRole("button", { name: "Share a scene", exact: true })
      .click();
    await page.getByRole("textbox", { name: "Find a scene" }).fill("-151, 0");
    assert.equal(
      await page.getByRole("button", { name: /Parcel -151/ }).count(),
      0,
    );
    await page.keyboard.press("Escape");
    await page.getByRole("dialog").waitFor({ state: "hidden" });
    assert.equal(await composer.inputValue(), "keep this draft");
    await page
      .getByRole("button", { name: "Share a scene", exact: true })
      .click();
    await page.getByRole("textbox", { name: "Find a scene" }).fill("-150, 150");
    await page.getByRole("button", { name: /Parcel -150, 150/ }).click();
    await page.getByRole("button",{name:"Share scene",exact:true}).click();
    await page.getByRole("button", { name: "Remove scene" }).click();
    assert.equal(await page.locator(".attachment").count(), 0);
    rejectNextMessage = true;
    await page
      .getByRole("button", { name: "Send message", exact: true })
      .click();
    await page
      .getByRole("alert")
      .filter({ hasText: "Signature declined" })
      .waitFor();
    assert.equal(await composer.inputValue(), "keep this draft");
    await page.getByRole("button", { name: "Dismiss error" }).click();

    await page
      .getByRole("textbox", { name: "Message", exact: true })
      .fill("hello from the real API");
    await page
      .getByRole("button", { name: "Send message", exact: true })
      .click();
    await page
      .locator(".message p")
      .filter({ hasText: "hello from the real API" })
      .waitFor();
    await page
      .getByRole("button", { name: "Share a scene", exact: true })
      .click();
    await page.getByRole("textbox", { name: "Find a scene" }).fill("Genesis");
    await page.getByRole("button", { name: /Genesis Plaza/ }).click();
    await page.getByRole("button",{name:"Share scene",exact:true}).click();
    await page
      .getByRole("button", { name: "Send message", exact: true })
      .click();
    await page.getByRole("link", { name: /Explore together/ }).waitFor();
    assert.ok(
      (
        await page
          .getByRole("link", { name: /Explore together/ })
          .getAttribute("href")
      ).includes("position=0,0"),
    );
    await page.getByRole("button", { name: /announcements/ }).click();
    await page.getByRole("button", { name: "Publish announcement" }).waitFor();
    await page
      .getByRole("textbox", { name: "Message", exact: true })
      .fill("Foundation relay test");
    await page.getByRole("button", { name: "Publish announcement" }).click();
    await page
      .locator(".post p")
      .filter({ hasText: "Foundation relay test" })
      .waitFor();
    assert.equal(posts[0].content, "Foundation relay test");
    await page.getByRole("button", { name: /# general/ }).click();
    await page
      .locator(".message p")
      .filter({ hasText: "hello from the real API" })
      .waitFor();
    const firstMessage = page.locator(".message").first();
    await firstMessage.hover();
    await firstMessage.getByRole("button", {name:"React with heart", exact:true}).click();
    await firstMessage.locator(".reactions button[aria-pressed=true]").waitFor();
    await firstMessage.getByRole("button", {name:"Reply to message", exact:true}).click();
    await page.getByRole("textbox", {name:"Reply",exact:true}).fill("Let's meet at the plaza.");
    await page.getByRole("button", {name:"Send reply",exact:true}).click();
    await page.locator(".panel-message").filter({hasText:"Let's meet at the plaza."}).waitFor();
    if (process.env.SOCIAL_SCREENSHOTS) { mkdirSync(process.env.SOCIAL_SCREENSHOTS,{recursive:true}); await page.screenshot({path:path.join(process.env.SOCIAL_SCREENSHOTS,"thread.png")}); }
    await page.getByRole("button", {name:"Close panel",exact:true}).click();
    await firstMessage.hover();
    await firstMessage.getByRole("button", {name:"Pin message",exact:true}).click();
    await page.locator(".pinned-strip").waitFor();
    await page.getByRole("button", {name:"Community members",exact:true}).click();
    await page.locator(".member-row").first().waitFor();
    await page.locator(".member-row .avatar").first().click();
    await page.getByRole("dialog", {name:"Profile",exact:true}).waitFor();
    await page.getByRole("button", {name:"Close dialog",exact:true}).click();
    await page.getByRole("button", {name:"Close panel",exact:true}).click();
    await page.getByRole("button",{name:"Community members",exact:true}).click();
    await page.getByText("Online in Explorer",{exact:false}).waitFor();
    assert.equal(await page.locator(".member-row").first().locator(".member-presence.is-online").count(),1);
    assert.equal(await page.getByRole("link",{name:"Join \u2197",exact:true}).getAttribute("href"),"https://decentraland.org/jump/?position=12,34");
    assert.equal(await page.getByText("Status unavailable",{exact:true}).count(),1);
    await page.getByRole("button",{name:`Manage member ${managedWallet}`,exact:true}).click();
    await page.getByLabel("Role",{exact:true}).selectOption("moderator");
    await page.getByRole("button",{name:"Save changes",exact:true}).click();
    await page.getByRole("dialog",{name:"Manage member",exact:true}).waitFor({state:"hidden"});
    assert.equal(managedRole,"moderator");
    await page.getByRole("button",{name:`Manage member ${managedWallet}`,exact:true}).click();
    await page.getByRole("button",{name:"Ban member",exact:true}).click();
    await page.getByRole("button",{name:"Confirm ban",exact:true}).click();
    await page.getByRole("dialog",{name:"Manage member",exact:true}).waitFor({state:"hidden"});
    assert.equal(banned,true);
    await page.getByRole("button",{name:"Close panel",exact:true}).click();
    await page.locator(".community-heading").click();
    await page.getByRole("button",{name:"Banned members",exact:true}).click();
    await page.getByRole("button",{name:"Unban",exact:true}).click();
    await page.getByText("No banned members.",{exact:true}).waitFor();
    assert.equal(banned,false);
    await page.getByRole("button",{name:"Close panel",exact:true}).click();
    await page.locator(".community-heading").click();
    await page.getByRole("button",{name:"Invite people",exact:true}).click();
    await page.getByRole("textbox",{name:"Invite wallet",exact:true}).fill(managedWallet);
    await page.getByRole("button",{name:"Send invitation",exact:true}).click();
    await page.getByText("Invitation sent.",{exact:true}).waitFor();
    assert.equal(invited,true);
    await page.getByRole("button",{name:"Close dialog",exact:true}).click();
    await page.getByRole("button", {name:"Search conversation",exact:true}).click();
    await page.getByRole("textbox", {name:"Search messages",exact:true}).fill("Let's meet at the plaza.");
    await page.locator(".search-result").waitFor();
    await page.getByRole("button", {name:"Close panel",exact:true}).click();
    await page.getByRole("button",{name:"Create channel",exact:true}).click();
    await page.getByRole("textbox",{name:"Channel name",exact:true}).fill("world-building");
    await page.getByRole("dialog",{name:"Create channel",exact:true}).getByRole("button",{name:"Create channel",exact:true}).click();
    await page.getByRole("textbox",{name:"Message",exact:true}).waitFor();
    await page.waitForURL(`**/world-building`);
    await page.getByRole("textbox",{name:"Message",exact:true}).fill("This stays in world-building");
    await page.getByRole("button",{name:"Send message",exact:true}).click();
    await page.locator(".message p").filter({hasText:"This stays in world-building"}).waitFor();
    await page.locator(".community-heading").click();
    await page.getByRole("button",{name:"Channel settings",exact:true}).click();
    await page.getByLabel("Slow mode",{exact:true}).selectOption("10");
    await page.getByRole("button",{name:"Save channel",exact:true}).click();
    await page.getByRole("dialog",{name:"Channel settings",exact:true}).waitFor({state:"hidden"});
    const renewed = page.waitForRequest(r=>r.url().endsWith('/api/actions')&&r.postDataJSON()?.operation?.type==='open_community');
    await page.evaluate(()=>{const original=Date.now;window.restoreChatClock=()=>{Date.now=original;};Date.now=()=>original()+100000;});
    const renewal=await renewed;
    assert.equal(renewal.postDataJSON().operation.channel,'world-building');
    await page.waitForResponse(r=>r.url().includes('/complete')&&r.status()===200);
    await page.evaluate(()=>window.restoreChatClock());
    assert.equal(await page.locator(".message p").filter({hasText:"hello from the real API"}).count(),0);
    await page.getByRole("button",{name:/# general/}).click();
    await page.waitForURL(`**/general`);
    await page.locator(".message p").filter({hasText:"hello from the real API"}).waitFor();
    assert.equal(await page.locator(".message p").filter({hasText:"This stays in world-building"}).count(),0);
    await page.locator(".community-heading").click();
    await page.getByRole("button",{name:"Join requests",exact:true}).click();
    await page.getByRole("button",{name:"Accept",exact:true}).click();
    await page.getByRole("dialog",{name:"Join requests",exact:true}).getByText("You're all caught up.").waitFor();
    assert.equal(pendingApproval,false);
    await page.getByRole("button",{name:"Close dialog",exact:true}).click();
    await page.locator(".community-heading").click();
    await page.getByRole("button",{name:"Community settings",exact:true}).click();
    await page.getByLabel("Community picture",{exact:true}).setInputFiles({name:"invalid.svg",mimeType:"image/svg+xml",buffer:Buffer.from('<svg>'+" ".repeat(1200)+'</svg>')});
    await page.getByText("Choose a PNG, JPEG, GIF or WebP picture between 1 KB and 500 KB.",{exact:true}).waitFor();
    const imageData=await page.evaluate(()=>{const c=document.createElement('canvas');c.width=128;c.height=128;const ctx=c.getContext('2d');const image=ctx.createImageData(128,128);for(let i=0;i<image.data.length;i+=4){image.data[i]=i%251;image.data[i+1]=(i*17)%255;image.data[i+2]=(i*31)%251;image.data[i+3]=255;}ctx.putImageData(image,0,0);return c.toDataURL('image/png').split(',')[1];});
    uploadedPicture=Buffer.from(imageData,'base64');assert.ok(uploadedPicture.length>=1024);
    await page.getByLabel("Community picture",{exact:true}).setInputFiles({name:"community.png",mimeType:"image/png",buffer:uploadedPicture});
    await page.getByAltText("Community picture preview",{exact:true}).waitFor();
    await page.getByRole("button",{name:"Save changes",exact:true}).click();
    await page.getByRole("dialog",{name:"Community settings",exact:true}).waitFor({state:"hidden"});
    assert.equal(communityUpdated,true);
    assert.equal(pictureUpdated,true);
    await page.getByRole("button",{name:"Hangouts",exact:true}).click();
    const hangoutDialog=page.getByRole("dialog",{name:"Community hangouts",exact:true});
    await hangoutDialog.getByText("No hangouts added yet.",{exact:true}).waitFor();
    await hangoutDialog.getByLabel("Find a community hangout",{exact:true}).fill("Genesis");
    await hangoutDialog.getByRole("button",{name:"Add hangout",exact:true}).click();
    await hangoutDialog.getByRole("heading",{name:"Builders Garden",exact:true}).waitFor();
    assert.deepEqual(hangouts,[hangoutId]);
    assert.equal(await hangoutDialog.getByRole("link",{name:"Visit \u2197",exact:true}).getAttribute("href"),"https://decentraland.org/jump/?position=12,34");
    await hangoutDialog.getByRole("button",{name:"Remove",exact:true}).click();
    await hangoutDialog.getByText("No hangouts added yet.",{exact:true}).waitFor();
    assert.deepEqual(hangouts,[]);
    await page.getByRole("button",{name:"Close dialog",exact:true}).click();
    hangouts=["tophub.dcl.eth","embedded.dcl.eth","unlisted.dcl.eth"];
    await page.getByRole("button",{name:"Hangouts",exact:true}).click();
    await hangoutDialog.getByRole("heading",{name:"TOPHUB",exact:true}).waitFor();
    await hangoutDialog.getByRole("heading",{name:"Embedded World",exact:true}).waitFor();
    await hangoutDialog.getByRole("heading",{name:"unlisted.dcl.eth",exact:true}).waitFor();
    const worldLinks=await hangoutDialog.getByRole("link",{name:"Visit \u2197",exact:true}).evaluateAll(links=>links.map(a=>a.getAttribute('href')));
    assert.deepEqual(worldLinks,["https://decentraland.org/jump/?realm=tophub.dcl.eth","https://decentraland.org/jump/?realm=embedded.dcl.eth","https://decentraland.org/jump/?realm=unlisted.dcl.eth"]);
    await page.getByRole("button",{name:"Close dialog",exact:true}).click();
    hangouts=[];

    console.log("PASS: reactions, replies, moderator pins, member profiles, conversation search, channel isolation, join approval and owner settings.");
    for (const width of [320, 390, 760, 1440]) {
      await page.setViewportSize({ width, height: 844 });
      assert.equal(
        await page.evaluate(
          () => document.documentElement.scrollWidth > innerWidth,
        ),
        false,
        `overflow at ${width}`,
      );
      if (width <= 760) {
        assert.equal(await page.locator(".appbar").isVisible(), false);
        await page.getByRole("button", { name: "Open navigation" }).click();
        assert.equal(await page.locator("main").getAttribute("inert"), "");
        await page.keyboard.press("Shift+Tab");
        assert.equal(
          await page.evaluate(
            () => !!document.activeElement?.closest("#navigation"),
          ),
          true,
        );
        await page.keyboard.press("Escape");
        assert.equal(
          await page
            .getByRole("button", { name: "Open navigation" })
            .getAttribute("aria-expanded"),
          "false",
        );
        await page.getByRole("button", { name: "Open navigation" }).click();
        await page
          .getByRole("button", { name: "Close navigation", exact: true })
          .last()
          .click();
      }
    }
    await page.screenshot({
      path: path.join(dir, "desktop.png"),
      fullPage: true,
    });
    if (process.env.SOCIAL_SCREENSHOTS) {
      mkdirSync(process.env.SOCIAL_SCREENSHOTS, { recursive: true });
      copyFileSync(
        path.join(dir, "desktop.png"),
        path.join(
          process.env.SOCIAL_SCREENSHOTS,
          `chat${prefix ? "-prefix" : ""}.png`,
        ),
      );
      await page.setViewportSize({ width: 390, height: 844 });
      await page.screenshot({
        path: path.join(process.env.SOCIAL_SCREENSHOTS, "chat-mobile.png"),
      });
      await page.setViewportSize({ width: 1440, height: 1000 });
    }
    await page
      .getByRole("button", { name: "Explore communities", exact: true })
      .click();
    await page.evaluate(() => { location.hash = "#/mine"; });
    await page.getByRole("heading", { name: "Your communities." }).waitFor();
    await page
      .getByRole("button", { name: /Genesis Builders A place/ })
      .waitFor();
    await page.getByRole("link", { name: "Explore communities", exact: true }).click();
    await page.getByRole("heading", { name: "Explore communities" }).waitFor();
    await page
      .getByRole("button", { name: /Genesis Builders A place/ })
      .click();
    await composer.waitFor();
    await page.reload();
    await composer.waitFor();
    await page
      .locator(".message p")
      .filter({ hasText: "hello from the real API" })
      .waitFor();
    await page.evaluate(() => {
      window.dclSocialIdentity.canSignSilently = false;
    });
    const expireRead = async (route) => {
      const response = await route.fetch();
      const data = await response.json();
      if (data.expiresAt) data.expiresAt = Date.now() + 100;
      await route.fulfill({ response, json: data });
    };
    await page.route("**/api/actions/*/complete", expireRead);
    await page.getByRole("button", { name: "Refresh conversation" }).click();
    await page
      .getByRole("button", { name: "Refresh chat", exact: true })
      .waitFor();
    assert.equal(await composer.isDisabled(), true);
    assert.equal(
      await page
        .getByRole("button", { name: "Send message", exact: true })
        .isDisabled(),
      true,
    );
    await page.unroute("**/api/actions/*/complete", expireRead);
    await page
      .getByRole("button", { name: "Refresh chat", exact: true })
      .click();
    await page.waitForFunction(
      () => !document.querySelector("textarea")?.disabled,
    );
    await page.evaluate(() => {
      window.dclSocialIdentity.canSignSilently = true;
    });
    member = false;
    await page
      .getByRole("alert")
      .filter({
        hasText: "Join this community in Decentraland to open its chat",
      })
      .waitFor({ timeout: 10000 });
    assert.equal(
      await page
        .locator(".message p")
        .filter({ hasText: "hello from the real API" })
        .count(),
      0,
    );
    await page.getByRole("button", { name: "Join community", exact: true }).click();
    await composer.waitFor();
    assert.equal(member, true);
    member = false;
    communityData.privacy = "private";
    await page.reload();
    await page.getByRole("button", { name: "Request to join", exact: true }).click();
    await page.getByRole("button", { name: "Request sent", exact: true }).waitFor();
    assert.equal(joinRequested, true);
    await page.reload();
    await page.getByRole("button", { name: "Request sent", exact: true }).waitFor();
    assert.equal(await page.getByRole("button", { name: "Request sent", exact: true }).isDisabled(), true);
    await page.getByRole("button", { name:"Cancel request", exact:true }).click();
    await page.getByRole("button", { name:"Request to join", exact:true }).waitFor();
    assert.equal(joinRequested,false);
    await page.evaluate(()=>{location.hash="#/invitations";});
    await page.getByRole("button",{name:"Sent requests",exact:true}).waitFor();
    await page.locator(".community-invitation").filter({hasText:"Another invitation"}).getByRole("button",{name:"Decline",exact:true}).click();
    await page.waitForFunction(()=>!document.querySelector(".community-invitations")?.textContent.includes("Another invitation"));
    assert.equal(inboxInvitations.get(inviteDecline),"rejected");
    await page.locator(".community-invitation").filter({hasText:"Builders invitation"}).getByRole("button",{name:"Accept invitation",exact:true}).click();
    await composer.waitFor();
    assert.equal(inboxInvitations.get(inviteAccept),"accepted");
    assert.equal(member,true);
    console.log("PASS: public community joining, private join requests/cancellation, invitation accept/decline, pending status after reload, persistent wallet session across browser contexts.");
    await page.evaluate(() => {
      window.dclSocialIdentity = undefined;
      window.dispatchEvent(new Event("dcl:identity-changed"));
    });
    await page.getByRole("button", { name: "Connect wallet", exact: true }).first().waitFor();
    await page.getByRole("link", { name: "Browse communities" }).click();
    await page.getByRole("heading", { name: "Explore communities" }).waitFor();
    assert.equal(
      await page.getByRole("textbox", { name: "Message", exact: true }).count(),
      0,
    );
    assert.deepEqual(errors, []);
    console.log(
      "PASS: signed sends, announcements, drafts, Back/Forward, deep links, scene controls, signature rejection, mobile focus, expired reads, and membership revocation.",
    );
  } finally {
    await browser?.close();
    backend?.kill("SIGTERM");
    if (proxy) await new Promise((resolve) => proxy.close(resolve));
    await new Promise((resolve) => fixture.close(resolve));
    rmSync(dir, { recursive: true, force: true });
  }
})().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
