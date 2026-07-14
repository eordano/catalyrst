---
id: client-chat-open
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A one-click chat affordance on the always-on sidebar makes nearby/scene chat
    the primary social entry point, raising the share of HUD sessions that open
    chat.
  because: >-
    Surfacing chat as a persistent, URL-addressable panel (?panel=chat) removes
    the friction of hunting for a chat key; an always-visible bubble invites the
    click, so cl_chat_opened over cl_hud_loaded captures the open-rate lift.
metric:
  primary: cl_chat_open_rate
  guardrails: []
experiment:
  key: client_chat_open
  unit: session
  variants:
    - id: sidebar-chat
      weight: 100
      flags:
        urlAddressable: true
  baseline: 0.0
  mde: 0.05
decision:
  rule: >-
    Single-variant rollout. Ship if cl_chat_opened / cl_hud_loaded clears a
    healthy open-rate and cl_chat_sent (the guardrail conversion to an actual
    message) does not collapse; otherwise revisit the chat affordance.
---

# Client H02 -- Open nearby chat from the HUD

From the loaded HUD, clicking Chat opens the chat panel at the URL-addressable
`/client?panel=chat`. The ui3 ChatWindow renders the nearby-chat surface; with
the engine bridge present it would subscribe to `GetChatStream` and send via
`SendChat`. Absent the bridge (the norm here) it shows the ui3 static
conversation state.

Journey:

1. `/client` (HUD loaded).
2. Click Chat in the Sidebar -> `/client?panel=chat` (URL-addressable).
3. ChatWindow renders; bridge `GetChatStream` subscribed (when present).
4. `cl_chat_opened` fires on panel open.
5. Type + Enter -> bridge `SendChat`; `cl_chat_sent` fires (guardrail).

Primary metric `cl_chat_open_rate` = `cl_chat_opened` / `cl_hud_loaded`,
derivable from the events above.
