use chrono::Local;

pub(super) fn system_line(provider_id: &str, line: String) -> Option<String> {
    let line = redact_log_identity_and_paths(&sanitize_line(line));
    let (timestamp, message) = split_optional_timestamp(&line)
        .map(|(timestamp, message)| (normalize_timestamp(timestamp), message.trim().to_string()))
        .unwrap_or_else(|| (current_log_timestamp(), line.trim().to_string()));
    let (level, message) = take_optional_bracket_level(&message)
        .map(|(level, message)| (level_from_token(level), message.trim().to_string()))
        .unwrap_or_else(|| (detect_native_level(&message), message));
    let message = cleanup_system_message(provider_id, &message)?;
    Some(format_native_line(Some(&timestamp), level, message))
}

#[cfg(test)]
pub(super) fn native_line(line: String) -> String {
    native_lines("provider", line)
        .into_iter()
        .next()
        .unwrap_or_default()
}

pub(super) fn native_lines(provider_id: &str, line: String) -> Vec<String> {
    let line = tunnelx_watchdog_protocol::redaction::redact_sensitive_text(
        strip_ansi_and_control_preserve_frames(&line),
    );
    let mut lines = Vec::new();
    for frame in split_native_frames(&line) {
        let Some(line) = normalize_native_frame(provider_id, &frame) else {
            continue;
        };
        if lines.last() != Some(&line) {
            lines.push(line);
        }
    }
    lines
}

pub(super) fn append_cleanup_failure(
    base: String,
    cleanup_name: &str,
    cleanup_success: Option<bool>,
    cleanup_error: Option<&str>,
) -> String {
    if cleanup_success == Some(false) {
        format!(
            "{base}; {} was not confirmed: {}",
            cleanup_name.trim(),
            super::cleanup_failure_detail(cleanup_error)
        )
    } else {
        base
    }
}

pub(crate) fn sanitize_line(line: String) -> String {
    compact_noisy_provider_line(tunnelx_watchdog_protocol::redaction::redact_sensitive_text(
        strip_ansi_and_control(&line),
    ))
}

fn strip_ansi_and_control(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0x1b {
            index += 1;
            if index < bytes.len() && bytes[index] == b'[' {
                index += 1;
                while index < bytes.len() {
                    let byte = bytes[index];
                    index += 1;
                    if (0x40..=0x7e).contains(&byte) {
                        break;
                    }
                }
            } else if index < bytes.len() {
                index += input
                    .get(index..)
                    .and_then(|rest| rest.chars().next())
                    .map(|ch| ch.len_utf8())
                    .unwrap_or(1);
            }
            continue;
        }
        let Some(rest) = input.get(index..) else {
            index += 1;
            continue;
        };
        let Some(ch) = rest.chars().next() else {
            break;
        };
        if !ch.is_control() || ch == '\t' {
            output.push(ch);
        }
        index += ch.len_utf8();
    }
    output
}

fn strip_ansi_and_control_preserve_frames(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0x1b {
            index += 1;
            if index < bytes.len() && bytes[index] == b'[' {
                index += 1;
                while index < bytes.len() {
                    let byte = bytes[index];
                    index += 1;
                    if (0x40..=0x7e).contains(&byte) {
                        break;
                    }
                }
            } else if index < bytes.len() {
                index += input
                    .get(index..)
                    .and_then(|rest| rest.chars().next())
                    .map(|ch| ch.len_utf8())
                    .unwrap_or(1);
            }
            continue;
        }
        let Some(rest) = input.get(index..) else {
            index += 1;
            continue;
        };
        let Some(ch) = rest.chars().next() else {
            break;
        };
        if matches!(ch, '\r' | '\n') {
            output.push('\n');
        } else if !ch.is_control() || ch == '\t' {
            output.push(ch);
        }
        index += ch.len_utf8();
    }
    output
}

fn compact_noisy_provider_line(line: String) -> String {
    let marker = "Spawning daemon process with options:";
    if let Some(index) = line.find(marker) {
        return format!("{}Spawning daemon process", &line[..index]);
    }
    line
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NativeLogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl NativeLogLevel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
        }
    }
}

