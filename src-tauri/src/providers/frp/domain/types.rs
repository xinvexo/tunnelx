use serde::{Deserialize, Serialize};

pub(crate) use crate::domain::{gen_id, is_false};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AuthMethod {
    #[default]
    Token,
    Oidc,
}
