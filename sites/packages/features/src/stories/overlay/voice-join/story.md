---
id: bevy-overlay-voice-join
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Breaking voice into explicit, legible steps (open the widget -> request a
    session -> connect -> talk push-to-talk) increases the share of opened
    voice widgets that reach an active mic-enabled session, even with the
    LiveKit connect simulated.
  because: >-
    The shipped nearby-voice widget is a single opaque "Speak" button with no
    visible session lifecycle, so users are unsure whether voice is connected
    or whether they are heard. Making request -> token -> connect -> talk
    explicit reduces that uncertainty, so more users who open the widget push
    through to an actually-talking state instead of bailing.
metric:
  primary: cl_voice_talk_rate
  numerator: cl_voice_join
  denominator: cl_voice_widget_opened
  guardrails:
    - cl_voice_widget_opened
    - cl_voice_session_failed
    - cl_voice_mute_toggled
experiment:
  key: cl_voice_join
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
        pushToTalk: true
  baseline: 0.45
  mde: 0.05
  min_sample: 4000
decision:
  rule: >-
    Ship if cl_voice_talk_rate improves by at least the MDE with no guardrail
    regression (widget-open volume holds, the session-failed path stays rare
    and recoverable, and mute usage does not spike as a frustration signal);
    otherwise hold.
---

# Join / mute / leave a voice chat (push-to-talk)

The bevy-overlay VoiceChat HUD widget (`/bevy-overlay/voice-join?panel=voice`)
breaks voice into explicit, URL-addressable steps: open the resting (mic-off)
widget, request a private/community session, receive a (simulated) LiveKit
token and connect, enable the mic / push-to-talk, toggle mute, then leave.

- **Primary metric:** `cl_voice_talk_rate` = `cl_voice_join` (mic enabled /
  talking reached) / `cl_voice_widget_opened`.
- **Guardrails:** widget-open volume (`cl_voice_widget_opened`), the
  session-failed path (`cl_voice_session_failed`), and mute churn
  (`cl_voice_mute_toggled`) must stay healthy.
- **Events:** `cl_voice_widget_opened` on entry, `cl_voice_session_requested`
  (`{kind}`) on request, `cl_voice_token_issued` (`{stub:true}`) on the
  simulated token/connect, `cl_voice_join` on mic enable / talk,
  `cl_voice_mute_toggled` (`{muted}`) on each mute toggle,
  `cl_voice_left` on ending the session, `cl_voice_session_failed` on a
  failed connect.

## Data reality (simulated, deferred)

Voice tokens come from catalyrst-comms voice handlers
(`private_messages_token`, `create_private_voice_chat`,
`get_voice_chat_status`, `end_private_voice_chat`) which issue LiveKit JWTs.
Those routes are gated by `voice_auth_layer` (a `COMMS_GATEKEEPER_AUTH_TOKEN`
bearer) and `verify_signed_fetch` requiring a `dcl:explorer`-signed auth chain,
so they are **not browser-reachable from the public edge**
(`catalyst.example.com/comms/status` -> 404). Tokens + mic state normally arrive
via `window.dclBridge` (`GetMicState` / `SetMicEnabled`), not a browser fetch.

So token issuance, the LiveKit connect, and mic state are **SIMULATED** in the
XState wizard (a clearly-noted stub, never a real network call); the wire
shapes are byte-faithful to `catalyrst-comms/src/handlers/voice.rs` and seeded
from the fallback session built into the route loader. The flow, states, and
metrics are real.