fn split_native_frames(input: &str) -> Vec<String> {
    let mut normalized = input.replace('\r', "\n");
    for marker in [
        "✔",
        "✓",
        "Tunnel:",
        "Authenticated as:",
        "Connection to tunnel server",
        "Your tunnel",
    ] {
        normalized = normalized.replace(marker, &format!("\n{marker}"));
    }
    normalized
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn normalize_native_frame(provider_id: &str, line: &str) -> Option<String> {
    let line = normalize_whitespace(line.trim());
    if let Some(rest) = strip_leading_level_prefix(&line) {
        return normalize_native_frame(provider_id, rest);
    }
    let line = normalize_whitespace(strip_known_native_prefixes(provider_id, &line));
    if line.is_empty() || should_drop_native_frame(provider_id, &line) {
        return None;
    }
    let line = redact_log_identity_and_paths(&line);

    if provider_id == "pinggy" {
        return normalize_pinggy_frame(provider_id, &line);
    }
    if is_logfmt_frame(&line) {
        return normalize_logfmt_frame(provider_id, &line);
    }
    if provider_id == "cloudflare" && is_cloudflared_frame(&line) {
        return normalize_cloudflared_frame(provider_id, &line);
    }
    if is_timestamped_bracket_frame(&line) {
        return normalize_timestamped_bracket_frame(provider_id, &line);
    }
    let message = cleanup_provider_message(provider_id, &line)?;
    Some(format_native_line(
        None,
        detect_native_level(&message),
        message,
    ))
}

fn normalize_pinggy_frame(provider_id: &str, line: &str) -> Option<String> {
    let line = strip_timestamped_level_prefix(line).unwrap_or(line);
    let line = strip_known_native_prefixes(provider_id, line);
    let line = line
        .trim_start_matches(|ch: char| {
            matches!(ch, '✔' | '✓' | '-' | '|' | '/' | '\\' | '*' | '>' | ' ')
        })
        .trim();
    if line.is_empty() || should_drop_native_frame(provider_id, line) {
        return None;
    }
    let lower = line.to_ascii_lowercase();
    if lower.contains("spawning daemon child")
        || lower.contains("initializing")
        || lower.contains("remote urls")
        || lower.contains("press ctrl+c")
        || lower.contains("dashboard.pinggy.io")
        || line.chars().all(|ch| matches!(ch, '_' | '-' | '=' | ' '))
        || first_url(line).is_some()
    {
        return None;
    }
    let message = if lower.contains("authenticated as") {
        "authenticated".to_string()
    } else if lower.contains("connected to pinggy") {
        "connected to Pinggy".to_string()
    } else if lower.contains("success: tunnel established") {
        "tunnel established".to_string()
    } else if let Some(url) = first_url(line) {
        format!("public URL ready: {url}")
    } else {
        cleanup_native_message(line)
    };
    Some(format_native_line(
        None,
        detect_native_level(&message),
        message,
    ))
}

fn normalize_logfmt_frame(provider_id: &str, line: &str) -> Option<String> {
    let message = field_value(line, "msg")?;
    let level = field_value(line, "level")
        .or_else(|| field_value(line, "lvl"))
        .as_deref()
        .map(level_from_token)
        .unwrap_or_else(|| detect_native_level(&message));
    let timestamp = field_value(line, "time")
        .or_else(|| field_value(line, "t"))
        .map(|value| normalize_timestamp(&value));
    let message = cleanup_provider_message_with_level(provider_id, level, &message)?;
    Some(format_native_line(timestamp.as_deref(), level, message))
}

fn is_logfmt_frame(line: &str) -> bool {
    field_value(line, "msg").is_some()
}

fn is_cloudflared_frame(line: &str) -> bool {
    let Some((_, rest)) = take_leading_timestamp(line) else {
        return false;
    };
    let Some((level, _)) = split_first_word(rest.trim_start()) else {
        return false;
    };
    matches!(
        level,
        "ERR" | "FTL" | "WRN" | "WRN!" | "DBG" | "TRC" | "INF"
    )
}

fn is_timestamped_bracket_frame(line: &str) -> bool {
    let Some((_, rest)) = take_leading_timestamp(line) else {
        return false;
    };
    take_optional_bracket_level(rest.trim_start()).is_some()
}

fn normalize_cloudflared_frame(provider_id: &str, line: &str) -> Option<String> {
    let (timestamp, rest) = take_leading_timestamp(line)?;
    let rest = rest.trim_start();
    let (level_token, message) = split_first_word(rest)?;
    let level = match level_token {
        "ERR" | "FTL" => NativeLogLevel::Error,
        "WRN" | "WRN!" => NativeLogLevel::Warn,
        "DBG" | "TRC" => NativeLogLevel::Debug,
        "INF" => NativeLogLevel::Info,
        _ => return None,
    };
    Some(format_native_line(
        Some(&normalize_timestamp(timestamp)),
        level,
        cleanup_provider_message_with_level(provider_id, level, message)?,
    ))
}

fn normalize_timestamped_bracket_frame(provider_id: &str, line: &str) -> Option<String> {
    let (timestamp, rest) = take_leading_timestamp(line)?;
    let rest = rest.trim_start();
    if !rest.starts_with('[') {
        return None;
    }
    let (level_token, rest) = take_bracket_token(rest)?;
    let level = level_from_token(level_token);
    let message = strip_source_brackets(rest.trim_start());
    Some(format_native_line(
        Some(&normalize_timestamp(timestamp)),
        level,
        cleanup_provider_message_with_level(provider_id, level, &message)?,
    ))
}

fn format_native_line(timestamp: Option<&str>, level: NativeLogLevel, message: String) -> String {
    match timestamp {
        Some(timestamp) if !timestamp.is_empty() => {
            format!("{timestamp} [{}] {message}", level.as_str())
        }
        _ => format!("{} [{}] {message}", current_log_timestamp(), level.as_str()),
    }
}

fn should_drop_native_frame(provider_id: &str, line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return true;
    }
    if trimmed
        .chars()
        .all(|ch| matches!(ch, '-' | '=' | '+' | '|' | ' '))
    {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    if provider_id == "pinggy" && lower.contains("waiting for connection") {
        return true;
    }
    if provider_id == "cpolar"
        && (lower.contains("read message")
            || lower.contains("writing message")
            || lower.contains("reading message with length")
            || lower.contains("waiting to read message"))
    {
        return true;
    }
    lower.contains("quick tunnel ready check")
        || lower.contains("connectivity check table")
        || lower == "connection to tunnel server"
}

fn cleanup_native_message(message: &str) -> String {
    let marker = "Spawning daemon process with options:";
    if let Some(index) = message.find(marker) {
        return normalize_whitespace(&format!("{}Spawning daemon process", &message[..index]));
    }
    let marker = "Spawning daemon child";
    if let Some(index) = message.find(marker) {
        return normalize_whitespace(&format!("{}Spawning daemon child", &message[..index]));
    }
    let mut message = strip_source_brackets(message.trim());
    message = drop_verbose_assignments(&message);
    if message.len() > 500 {
        message.truncate(497);
        message.push_str("...");
    }
    normalize_whitespace(&message)
}

fn cleanup_provider_message(provider_id: &str, message: &str) -> Option<String> {
    cleanup_provider_message_with_level(provider_id, detect_native_level(message), message)
}

fn cleanup_provider_message_with_level(
    provider_id: &str,
    level: NativeLogLevel,
    message: &str,
) -> Option<String> {
    match provider_id {
        "cpolar" => cleanup_cpolar_message(level, message),
        "cloudflare" => cleanup_cloudflared_message(level, message),
        "frp" => cleanup_frp_message(level, message),
        _ => {
            let message = cleanup_native_message(message);
            (!message.is_empty()).then_some(message)
        }
    }
}

fn cleanup_system_message(provider_id: &str, message: &str) -> Option<String> {
    let message = cleanup_native_message(message);
    if message.is_empty() {
        return None;
    }
    let lower = message.to_ascii_lowercase();
    match provider_id {
        "cpolar" if is_cpolar_noise(NativeLogLevel::Debug, &lower) => None,
        "cloudflare" if is_cloudflared_noise(NativeLogLevel::Info, &lower) => None,
        "pinggy" if is_pinggy_noise(&message) => None,
        _ => Some(message),
    }
}

fn cleanup_cpolar_message(level: NativeLogLevel, message: &str) -> Option<String> {
    let message = strip_leading_bracket_groups(message);
    let lower = message.to_ascii_lowercase();
    if is_cpolar_noise(level, &lower) {
        return None;
    }
    if let Some(url) = message
        .strip_prefix("Tunnel established at ")
        .and_then(first_url)
    {
        return Some(format!("public URL ready: {url}"));
    }
    let message = cleanup_native_message(&message);
    (!message.is_empty()).then_some(message)
}

pub(super) fn cleanup_cloudflared_message(level: NativeLogLevel, message: &str) -> Option<String> {
    let message = cleanup_native_message(message);
    let lower = message.to_ascii_lowercase();
    if is_cloudflared_noise(level, &lower) {
        return None;
    }
    (!message.is_empty()).then_some(message)
}

fn cleanup_frp_message(_level: NativeLogLevel, message: &str) -> Option<String> {
    let message = cleanup_native_message(message);
    let lower = message.to_ascii_lowercase();
    if lower.starts_with("start frpc service for config file") {
        return Some("start frpc service with aggregated configuration".to_string());
    }
    if lower.starts_with("frpc service for config file") && lower.ends_with("stopped") {
        return Some("frpc service stopped".to_string());
    }
    (!message.is_empty()).then_some(message)
}

fn is_cpolar_noise(level: NativeLogLevel, lower_message: &str) -> bool {
    lower_message.starts_with("read message")
        || lower_message.starts_with("writing message")
        || lower_message.starts_with("reading message with length")
        || lower_message.starts_with("waiting to read message")
        || lower_message.starts_with("event <-updates")
        || lower_message.starts_with("connonline update flash")
        || lower_message.starts_with("ctl init")
        || lower_message.starts_with("checkuserinfochange")
        || lower_message.starts_with("getuserinfo()")
        || lower_message.starts_with("parentid")
        || lower_message.starts_with("currentpid")
        || lower_message.starts_with("check isterminal")
        || lower_message.starts_with("who are you")
        || lower_message.starts_with("new connection to:")
        || lower_message.starts_with("authenticated with")
        || lower_message.starts_with("c.getisreqruntunnelruntimelist")
        || lower_message.starts_with("starttunnel")
        || lower_message.starts_with("new watchdog create")
        || lower_message.starts_with("trusting root cas")
        || lower_message.starts_with("dial:current")
        || lower_message.starts_with("currentdnsserver")
        || lower_message.starts_with("tunnelservermode")
        || lower_message.starts_with("set tunnel server init")
        || lower_message.starts_with("close with connection pxy:")
        || (lower_message.starts_with("server failed to read startproxy")
            && (lower_message.contains("use of closed network connection")
                || lower_message.contains("eof")))
        || (level == NativeLogLevel::Debug
            && !lower_message.contains("error")
            && !lower_message.contains("failed")
            && !lower_message.contains("public url"))
}

fn is_cloudflared_noise(level: NativeLogLevel, lower_message: &str) -> bool {
    lower_message.starts_with("settings:")
        || lower_message.starts_with("version ")
        || lower_message.starts_with("goos:")
        || lower_message.starts_with("autoupdate frequency")
        || lower_message.starts_with("generated connector id")
        || lower_message.starts_with("initial protocol")
        || lower_message.starts_with("icmp proxy will use")
        || lower_message.starts_with("created icmp proxy listening")
        || lower_message.starts_with("tunnel connection curve preferences")
        || lower_message.starts_with("starting metrics server")
        || lower_message.starts_with("you requested 4 ha connections")
        || lower_message.contains("connectivity pre-checks")
        || lower_message.contains("component target status details")
        || lower_message.contains("dns resolution region")
        || lower_message.contains("udp connectivity region")
        || lower_message.contains("tcp connectivity region")
        || lower_message.contains("cloudflare api api.cloudflare.com")
        || lower_message
            .chars()
            .all(|ch| matches!(ch, '+' | '-' | '|' | ' '))
        || (level == NativeLogLevel::Debug && !lower_message.contains("error"))
}

fn is_pinggy_noise(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("spawning daemon child")
        || lower.contains("initializing")
        || lower.contains("remote urls")
        || lower.contains("dashboard.pinggy.io")
        || lower.contains("press ctrl+c")
        || message
            .chars()
            .all(|ch| matches!(ch, '_' | '-' | '=' | ' '))
}

fn drop_verbose_assignments(message: &str) -> String {
    let mut kept = Vec::new();
    let mut skip_path_tail = false;
    for token in message.split_whitespace() {
        if skip_path_tail && !contains_assignment_separator(token) {
            continue;
        }
        skip_path_tail = false;
        if !contains_assignment_separator(token) {
            kept.push(token);
            continue;
        }
        let (key, value) = split_assignment_token(token).unwrap_or((token, ""));
        let key = key
            .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
            .to_ascii_lowercase();
        if matches!(
            key.as_str(),
            "config"
                | "configfile"
                | "credentials"
                | "credentialsfile"
                | "credfile"
                | "cert"
                | "certfile"
                | "origincert"
                | "origincertfile"
                | "path"
                | "file"
                | "tunnelid"
                | "connectorid"
                | "clientid"
                | "connindex"
                | "event"
                | "ip"
                | "autoupdatefreq"
                | "token"
                | "authtoken"
                | "authorization"
        ) {
            if value.starts_with("<path>") || value.starts_with('/') {
                skip_path_tail = true;
            }
            continue;
        }
        kept.push(token);
    }
    kept.join(" ")
}

fn contains_assignment_separator(token: &str) -> bool {
    split_assignment_token(token).is_some()
}

fn split_assignment_token(token: &str) -> Option<(&str, &str)> {
    let separator = token.find('=').or_else(|| token.find(':'))?;
    let key = &token[..separator];
    if key.is_empty()
        || !key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '['))
    {
        return None;
    }
    Some((key, &token[separator + 1..]))
}

