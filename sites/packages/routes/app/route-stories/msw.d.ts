declare module "msw" {
  export interface HttpResponseInit {
    status?: number;
    statusText?: string;
    headers?: Record<string, string>;
  }
  export const HttpResponse: {
    json(body: unknown, init?: HttpResponseInit): Response;
    text(body: string, init?: HttpResponseInit): Response;
    error(): Response;
  };
  export type HttpResolverInfo = {
    request: Request;
    params: Record<string, string | readonly string[]>;
  };
  export type HttpResolver = (
    info: HttpResolverInfo,
  ) => Response | undefined | Promise<Response | undefined>;
  export type HttpHandler = { readonly __brand?: "HttpHandler" };
  export const http: Record<
    "all" | "get" | "post" | "put" | "patch" | "delete" | "options" | "head",
    (predicate: string | RegExp, resolver: HttpResolver) => HttpHandler
  >;
  export function delay(ms?: number): Promise<void>;
}
