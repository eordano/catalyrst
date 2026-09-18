import type { SocialEvent } from "./Events";
const date = (raw: string) =>
  new Date(raw).toISOString().replace(/[-:]|\.\d{3}/g, "");
const escape = (value: string) =>
  value
    .replace(/\\/g, "\\\\")
    .replace(/\r\n|\r|\n/g, "\\n")
    .replace(/;/g, "\\;")
    .replace(/,/g, "\\,");
function fold(line: string) {
  let result = "",
    bytes = 0;
  const encoder = new TextEncoder();
  for (const char of line) {
    const size = encoder.encode(char).length;
    if (bytes + size > 75) {
      result += "\r\n ";
      bytes = 1;
    }
    result += char;
    bytes += size;
  }
  return result;
}
export function calendarFile(event: SocialEvent, location: string) {
  return (
    [
      "BEGIN:VCALENDAR",
      "VERSION:2.0",
      "PRODID:-//dcl.social//Events//EN",
      "CALSCALE:GREGORIAN",
      "BEGIN:VEVENT",
      `UID:${event.id}@events.decentraland.org`,
      `DTSTAMP:${date(new Date().toISOString())}`,
      `DTSTART:${date(event.next_start_at || event.start_at)}`,
      `DTEND:${date(event.next_finish_at || event.finish_at)}`,
      `SUMMARY:${escape(event.name)}`,
      `DESCRIPTION:${escape(event.description)}`,
      `LOCATION:${escape(location)}`,
      `URL:${location}`,
      "END:VEVENT",
      "END:VCALENDAR",
    ]
      .map(fold)
      .join("\r\n") + "\r\n"
  );
}
export function googleCalendar(event: SocialEvent, location: string) {
  const query = new URLSearchParams({
    action: "TEMPLATE",
    text: event.name,
    dates: `${date(event.next_start_at || event.start_at)}/${date(event.next_finish_at || event.finish_at)}`,
    details: event.description,
    location,
  });
  return `https://calendar.google.com/calendar/render?${query}`;
}
export function downloadCalendar(event: SocialEvent, location: string) {
  const url = URL.createObjectURL(
    new Blob([calendarFile(event, location)], {
      type: "text/calendar;charset=utf-8",
    }),
  );
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = `decentraland-${event.id}.ics`;
  anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
