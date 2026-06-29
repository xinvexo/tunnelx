import type { IngressRule, TunnelResource, TunnelRuntimeInfo } from '@/providers/contract'
import {
  endpointPublicUrls,
  endpointPublicUrl,
  record,
  runtimePublicUrls,
  type RuntimePublicUrl,
} from '@/providers/runtimeDetails'

export interface NgrokEndpoint {
  id: string
  name: string
  proto: 'http' | 'tcp' | 'tls'
  addr: string
  domain: string
  enabled: boolean
}

export interface NgrokMetadata {
  authtoken: string
  region: string
  endpoints: NgrokEndpoint[]
}

export type NgrokRuntimePublicUrl = RuntimePublicUrl

function proto(value: unknown): NgrokEndpoint['proto'] {
  return value === 'tcp' || value === 'tls' ? value : 'http'
}

function endpointFrom(value: unknown): NgrokEndpoint {
  const raw = record(value)
  return {
    id: String(raw.id || crypto.randomUUID()),
    name: String(raw.name ?? ''),
    proto: proto(raw.proto),
    addr: String(raw.addr ?? 'http://localhost:8080'),
    domain: String(raw.domain ?? ''),
    enabled: raw.enabled !== false,
  }
}

export function ngrokMetadata(resource: TunnelResource): NgrokMetadata {
  const raw = record(resource.metadata)
  const endpoints = Array.isArray(raw.endpoints)
    ? raw.endpoints.map(endpointFrom)
    : resource.ingress.map((rule) => ({
      id: rule.id,
      name: rule.name,
      proto: 'http' as const,
      addr: rule.service || 'http://localhost:8080',
      domain: '',
      enabled: rule.enabled,
    }))
  return {
    authtoken: String(raw.authtoken ?? ''),
    region: String(raw.region ?? ''),
    endpoints,
  }
}

export function newNgrokEndpoint(): NgrokEndpoint {
  return {
    id: crypto.randomUUID(),
    name: '',
    proto: 'http',
    addr: 'http://localhost:8080',
    domain: '',
    enabled: true,
  }
}

export function applyNgrokMetadata(resource: TunnelResource, metadata: NgrokMetadata): TunnelResource {
  const endpoints = metadata.endpoints.map((endpoint) => ({
    ...endpoint,
    name: endpoint.name.trim(),
    addr: endpoint.addr.trim(),
    domain: endpoint.domain.trim(),
  }))
  const ingress: IngressRule[] = endpoints.map((endpoint) => ({
    id: endpoint.id,
    name: endpoint.name,
    hostname: endpoint.domain || endpoint.name,
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

export function ngrokRuntimePublicUrls(runtime?: TunnelRuntimeInfo | null): NgrokRuntimePublicUrl[] {
  return runtimePublicUrls(runtime)
}

export function ngrokEndpointPublicUrl(endpoint: NgrokEndpoint, runtime?: TunnelRuntimeInfo | null): string {
  return endpointPublicUrl(endpoint.name, runtime)
}

export function ngrokEndpointPublicUrls(endpoint: NgrokEndpoint, runtime?: TunnelRuntimeInfo | null): NgrokRuntimePublicUrl[] {
  return endpointPublicUrls(endpoint.name, runtime)
}