fn strip_source_brackets(input: &str) -> String {
    let mut rest = input.trim();
    let mut output = String::new();
    while let Some((token, after)) = take_bracket_token(rest) {
        if token.contains(".go:") || token.contains(".rs:") {
            rest = after.trim_start();
            continue;
        }
        if !output.is_empty() {
            output.push(' ');
        }
        output.push('[');
        output.push_str(token);
        output.push(']');
        rest = after.trim_start();
    }
    if !rest.is_empty() {
        if !output.is_empty() {
            output.push(' ');
        }
        output.push_str(rest);
    }
    output
}

fn strip_leading_bracket_groups(input: &str) -> String {
    let mut rest = input.trim();
    while let Some((_, after)) = take_bracket_token(rest) {
        rest = after.trim_start();
    }
    rest.to_string()
}

fn strip_known_native_prefixes<'a>(provider_id: &str, input: &'a str) -> &'a str {
    let provider = match provider_id {
        "cloudflare" => "cloudflared",
        "frp" => "frpc",
        other => other,
    };
    let mut rest = input.trim();
    while let Some((token, after)) = take_bracket_token(rest) {
        let normalized = token.trim().to_ascii_lowercase();
        if matches!(
            normalized.as_str(),
            "info" | "debug" | "warn" | "warning" | "error" | "fatal"
        ) || normalized == provider
        {
            rest = after.trim_start();
            continue;
        }
        break;
    }
    rest
}

