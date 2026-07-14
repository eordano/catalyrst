import { engine } from '@dcl/sdk/ecs'
import { initAssetPacks } from '@dcl/asset-packs/dist/scene-entrypoint'
import { getSceneInformation } from '~system/Runtime'

import { GAMES } from './games'
import { setGameDebugMode, startGame, type GameDef, type RunningGame } from './harness'

initAssetPacks(engine)

const POLL_INTERVAL_S = 0.4

interface OnePlayMeta {
  template?: unknown
  gated?: unknown
}

export function main(): void {
  boot().catch((e) => {
    console.error('[template-games] boot failed', e)
  })
}

async function boot(): Promise<void> {
  const info = await getSceneInformation({})
  let meta: { one_play?: OnePlayMeta; tags?: unknown } = {}
  try {
    meta = JSON.parse(info.metadataJson || '{}') as typeof meta
  } catch {
  }
  const onePlay = meta.one_play
  const fromTags = Array.isArray(meta.tags) ? meta.tags[0] : undefined
  const templateId =
    typeof onePlay?.template === 'string' && onePlay.template
      ? onePlay.template
      : typeof fromTags === 'string'
        ? fromTags
        : ''
  const game = GAMES[templateId]
  if (game === undefined) {
    console.log(`[template-games] no game for template '${templateId}' \u{2014} idle runtime`)
    return
  }

  if (onePlay?.gated === true) {
    console.log(`[template-games] '${game.id}' gated \u{2014} waiting for play-state`)
    watchPlayState(playStateUrl(info.baseUrl), game)
  } else {
    startGame(game)
  }
}

function playStateUrl(baseUrl: string): string {
  let b = baseUrl.replace(/\/+$/, '')
  while (/\/contents$/i.test(b)) b = b.slice(0, -'/contents'.length).replace(/\/+$/, '')
  return b + '/contents/one-play-state'
}

function watchPlayState(url: string, game: GameDef): void {
  let running: RunningGame | null = null
  let inFlight = false
  let elapsed = POLL_INTERVAL_S
  let polls = 0
  let lastError = ''
  engine.addSystem((dt: number) => {
    elapsed += dt
    if (elapsed < POLL_INTERVAL_S || inFlight) return
    elapsed = 0
    inFlight = true
    polls += 1
    const n = polls
    let status = 0
    fetch(url)
      .then((r) => {
        status = r.status
        return r.ok ? (r.json() as Promise<{ playing?: unknown; debug?: unknown }>) : null
      })
      .then((state) => {
        if (n === 1 || n % 25 === 0) {
          console.log(
            `[template-games] poll #${n} status=${status} url=${url} playing=${state === null ? 'unreadable' : state.playing === true}`
          )
        }
        const playing = state !== null && state.playing === true
        setGameDebugMode(state !== null && state.debug === true)
        if (playing && running === null) {
          running = startGame(game)
        } else if (!playing && running !== null) {
          running.stop()
          running = null
        }
      })
      .catch((e: unknown) => {
        const msg = String(e)
        if (msg !== lastError) {
          lastError = msg
          console.log(`[template-games] poll #${n} fetch failed: ${msg.slice(0, 160)}`)
        }
      })
      .then(() => {
        inFlight = false
      })
  })
}
