import { withDefaults } from '@/domain/util'

export type AuthMethod = 'token' | 'oidc'

export interface Oidc {
    clientID: string
    clientSecret: string
    audience: string
    scope: string
    tokenEndpointURL: string
    trustedCaFile: string
    insecureSkipVerify: boolean
    additionalEndpointParams: Record<string, string> | null
}

export interface Auth {
    method: AuthMethod
    token: string
    additionalScopes: string[]
    oidc: Oidc
}

export interface Tls {
    enable: boolean | null
    disableCustomTLSFirstByte: boolean | null
    certFile: string
    keyFile: string
    trustedCaFile: string
    serverName: string
}

export interface TlsFiles {
    certFile: string
    keyFile: string
    trustedCaFile: string
    serverName: string
}

export interface Quic {
    keepalivePeriod: number | null
    maxIdleTimeout: number | null
    maxIncomingStreams: number | null
}

export interface Transport {
    protocol: string
    dialServerTimeout: number | null
    dialServerKeepalive: number | null
    connectServerLocalIP: string
    proxyURL: string
    poolCount: number | null
    tcpMux: boolean | null
    tcpMuxKeepaliveInterval: number | null
    quic: Quic
    heartbeatInterval: number | null
    heartbeatTimeout: number | null
    tls: Tls
}

export interface Log {
    to: string
    level: string
    maxDays: number | null
    disablePrintColor: boolean
}

export interface WebServer {
    addr: string
    port: number
    user: string
    password: string
    assetsDir: string
    pprofEnable: boolean
    tls: TlsFiles | null
}

export interface ServerConfig {
    user: string
    clientID: string
    serverAddr: string
    serverPort: number
    auth: Auth
    transport: Transport
    log: Log
    webServer: WebServer
    dnsServer: string
    natHoleStunServer: string
    loginFailExit: boolean | null
    udpPacketSize: number | null
    metadatas: Record<string, string> | null
}

export function defaultServerConfig(): ServerConfig {
    return {
        user: '',
        clientID: '',
        serverAddr: '127.0.0.1',
        serverPort: 7000,
        auth: {
            method: 'token',
            token: '',
            additionalScopes: [],
            oidc: {
                clientID: '',
                clientSecret: '',
                audience: '',
                scope: '',
                tokenEndpointURL: '',
                trustedCaFile: '',
                insecureSkipVerify: false,
                additionalEndpointParams: null,
            },
        },
        transport: {
            protocol: 'tcp',
            dialServerTimeout: 10,
            dialServerKeepalive: 7200,
            connectServerLocalIP: '',
            proxyURL: '',
            poolCount: 4,
            tcpMux: null,
            tcpMuxKeepaliveInterval: null,
            quic: { keepalivePeriod: null, maxIdleTimeout: null, maxIncomingStreams: null },
            heartbeatInterval: 30,
            heartbeatTimeout: 90,
            tls: {
                enable: null,
                disableCustomTLSFirstByte: null,
                certFile: '',
                keyFile: '',
                trustedCaFile: '',
                serverName: '',
            },
        },
        log: { to: '', level: '', maxDays: null, disablePrintColor: false },
        webServer: {
            addr: '',
            port: 0,
            user: '',
            password: '',
            assetsDir: '',
            pprofEnable: false,
            tls: null,
        },
        dnsServer: '',
        natHoleStunServer: '',
        loginFailExit: null,
        udpPacketSize: null,
        metadatas: null,
    }
}

export function hydrateServerConfig(raw: unknown): ServerConfig {
    return withDefaults(defaultServerConfig(), raw)
}

export function newTlsFiles(): TlsFiles {
    return { certFile: '', keyFile: '', trustedCaFile: '', serverName: '' }
}
