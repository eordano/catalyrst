const assert = require("node:assert/strict");
const { chromium } = require("playwright");
const path = require("node:path");
(async () => {
  const { createServer } = await import("vite");
  const server = await createServer({
    root: path.resolve(__dirname, ".."),
    server: { host: "127.0.0.1", port: 0, hmr: false, watch: null },
  });
  let browser;
  try {
    await server.listen();
    browser = await chromium.launch({
      headless: true,
      executablePath: process.env.CHROMIUM_PATH || chromium.executablePath(),
      args: ["--no-sandbox"],
    });
    const page = await browser.newPage();
    const own = "0x0000000000000000000000000000000000000001";
    await page.addInitScript((own) => {
      window.dclSocialIdentity = {
        address: own,
        canSignSilently: true,
        signRequest: async () => [],
      };
    }, own);
    let friendGoing = false, createFails = true, worldsFail = false;
    const peer = "0x0000000000000000000000000000000000000002";
    let going = false,
      fail = false;
    const errors = [],
      actions = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.route("**/api/**", (route) => {
      const u = new URL(route.request().url());
      if (u.pathname === "/api/communities")
        return route.fulfill({ json: { data: { results: [], total: 0 } } });
      if (u.pathname === "/api/events")
        return route.fulfill({
          json: {
            data: [
              {
                id: "11111111-1111-4111-8111-111111111111",
                name: "World dance party",
                description: "Meet friends in a world.",
                start_at: "2026-12-20T20:00:00Z",
                finish_at: "2026-12-20T22:00:00Z",
                world: true,
                world_name: "party.dcl.eth",
                total_attendees: 2,
              },
              {
                id: "invalid",
                name: "Invalid date",
                start_at: "bad",
                finish_at: "bad",
              },
            ],
          },
        });
      if (u.pathname === "/api/worlds")
        return route.fulfill({
          ...(worldsFail ? { status: 503 } : {}),
          json: {
            total: 1,
            data: [
              {
                id: "party.dcl.eth",
                world_name: "party.dcl.eth",
                title: "Party World",
                description: "Dance and meet friends",
                user_count: 3,
              },
            ],
          },
        });
      if (u.pathname.match(/^\/api\/events\/[^/]+\/attendees$/))
        return route.fulfill({
          json: {
            data: [
              ...(going ? [{ user: own }] : []),
              ...(friendGoing
                ? [{ user: peer, created_at: "2026-09-16T01:00:00Z" }]
                : []),
            ],
          },
        });
      if (u.pathname.endsWith("/my_events/complete"))
        return route.fulfill({
          json: {
            data: [
              ...(going
                ? [
                    {
                      id: "11111111-1111-4111-8111-111111111111",
                      name: "World dance party",
                      description: "My RSVP",
                      attending: true,
                      start_at: "2026-12-20T20:00:00Z",
                      finish_at: "2026-12-20T22:00:00Z",
                      world: true,
                      world_name: "party.dcl.eth",
                      total_attendees: 2,
                    },
                  ]
                : []),
              {
                id: "22222222-2222-4222-8222-222222222222",
                name: "Not attending this",
                description: "Not my RSVP",
                start_at: "2026-12-20T20:00:00Z",
                finish_at: "2026-12-20T22:00:00Z",
                attending: false,
              },
            ],
          },
        });
      if (u.pathname === "/api/actions") {
        const operation = route.request().postDataJSON().operation;
        actions.push(operation);
        return route.fulfill({
          json: { id: operation.type, operation, payload: "test" },
        });
      }
      if (u.pathname.endsWith("/create_event/complete"))
        return route.fulfill(createFails ? {status:503,json:{error:"Event service unavailable"}} : {json:{data:{id:"33333333-3333-4333-8333-333333333333",approved:false}}});
      if (u.pathname.endsWith("/event_attendees/complete"))
        return route.fulfill({ json: { data: going ? [{ user: own }] : [] } });
      if (u.pathname.endsWith("/event_rsvp/complete")) {
        if (fail)
          return route.fulfill({
            status: 503,
            json: { error: "RSVP unavailable" },
          });
        going = actions.at(-1).attending;
        return route.fulfill({ json: { data: going ? [{ user: own }] : [] } });
      }
      if (u.pathname.includes("my_communities"))
        return route.fulfill({ json: { data: { results: [], total: 0 } } });
      if (u.pathname.includes("private_chat_token"))
        return route.fulfill({
          status: 503,
          json: { error: "Fixture offline" },
        });
      return route.fulfill({ json: { data: [], locations: {} } });
    });
    await page.goto(
      `http://127.0.0.1:${server.httpServer.address().port}/#/events`,
    );
    await page.getByRole("button", {name: "Create an Event", exact: true}).click();
    const editor = page.getByRole("dialog", {name: "Create an Event", exact: true});
    await editor.getByLabel("Name", {exact: true}).fill("Friends gathering");
    await editor.getByLabel("Description", {exact: true}).fill("A night in our World.");
    await editor.getByLabel("Starts", {exact: true}).fill("2026-12-21T20:00");
    await editor.getByLabel("Ends", {exact: true}).fill("2026-12-21T22:00");
    await editor.getByRole("button", {name: /Choose a place or World/}).click();
    await page.getByRole("dialog", {name:"Choose a location"}).getByRole("button", {name:"Worlds",exact:true}).click();
    await page.getByRole("button", {name:/Party World/}).click();
    await page.getByRole("button", {name:"Use this location"}).click();
    await editor.getByRole("button", {name:"Submit event",exact:true}).click();
    await editor.getByText("Event service unavailable", {exact:true}).waitFor();
    assert.equal(await editor.getByLabel("Name", {exact:true}).inputValue(), "Friends gathering");
    createFails = false;
    await editor.getByRole("button", {name:"Submit event",exact:true}).click();
    await editor.getByRole("heading", {name:"Event submitted"}).waitFor();
    const submitted = actions.filter(a=>a.type==="create_event").at(-1).event;
    assert.equal(submitted.scene.world,"party.dcl.eth");
    assert.equal(submitted.duration,7200000);
    assert.equal(submitted.name,"Friends gathering");
    assert.ok(page.url().endsWith("#/events"));
    await editor.getByRole("button", {name:"Done",exact:true}).click();
    await page.getByRole("button", { name: /World dance party/ }).click();
    await page.getByRole("button", { name: "RSVP", exact: true }).click();
    await page.getByRole("button", { name: "Going \u2713 \u00b7 Cancel RSVP" }).waitFor();
    assert.equal(going, true);
    const google = new URL(
      await page
        .getByRole("link", { name: "Google Calendar \u2197" })
        .getAttribute("href"),
    );
    assert.equal(
      google.searchParams.get("dates"),
      "20261220T200000Z/20261220T220000Z",
    );
    const download = page.waitForEvent("download");
    await page.getByRole("button", { name: "Download calendar file" }).click();
    const file = await download;
    const ics = require("node:fs").readFileSync(await file.path(), "utf8");
    assert.ok(ics.includes("DTSTART:20261220T200000Z"));
    assert.ok(
      ics.includes(
        "LOCATION:https://decentraland.org/jump/?realm=party.dcl.eth",
      ),
    );

    assert.equal(
      await page
        .getByRole("dialog")
        .getByRole("link", { name: /Visit world/ })
        .getAttribute("href"),
      "https://decentraland.org/jump/?realm=party.dcl.eth",
    );
    await page.getByRole("button", { name: "Going \u2713 \u00b7 Cancel RSVP" }).click();
    await page.getByRole("button", { name: "RSVP", exact: true }).waitFor();
    assert.equal(going, false);
    fail = true;
    await page.getByRole("button", { name: "RSVP", exact: true }).click();
    await page
      .getByRole("alert")
      .filter({ hasText: "RSVP unavailable" })
      .waitFor();
    await page.getByRole("button", { name: "Close event" }).click();
    assert.equal(
      await page.getByRole("button", { name: /Invalid date/ }).count(),
      0,
    );
    await page.goto(
      `http://127.0.0.1:${server.httpServer.address().port}/#/worlds`,
    );
    await page.getByRole("button", {name: "View Party World"}).click();
    assert.equal(await page.getByRole("dialog").getByRole("link", {name: "Open Decentraland \u2197"}).getAttribute("href"), "decentraland://?realm=party.dcl.eth");
    assert.ok(page.url().endsWith("#/worlds"));
    await page.getByRole("button", {name: "Close dialog"}).click();
    await page.evaluate(async () => {
      const React = (await import("/node_modules/.vite/deps/react.js")).default;
      const { createRoot } = (
        await import("/node_modules/.vite/deps/react-dom_client.js")
      ).default;
      const { ScenePicker } = await import("/src/ScenePicker.tsx");
      const { PlaceCard } = await import("/src/PlaceCard.tsx");
      const host = document.createElement("div");
      document.body.append(host);
      const root = createRoot(host);
      root.render(
        React.createElement(ScenePicker, {
          onClose: () => root.unmount(),
          onSelect: (scene) =>
            root.render(React.createElement(PlaceCard, { scene })),
        }),
      );
    });
    await page
      .getByRole("dialog")
      .getByRole("button", { name: "Worlds", exact: true })
      .click();
    await page.getByRole("dialog").getByRole("button", { name: /Party World/ }).click();
    await page
      .getByRole("button", { name: "Share scene", exact: true })
      .click();
    const shared = page.getByRole("link", {
      name: "Explore together at party.dcl.eth",
    });
    await shared.waitFor();
    assert.equal(
      await shared.getAttribute("href"),
      "https://decentraland.org/jump/?realm=party.dcl.eth",
    );
    await page.goto(
      `http://127.0.0.1:${server.httpServer.address().port}/#/events`,
    );
    going = true;
    const myRequest = page.waitForRequest(
      (r) =>
        r.url().endsWith("/api/actions") &&
        r.postDataJSON()?.operation?.type === "my_events",
    );
    await page.getByRole("link", { name: "My RSVPs", exact: true }).click();
    await myRequest;
    await page.getByRole("button", { name: /World dance party/ }).waitFor();
    assert.ok(actions.some((a) => a.type === "my_events"));
    assert.equal(
      await page.getByRole("button", { name: /Not attending this/ }).count(),
      0,
    );
    going = false;
    await page.evaluate(
      async ({ own, peer }) => {
        const React = (await import("/node_modules/.vite/deps/react.js"))
          .default;
        const { createRoot } = (
          await import("/node_modules/.vite/deps/react-dom_client.js")
        ).default;
        const { EventActivity } = await import("/src/EventActivity.tsx");
        const { EventsPreview } = await import("/src/Events.tsx");
        const host = document.createElement("div");
        host.id = "event-activity-test";
        document.body.append(host);
        window.eventNotices = [];
        window.eventTestRoot = createRoot(host);
        window.eventTestProps = {
          identity: {
            address: own,
            canSignSilently: true,
            signRequest: async () => [],
          },
          friendsState: {
            friends: [{ address: peer, name: "Ada" }],
            ready: true,
            error: "",
          },
        };
        window.eventTestRoot.render(
          React.createElement(
            React.Fragment,
            null,
            React.createElement(EventActivity, {
              ...window.eventTestProps,
              onNotify: (n) => window.eventNotices.push(n),
            }),
            React.createElement(EventsPreview, {
              ...window.eventTestProps,
              onConnect: () => {},
            }),
          ),
        );
      },
      { own, peer },
    );
    await page.waitForFunction(
      (own) =>
        localStorage
          .getItem(`dcl.social.event-activity.${own}`)
          ?.includes("11111111"),
      own,
    );
    assert.equal(await page.evaluate(() => window.eventNotices.length), 0);
    friendGoing = true;
    await page.evaluate(async () => {
      const { invalidateAttendance } = await import("/src/event-attendance.ts");
      invalidateAttendance("11111111-1111-4111-8111-111111111111");
      document.dispatchEvent(new Event("visibilitychange"));
    });
    await page.waitForFunction(() => window.eventNotices.length === 1);
    await page
      .locator("#event-activity-test")
      .getByText("Ada is going", { exact: true })
      .waitFor();
    await page.evaluate(() =>
      document.dispatchEvent(new Event("visibilitychange")),
    );
    assert.equal(await page.evaluate(() => window.eventNotices.length), 1);
    assert.equal(
      await page.evaluate(() => window.eventNotices[0].href),
      "#/events/11111111-1111-4111-8111-111111111111",
    );
    const secondWallet = "0x0000000000000000000000000000000000000003";
    await page.evaluate(async (address) => {
      const React = (await import("/node_modules/.vite/deps/react.js")).default;
      const { EventActivity } = await import("/src/EventActivity.tsx");
      window.eventTestRoot.render(
        React.createElement(EventActivity, {
          ...window.eventTestProps,
          identity: { ...window.eventTestProps.identity, address },
          onNotify: (n) => window.eventNotices.push(n),
        }),
      );
    }, secondWallet);
    await page.waitForFunction(
      (wallet) =>
        localStorage
          .getItem(`dcl.social.event-activity.${wallet}`)
          ?.includes("11111111"),
      secondWallet,
    );
    assert.equal(await page.evaluate(() => window.eventNotices.length), 1);
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/#/worlds`);
    const worldCard = page.locator('.discovery-page').getByText("Party World", { exact: true });
    await worldCard.waitFor();
    await page.clock.install();
    await page.clock.fastForward(31_000);
    worldsFail = true;
    await page.getByRole("link", { name: "Home", exact: true }).click();
    await page.getByRole("link", { name: "Worlds", exact: true }).click();
    await page.locator('.discovery-page').getByText(/Showing saved results/).waitFor();
    assert.equal(await worldCard.count(), 1);
    await page.clock.fastForward(270_000);
    await page.getByText("Worlds could not be loaded.", { exact: false }).waitFor();
    await worldCard.waitFor({ state: 'hidden' });
    assert.deepEqual(errors, []);
    console.log(
      "Discovery: RSVP/calendar/My RSVPs, friends attending, notification baseline/dedup/wallet isolation, world sharing, stale refresh notice and five-minute expiry passed",
    );
  } finally {
    await browser?.close();
    await server.close();
  }
})().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
