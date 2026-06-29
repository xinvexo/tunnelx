export function tunnelItemTag(value: unknown, fallback = 'SERVICE') {
    const tag = String(value ?? '').trim()
    return (tag || fallback).toUpperCase()
}

export function serviceTag(service: unknown) {
    const value = String(service ?? '').trim()
    const scheme = value.match(/^([a-z][a-z0-9+.-]*):/i)?.[1]
    if (scheme) return tunnelItemTag(scheme)
    if (value.startsWith('http_status:')) return 'HTTP_STATUS'
    return 'SERVICE'
}

export function publicUrlsFromHostname(hostname: unknown): string[] {
    const host = String(hostname ?? '').trim()
    if (!host) return []
    if (/^[a-z][a-z0-9+.-]*:\/\//i.test(host)) return [host]
    return [`https://${host}`]
}
