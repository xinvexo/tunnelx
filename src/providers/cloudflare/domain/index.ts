export type CloudflareAuthMode = 'authorization' | 'apiToken'

export interface CloudflareAccountConfig {
    authMode: CloudflareAuthMode
    apiToken: string
    tokenAccountId: string
    tokenAccountName: string
    authorizationAccountId: string
    authorizationZoneId: string
}

export interface CloudflareIngressRule {
    id: string
    name: string
    hostname: string
    service: string
    enabled: boolean
    dnsRouted: boolean
}

export interface CloudflareTunnel {
    id: string
    name: string
    account: CloudflareAccountConfig
    certFile: string
    tunnelId: string
    credentialsFile: string
    configFile: string
    ingress: CloudflareIngressRule[]
    createdAt: number
    updatedAt: number
}

export interface CloudflareData {
    tunnels: CloudflareTunnel[]
}

export interface CloudflaredStatus {
    available: boolean
    managed: boolean
    version: string | null
    path: string
    certPath: string | null
    homeCredentialsDir: string | null
    managedCredentialsDir: string
}

export interface CloudflareAccount {
    id: string
    name: string
}

export interface CloudflareZone {
    id: string
    name: string
    status: string
}

export function newCloudflareTunnel(name = 'new-tunnel'): CloudflareTunnel {
    const now = Date.now()
    return {
        id: crypto.randomUUID(),
        name,
        account: {
            authMode: 'authorization',
            apiToken: '',
            tokenAccountId: '',
            tokenAccountName: '',
            authorizationAccountId: '',
            authorizationZoneId: '',
        },
        certFile: '',
        tunnelId: '',
        credentialsFile: '',
        configFile: '',
        ingress: [],
        createdAt: now,
        updatedAt: now,
    }
}

export function newIngressRule(): CloudflareIngressRule {
    return {
        id: crypto.randomUUID(),
        name: '',
        hostname: '',
        service: 'http://localhost:8080',
        enabled: true,
        dnsRouted: false,
    }
}
