export const charCounter = (current: number, limit: number) => `(${current} out of ${limit} characters)`;

export const MdMark = ({ d, fill }: { d: string; fill?: boolean }) => (
  <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
    <path d={d} fill={fill ? "currentColor" : "none"} stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);

export const MD_TOOLBAR: { k: string; d: string }[] = [
  { k: "bold", d: "M5 3h4.2a2.4 2.4 0 0 1 0 4.8H5zM5 7.8h4.8a2.6 2.6 0 0 1 0 5.2H5z" },
  { k: "italic", d: "M7 3h5M4 13h5M10 3 6 13" },
  { k: "link", d: "M6.5 9.5 9.5 6.5M7 11l-1.5 1.5a2.5 2.5 0 0 1-3.5-3.5L3.5 7.5M9 5l1.5-1.5a2.5 2.5 0 0 1 3.5 3.5L12.5 8.5" },
  { k: "list", d: "M6 4.5h7M6 8h7M6 11.5h7M3 4.5h.01M3 8h.01M3 11.5h.01" },
  { k: "quote", d: "M4 5h3v4H4zM9 5h3v4H9zM4 9c0 2-1 2.5-2 3M9 9c0 2-1 2.5-2 3" },
  { k: "code", d: "M6 4 2.5 8 6 12M10 4l3.5 4-3.5 4" },
];
