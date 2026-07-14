// Emoji dataset ported from the SDK7 scene (emojis_complete.json). Emojis are
// plain Unicode inserted into the message string — no engine API involved. We use
// it for the grid picker and for ":shortcode:" autocomplete in the chat input.
// The 716KB JSON is dynamically imported on first use so it stays out of the
// boot chunk; callers kick off loadEmojiData() when the picker opens or a
// shortcode is typed, then re-query the sync accessors once it resolves.

export interface Emoji {
  code: string
  emoji: string
  /** ":grinning_face:" — colon-wrapped shortcode. */
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
  /** A representative glyph used as the category tab icon. */
  icon: string
  emojis: Emoji[]
}

// Skin-tone/hair "Component" emojis have no category tab — drop them from the grid.
const CATEGORY_ICONS: Record<string, string> = {
  'Smileys & Emotion': '😀',
  'People & Body': '👋',
  'Animals & Nature': '🐻',
  'Food & Drink': '🍔',
  'Travel & Places': '✈️',
  Activities: '⚽',
  Objects: '💡',
  Symbols: '❤️',
  Flags: '🏳️'
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
            icon: CATEGORY_ICONS[c.name] ?? '⭐',
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

// "Frequently used" — persisted across sessions, most-recent first.
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
    // ignore quota / privacy-mode failures
  }
  return next
}

/** Match a trailing ":word" the user is typing (caret assumed at end). */
export const SHORTCODE_RE = /:([a-z0-9_+-]{2,})$/i

/** Rank emoji whose shortcode starts-with then contains `query` (no colons).
 *  Returns [] until loadEmojiData() has resolved. */
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
