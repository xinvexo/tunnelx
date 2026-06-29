import type { ServerConfig } from './server'
import { hydrateServerConfig } from './server'
import type { ProfileProxy } from './proxy'
import { hydrateProfileProxy } from './proxy'

export interface Profile {
    id: string
    name: string
    server: ServerConfig
    proxies: ProfileProxy[]
    frpcVersion: string | null
    createdAt: number
    updatedAt: number
}

export interface ProfileSummary {
    id: string
    name: string
    serverAddr: string
    serverPort: number
    proxyCount: number
    enabledProxyCount: number
    frpcVersion: string | null
    updatedAt: number
}

/** 把后端返回的（精简过的）Profile 填充成完整可编辑对象。 */
export function hydrateProfile(raw: Profile): Profile {
    return {
        ...raw,
        server: hydrateServerConfig(raw.server),
        proxies: (raw.proxies ?? []).map(hydrateProfileProxy),
    }
}
