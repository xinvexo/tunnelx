import { withDefaults } from '@/domain/util'

export interface HeaderOps {
    set: Record<string, string> | null
}

export type PluginType =
    | 'http_proxy'
    | 'socks5'
    | 'static_file'
    | 'unix_domain_socket'
    | 'http2http'
    | 'http2https'
    | 'https2http'
    | 'https2https'
    | 'tls2raw'
    | 'virtual_net'

export interface HttpProxyPlugin {
    type: 'http_proxy'
    httpUser: string
    httpPassword: string
}
export interface Socks5Plugin {
    type: 'socks5'
    username: string
    password: string
}
export interface StaticFilePlugin {
    type: 'static_file'
    localPath: string
    stripPrefix: string
    httpUser: string
    httpPassword: string
}
export interface UnixSocketPlugin {
    type: 'unix_domain_socket'
    unixPath: string
}
export interface Http2HttpPlugin {
    type: 'http2http'
    localAddr: string
    hostHeaderRewrite: string
    requestHeaders: HeaderOps | null
}
export interface Http2HttpsPlugin {
    type: 'http2https'
    localAddr: string
    hostHeaderRewrite: string
    requestHeaders: HeaderOps | null
}
export interface Https2HttpPlugin {
    type: 'https2http'
    localAddr: string
    hostHeaderRewrite: string
    requestHeaders: HeaderOps | null
    enableHTTP2: boolean | null
    crtPath: string
    keyPath: string
}
export interface Https2HttpsPlugin {
    type: 'https2https'
    localAddr: string
    hostHeaderRewrite: string
    requestHeaders: HeaderOps | null
    enableHTTP2: boolean | null
    crtPath: string
    keyPath: string
}
export interface Tls2RawPlugin {
    type: 'tls2raw'
    localAddr: string
    crtPath: string
    keyPath: string
}
export interface VirtualNetPlugin {
    type: 'virtual_net'
}

export type PluginOptions =
    | HttpProxyPlugin
    | Socks5Plugin
    | StaticFilePlugin
    | UnixSocketPlugin
    | Http2HttpPlugin
    | Http2HttpsPlugin
    | Https2HttpPlugin
    | Https2HttpsPlugin
    | Tls2RawPlugin
    | VirtualNetPlugin

export function newPlugin(type: PluginType): PluginOptions {
    switch (type) {
        case 'http_proxy':
            return { type, httpUser: '', httpPassword: '' }
        case 'socks5':
            return { type, username: '', password: '' }
        case 'static_file':
            return { type, localPath: '', stripPrefix: '', httpUser: '', httpPassword: '' }
        case 'unix_domain_socket':
            return { type, unixPath: '' }
        case 'http2http':
        case 'http2https':
            return { type, localAddr: '', hostHeaderRewrite: '', requestHeaders: null }
        case 'https2http':
        case 'https2https':
            return {
                type,
                localAddr: '',
                hostHeaderRewrite: '',
                requestHeaders: null,
                enableHTTP2: null,
                crtPath: '',
                keyPath: '',
            }
        case 'tls2raw':
            return { type, localAddr: '', crtPath: '', keyPath: '' }
        case 'virtual_net':
            return { type }
    }
}

export function hydratePlugin(raw: PluginOptions): PluginOptions {
    return withDefaults(newPlugin(raw.type), raw)
}
