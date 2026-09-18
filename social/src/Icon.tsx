const paths = {
  communities: 'M3 3h7v7H3V3ZM14 3h7v7h-7V3ZM3 14h7v7H3v-7ZM14 14h7v7h-7v-7Z',
  home: 'm3 10 9-7 9 7v11h-6v-8H9v8H3V10Z',
  calendar: 'M8 2v4M16 2v4M3 10h18M3 4h18v18H3V4ZM7 14h2M12 14h2M7 18h2',
  search: 'M21 21l-5-5M10 18a8 8 0 1 0 0-16 8 8 0 0 0 0 16',
  people: 'M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8M20 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75',
  chat: 'M21 11.5a8.5 8.5 0 0 1-8.5 8.5H4l-3 2 2-6a8.5 8.5 0 1 1 18-4.5Z',
  globe: 'M21 12a9 9 0 1 0-18 0 9 9 0 0 0 18 0M3 12h18M12 3c5 5 5 13 0 18-5-5-5-13 0-18',
  plus: 'M12 5v14M5 12h14', down: 'm6 9 6 6 6-6', close: 'm6 6 12 12M6 18 18 6',
  send: 'm22 2-7 20-4-9-9-4 20-7ZM22 2 11 13',
  smile: 'M21 12a9 9 0 1 0-18 0 9 9 0 0 0 18 0M8 14s1 3 4 3 4-3 4-3M8 8h.01M16 8h.01',
  pin: 'm16 3 5 5-4 1-4 4 1 4-3 3-3-7-5-3 3-3 4 1 4-4 2-4ZM2 22l6-6',
  reply: 'm9 5-7 7 7 7M2 12h12a7 7 0 0 1 7 7',
  invitation: 'M3 5h18v14H3V5Zm0 0 9 7 9-7M15 18h7M19 15l3 3-3 3',
  bell: 'M18 8a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9M10 21h4',
  settings: 'M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8M9 2h6l1 4 4 1 2 5-3 3v4l-5 3-3-3-4 1-4-5 2-3-1-4 5-2V2Z',
  external: 'M15 3h6v6M21 3 10 14M10 3H3v18h18v-7',
} as const;
export function Icon({ name }: { name: keyof typeof paths }) { return <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={paths[name]} /></svg>; }
