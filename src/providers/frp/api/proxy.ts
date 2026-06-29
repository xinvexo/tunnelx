import { call } from '@/api/_invoke'
import type { ProfileProxy } from '@/providers/frp/domain/proxy'

/** 整体覆盖某个 Profile 的隧道列表。 */
export const updateProxies = (profileId: string, proxies: ProfileProxy[]) => call<void>('update_proxies', { profileId, proxies })
