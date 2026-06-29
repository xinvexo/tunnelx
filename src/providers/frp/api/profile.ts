import { call } from '@/api/_invoke'
import type { Profile, ProfileSummary } from '@/providers/frp/domain/profile'

export const listProfiles = () => call<ProfileSummary[]>('list_profiles')
export const getProfile = (id: string) => call<Profile>('get_profile', { id })
export const updateProfile = (profile: Profile) => call<Profile>('update_profile', { profile })
export const reorderProfiles = (ids: string[]) => call<void>('reorder_profiles', { ids })
