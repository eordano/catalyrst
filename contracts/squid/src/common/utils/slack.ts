export interface SlackMessageResponse {
  ok: boolean;
  ts?: string;
  channel?: string;
}

export interface ISlackComponent {
  sendMessage(channel: string, message: string): Promise<SlackMessageResponse>;
}

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
