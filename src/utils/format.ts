/**
 * 共享的格式化工具函数
 */

/**
 * 格式化文件大小（字节 -> 人类可读格式）
 */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(2)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(2)} MB`
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
}

export function formatSpeed(bytesPerSecond: number): string {
  const value = Math.max(0, bytesPerSecond)
  const units = ['B/s', 'KB/s', 'MB/s', 'GB/s', 'TB/s']
  let scaled = value
  let unitIndex = 0
  while (scaled >= 1024 && unitIndex < units.length - 1) {
    scaled /= 1024
    unitIndex += 1
  }
  const digits = scaled >= 100 || unitIndex === 0 ? 0 : scaled >= 10 ? 1 : 2
  return `${scaled.toFixed(digits)} ${units[unitIndex]}`
}

export function formatCompactSpeed(bytesPerSecond: number): string {
  const value = Math.max(0, bytesPerSecond)
  const units = ['B/s', 'K/s', 'M/s', 'G/s', 'T/s']
  let scaled = value
  let unitIndex = 0
  while (scaled >= 1024 && unitIndex < units.length - 1) {
    scaled /= 1024
    unitIndex += 1
  }
  const digits = scaled >= 100 || unitIndex === 0 ? 0 : scaled >= 10 ? 1 : 2
  return `${scaled.toFixed(digits)}${units[unitIndex]}`
}

/**
 * 格式化日期时间
 */
export function formatDate(timestamp: number | string | Date): string {
  const date = new Date(timestamp)
  // 解析失败时返回空串，而不是渲染 "NaN-NaN-NaN NaN:NaN"
  if (Number.isNaN(date.getTime())) return ''
  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  const hours = String(date.getHours()).padStart(2, '0')
  const minutes = String(date.getMinutes()).padStart(2, '0')
  return `${year}-${month}-${day} ${hours}:${minutes}`
}

/**
 * 格式化内存大小（字节 -> MB/GB）
 */
export function formatMemory(bytes: number | null): string {
  if (bytes == null) return '--'
  const mb = bytes / 1024 / 1024
  if (mb < 1024) return `${mb.toFixed(mb >= 100 ? 0 : 1)} MB`
  return `${(mb / 1024).toFixed(2)} GB`
}

/**
 * 格式化时长（毫秒 -> 人类可读）
 */
export function formatDuration(ms: number): string {
  const seconds = Math.floor(ms / 1000)
  const minutes = Math.floor(seconds / 60)
  const hours = Math.floor(minutes / 60)
  const days = Math.floor(hours / 24)

  if (days > 0) return `${days}d ${hours % 24}h`
  if (hours > 0) return `${hours}h ${minutes % 60}m`
  if (minutes > 0) return `${minutes}m ${seconds % 60}s`
  return `${seconds}s`
}
