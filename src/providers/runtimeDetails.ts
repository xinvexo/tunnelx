import type { TunnelRuntimeInfo } from '@/providers/contract'

export interface RuntimePublicUrl {
  name: string
  proto: string
  publicUrl: string
}

export function record(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null ? value as Record<string, unknown> : {}
}

export function runtimePublicUrls(runtime?: TunnelRuntimeInfo | null): RuntimePublicUrl[] {
  const details = record(runtime?.details)
  const urls = Array.isArray(details.publicUrls) ? details.publicUrls : []
  return urls
    .map((item) => {
      const raw = record(item)
      return {
        name: String(raw.name ?? '').trim(),
        proto: String(raw.proto ?? '').trim(),
        publicUrl: String(raw.publicUrl ?? raw.public_url ?? '').trim(),
      }
    })
    .filter((item) => item.publicUrl)
}

export function endpointPublicUrls(name: string, runtime?: TunnelRuntimeInfo | null, fallbackFirst = false): RuntimePublicUrl[] {
  const urls = runtimePublicUrls(runtime)
  const matched = urls.filter((item) => item.name === name)
  if (matched.length) return matched
  const names = new Set(urls.map((item) => item.name).filter(Boolean))
  if (names.size <= 1) return urls
  return fallbackFirst ? urls : []
}

export function endpointPublicUrl(name: string, runtime?: TunnelRuntimeInfo | null, fallbackFirst = false): string {
  return endpointPublicUrls(name, runtime, fallbackFirst)[0]?.publicUrl ?? ''
}
