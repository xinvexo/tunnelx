import type { IngressRule, TunnelResource, TunnelRuntimeInfo } from '@/providers/contract'
import {
  endpointPublicUrls,
  endpointPublicUrl,
  record,
  runtimePublicUrls,
  type RuntimePublicUrl,
} from '@/providers/runtimeDetails'

export interface CpolarEndpoint {
  id: string
  name: string
  proto: 'http' | 'tcp'
  addr: string
  hostname: string
  remoteAddr: string
  enabled: boolean
}

export interface CpolarMetadata {
  authtoken: string
  region: string
  endpoints: CpolarEndpoint[]
}

export type CpolarRuntimePublicUrl = RuntimePublicUrl

function proto(value: unknown): CpolarEndpoint['proto'] {
  return value === 'tcp' ? 'tcp' : 'http'
}

function endpointFrom(value: unknown): CpolarEndpoint {
  const raw = record(value)
  return {
    id: String(raw.id || crypto.randomUUID()),
    name: String(raw.name ?? ''),
    proto: proto(raw.proto),
    addr: String(raw.addr ?? 'localhost:8080'),
    hostname: String(raw.hostname ?? ''),
    remoteAddr: String(raw.remoteAddr ?? ''),
    enabled: raw.enabled !== false,
  }
}

export function cpolarMetadata(resource: TunnelResource): CpolarMetadata {
  const raw = record(resource.metadata)
  const endpoints = Array.isArray(raw.endpoints)
    ? raw.endpoints.map(endpointFrom)
    : resource.ingress.map((rule) => ({
      id: rule.id,
      name: rule.name,
      proto: 'http' as const,
      addr: rule.service || 'localhost:8080',
      hostname: rule.hostname || '',
      remoteAddr: '',
      enabled: rule.enabled,
    }))
  return {
    authtoken: String(raw.authtoken ?? ''),
    region: String(raw.region ?? ''),
    endpoints,
  }
}

export function newCpolarEndpoint(): CpolarEndpoint {
  return {
    id: crypto.randomUUID(),
    name: '',
    proto: 'http',
    addr: 'localhost:8080',
    hostname: '',
    remoteAddr: '',
    enabled: true,
  }
}

export function applyCpolarMetadata(resource: TunnelResource, metadata: CpolarMetadata): TunnelResource {
  const endpoints = metadata.endpoints.map((endpoint) => ({
    ...endpoint,
    name: endpoint.name.trim(),
    addr: endpoint.addr.trim(),
    hostname: endpoint.hostname.trim(),
    remoteAddr: endpoint.remoteAddr.trim(),
  }))
  const ingress: IngressRule[] = endpoints.map((endpoint) => ({
    id: endpoint.id,
    name: endpoint.name,
    hostname: endpoint.hostname || endpoint.name,
    service: endpoint.addr,
    enabled: endpoint.enabled,
    dnsRouted: false,
  }))
  return {
    ...resource,
    ingress,
    metadata: {
      authtoken: metadata.authtoken.trim(),
      region: metadata.region.trim(),
      endpoints,
    },
  }
}

export function cpolarRuntimePublicUrls(runtime?: TunnelRuntimeInfo | null): CpolarRuntimePublicUrl[] {
  return runtimePublicUrls(runtime)
}

export function cpolarEndpointPublicUrl(endpoint: CpolarEndpoint, runtime?: TunnelRuntimeInfo | null): string {
  return endpointPublicUrl(endpoint.name, runtime)
}

export function cpolarEndpointPublicUrls(endpoint: CpolarEndpoint, runtime?: TunnelRuntimeInfo | null): CpolarRuntimePublicUrl[] {
  return endpointPublicUrls(endpoint.name, runtime)
}
