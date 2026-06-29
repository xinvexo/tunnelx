use crate::providers::contract::FRP_PROVIDER_ID;
use crate::providers::frp::runtime_state::RuntimeState;
use crate::state::AppState;
use std::sync::Arc;

#[derive(Default)]
pub struct FrpState {
    pub runtime: RuntimeState,
}

pub fn frp_state(state: &AppState) -> Arc<FrpState> {
    state.provider_states.get(FRP_PROVIDER_ID)
}
