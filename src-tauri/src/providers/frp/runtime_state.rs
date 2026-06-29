use crate::sync_ext::MutexExt;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

/// 单个 frpc 实例的运行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FrpcStatus {
    #[default]
    Stopped,
    Starting,
    Running,
    Warning,
    Stopping,
    Errored,
}

impl FrpcStatus {
    /// 已连接：运行中，或带警告但仍在跑。
    pub fn is_running(self) -> bool {
        matches!(self, FrpcStatus::Running | FrpcStatus::Warning)
    }

    /// 活跃态（运行 / 警告 / 启动中 / 停止中）：占用中、不能重复 start、需要走 stop 流程。
    pub fn is_active(self) -> bool {
        matches!(
            self,
            FrpcStatus::Running | FrpcStatus::Warning | FrpcStatus::Starting | FrpcStatus::Stopping
        )
    }
}

/// 一个 Profile 对应的运行实例（状态 + provider-specific runtime metadata），不持久化。
///
/// frpc 进程本身现在由平台级看门狗 sidecar 持有，
/// 这里只保留该 Profile 的状态/配置元信息，日志由平台级 provider_runtime 统一维护。
#[derive(Default)]
pub struct FrpcInstance {
    pub status: FrpcStatus,
    pub proxy_running: HashSet<String>,
    pub proxy_warning: HashSet<String>,
    pub pid: Option<u32>,
    pub config_path: Option<PathBuf>,
    pub needs_restart: bool,
    /// server/frps/frpc 版本等运行期公共配置已变更，admin reload 不能应用，必须进程级重启。
    pub process_restart_required: bool,
    /// 本地 frpc admin 端口，用于停止时经 admin API 优雅断开（让 frps 立即注销隧道），
    /// 以及运行期经 `GET /api/status` 查询每条隧道的真实状态与公网地址。
    pub admin_port: Option<u16>,
    /// admin API 的 Authorization 头（用户自配 webServer 凭据时需要）。
    /// 仅存内存，随实例重启刷新。
    pub admin_auth: Option<String>,
}

impl FrpcInstance {
    /// 该实例的 admin 端点（端口 + 预先算好的 Authorization 头），用于 admin API 调用；
    /// 端口未就绪时返回 None。
    pub fn admin_endpoint(&self) -> Option<(u16, Option<String>)> {
        self.admin_port.map(|port| (port, self.admin_auth.clone()))
    }
}

#[derive(Default)]
pub struct RuntimeInner {
    /// key = profileId
    pub instances: HashMap<String, FrpcInstance>,
}

impl RuntimeInner {
    pub fn instance_mut(&mut self, id: &str) -> &mut FrpcInstance {
        self.instances.entry(id.to_string()).or_default()
    }
}

/// 运行态：每个 Profile 独立的进程/状态。
#[derive(Clone, Default)]
pub struct RuntimeState(pub Arc<Mutex<RuntimeInner>>);

impl RuntimeState {
    /// 运行态只是进程/状态镜像；锁中毒时恢复使用，避免一次 panic 拖垮整个应用。
    pub fn lock(&self) -> MutexGuard<'_, RuntimeInner> {
        self.0.lock_recover()
    }

    pub fn status(&self, id: &str) -> FrpcStatus {
        self.lock()
            .instances
            .get(id)
            .map(|i| i.status)
            .unwrap_or_default()
    }

    pub fn needs_restart(&self, id: &str) -> bool {
        self.lock()
            .instances
            .get(id)
            .map(|i| i.needs_restart)
            .unwrap_or(false)
    }

    pub fn mark_process_restart_required(&self, id: &str) -> bool {
        let mut inner = self.lock();
        let Some(instance) = inner.instances.get_mut(id) else {
            return false;
        };
        if matches!(
            instance.status,
            FrpcStatus::Running | FrpcStatus::Warning | FrpcStatus::Starting
        ) {
            instance.needs_restart = true;
            instance.process_restart_required = true;
            return true;
        }
        false
    }

    pub fn mark_hot_reload_failed(&self, id: &str) -> bool {
        let mut inner = self.lock();
        let Some(instance) = inner.instances.get_mut(id) else {
            return false;
        };
        if instance.status.is_running() {
            instance.needs_restart = true;
            return true;
        }
        false
    }

    /// 热重载成功只清除“隧道变更待应用”；若之前 server/frps 级配置已变更，
    /// 仍然保留进程级重启提示。
    pub fn mark_hot_reload_succeeded(&self, id: &str) -> Option<bool> {
        let mut inner = self.lock();
        let instance = inner.instances.get_mut(id)?;
        if !instance.process_restart_required {
            instance.needs_restart = false;
        }
        Some(instance.needs_restart)
    }
}
