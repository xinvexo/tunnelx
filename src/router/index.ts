import { createRouter, createWebHistory } from 'vue-router'
import type { RouteRecordRaw } from 'vue-router'
import { useConnectionStore } from '@/providers/connections'
import { connectionPath } from '@/providers/routes'
import ConnectionsView from '@/views/ConnectionsView.vue'
import ProviderWorkspaceLayout from '@/layouts/ProviderWorkspaceLayout.vue'
import ConnectionOverview from '@/views/ConnectionOverview.vue'
import ConnectionTunnels from '@/views/ConnectionTunnels.vue'
import ConnectionLogs from '@/views/ConnectionLogs.vue'
import ConnectionSettings from '@/views/ConnectionSettings.vue'
import LogsView from '@/views/LogsView.vue'
import SystemSettings from '@/views/SystemSettings.vue'
import ProviderEnvironmentHost from '@/views/ProviderEnvironmentHost.vue'
import AboutView from '@/views/AboutView.vue'

const routes: RouteRecordRaw[] = [
  { path: '/', redirect: '/connections' },
  { path: '/connections', component: ConnectionsView },
  {
    path: '/connections/:providerId/:id',
    component: ProviderWorkspaceLayout,
    children: [
      { path: '', redirect: (to) => connectionPath(String(to.params.providerId), String(to.params.id), 'overview') },
      { path: 'overview', name: 'connection-overview', component: ConnectionOverview },
      { path: 'tunnels', name: 'connection-tunnels', component: ConnectionTunnels },
      { path: 'logs', name: 'connection-logs', component: ConnectionLogs },
      { path: 'settings', name: 'connection-settings', component: ConnectionSettings },
    ],
  },
  { path: '/activity', component: LogsView },
  { path: '/settings', redirect: '/settings/general' },
  { path: '/settings/general', component: SystemSettings },
  { path: '/settings/versions', component: ProviderEnvironmentHost },
  { path: '/settings/about', component: AboutView },
  { path: '/versions', redirect: '/settings/versions' },
]

export const router = createRouter({
  history: createWebHistory(),
  routes,
})

router.beforeEach(async (to) => {
  if (to.path.startsWith('/connections/') && to.params.providerId && to.params.id) {
    const store = useConnectionStore()
    if (!store.loaded) await store.init()
    if (!store.has(String(to.params.providerId), String(to.params.id))) return '/connections'
  }
  return true
})
