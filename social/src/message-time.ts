const relative = new Intl.RelativeTimeFormat(undefined, { numeric: "always" });

function monthsBetween(date: Date, now: Date) {
  const months = (now.getFullYear() - date.getFullYear()) * 12 + now.getMonth() - date.getMonth();
  return now.getDate() < date.getDate() ? months - 1 : months;
}

/** Older than a month reads as "2 months ago"; newer keeps its date, and today keeps only the time. */
export function messageTime(value: number | string, withTime: boolean, now = new Date()): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  const months = monthsBetween(date, now);
  if (months >= 12) return relative.format(-Math.floor(months / 12), "year");
  if (months >= 1) return relative.format(-months, "month");
  const day = date.toLocaleDateString(undefined, { month: "short", day: "numeric" });
  if (!withTime) return day;
  const time = date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return date.toDateString() === now.toDateString() ? time : `${day}, ${time}`;
}

export function isoTime(value: number | string): string | undefined {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? undefined : date.toISOString();
}

export function fullMessageTime(value: number | string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "" : date.toLocaleString(undefined, { dateStyle: "full", timeStyle: "short" });
}
