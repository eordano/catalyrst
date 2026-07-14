import {
  engine,
  InputAction,
  Name,
  pointerEventsSystem,
  TextShape,
  Transform,
  type Entity,
} from '@dcl/sdk/ecs'
import type { Vector3 } from '@dcl/sdk/math'

export type SystemFn = (dt: number) => void

export interface GameHandle {
  addEntity(name?: string): Entity
  addSystem(fn: SystemFn, debugName?: string): void
  removeSystem(fn: SystemFn): void
  onPointerDown(entity: Entity, hoverText: string, cb: () => void): void
  byName(value: string): Entity | null
  makeStatusBoard(text: string, position: Vector3, name?: string): Entity
}

export interface GameDef {
  id: string
  start(h: GameHandle): void
}

export interface RunningGame {
  stop(): void
}

export function byName(value: string): Entity | null {
  for (const [entity, name] of engine.getEntitiesWith(Name)) {
    if (name.value === value) return entity
  }
  return null
}

export const DEBUG_LOG_PREFIX = 'one-dbg '

interface DebugSystemMeta {
  name: string
  runs: number
  ranThisTick: boolean
}

interface DebugTracker {
  gameId: string
  tick: number
  systems: Map<SystemFn, DebugSystemMeta>
  createdThisTick: number[]
  handlerCount: () => number
}

let debugOn = false
let activeTracker: DebugTracker | null = null

function publishManifest(registry: boolean): void {
  const t = activeTracker
  if (t === null) return
  try {
    console.log(
      DEBUG_LOG_PREFIX +
        JSON.stringify({
          game: t.gameId,
          tick: t.tick,
          systems: [...t.systems.values()].map((m) => ({
            name: m.name,
            ran: m.ranThisTick,
            runs: m.runs,
          })),
          created: t.createdThisTick.slice(),
          handlers: t.handlerCount(),
          ...(registry ? { registry: true } : {}),
        })
    )
  } catch {
  }
}

export function setGameDebugMode(on: boolean): void {
  if (on === debugOn) return
  debugOn = on
  if (debugOn) publishManifest(true)
}

export function startGame(game: GameDef): RunningGame {
  const entities = new Set<Entity>()
  const systems = new Set<SystemFn>()
  const wrappedSystems = new Map<SystemFn, SystemFn>()
  const pointerEntities = new Set<Entity>()
  let stopped = false
  let anonSystems = 0

  const tracker: DebugTracker = {
    gameId: game.id,
    tick: 0,
    systems: new Map(),
    createdThisTick: [],
    handlerCount: () => pointerEntities.size,
  }
  activeTracker = tracker

  const debugReporter: SystemFn = () => {
    tracker.tick += 1
    if (debugOn) publishManifest(false)
    for (const m of tracker.systems.values()) m.ranThisTick = false
    tracker.createdThisTick.length = 0
  }
  engine.addSystem(debugReporter, 1)

  const h: GameHandle = {
    addEntity(name?: string): Entity {
      const e = engine.addEntity()
      if (name !== undefined) Name.createOrReplace(e, { value: name })
      entities.add(e)
      tracker.createdThisTick.push(e as unknown as number)
      return e
    },
    addSystem(fn: SystemFn, debugName?: string): void {
      if (stopped) return
      systems.add(fn)
      const meta: DebugSystemMeta = {
        name: debugName || fn.name || `system ${(anonSystems += 1)}`,
        runs: 0,
        ranThisTick: false,
      }
      const wrapped: SystemFn = (dt) => {
        meta.runs += 1
        meta.ranThisTick = true
        fn(dt)
      }
      tracker.systems.set(fn, meta)
      wrappedSystems.set(fn, wrapped)
      engine.addSystem(wrapped)
    },
    removeSystem(fn: SystemFn): void {
      if (!systems.delete(fn)) return
      const wrapped = wrappedSystems.get(fn) ?? fn
      wrappedSystems.delete(fn)
      tracker.systems.delete(fn)
      try {
        engine.removeSystem(wrapped)
      } catch {
      }
    },
    onPointerDown(entity: Entity, hoverText: string, cb: () => void): void {
      if (stopped) return
      pointerEventsSystem.onPointerDown(
        { entity, opts: { button: InputAction.IA_POINTER, hoverText } },
        () => {
          if (!stopped) cb()
        }
      )
      pointerEntities.add(entity)
    },
    byName,
    makeStatusBoard(text: string, position: Vector3, name = 'Status Board'): Entity {
      const board = h.addEntity(name)
      Transform.create(board, { position })
      TextShape.create(board, { text, fontSize: 3 })
      return board
    },
  }

  console.log(`[template-games] starting ${game.id}`)
  game.start(h)
  if (debugOn) publishManifest(true)

  return {
    stop(): void {
      if (stopped) return
      stopped = true
      if (activeTracker === tracker) activeTracker = null
      try {
        engine.removeSystem(debugReporter)
      } catch {
      }
      for (const fn of systems) {
        try {
          engine.removeSystem(wrappedSystems.get(fn) ?? fn)
        } catch {
        }
      }
      systems.clear()
      wrappedSystems.clear()
      tracker.systems.clear()
      for (const e of pointerEntities) {
        try {
          pointerEventsSystem.removeOnPointerDown(e)
        } catch {
        }
      }
      pointerEntities.clear()
      for (const e of entities) {
        try {
          engine.removeEntity(e)
        } catch {
        }
      }
      entities.clear()
      console.log(`[template-games] stopped ${game.id}`)
    },
  }
}
