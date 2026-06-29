import { call } from './_invoke'
import type { AppSettings } from '@/domain/settings'

export const getSettings = () => call<AppSettings>('get_settings')
export const setSettings = (
    silentStart: boolean,
    autoConnect: boolean,
    lightweightMode: boolean,
    autoUpdate: boolean,
    trafficStatsEnabled: boolean,
) =>
    call<void>('set_settings', {
        options: {
            silentStart,
            autoConnect,
            lightweightMode,
            autoUpdate,
            trafficStatsEnabled,
        },
    })
export const setTrayLocale = (locale: string) => call<void>('set_tray_locale', { locale })
