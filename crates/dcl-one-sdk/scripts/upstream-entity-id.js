#!/usr/bin/env node
'use strict'

const fs = require('fs')
const fsp = require('fs/promises')
const path = require('path')
const { createRequire } = require('module')

const VENDOR_ROOT =
  process.env.DCL_VENDOR_ROOT || process.cwd()

function usage() {
  console.error('usage: upstream-entity-id.js <sceneDir> [timestampMs] [--entity]')
  process.exit(2)
}

async function main() {
  const args = process.argv.slice(2)
  const dumpEntity = args.includes('--entity')
  const positional = args.filter((a) => a !== '--entity')
  if (positional.length < 1) usage()
  const sceneDir = path.resolve(positional[0])
  const timestamp = positional[1] ? Number(positional[1]) : Date.now()
  if (!Number.isFinite(timestamp)) usage()

  const vreq = createRequire(path.join(VENDOR_ROOT, 'noop.js'))
  const sceneValidations = vreq('@dcl/sdk-commands/dist/logic/scene-validations.js')
  const { DeploymentBuilder } = vreq('dcl-catalyst-client')
  const { EntityType } = vreq('@dcl/schemas')

  const components = {
    fs: {
      readFile: (p, enc) => fsp.readFile(p, enc),
      fileExists: async (p) => {
        try {
          await fsp.access(p)
          return true
        } catch {
          return false
        }
      },
      stat: (p) => fsp.stat(p)
    }
  }

  let sdkVersion = 'unknown'
  try {
    const pkgPath = require.resolve('@dcl/sdk/package.json', { paths: [sceneDir] })
    sdkVersion = JSON.parse(fs.readFileSync(pkgPath, 'utf8')).version ?? 'unknown'
  } catch {}

  const scene = JSON.parse(fs.readFileSync(path.join(sceneDir, 'scene.json'), 'utf8'))
  const sceneJson = { sdkVersion, ...scene }

  const files = await sceneValidations.getFiles(components, sceneDir)
  sceneValidations.validateFilesSizes(files)
  const contentFiles = new Map(files.map((f) => [f.path, new Uint8Array(f.content)]))

  const { entityId, files: entityFiles } = await DeploymentBuilder.buildEntity({
    type: EntityType.SCENE,
    pointers: sceneJson.scene.parcels,
    files: contentFiles,
    metadata: sceneJson,
    timestamp
  })

  console.log(entityId)
  if (dumpEntity) {
    process.stderr.write(Buffer.from(entityFiles.get(entityId)).toString('utf8') + '\n')
  }
}

main().catch((e) => {
  console.error(e && e.stack ? e.stack : String(e))
  process.exit(1)
})
