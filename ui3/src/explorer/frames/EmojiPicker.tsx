// Emoji picker -- category tabs + search + "Frequently used" + per-category grid,
// matching the Explorer 2.0 emoji panel. Clicking inserts the Unicode glyph and
// records it in recents. Ported/expanded from the SDK7 scene's emoji button.

import { useEffect, useMemo, useState } from 'react'
import {
  getEmojiData,
  loadEmojiData,
  loadRecents,
  pushRecent,
  searchByShortcode,
  type Emoji,
  type EmojiData
} from './emojiData'
import styles from './EmojiPicker.module.css'

function Grid({
  emojis,
  onPick
}: {
  emojis: Emoji[]
  onPick: (e: Emoji) => void
}): React.JSX.Element {
  return (
    <div className={styles.grid}>
      {emojis.map((e) => (
        <button
          key={e.code}
          type="button"
          className={styles.emoji}
          title={e.expression}
          onClick={() => onPick(e)}
        >
          {e.emoji}
        </button>
      ))}
    </div>
  )
}

export function EmojiPicker({
  onPick,
  onClose
}: {
  onPick: (glyph: string) => void
  onClose: () => void
}): React.JSX.Element {
  const [active, setActive] = useState(0)
  const [query, setQuery] = useState('')
  const [recents, setRecents] = useState<string[]>(() => loadRecents())
  const [data, setData] = useState<EmojiData | null>(() => getEmojiData())

  useEffect(() => {
    if (data) return undefined
    let alive = true
    void loadEmojiData().then((d) => {
      if (alive) setData(d)
    })
    return () => {
      alive = false
    }
  }, [data])

  const pick = (e: Emoji): void => {
    onPick(e.emoji)
    setRecents(pushRecent(e.code))
  }

  const q = query.trim()
  const results = useMemo(() => (q && data ? searchByShortcode(q, 80) : []), [q, data])
  const recentEmojis = useMemo(
    () => recents.map((c) => data?.byCode.get(c)).filter((e): e is Emoji => e != null),
    [recents, data]
  )
  const groups = data?.groups ?? []
  const group = groups[active] ?? groups[0]

  return (
    <div className={styles.root} role="dialog" aria-label="Emoji picker">
      <div className={styles.tabs}>
        {groups.map((g, i) => (
          <button
            key={g.name}
            type="button"
            className={`${styles.tab} ${i === active && !q ? styles.tabActive : ''}`.trim()}
            title={g.name}
            onClick={() => {
              setActive(i)
              setQuery('')
            }}
          >
            {g.icon}
          </button>
        ))}
        <button type="button" className={styles.close} title="Close" onClick={onClose}>
          &#xD7;
        </button>
      </div>

      <div className={styles.searchRow}>
        <span className={styles.searchIcon} aria-hidden="true">
          &#x1F50D;
        </span>
        <input
          className={styles.search}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search emoji"
          onKeyDown={(e) => e.stopPropagation()}
        />
      </div>

      <div className={styles.body}>
        {!data ? (
          <div className={styles.noResults}>Loading&#x2026;</div>
        ) : q ? (
          results.length > 0 ? (
            <Grid emojis={results} onPick={pick} />
          ) : (
            <div className={styles.noResults}>No results</div>
          )
        ) : (
          <>
            {recentEmojis.length > 0 && (
              <>
                <div className={styles.sectionHeader}>Frequently used</div>
                <Grid emojis={recentEmojis} onPick={pick} />
              </>
            )}
            {group && (
              <>
                <div className={styles.sectionHeader}>{group.name}</div>
                <Grid emojis={group.emojis} onPick={pick} />
              </>
            )}
          </>
        )}
      </div>
    </div>
  )
}
