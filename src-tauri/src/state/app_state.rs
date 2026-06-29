use crate::services::process_watchdog::ProcessWatchdogState;
use crate::services::watchdog_relay::WatchdogRelayState;
use crate::state::config_state::ConfigState;
use crate::state::provider_runtime_state::ProviderRuntimeState;
use crate::state::provider_state_store::ProviderStateStore;
use crate::state::session_state::SessionState;

#[derive(Clone, Default)]
pub struct AppState {
    pub config: ConfigState,
    pub session: SessionState,
    pub provider_runtime: ProviderRuntimeState,
    pub provider_states: ProviderStateStore,
    pub process_watchdog: ProcessWatchdogState,
    pub watchdog_relay: WatchdogRelayState,
}
