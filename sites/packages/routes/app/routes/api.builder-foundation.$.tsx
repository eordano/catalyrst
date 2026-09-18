import { relayFoundation } from "@data/lib/catalyst/builder/foundation-relay.server";

function handle({request,params}:{request:Request;params:Record<string,string|undefined>}) {
  return relayFoundation(request,`/${params["*"]??""}`);
}
export const loader=handle;
export const action=handle;
