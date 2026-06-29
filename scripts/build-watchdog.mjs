// 构建 tunnelx-watchdog sidecar，并按 Tauri externalBin 的命名规则
// （<name>-<target-triple>[.exe]）拷贝到 src-tauri/binaries/。
//
// 跨平台（Windows / macOS / Linux）。在 `tauri dev` / `tauri build` 前由
// tauri.conf.json 的 beforeDevCommand / beforeBuildCommand 调用，也可手动运行：
//   pnpm build:watchdog
//
// 交叉编译时设置环境变量指定目标三元组（否则使用本机 host）：
//   WATCHDOG_TARGET_TRIPLE=aarch64-apple-darwin pnpm build:watchdog
import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync, existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const binariesDir = join(root, 'src-tauri', 'binaries')

function hostTriple() {
    const out = execFileSync('rustc', ['-vV'], { encoding: 'utf8' })
    const m = out.match(/^host:\s*(.+)$/m)
    if (!m) throw new Error('无法从 `rustc -vV` 解析 host triple')
    return m[1].trim()
}

// 允许通过环境变量指定目标 triple（交叉编译）；否则用本机 host。
const explicitTriple = process.env.WATCHDOG_TARGET_TRIPLE || process.env.TAURI_ENV_TARGET_TRIPLE
const triple = explicitTriple || hostTriple()
const exeExt = triple.includes('windows') ? '.exe' : ''

const cargoArgs = ['build', '--release', '-p', 'tunnelx-watchdog']
if (explicitTriple) cargoArgs.push('--target', triple)

console.log(`[build-watchdog] cargo ${cargoArgs.join(' ')}`)
execFileSync('cargo', cargoArgs, { cwd: root, stdio: 'inherit' })

const profileDir = explicitTriple
    ? join(root, 'target', triple, 'release')
    : join(root, 'target', 'release')
const builtBin = join(profileDir, `tunnelx-watchdog${exeExt}`)
if (!existsSync(builtBin)) throw new Error(`未找到编译产物: ${builtBin}`)

mkdirSync(binariesDir, { recursive: true })
const dest = join(binariesDir, `tunnelx-watchdog-${triple}${exeExt}`)
copyFileSync(builtBin, dest)
console.log(`[build-watchdog] ${builtBin} -> ${dest}`)
