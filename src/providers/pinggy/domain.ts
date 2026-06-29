import type { IngressRule, TunnelResource, TunnelRuntimeInfo } from '@/providers/contract'
import {
  endpointPublicUrls,
  endpointPublicUrl,
  record,
  runtimePublicUrls,
  type RuntimePublicUrl,
} from '@/providers/runtimeDetails'

export interface PinggyEndpoint {
  id: string
  name: string
  tunnelType: 'http' | 'tcp' | 'udp' | 'tls' | 'tlstcp'
  localAddr: string
  enabled: boolean
}

export type PinggyRuntimePublicUrl = RuntimePublicUrl

export interface PinggyMetadata {
  token: string
  server: string
  serverPort: number
  debuggerPort: number | null
  endpoints: PinggyEndpoint[]
}

function tunnelType(value: unknown): PinggyEndpoint['tunnelType'] {
  if (value === 'tcp' || value === 'udp' || value === 'tls' || value === 'tlstcp') return value
  return 'http'
}

function endpointFrom(value: unknown): PinggyEndpoint {
  const raw = record(value)
  return {
    id: String(raw.id || crypto.randomUUID()),
    name: String(raw.name ?? ''),
    tunnelType: tunnelType(raw.tunnelType),
    localAddr: String(raw.localAddr ?? 'http://localhost:8080'),
    enabled: raw.enabled !== false,
  }
}

export function pinggyMetadata(resource: TunnelResource): PinggyMetadata {
  const raw = record(resource.metadata)
  const endpoints = Array.isArray(raw.endpoints)
    ? raw.endpoints.map(endpointFrom)
    : resource.ingress.map((rule) => ({
      id: rule.id,
      name: rule.name,
      tunnelType: 'http' as const,
      localAddr: rule.service || 'http://localhost:8080',
      enabled: rule.enabled,
    }))
  return {
    token: String(raw.token ?? ''),
    server: String(raw.server ?? 'free.pinggy.io'),
    serverPort: Number(raw.serverPort || 443),
    debuggerPort: raw.debuggerPort == null ? null : Number(raw.debuggerPort),
    endpoints,
  }
}

export function newPinggyEndpoint(): PinggyEndpoint {
  return {
    id: crypto.randomUUID(),
    name: '',
    tunnelType: 'http',
    localAddr: 'http://localhost:8080',
    enabled: true,
  }
}

export function enabledEndpointCount(metadata: PinggyMetadata) {
  return metadata.endpoints.filter((endpoint) => endpoint.enabled).length
}

export function applyPinggyMetadata(resource: TunnelResource, metadata: PinggyMetadata): TunnelResource {
  const endpoints = metadata.endpoints.map((endpoint) => ({
    ...endpoint,
    name: endpoint.name.trim(),
    localAddr: endpoint.localAddr.trim(),
  }))
  const ingress: IngressRule[] = endpoints.map((endpoint) => ({
    id: endpoint.id,
    name: endpoint.name,
    hostname: endpoint.name,
    service: endpoint.localAddr,
    enabled: endpoint.enabled,
    dnsRouted: false,
  }))
  return {
    ...resource,
    ingress,
    metadata: {
      token: metadata.token.trim(),
      server: metadata.server.trim() || 'free.pinggy.io',
      serverPort: metadata.serverPort || 443,
      debuggerPort: metadata.debuggerPort || null,
      endpoints,
    },
  }
}

export function pinggyRuntimePublicUrls(runtime?: TunnelRuntimeInfo | null): PinggyRuntimePublicUrl[] {
  return runtimePublicUrls(runtime)
}

export function pinggyEndpointPublicUrl(endpoint: PinggyEndpoint, runtime?: TunnelRuntimeInfo | null): string {
  return endpointPublicUrl(endpoint.name, runtime, true)
}

export function pinggyEndpointPublicUrls(endpoint: PinggyEndpoint, runtime?: TunnelRuntimeInfo | null): PinggyRuntimePublicUrl[] {
  return endpointPublicUrls(endpoint.name, runtime, true)
}
