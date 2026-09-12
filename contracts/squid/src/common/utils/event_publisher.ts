/* eslint-disable @typescript-eslint/naming-convention */
import { Event } from '@dcl/schemas'

class EventPublisher {
  async publishMessage(_event: Event): Promise<string | undefined> {
    return undefined
  }
}

export default new EventPublisher()
