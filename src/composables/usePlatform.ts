export type Platform = 'macos' | 'windows' | 'linux'

function detect(): Platform {
    const ua = navigator.userAgent
    if (/Mac/i.test(ua)) return 'macos'
    if (/Win/i.test(ua)) return 'windows'
    return 'linux'
}

export const platform: Platform = detect()
export const isMac = platform === 'macos'