fn redact_log_identity_and_paths(line: &str) -> String {
    redact_absolute_paths(&redact_email_addresses(line))
}

fn redact_email_addresses(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut index = 0;
    while let Some(relative) = line[index..].find('@') {
        let at = index + relative;
        let start = line[..at]
            .rfind(|ch: char| ch.is_whitespace() || matches!(ch, '<' | '>' | '"' | '\''))
            .map(|pos| pos + 1)
            .unwrap_or(0);
        let end = line[at..]
            .find(|ch: char| ch.is_whitespace() || matches!(ch, '<' | '>' | '"' | '\''))
            .map(|pos| at + pos)
            .unwrap_or(line.len());
        let candidate = &line[start..end];
        if candidate.contains('.') {
            output.push_str(&line[index..start]);
            output.push_str("<email>");
            index = end;
        } else {
            output.push_str(&line[index..=at]);
            index = at + 1;
        }
    }
    output.push_str(&line[index..]);
    output
}

fn redact_absolute_paths(line: &str) -> String {
    let prefixes = [
        "/Users/",
        "/home/",
        "/var/folders/",
        "/private/var/",
        "/tmp/",
    ];
    let mut output = String::with_capacity(line.len());
    let mut index = 0;
    while index < line.len() {
        let Some((relative, _)) = prefixes
            .iter()
            .filter_map(|prefix| line[index..].find(prefix).map(|pos| (pos, *prefix)))
            .min_by_key(|(pos, _)| *pos)
        else {
            output.push_str(&line[index..]);
            break;
        };
        let start = index + relative;
        output.push_str(&line[index..start]);
        let end = line[start..]
            .find(|ch: char| ch.is_whitespace() || matches!(ch, '"' | '\'' | ',' | ';' | ')' | ']'))
            .map(|pos| start + pos)
            .unwrap_or(line.len());
        let end = if line[end..].starts_with(" Support/") {
            line[end + " Support/".len()..]
                .find(|ch: char| {
                    ch.is_whitespace() || matches!(ch, '"' | '\'' | ',' | ';' | ')' | ']')
                })
                .map(|pos| end + " Support/".len() + pos)
                .unwrap_or(line.len())
        } else {
            end
        };
        output.push_str("<path>");
        index = end;
    }
    output
}

