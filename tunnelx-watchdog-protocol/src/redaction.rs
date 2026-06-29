pub fn redact_sensitive_text(text: impl Into<String>) -> String {
    let mut text = text.into();
    for flag in [
        "--token",
        "--authtoken",
        "--api-token",
        "--access-token",
        "--remote-management",
    ] {
        text = redact_flag_value(text, flag);
    }
    for key in [
        "token",
        "authtoken",
        "accessToken",
        "access_token",
        "apiToken",
        "api_token",
        "secret",
        "client_secret",
        "tunnel_secret",
        "TunnelSecret",
        "password",
        "authorization",
        "Authorization",
    ] {
        text = redact_json_field(text, key);
        text = redact_assignment(text, key);
    }
    text = redact_auth_scheme(text, "Bearer ");
    redact_auth_scheme(text, "Basic ")
}

fn redact_flag_value(line: String, flag: &str) -> String {
    let mut output = line;
    let mut search_from = 0usize;
    while let Some(relative) = output[search_from..].find(flag) {
        let flag_start = search_from + relative;
        if !is_boundary(output[..flag_start].chars().next_back()) {
            search_from = flag_start + flag.len();
            continue;
        }
        let after_flag = flag_start + flag.len();
        if !is_boundary(output[after_flag..].chars().next()) {
            search_from = after_flag;
            continue;
        }
        let Some((start, end)) = value_range_after(&output, after_flag) else {
            search_from = after_flag;
            continue;
        };
        output.replace_range(start..end, "***");
        search_from = start + 3;
    }
    output
}

fn redact_json_field(line: String, key: &str) -> String {
    let mut output = line;
    let needle = format!("\"{key}\"");
    let mut search_from = 0usize;
    while let Some(relative) = output[search_from..].find(&needle) {
        let key_start = search_from + relative;
        let after_key = key_start + needle.len();
        let Some(colon_relative) = output[after_key..].find(':') else {
            search_from = after_key;
            continue;
        };
        let after_colon = after_key + colon_relative + 1;
        let Some((start, end)) = value_range_after(&output, after_colon) else {
            search_from = after_colon;
            continue;
        };
        output.replace_range(start..end, "***");
        search_from = start + 3;
    }
    output
}

fn redact_assignment(line: String, key: &str) -> String {
    let mut output = line;
    for separator in ['=', ':'] {
        let needle = format!("{key}{separator}");
        let mut search_from = 0usize;
        while let Some(relative) = output[search_from..].find(&needle) {
            let key_start = search_from + relative;
            if !is_boundary(output[..key_start].chars().next_back()) {
                search_from = key_start + needle.len();
                continue;
            }
            let after_key = key_start + needle.len();
            let Some((start, end)) = value_range_after(&output, after_key) else {
                search_from = after_key;
                continue;
            };
            output.replace_range(start..end, "***");
            search_from = start + 3;
        }
    }
    output
}

fn redact_auth_scheme(line: String, scheme: &str) -> String {
    let mut output = line;
    let mut search_from = 0usize;
    while let Some(relative) = output[search_from..].find(scheme) {
        let start = search_from + relative + scheme.len();
        let end = value_end(&output, start);
        if start < end {
            output.replace_range(start..end, "***");
            search_from = start + 3;
        } else {
            search_from = start;
        }
    }
    output
}

fn value_range_after(input: &str, start: usize) -> Option<(usize, usize)> {
    let start = next_value_start(input, start)?;
    let end = if input[start..].starts_with("Bearer ") {
        value_end(input, start + "Bearer ".len())
    } else if input[start..].starts_with("Basic ") {
        value_end(input, start + "Basic ".len())
    } else if let Some(quote) = previous_value_quote(input, start) {
        input[start..]
            .find(quote)
            .map(|relative| start + relative)
            .unwrap_or_else(|| value_end(input, start))
    } else {
        value_end(input, start)
    };
    (start < end).then_some((start, end))
}

fn next_value_start(input: &str, mut index: usize) -> Option<usize> {
    while index < input.len() {
        let rest = input.get(index..)?;
        let ch = rest.chars().next()?;
        if !matches!(
            ch,
            ' ' | '\t' | '\r' | '\n' | '"' | '\'' | ':' | '=' | ',' | '[' | ']'
        ) {
            return Some(index);
        }
        index += ch.len_utf8();
    }
    None
}

fn value_end(input: &str, mut index: usize) -> usize {
    while index < input.len() {
        let Some(rest) = input.get(index..) else {
            return index;
        };
        let Some(ch) = rest.chars().next() else {
            return index;
        };
        if matches!(
            ch,
            ' ' | '\t' | '\r' | '\n' | '"' | '\'' | ',' | ']' | '}' | ')' | '&'
        ) {
            return index;
        }
        index += ch.len_utf8();
    }
    index
}

fn previous_value_quote(input: &str, start: usize) -> Option<char> {
    input[..start]
        .chars()
        .rev()
        .find_map(|ch| match ch {
            '"' | '\'' => Some(ch),
            ' ' | '\t' | '\r' | '\n' | ':' | '=' | ',' | '[' | ']' => None,
            _ => Some('\0'),
        })
        .filter(|quote| *quote != '\0')
}

fn is_boundary(ch: Option<char>) -> bool {
    ch.map(|ch| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_cli_json_auth_and_query_values() {
        let line = r#"start --token cli-token https://api.example.test?token=query-token&ok=1 {"Authorization":"Bearer abc","TunnelSecret":"secret-value","client_secret":"client-value"}"#;

        let redacted = redact_sensitive_text(line);

        assert!(redacted.contains("--token ***"));
        assert!(redacted.contains("token=***"));
        assert!(redacted.contains(r#""Authorization":"***""#));
        assert!(redacted.contains(r#""TunnelSecret":"***""#));
        assert!(redacted.contains(r#""client_secret":"***""#));
        assert!(!redacted.contains("cli-token"));
        assert!(!redacted.contains("query-token"));
        assert!(!redacted.contains("Bearer abc"));
        assert!(!redacted.contains("secret-value"));
        assert!(!redacted.contains("client-value"));
    }

    #[test]
    fn redacts_assignment_auth_scheme_value() {
        let redacted = redact_sensitive_text("Authorization=Bearer abc token=query-token");

        assert!(redacted.contains("Authorization=***"));
        assert!(redacted.contains("token=***"));
        assert!(!redacted.contains("Bearer"));
        assert!(!redacted.contains("abc"));
        assert!(!redacted.contains("query-token"));
    }
}
