use serde_json::Value;

pub const REDACTED_SECRET: &str = "***";

pub fn text(input: impl Into<String>) -> String {
    tunnelx_watchdog_protocol::redaction::redact_sensitive_text(input.into())
}

pub fn json_value(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| {
                    let value = if is_sensitive_key(&key) {
                        Value::String(REDACTED_SECRET.into())
                    } else {
                        json_value(value)
                    };
                    (key, value)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(json_value).collect()),
        Value::String(value) => Value::String(text(value)),
        other => other,
    }
}

pub fn restore_if_redacted(value: &mut String, previous: &str) {
    if value.trim() == REDACTED_SECRET {
        *value = previous.to_string();
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    matches!(
        normalized.as_str(),
        "token"
            | "authtoken"
            | "accesstoken"
            | "apitoken"
            | "secret"
            | "clientsecret"
            | "tunnelsecret"
            | "password"
            | "authorization"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_value_redacts_sensitive_fields_and_embedded_query_values() {
        let redacted = json_value(serde_json::json!({
            "token": "secret-token",
            "url": "https://api.example.test?token=query-token&ok=1",
            "nested": {
                "Authorization": "Bearer abc",
                "client_secret": "client-value"
            }
        }));

        assert_eq!(redacted["token"], "***");
        assert_eq!(redacted["url"], "https://api.example.test?token=***&ok=1");
        assert_eq!(redacted["nested"]["Authorization"], "***");
        assert_eq!(redacted["nested"]["client_secret"], "***");
        assert!(!redacted.to_string().contains("secret-token"));
        assert!(!redacted.to_string().contains("query-token"));
        assert!(!redacted.to_string().contains("Bearer abc"));
        assert!(!redacted.to_string().contains("client-value"));
    }

    #[test]
    fn json_value_keeps_structure_when_sensitive_values_contain_quotes() {
        let redacted = json_value(serde_json::json!({
            "token": "secret with \"quote\"",
            "items": [
                {
                    "url": "https://api.example.test?token=query-token&ok=1"
                }
            ]
        }));

        assert_eq!(redacted["token"], "***");
        assert_eq!(
            redacted["items"][0]["url"],
            "https://api.example.test?token=***&ok=1"
        );
        assert!(redacted["items"].is_array());
    }

    #[test]
    fn restore_if_redacted_preserves_existing_secret_only_for_placeholder() {
        let mut value = REDACTED_SECRET.to_string();
        restore_if_redacted(&mut value, "real-token");
        assert_eq!(value, "real-token");

        let mut changed = "new-token".to_string();
        restore_if_redacted(&mut changed, "real-token");
        assert_eq!(changed, "new-token");
    }
}