fn field_value(line: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=");
    let mut search_from = 0;
    while let Some(relative) = line[search_from..].find(&needle) {
        let start = search_from + relative;
        if !is_field_boundary(line[..start].chars().next_back()) {
            search_from = start + needle.len();
            continue;
        }
        let value_start = start + needle.len();
        return read_field_value(line, value_start);
    }
    None
}

fn read_field_value(line: &str, start: usize) -> Option<String> {
    let rest = line.get(start..)?.trim_start();
    let start = line.len() - rest.len();
    if let Some(stripped) = rest.strip_prefix('"') {
        let mut escaped = false;
        for (offset, ch) in stripped.char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == '"' {
                let end = start + 1 + offset;
                return serde_json::from_str::<String>(&line[start..=end]).ok();
            }
        }
        None
    } else {
        let end = line[start..]
            .find(char::is_whitespace)
            .map(|pos| start + pos)
            .unwrap_or(line.len());
        (start < end).then(|| line[start..end].trim_matches('"').to_string())
    }
}

pub(super) fn take_leading_timestamp(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    if bytes.len() < 19 || !has_leading_timestamp(line) {
        return None;
    }
    let mut end = 19;
    while end < line.len() {
        let ch = line[end..].chars().next()?;
        if ch.is_ascii_digit()
            || matches!(ch, '.' | 'T' | 'Z' | '+' | '-' | ':' | '/')
            || (end == 10 && ch == ' ')
        {
            end += ch.len_utf8();
        } else {
            break;
        }
    }
    Some((&line[..end], &line[end..]))
}

