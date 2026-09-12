
export interface Emoji {
  code: string
  emoji: string
  expression: string
  category: string
  subcategory: string
}

interface RawCategory {
  name: string
  spriteName: string
  subcategories: string[]
}

export interface EmojiGroup {
  name: string
  icon: string
  emojis: Emoji[]
}

const CATEGORY_ICONS: Record<string, string> = {
  'Smileys & Emotion': '\u{1F600}',
  'People & Body': '\u{1F44B}',
  'Animals & Nature': '\u{1F43B}',
  'Food & Drink': '\u{1F354}',
  'Travel & Places': '\u{2708}\u{FE0F}',
  Activities: '\u{26BD}',
  Objects: '\u{1F4A1}',
  Symbols: '\u{2764}\u{FE0F}',
  Flags: '\u{1F3F3}\u{FE0F}'
}

export interface EmojiData {
  all: Emoji[]
  groups: EmojiGroup[]
  byCode: Map<string, Emoji>
}

let cache: EmojiData | null = null
let pending: Promise<EmojiData> | null = null

export function getEmojiData(): EmojiData | null {
  return cache
}

export function loadEmojiData(): Promise<EmojiData> {
  if (cache) return Promise.resolve(cache)
  if (!pending) {
    pending = import('./emojis_complete.json')
      .then((mod) => {
        const data = mod.default as { emojis: Emoji[]; categories: RawCategory[] }
        const all = data.emojis
        cache = {
          all,
          groups: data.categories.map((c) => ({
            name: c.name,
            icon: CATEGORY_ICONS[c.name] ?? '\u{2B50}',
            emojis: all.filter((e) => e.category === c.name)
          })),
          byCode: new Map(all.map((e) => [e.code, e]))
        }
        return cache
      })
      .catch((err) => {
        pending = null
        throw err
      })
  }
  return pending
}

const RECENTS_KEY = 'dcl-emoji-recents'
const RECENTS_MAX = 18

export function loadRecents(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(RECENTS_KEY) ?? '[]')
    return Array.isArray(v) ? (v as string[]) : []
  } catch {
    return []
  }
}

export function pushRecent(code: string): string[] {
  const next = [code, ...loadRecents().filter((c) => c !== code)].slice(0, RECENTS_MAX)
  try {
    localStorage.setItem(RECENTS_KEY, JSON.stringify(next))
  } catch {
  }
  return next
}

export const SHORTCODE_RE = /:([a-z0-9_+-]{2,})$/i

export function searchByShortcode(query: string, limit = 8): Emoji[] {
  const q = query.toLowerCase()
  if (!q || !cache) return []
  const starts: Emoji[] = []
  const contains: Emoji[] = []
  for (const e of cache.all) {
    const name = e.expression.slice(1, -1)
    if (name.startsWith(q)) starts.push(e)
    else if (name.includes(q)) contains.push(e)
    if (starts.length >= limit) break
  }
  return [...starts, ...contains].slice(0, limit)
}
