pub use super::account::credentials::login_connection;
pub use super::account::data;
pub use super::environment::cli::status;
pub use super::environment::{check_update, install, uninstall};
pub use super::tunnel::api::{list_accounts, list_remote_tunnels, list_zones, verify_token};
pub use super::tunnel::runtime::{start_tunnel, stop_tunnel, tunnel_status};
pub use super::tunnel::{create_tunnel, delete_tunnel, save_tunnel};
use crate::error::AppResult;
use crate::state::AppState;
use tauri::AppHandle;

pub fn cleanup_on_start(app: &AppHandle, state: &AppState) -> AppResult<()> {
    super::account::authorization::cleanup_stale_authorization_profiles(app);
    super::tunnel::runtime::cleanup_on_start(app, state)
}