fn strip_timestamped_level_prefix(line: &str) -> Option<&str> {
    let (_, rest) = take_leading_timestamp(line)?;
    let rest = rest.trim_start();
    let (_, rest) = take_bracket_token(rest)?;
    Some(rest.trim_start())
}

pub(super) fn strip_leading_level_prefix(input: &str) -> Option<&str> {
    for prefix in [
        "[INFO]",
        "[info]",
        "[DEBUG]",
        "[debug]",
        "[WARN]",
        "[warn]",
        "[WARNING]",
        "[warning]",
        "[ERROR]",
        "[error]",
        "[FATAL]",
        "[fatal]",
    ] {
        if let Some(rest) = input.trim_start().strip_prefix(prefix) {
            return Some(rest.trim_start());
        }
    }
    None
}

fn split_optional_timestamp(line: &str) -> Option<(&str, &str)> {
    take_leading_timestamp(line)
}

fn take_optional_bracket_level(input: &str) -> Option<(&str, &str)> {
    let (level, rest) = take_bracket_token(input.trim_start())?;
    matches!(
        level.trim().to_ascii_lowercase().as_str(),
        "i" | "info"
            | "d"
            | "debug"
            | "t"
            | "trace"
            | "w"
            | "warn"
            | "warning"
            | "e"
            | "error"
            | "f"
            | "fatal"
    )
    .then_some((level, rest))
}

