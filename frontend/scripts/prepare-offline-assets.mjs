import { cpSync, existsSync, mkdirSync, rmSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const frontendRoot = resolve(scriptDir, '..')
const sourceDir = resolve(frontendRoot, 'node_modules/vditor/dist')
const targetDir = resolve(frontendRoot, 'public/vendor/vditor/dist')

if (!existsSync(sourceDir)) {
  throw new Error('Vditor dist not found. Run bun install or npm install first.')
}

mkdirSync(resolve(targetDir, '..'), { recursive: true })
rmSync(targetDir, { recursive: true, force: true })
cpSync(sourceDir, targetDir, { recursive: true })

console.log(`Prepared offline Vditor assets at ${targetDir}`)
