use crate::domain::{AppData, AppSettings};
use crate::error::{AppError, AppResult};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

#[derive(Clone, Default)]
pub struct ConfigState(Arc<RwLock<AppData>>);

const LOCK_POISONED_MSG: &str = "Config state is damaged; restart TunnelX";

impl ConfigState {
    #[inline]
    fn lock_error() -> AppError {
        AppError::Msg(LOCK_POISONED_MSG.into())
    }

    fn read(&self) -> AppResult<RwLockReadGuard<'_, AppData>> {
        self.0.read().map_err(|_| Self::lock_error())
    }

    fn write(&self) -> AppResult<RwLockWriteGuard<'_, AppData>> {
        self.0.write().map_err(|_| Self::lock_error())
    }

    pub fn replace(&self, data: AppData) {
        if let Ok(mut guard) = self.write() {
            *guard = data;
        }
    }

    pub fn snapshot(&self) -> AppResult<AppData> {
        Ok(self.read()?.clone())
    }

    pub fn settings(&self) -> AppSettings {
        self.read().map(|g| g.settings.clone()).unwrap_or_default()
    }

    pub fn connection_order(&self) -> Vec<String> {
        self.read()
            .map(|g| g.connection_order.clone())
            .unwrap_or_default()
    }

    pub fn set_connection_order(&self, order: Vec<String>) -> AppResult<()> {
        let mut g = self.write()?;
        g.connection_order = order;
        Ok(())
    }

    pub fn set_options(
        &self,
        silent_start: bool,
        auto_connect: bool,
        lightweight_mode: bool,
        auto_update: bool,
        traffic_stats_enabled: bool,
    ) -> AppResult<()> {
        let mut g = self.write()?;
        g.settings.silent_start = silent_start;
        g.settings.auto_connect = auto_connect;
        g.settings.lightweight_mode = lightweight_mode;
        g.settings.auto_update = auto_update;
        g.settings.traffic_stats_enabled = traffic_stats_enabled;
        Ok(())
    }

    pub fn provider_data<T>(&self, provider_id: &str) -> AppResult<T>
    where
        T: DeserializeOwned + Default,
    {
        let guard = self.read()?;
        match guard.providers.get(provider_id) {
            Some(value) => serde_json::from_value(value.clone()).map_err(Into::into),
            None => Ok(T::default()),
        }
    }

    pub fn update_provider_data<T, R>(
        &self,
        provider_id: &str,
        mutate: impl FnOnce(&mut T) -> AppResult<R>,
    ) -> AppResult<R>
    where
        T: DeserializeOwned + Serialize + Default,
    {
        let mut guard = self.write()?;
        let mut data = guard
            .providers
            .get(provider_id)
            .cloned()
            .map(serde_json::from_value)
            .transpose()?
            .unwrap_or_default();
        let result = mutate(&mut data)?;
        guard
            .providers
            .insert(provider_id.to_string(), serde_json::to_value(data)?);
        Ok(result)
    }
}
