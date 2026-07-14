import { MeshCollider, TextShape, Transform, type Entity } from '@dcl/sdk/ecs'
import { Vector3 } from '@dcl/sdk/math'
import type { GameDef } from '../harness'

const PADS = ['Pad Red', 'Pad Green', 'Pad Blue', 'Pad Yellow']

export const memoryGame: GameDef = {
  id: 'memory-game',
  start(h) {
    const sequence: number[] = []
    let guessIndex = 0
    let showing = false

    const board = h.makeStatusBoard('MEMORY GAME', Vector3.create(8, 3, 12.8))
    const setBoard = (text: string): void => {
      TextShape.getMutable(board).text = text
    }

    const flash = (pad: Entity, after: () => void): void => {
      const t = Transform.getMutable(pad)
      const baseY = t.position.y
      t.position.y = baseY + 0.4
      let elapsed = 0
      const sys = (dt: number): void => {
        elapsed += dt
        if (elapsed >= 0.45) {
          Transform.getMutable(pad).position.y = baseY
          h.removeSystem(sys)
          after()
        }
      }
      h.addSystem(sys, 'pad flash timer')
    }

    const showSequence = (i = 0): void => {
      showing = true
      if (i >= sequence.length) {
        showing = false
        guessIndex = 0
        setBoard('MEMORY GAME\nYour turn \u{2014} round ' + sequence.length)
        return
      }
      const pad = h.byName(PADS[sequence[i]])
      if (pad === null) return
      flash(pad, () => showSequence(i + 1))
    }

    const nextRound = (): void => {
      sequence.push(Math.floor(Math.random() * PADS.length))
      setBoard('MEMORY GAME\nWatch the pads...')
      showSequence()
    }

    PADS.forEach((padName, padIndex) => {
      const pad = h.byName(padName)
      if (pad === null) return
      MeshCollider.setBox(pad)
      h.onPointerDown(pad, padName, () => {
        if (showing || sequence.length === 0) return
        if (sequence[guessIndex] === padIndex) {
          guessIndex += 1
          if (guessIndex >= sequence.length) nextRound()
        } else {
          setBoard(
            'MEMORY GAME\nWrong pad! Score: ' + (sequence.length - 1) + ' \u{2014} starting over'
          )
          sequence.length = 0
          guessIndex = 0
        }
      })
    })

    nextRound()
  },
}