fn current_log_timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn normalize_timestamp(timestamp: &str) -> String {
    let timestamp = timestamp.trim().trim_matches('"').replace('/', "-");
    let timestamp = timestamp.replace('T', " ");
    let timestamp = timestamp.trim_end_matches('Z').to_string();
    let timestamp = timestamp
        .split_once('+')
        .map(|(head, _)| head.to_string())
        .unwrap_or(timestamp);
    let timestamp = if timestamp.len() > 19 {
        if let Some(offset) = timestamp[19..].find('-') {
            timestamp[..19 + offset].to_string()
        } else {
            timestamp
        }
    } else {
        timestamp
    };
    if timestamp.len() < 19 {
        return timestamp;
    }
    let base = &timestamp[..19];
    base.to_string()
}

fn split_first_word(input: &str) -> Option<(&str, &str)> {
    let input = input.trim_start();
    let end = input.find(char::is_whitespace).unwrap_or(input.len());
    (end > 0).then(|| (&input[..end], input[end..].trim_start()))
}

fn take_bracket_token(input: &str) -> Option<(&str, &str)> {
    let rest = input.strip_prefix('[')?;
    let end = rest.find(']')?;
    Some((&rest[..end], &rest[end + 1..]))
}

fn level_from_token(token: &str) -> NativeLogLevel {
    match token.trim().to_ascii_lowercase().as_str() {
        "e" | "f" | "err" | "error" | "fatal" | "panic" => NativeLogLevel::Error,
        "w" | "wrn" | "warn" | "warning" => NativeLogLevel::Warn,
        "d" | "t" | "dbg" | "debug" | "trace" => NativeLogLevel::Debug,
        _ => NativeLogLevel::Info,
    }
}

fn detect_native_level(line: &str) -> NativeLogLevel {
    let lower = line.to_ascii_lowercase();
    if lower.contains("fatal")
        || lower.contains("panic")
        || lower.contains(" error")
        || lower.starts_with("error")
        || lower.contains("failed")
        || lower.contains("failure")
    {
        NativeLogLevel::Error
    } else if lower.contains("warn")
        || lower.contains("timeout")
        || lower.contains("unavailable")
        || lower.contains("not listening")
    {
        NativeLogLevel::Warn
    } else if lower.contains("debug") || lower.contains("trace") {
        NativeLogLevel::Debug
    } else {
        NativeLogLevel::Info
    }
}

fn first_url(line: &str) -> Option<&str> {
    let start = line
        .find("https://")
        .or_else(|| line.find("http://"))
        .or_else(|| line.find("tcp://"))
        .or_else(|| line.find("tls://"))?;
    let end = line[start..]
        .find(char::is_whitespace)
        .map(|pos| start + pos)
        .unwrap_or(line.len());
    Some(line[start..end].trim_matches(|ch| matches!(ch, ',' | ';' | ')' | ']')))
}

fn normalize_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_field_boundary(ch: Option<char>) -> bool {
    ch.map(|ch| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
        .unwrap_or(true)
}

pub(super) fn has_leading_timestamp(line: &str) -> bool {
    let bytes = line.as_bytes();
    if bytes.len() < 19 {
        return false;
    }
    matches!(
        bytes,
        [y1, y2, y3, y4, date_sep_1, m1, m2, date_sep_2, d1, d2, separator, h1, h2, b':', n1, n2, b':', s1, s2, ..]
            if y1.is_ascii_digit()
                && y2.is_ascii_digit()
                && y3.is_ascii_digit()
                && y4.is_ascii_digit()
                && matches!(date_sep_1, b'-' | b'/')
                && matches!(date_sep_2, b'-' | b'/')
                && matches!(separator, b' ' | b'T')
                && m1.is_ascii_digit()
                && m2.is_ascii_digit()
                && d1.is_ascii_digit()
                && d2.is_ascii_digit()
                && h1.is_ascii_digit()
                && h2.is_ascii_digit()
                && n1.is_ascii_digit()
                && n2.is_ascii_digit()
                && s1.is_ascii_digit()
                && s2.is_ascii_digit()
    )
}
