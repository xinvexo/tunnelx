export function connectionPath(providerId: string, id: string, tab = 'overview') {
    return `/connections/${encodeURIComponent(providerId)}/${encodeURIComponent(id)}/${tab}`
}
