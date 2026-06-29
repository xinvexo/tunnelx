// 构建期抽取脚本：扫描源码里用到的离线 Iconify 图标，从对应离线图标集
// 中只取这些图标，生成精简的 IconifyJSON 到 src/icons.generated.ts。
//
// 这样图标完全离线打包（断网/GFW 下照常显示），且只打进实际用到的图标，
// 不引入任何全量图标集。新增图标后重跑本脚本（已接入 prebuild/predev）。
import { readFileSync, writeFileSync, readdirSync, statSync } from 'node:fs'
import { execSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join, resolve } from 'node:path'

const __dirname = dirname(fileURLToPath(import.meta.url))
const root = resolve(__dirname, '..')
const srcDir = join(root, 'src')

const COLLECTIONS = {
  lucide: '@iconify-json/lucide',
  'simple-icons': '@iconify-json/simple-icons',
}

// 定位离线图标集 icons.json。优先标准解析路径；pnpm 严格模式下顶层无符号链接时，
// 用 find 在 .pnpm store 里查（比 readdir 可靠，避免符号链接/沙箱目录视图不一致）。
function locateIconsJson(packageName) {
  const direct = join(root, 'node_modules', packageName, 'icons.json')
  try {
    statSync(direct)
    return direct
  } catch {
    // 回退到 .pnpm store
  }
  try {
    const found = execSync(
      `find "${join(root, 'node_modules/.pnpm')}" -path '*${packageName}/icons.json' -print -quit`,
      { encoding: 'utf8' },
    ).trim()
    if (found) {
      statSync(found)
      return found
    }
  } catch {
    // find 失败则落到下面报错
  }
  throw new Error(`找不到 ${packageName}/icons.json，请先 pnpm install`)
}

// 递归收集源码文件。
function collectSourceFiles(dir) {
  const out = []
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry)
    const st = statSync(full)
    if (st.isDirectory()) {
      out.push(...collectSourceFiles(full))
    } else if (/\.(vue|ts|tsx|js)$/.test(entry)) {
      out.push(full)
    }
  }
  return out
}

// 从源码里抓出支持的 `<prefix>:<name>`（含动态绑定里的字符串字面量）。
function findUsedIcons(files) {
  const icons = new Map(Object.keys(COLLECTIONS).map((prefix) => [prefix, new Set()]))
  const pattern = new RegExp(`(${Object.keys(COLLECTIONS).join('|')}):([a-z0-9-]+)`, 'g')
  for (const file of files) {
    const text = readFileSync(file, 'utf8')
    let m
    while ((m = pattern.exec(text)) !== null) {
      icons.get(m[1])?.add(m[2])
    }
  }
  return Object.fromEntries([...icons.entries()].map(([prefix, names]) => [prefix, [...names].sort()]))
}

const used = findUsedIcons(collectSourceFiles(srcDir))

const collections = []
const missing = []
let usedCount = 0
for (const [prefix, names] of Object.entries(used)) {
  if (!names.length) continue
  const iconsJsonPath = locateIconsJson(COLLECTIONS[prefix])
  const full = JSON.parse(readFileSync(iconsJsonPath, 'utf8'))
  const icons = {}
  for (const name of names) {
    if (full.icons[name]) {
      icons[name] = full.icons[name]
    } else {
      missing.push(`${prefix}:${name}`)
    }
  }
  usedCount += names.length
  collections.push({
    prefix: full.prefix,
    width: full.width,
    height: full.height,
    icons,
  })
}
if (missing.length) {
  console.error(`[build-icons] 警告：离线集中找不到这些图标，将无法显示：${missing.join(', ')}`)
}

// 生成精简 IconifyJSON：保留 prefix 和全局 width/height，逐图标 body。
const banner = `// 本文件由 scripts/build-icons.mjs 自动生成，请勿手动编辑。\n// 离线 Iconify 图标子集（${usedCount} 个），供 main.ts 通过 addCollection 注册。\n`
const content = `${banner}import type { IconifyJSON } from '@iconify/vue'\n\nexport const offlineIconCollections: IconifyJSON[] = ${JSON.stringify(collections)}\n`
const outPath = join(srcDir, 'icons.generated.ts')
writeFileSync(outPath, content)
console.log(`[build-icons] 已写入 ${outPath}：${usedCount} 个图标，缺失 ${missing.length} 个`)
