use crate::sync_ext::MutexExt;
use std::any::{type_name, Any};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct ProviderStateStore(Arc<Mutex<HashMap<String, Arc<dyn Any + Send + Sync>>>>);

impl ProviderStateStore {
    pub fn get_or_insert_with<T>(&self, provider_id: &str, init: impl FnOnce() -> T) -> Arc<T>
    where
        T: Send + Sync + 'static,
    {
        let mut guard = self.0.lock_recover();
        if let Some(existing) = guard.get(provider_id) {
            if let Ok(state) = existing.clone().downcast::<T>() {
                return state;
            }
            crate::diag::warn(
                &crate::diag::provider_scope(provider_id),
                format!("state type mismatch; rebuilt as {}", type_name::<T>()),
            );
        }

        let state = Arc::new(init());
        guard.insert(provider_id.to_string(), state.clone());
        state
    }

    pub fn get<T>(&self, provider_id: &str) -> Arc<T>
    where
        T: Default + Send + Sync + 'static,
    {
        self.get_or_insert_with(provider_id, T::default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FirstState {
        value: u8,
    }

    #[derive(Default)]
    struct SecondState {
        value: u8,
    }

    #[test]
    fn mismatched_provider_state_type_is_rebuilt() {
        let store = ProviderStateStore::default();
        let first = store.get_or_insert_with("provider-a", || FirstState { value: 1 });
        assert_eq!(first.value, 1);

        let second = store.get_or_insert_with("provider-a", || SecondState { value: 2 });
        assert_eq!(second.value, 2);

        let second_again = store.get::<SecondState>("provider-a");
        assert_eq!(second_again.value, 2);
    }
}
