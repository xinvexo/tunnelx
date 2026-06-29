import { call } from './_invoke'

export interface UpdateInfo {
    available: boolean
    currentVersion: string
    latestVersion: string
    notes: string
}

export interface AppUpdateProgress {
    receivedBytes: number
    totalBytes: number | null
    percent: number | null
}

export const appVersion = () => call<string>('app_version')
export const checkUpdate = () => call<UpdateInfo>('check_update')
export const installUpdate = () => call<void>('install_update')
