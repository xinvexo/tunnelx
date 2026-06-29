/// serde `skip_serializing_if` 辅助：值为 false 时跳过。
pub(crate) fn is_false(b: &bool) -> bool {
    !*b
}

pub(crate) fn gen_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
