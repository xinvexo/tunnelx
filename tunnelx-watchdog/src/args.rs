pub(crate) struct Args {
    pub(crate) parent_pid: u32,
    pub(crate) session_path: Option<String>,
    pub(crate) session_token: Option<String>,
    pub(crate) heartbeat_timeout_ms: u64,
}

const DEFAULT_HEARTBEAT_TIMEOUT_MS: u64 = 180_000;

pub(crate) fn parse_args() -> Result<Args, String> {
    let mut parent_pid: Option<u32> = None;
    let mut session_path: Option<String> = None;
    let mut session_token: Option<String> = None;
    let mut heartbeat_timeout_ms: u64 = DEFAULT_HEARTBEAT_TIMEOUT_MS;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--parent-pid" => {
                let value = it.next().ok_or("--parent-pid requires a value")?;
                parent_pid = Some(value.parse().map_err(|_| "--parent-pid must be a number")?);
            }
            "--session-path" => {
                session_path = Some(it.next().ok_or("--session-path requires a value")?);
            }
            "--session-token" => {
                session_token = Some(it.next().ok_or("--session-token requires a value")?);
            }
            "--heartbeat-timeout-ms" => {
                let value = it.next().ok_or("--heartbeat-timeout-ms requires a value")?;
                heartbeat_timeout_ms = value
                    .parse()
                    .map_err(|_| "--heartbeat-timeout-ms must be a number")?;
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Args {
        parent_pid: parent_pid.ok_or("missing --parent-pid")?,
        session_path,
        session_token,
        heartbeat_timeout_ms,
    })
}
