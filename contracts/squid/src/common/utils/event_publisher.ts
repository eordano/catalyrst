/* eslint-disable @typescript-eslint/naming-convention */
import { Event } from '@dcl/schemas'

// The SNS notification pipeline is not part of this deployment: AWS_SNS_ARN is
// never set, so publishing was already dead weight whose only effect was an
// exception the caller swallowed. Publishing is a no-op here, which drops the
// @aws-sdk/client-sns dependency while keeping the call sites intact.
class EventPublisher {
  async publishMessage(_event: Event): Promise<string | undefined> {
    return undefined
  }
}

export default new EventPublisher()
