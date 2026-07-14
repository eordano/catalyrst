import type { paths as EventsPaths } from "@ui/generated/catalyst/openapi/events";
import type { paths as PlacesPaths } from "@ui/generated/catalyst/openapi/places";
import type { paths as WorldsPaths } from "@ui/generated/catalyst/openapi/worlds";

export type { EventsPaths, PlacesPaths, WorldsPaths };

export type Method = "get" | "post" | "put" | "patch" | "delete" | "head";

type PathParams<K extends string> = K extends `${string}{${infer P}}${infer R}`
  ? P | PathParams<R>
  : never;

type WithMethod<P, M extends Method> = {
  [K in keyof P & string]: P[K] extends Record<M, unknown> ? K : never;
}[keyof P & string];

function servicePath<P>(prefix: string) {
  return <M extends Method, K extends WithMethod<P, M>>(
    _method: M,
    path: K,
    ...params: PathParams<K> extends never
      ? []
      : [Record<PathParams<K>, string | number>]
  ): string => {
    let out: string = path;
    const map = (params[0] ?? {}) as Record<string, string | number>;
    for (const [key, value] of Object.entries(map)) {
      out = out.replace(`{${key}}`, encodeURIComponent(String(value)));
    }
    return prefix + out;
  };
}

export const eventsApiPath = servicePath<EventsPaths>("/events");
export const placesApiPath = servicePath<PlacesPaths>("/places");
export const worldsApiPath = servicePath<WorldsPaths>("");

type JsonOf<T> = T extends { content: { "application/json": infer J } }
  ? J
  : never;

export type ServiceResponse<
  P,
  K extends keyof P,
  M extends keyof P[K],
  S extends number,
> = P[K][M] extends { responses: infer R }
  ? S extends keyof R
    ? JsonOf<R[S]>
    : never
  : never;
