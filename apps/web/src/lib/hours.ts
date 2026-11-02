// The server keeps the window in UTC minutes; the page shows local hours.
const tzOffset = new Date().getTimezoneOffset();

export const toUtc = (localHour: number) => (((localHour * 60 + tzOffset) % 1440) + 1440) % 1440;

export const toLocal = (utcMin: number) => Math.floor(((((utcMin - tzOffset) % 1440) + 1440) % 1440) / 60);

export const hh = (h: number) => `${String(h).padStart(2, "0")}:00`;

export const ago = (unix: number) => {
  const s = Math.max(0, Date.now() / 1000 - unix);
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  return `${Math.floor(s / 3600)} h ago`;
};
