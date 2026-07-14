export interface SlackMessageResponse {
  ok: boolean;
  ts?: string;
  channel?: string;
}

export interface ISlackComponent {
  sendMessage(channel: string, message: string): Promise<SlackMessageResponse>;
}

// Ops alerting is not wired up in this deployment, so this is a no-op that keeps
// the component's shape. Callers already treat Slack as optional -- they skip it
// when SLACK_BOT_TOKEN/SLACK_SIGNING_SECRET are unset -- and this drops the
// @slack/bolt dependency without touching them.
export function createSlackComponent(_config: {
  botToken: string;
  signingSecret: string;
}): ISlackComponent {
  async function sendMessage(
    channel: string,
    _message: string
  ): Promise<SlackMessageResponse> {
    return { ok: false, channel };
  }

  return { sendMessage };
}
