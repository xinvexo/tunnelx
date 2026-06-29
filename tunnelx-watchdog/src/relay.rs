use crate::emitter::Emitter;
use crate::process::ProcessKey;
use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::runtime::{Builder, Runtime};
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;
use tunnelx_watchdog_protocol::{
    WatchdogEvent, WatchdogRelayBinding, WatchdogRelayEndpoint, WatchdogRelayProtocol,
    WatchdogRelaySample, WatchdogRelayStats,
};

const RELAY_HOST: &str = "127.0.0.1";
const RELAY_BIND: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const ACCEPT_BACKOFF: Duration = Duration::from_millis(25);
const UDP_RECV_BACKOFF: Duration = Duration::from_millis(10);
const UDP_PEER_IDLE: Duration = Duration::from_secs(60);
const RELAY_ERROR_THROTTLE: Duration = Duration::from_secs(5);
const HISTORY_KEEP_MS: i64 = 30_000;
const COPY_BUF_LEN: usize = 16 * 1024;
const UDP_BUF_LEN: usize = 64 * 1024;
// Worker threads for the relay runtime. The relay is IO-bound and multiplexes every connection
// onto these few threads via async tasks, so a small fixed pool scales to many connections.
const RELAY_WORKER_THREADS: usize = 2;
// Upper bound on dialing the real local service. Loopback refusals are instant; this only caps the
// pathological case (firewalled LAN/remote target) so a connection task can't hang for the OS
// default (tens of seconds).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Owns the async runtime that drives every relay and tracks the active relay set per connection.
/// The public surface stays synchronous so the (single-threaded) supervisor loop is unchanged: it
/// just calls prepare/release/clear/emit_stats.
#[derive(Default)]
pub(crate) struct RelayManager {
    // `active` is dropped before `runtime` (field order): cancelling the relay sets first lets the
    // tasks unwind, then the runtime shuts down cleanly.
    active: HashMap<ProcessKey, ActiveRelaySet>,
    runtime: Option<Arc<Runtime>>,
}

impl RelayManager {
    fn runtime(&mut self) -> Result<Arc<Runtime>, String> {
        if let Some(runtime) = &self.runtime {
            return Ok(runtime.clone());
        }
        let runtime = Builder::new_multi_thread()
            .worker_threads(RELAY_WORKER_THREADS)
            .enable_all()
            .thread_name("tunnelx-relay")
            .build()
            .map_err(|error| format!("failed to start relay runtime: {error}"))?;
        let runtime = Arc::new(runtime);
        self.runtime = Some(runtime.clone());
        Ok(runtime)
    }

    pub(crate) fn prepare(
        &mut self,
        key: ProcessKey,
        endpoints: Vec<WatchdogRelayEndpoint>,
        emitter: &Emitter,
    ) -> Result<Vec<WatchdogRelayBinding>, String> {
        self.release(&key);
        if endpoints.is_empty() {
            return Ok(Vec::new());
        }
        let runtime = self.runtime()?;
        let active = ActiveRelaySet::start(&runtime, key.clone(), endpoints, emitter)?;
        let bindings = active.bindings();
        self.active.insert(key, active);
        Ok(bindings)
    }

    pub(crate) fn release(&mut self, key: &ProcessKey) {
        self.active.remove(key);
    }

    pub(crate) fn clear(&mut self) {
        self.active.clear();
    }

    pub(crate) fn emit_stats(&self, emitter: &Emitter) {
        for (key, active) in &self.active {
            let endpoints = active.stats();
            if endpoints.is_empty() {
                continue;
            }
            emitter.send_event(WatchdogEvent::RelayStats {
                provider_id: key.provider_id.clone(),
                tunnel_id: key.tunnel_id.clone(),
                endpoints,
            });
        }
    }

    #[cfg(test)]
    pub(crate) fn stats_for(&self, key: &ProcessKey) -> Vec<WatchdogRelayStats> {
        self.active
            .get(key)
            .map(ActiveRelaySet::stats)
            .unwrap_or_default()
    }
}

struct ActiveRelaySet {
    cancel: CancellationToken,
    entries: Vec<RelayEntry>,
}

impl ActiveRelaySet {
    fn start(
        runtime: &Runtime,
        key: ProcessKey,
        endpoints: Vec<WatchdogRelayEndpoint>,
        emitter: &Emitter,
    ) -> Result<Self, String> {
        let cancel = CancellationToken::new();
        let mut entries = Vec::new();
        for endpoint in endpoints {
            match start_endpoint(runtime, &cancel, &key, endpoint, emitter) {
                Ok(entry) => entries.push(entry),
                Err(error) => {
                    // Tear down the endpoints already spawned for this set before bailing out.
                    cancel.cancel();
                    return Err(error);
                }
            }
        }
        Ok(Self { cancel, entries })
    }

    fn bindings(&self) -> Vec<WatchdogRelayBinding> {
        self.entries
            .iter()
            .map(|entry| entry.binding.clone())
            .collect()
    }

    fn stats(&self) -> Vec<WatchdogRelayStats> {
        self.entries.iter().map(RelayEntry::stats).collect()
    }
}

impl Drop for ActiveRelaySet {
    fn drop(&mut self) {
        // Cancelling the parent token cancels every endpoint loop and every per-connection task
        // spawned under it (they hold child tokens), so the whole relay set unwinds.
        self.cancel.cancel();
    }
}

/// Bind the loopback relay port synchronously (so the caller gets the port immediately) and spawn
/// the async accept loop onto the runtime. Returns the stats-bearing entry.
fn start_endpoint(
    runtime: &Runtime,
    cancel: &CancellationToken,
    key: &ProcessKey,
    endpoint: WatchdogRelayEndpoint,
    emitter: &Emitter,
) -> Result<RelayEntry, String> {
    let target = RelayTarget::parse(&endpoint.target, endpoint.protocol)?;
    let (bind, relay_port) = bind_std(endpoint.protocol)?;
    let binding = WatchdogRelayBinding {
        endpoint_id: endpoint.endpoint_id,
        endpoint_name: endpoint.endpoint_name,
        endpoint_type: endpoint.endpoint_type,
        protocol: endpoint.protocol,
        target: endpoint.target,
        relay_host: RELAY_HOST.into(),
        relay_port,
        relay_target: target.relay_target(relay_port),
    };
    let counters = Arc::new(RelayCounters::default());
    let reporter = RelayErrorReporter::new(key, &binding, emitter);
    let token = cancel.child_token();
    let dial = target.dial.clone();
    match bind {
        StdBind::Tcp(listener) => {
            runtime.spawn(run_tcp_relay(
                listener,
                dial,
                counters.clone(),
                token,
                reporter,
            ));
        }
        StdBind::Udp(socket) => {
            runtime.spawn(run_udp_relay(
                socket,
                dial,
                counters.clone(),
                token,
                reporter,
            ));
        }
    }
    Ok(RelayEntry {
        binding,
        counters,
        sample: Arc::new(Mutex::new(RelaySampleState::default())),
    })
}

struct RelayEntry {
    binding: WatchdogRelayBinding,
    counters: Arc<RelayCounters>,
    sample: Arc<Mutex<RelaySampleState>>,
}

impl RelayEntry {
    fn stats(&self) -> WatchdogRelayStats {
        let now = now_ms();
        let download_bytes = self.counters.download_bytes.load(Ordering::Relaxed);
        let upload_bytes = self.counters.upload_bytes.load(Ordering::Relaxed);
        let mut sample = self
            .sample
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let elapsed_ms = (now - sample.last_timestamp).max(0);
        let had_previous = sample.last_timestamp > 0;
        let bytes_changed = download_bytes != sample.last_download_bytes
            || upload_bytes != sample.last_upload_bytes;
        let was_moving = sample.download_speed > 0 || sample.upload_speed > 0;
        let (download_speed, upload_speed) = if had_previous && elapsed_ms > 0 {
            let elapsed = elapsed_ms as u64;
            (
                download_bytes
                    .saturating_sub(sample.last_download_bytes)
                    .saturating_mul(1000)
                    / elapsed,
                upload_bytes
                    .saturating_sub(sample.last_upload_bytes)
                    .saturating_mul(1000)
                    / elapsed,
            )
        } else {
            (0, 0)
        };
        sample.last_timestamp = now;
        sample.last_download_bytes = download_bytes;
        sample.last_upload_bytes = upload_bytes;
        sample.download_speed = download_speed;
        sample.upload_speed = upload_speed;
        if !had_previous || bytes_changed || was_moving {
            sample.history.push(WatchdogRelaySample {
                timestamp: now,
                download_bytes,
                upload_bytes,
                download_speed,
                upload_speed,
            });
        }
        let cutoff = now - HISTORY_KEEP_MS;
        sample.history.retain(|item| item.timestamp >= cutoff);

        WatchdogRelayStats {
            endpoint_id: self.binding.endpoint_id.clone(),
            endpoint_name: self.binding.endpoint_name.clone(),
            endpoint_type: self.binding.endpoint_type.clone(),
            protocol: self.binding.protocol,
            target: self.binding.target.clone(),
            relay_host: self.binding.relay_host.clone(),
            relay_port: self.binding.relay_port,
            download_bytes,
            upload_bytes,
            download_speed,
            upload_speed,
            total_bytes: download_bytes.saturating_add(upload_bytes),
            last_updated_at: now,
            history: sample.history.clone(),
        }
    }
}

#[derive(Default)]
struct RelayCounters {
    download_bytes: AtomicU64,
    upload_bytes: AtomicU64,
}

#[derive(Default)]
struct RelaySampleState {
    last_timestamp: i64,
    last_download_bytes: u64,
    last_upload_bytes: u64,
    download_speed: u64,
    upload_speed: u64,
    history: Vec<WatchdogRelaySample>,
}

struct RelayTarget {
    dial: DialTarget,
    prefix: Option<String>,
    suffix: String,
}

impl RelayTarget {
    fn parse(value: &str, protocol: WatchdogRelayProtocol) -> Result<Self, String> {
        let value = value.trim();
        if value.is_empty() {
            return Err("relay target is empty".into());
        }
        if protocol == WatchdogRelayProtocol::Udp {
            return Ok(Self {
                dial: DialTarget::new(value, None)?,
                prefix: None,
                suffix: String::new(),
            });
        }
        if let Some((scheme, rest)) = value.split_once("://") {
            let (authority, suffix) = split_authority(rest);
            let default_port = match scheme.to_ascii_lowercase().as_str() {
                "http" => Some(80),
                "https" => Some(443),
                _ => None,
            };
            return Ok(Self {
                dial: DialTarget::new(authority, default_port)?,
                prefix: Some(format!("{scheme}://")),
                suffix: suffix.to_string(),
            });
        }
        Ok(Self {
            dial: DialTarget::new(value, None)?,
            prefix: None,
            suffix: String::new(),
        })
    }

    fn relay_target(&self, relay_port: u16) -> String {
        match &self.prefix {
            Some(prefix) => format!("{prefix}{RELAY_HOST}:{relay_port}{}", self.suffix),
            None => format!("{RELAY_HOST}:{relay_port}"),
        }
    }
}

/// The address the relay dials for the real local service. An IP-literal target is resolved once
/// and reused (no per-connection DNS). A hostname target keeps its authority and re-resolves per
/// connection via async DNS, so record changes / failover take effect instead of pinning the first
/// address for the whole tunnel lifetime.
#[derive(Clone)]
struct DialTarget {
    authority: String,
    fixed: Option<SocketAddr>,
}

impl DialTarget {
    fn new(host_or_authority: &str, default_port: Option<u16>) -> Result<Self, String> {
        let authority = dial_authority(host_or_authority, default_port)?;
        // Validate up front (fail fast at prepare time) and cache the first resolved address.
        let first = authority
            .to_socket_addrs()
            .map_err(|error| {
                format!("failed to resolve relay target `{host_or_authority}`: {error}")
            })?
            .next()
            .ok_or_else(|| format!("failed to resolve relay target `{host_or_authority}`"))?;
        let fixed = host_is_ip_literal(&authority).then_some(first);
        Ok(Self { authority, fixed })
    }

    async fn resolve(&self) -> io::Result<SocketAddr> {
        if let Some(addr) = self.fixed {
            return Ok(addr);
        }
        tokio::net::lookup_host(self.authority.as_str())
            .await?
            .next()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("relay target `{}` did not resolve", self.authority),
                )
            })
    }
}

#[derive(Clone)]
struct RelayErrorReporter {
    emitter: Emitter,
    provider_id: String,
    tunnel_id: String,
    endpoint_id: String,
    endpoint_name: String,
    endpoint_type: String,
    target: String,
    last_sent: Arc<Mutex<HashMap<String, Instant>>>,
}

impl RelayErrorReporter {
    fn new(key: &ProcessKey, binding: &WatchdogRelayBinding, emitter: &Emitter) -> Self {
        Self {
            emitter: emitter.clone(),
            provider_id: key.provider_id.clone(),
            tunnel_id: key.tunnel_id.clone(),
            endpoint_id: binding.endpoint_id.clone(),
            endpoint_name: binding.endpoint_name.clone(),
            endpoint_type: binding.endpoint_type.clone(),
            target: binding.target.clone(),
            last_sent: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn send(&self, message: impl Into<String>) {
        let message = message.into();
        if !self.should_send(&message) {
            return;
        }
        self.emitter.send_event(WatchdogEvent::RelayError {
            provider_id: self.provider_id.clone(),
            tunnel_id: self.tunnel_id.clone(),
            endpoint_id: self.endpoint_id.clone(),
            endpoint_name: self.endpoint_name.clone(),
            endpoint_type: self.endpoint_type.clone(),
            target: self.target.clone(),
            message,
        });
    }

    fn should_send(&self, message: &str) -> bool {
        let now = Instant::now();
        let Ok(mut last_sent) = self.last_sent.lock() else {
            return true;
        };
        match last_sent.get(message) {
            Some(previous) if previous.elapsed() < RELAY_ERROR_THROTTLE => false,
            _ => {
                // Drop entries whose throttle window has elapsed so this map stays bounded to
                // recently seen messages instead of growing once per distinct error string.
                last_sent.retain(|_, sent| sent.elapsed() < RELAY_ERROR_THROTTLE);
                last_sent.insert(message.to_string(), now);
                true
            }
        }
    }
}

enum StdBind {
    Tcp(std::net::TcpListener),
    Udp(std::net::UdpSocket),
}

/// Bind synchronously with std so the relay port is known before any async work, then hand the
/// socket to tokio via `from_std` inside the spawned task. `from_std` requires non-blocking mode.
fn bind_std(protocol: WatchdogRelayProtocol) -> Result<(StdBind, u16), String> {
    let addr = SocketAddr::new(RELAY_BIND, 0);
    match protocol {
        WatchdogRelayProtocol::Tcp => {
            let listener = std::net::TcpListener::bind(addr)
                .map_err(|error| format!("failed to bind TCP relay: {error}"))?;
            listener
                .set_nonblocking(true)
                .map_err(|error| format!("failed to configure TCP relay: {error}"))?;
            let port = listener
                .local_addr()
                .map_err(|error| format!("failed to read TCP relay address: {error}"))?
                .port();
            Ok((StdBind::Tcp(listener), port))
        }
        WatchdogRelayProtocol::Udp => {
            let socket = std::net::UdpSocket::bind(addr)
                .map_err(|error| format!("failed to bind UDP relay: {error}"))?;
            socket
                .set_nonblocking(true)
                .map_err(|error| format!("failed to configure UDP relay: {error}"))?;
            // The inbound relay socket is unconnected and send_to()s responses back to clients; on
            // Windows that can make recv_from spuriously fail with WSAECONNRESET (see helper).
            disable_udp_conn_reset(&socket);
            let port = socket
                .local_addr()
                .map_err(|error| format!("failed to read UDP relay address: {error}"))?
                .port();
            Ok((StdBind::Udp(socket), port))
        }
    }
}

async fn run_tcp_relay(
    listener: std::net::TcpListener,
    target: DialTarget,
    counters: Arc<RelayCounters>,
    cancel: CancellationToken,
    reporter: RelayErrorReporter,
) {
    let listener = match TcpListener::from_std(listener) {
        Ok(listener) => listener,
        Err(error) => {
            reporter.send(format!("failed to start TCP relay: {error}"));
            return;
        }
    };
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    // Transparent loopback relay: disable Nagle so small request/response payloads
                    // aren't delayed by Nagle + delayed-ACK interactions.
                    let _ = stream.set_nodelay(true);
                    tokio::spawn(handle_tcp_connection(
                        stream,
                        target.clone(),
                        counters.clone(),
                        cancel.child_token(),
                        reporter.clone(),
                    ));
                }
                Err(error) => {
                    // A transient accept error (ECONNABORTED, EMFILE/ENFILE, EINTR) must not kill
                    // the relay; back off briefly and keep serving. Teardown exits via the token.
                    reporter.send(format!("relay accept failed: {error}"));
                    sleep(ACCEPT_BACKOFF).await;
                }
            },
        }
    }
}

async fn handle_tcp_connection(
    client: TcpStream,
    target: DialTarget,
    counters: Arc<RelayCounters>,
    cancel: CancellationToken,
    reporter: RelayErrorReporter,
) {
    // Resolve per connection (a no-op for IP-literal targets) and dial, all under one timeout.
    let server = match timeout(CONNECT_TIMEOUT, async {
        let addr = target.resolve().await?;
        TcpStream::connect(addr).await
    })
    .await
    {
        Ok(Ok(server)) => server,
        Ok(Err(error)) => {
            reporter.send(format!("unreachable: {error}"));
            return;
        }
        Err(_) => {
            reporter.send("unreachable: connect timed out");
            return;
        }
    };
    let _ = server.set_nodelay(true);
    let (client_reader, client_writer) = client.into_split();
    let (server_reader, server_writer) = server.into_split();
    let pump = async {
        // Each direction runs to its own EOF/error. A clean EOF only half-closes the peer's write
        // side (so an in-flight response on the other direction is never truncated); the join only
        // completes once both directions are done.
        tokio::join!(
            copy_counted(
                client_reader,
                server_writer,
                &counters.upload_bytes,
                &reporter,
                "upload",
            ),
            copy_counted(
                server_reader,
                client_writer,
                &counters.download_bytes,
                &reporter,
                "download",
            ),
        )
    };
    tokio::select! {
        _ = cancel.cancelled() => {}
        _ = pump => {}
    }
}

async fn copy_counted(
    mut reader: OwnedReadHalf,
    mut writer: OwnedWriteHalf,
    counter: &AtomicU64,
    reporter: &RelayErrorReporter,
    label: &str,
) {
    let mut buf = vec![0u8; COPY_BUF_LEN];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => {
                // Clean EOF: propagate a half-close so the peer learns this direction is done,
                // without tearing down the other direction that may still be transferring.
                let _ = writer.shutdown().await;
                return;
            }
            Ok(n) => {
                // Count bytes as they enter the relay (before forwarding) so the metric is settled
                // by the time the destination observes them.
                counter.fetch_add(n as u64, Ordering::Relaxed);
                if let Err(error) = writer.write_all(&buf[..n]).await {
                    report_stream_error(reporter, label, &error);
                    return;
                }
            }
            Err(error) => {
                report_stream_error(reporter, label, &error);
                return;
            }
        }
    }
}

fn report_stream_error(reporter: &RelayErrorReporter, label: &str, error: &io::Error) {
    // Connection-teardown kinds are normal at end of stream; only surface the rest.
    if is_reportable_stream_error(error) {
        reporter.send(format!("{label} stream failed: {error}"));
    }
}

fn is_reportable_stream_error(error: &io::Error) -> bool {
    !matches!(
        error.kind(),
        io::ErrorKind::WouldBlock
            | io::ErrorKind::TimedOut
            | io::ErrorKind::Interrupted
            | io::ErrorKind::UnexpectedEof
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::BrokenPipe
    )
}

struct UdpPeer {
    socket: Arc<UdpSocket>,
    task: JoinHandle<()>,
}

async fn run_udp_relay(
    socket: std::net::UdpSocket,
    target: DialTarget,
    counters: Arc<RelayCounters>,
    cancel: CancellationToken,
    reporter: RelayErrorReporter,
) {
    let inbound = match UdpSocket::from_std(socket) {
        Ok(socket) => Arc::new(socket),
        Err(error) => {
            reporter.send(format!("failed to start UDP relay: {error}"));
            return;
        }
    };
    let mut buf = vec![0u8; UDP_BUF_LEN];
    let mut peers: HashMap<SocketAddr, UdpPeer> = HashMap::new();
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            received = inbound.recv_from(&mut buf) => match received {
                Ok((n, peer)) => {
                    // Count at receive (before forwarding) so the metric is settled deterministically.
                    counters.upload_bytes.fetch_add(n as u64, Ordering::Relaxed);
                    let outbound = match peers.get(&peer) {
                        Some(existing) if !existing.task.is_finished() => existing.socket.clone(),
                        _ => match open_udp_peer(
                            &target,
                            peer,
                            inbound.clone(),
                            counters.clone(),
                            cancel.child_token(),
                            &reporter,
                        )
                        .await
                        {
                            Some(udp_peer) => {
                                let socket = udp_peer.socket.clone();
                                peers.insert(peer, udp_peer);
                                socket
                            }
                            None => continue,
                        },
                    };
                    if outbound.send(&buf[..n]).await.is_err() {
                        reporter.send("rejected UDP packet");
                    }
                    peers.retain(|_, udp_peer| !udp_peer.task.is_finished());
                }
                Err(error) => {
                    // ECONNREFUSED can surface here on Linux after a prior send to a down target;
                    // back off and keep serving rather than killing the relay.
                    reporter.send(format!("relay recv failed: {error}"));
                    sleep(UDP_RECV_BACKOFF).await;
                }
            },
        }
    }
}

async fn open_udp_peer(
    target: &DialTarget,
    peer: SocketAddr,
    inbound: Arc<UdpSocket>,
    counters: Arc<RelayCounters>,
    cancel: CancellationToken,
    reporter: &RelayErrorReporter,
) -> Option<UdpPeer> {
    // Resolve per new peer (a no-op for IP-literal targets) so hostname targets pick up DNS changes.
    let addr = match target.resolve().await {
        Ok(addr) => addr,
        Err(error) => {
            reporter.send(format!("unreachable: {error}"));
            return None;
        }
    };
    let outbound = match UdpSocket::bind(unspecified_socket_addr(addr)).await {
        Ok(socket) => socket,
        Err(error) => {
            reporter.send(format!("unreachable: {error}"));
            return None;
        }
    };
    // A connected UDP socket to a down target surfaces ICMP errors as recv failures; on Windows
    // that is WSAECONNRESET, which we suppress so a transient blip doesn't drop the peer.
    disable_udp_conn_reset(&outbound);
    if let Err(error) = outbound.connect(addr).await {
        reporter.send(format!("unreachable: {error}"));
        return None;
    }
    let outbound = Arc::new(outbound);
    let task = tokio::spawn(run_udp_peer(
        outbound.clone(),
        inbound,
        peer,
        counters,
        cancel,
    ));
    Some(UdpPeer {
        socket: outbound,
        task,
    })
}

async fn run_udp_peer(
    outbound: Arc<UdpSocket>,
    inbound: Arc<UdpSocket>,
    peer: SocketAddr,
    counters: Arc<RelayCounters>,
    cancel: CancellationToken,
) {
    let mut buf = vec![0u8; UDP_BUF_LEN];
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            received = timeout(UDP_PEER_IDLE, outbound.recv(&mut buf)) => match received {
                Ok(Ok(n)) => {
                    counters.download_bytes.fetch_add(n as u64, Ordering::Relaxed);
                    let _ = inbound.send_to(&buf[..n], peer).await;
                }
                // Recv error or idle timeout: this peer is done; the main loop reaps it.
                _ => break,
            },
        }
    }
}

fn unspecified_socket_addr(target: SocketAddr) -> SocketAddr {
    match target {
        SocketAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        SocketAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
    }
}

/// Disable Windows' `SIO_UDP_CONNRESET` behaviour on a UDP socket.
///
/// On Windows a UDP socket that has sent a datagram to an unreachable destination fails the *next*
/// recv with `WSAECONNRESET` (10054) when the ICMP port-unreachable comes back — even for an
/// unconnected socket. For a relay that means one dead client (inbound socket) or a momentarily
/// down target (peer socket) makes recv spuriously error. Linux/macOS only deliver such errors to
/// connected sockets, so this quirk is Windows-only; turning it off keeps the relay serving. Other
/// platforms: no-op.
#[cfg(windows)]
fn disable_udp_conn_reset<S: std::os::windows::io::AsRawSocket>(socket: &S) {
    use windows_sys::Win32::Networking::WinSock::{WSAIoctl, SIO_UDP_CONNRESET};
    let disabled: u32 = 0; // FALSE — stop surfacing ICMP resets on recv.
    let mut returned: u32 = 0;
    // Best effort: on failure the socket still works, just with the default reset behaviour.
    unsafe {
        WSAIoctl(
            socket.as_raw_socket() as _,
            SIO_UDP_CONNRESET,
            &disabled as *const u32 as *const core::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
            None,
        );
    }
}

#[cfg(not(windows))]
fn disable_udp_conn_reset<S>(_socket: &S) {}

fn split_authority(value: &str) -> (&str, &str) {
    match value.find('/') {
        Some(index) => (&value[..index], &value[index..]),
        None => (value, ""),
    }
}

/// Normalize a target into a `host:port` authority, filling in a scheme default port when the
/// caller didn't specify one.
fn dial_authority(value: &str, default_port: Option<u16>) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("relay target host is empty".into());
    }
    Ok(match default_port {
        Some(port) if !has_port(value) => format!("{trimmed}:{port}"),
        _ => trimmed.to_string(),
    })
}

/// Whether the host part of a `host:port` (or `[v6]:port`) authority is an IP literal, in which case
/// it never needs DNS resolution.
fn host_is_ip_literal(authority: &str) -> bool {
    let host = if let Some(rest) = authority.strip_prefix('[') {
        rest.split_once(']').map(|(host, _)| host).unwrap_or(rest)
    } else {
        authority
            .rsplit_once(':')
            .map(|(host, _)| host)
            .unwrap_or(authority)
    };
    host.parse::<IpAddr>().is_ok()
}

fn has_port(value: &str) -> bool {
    if value.starts_with('[') {
        return value
            .rsplit_once("]:")
            .and_then(|(_, port)| port.parse::<u16>().ok())
            .is_some();
    }
    value
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .is_some()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::{Read, Write};

    use std::net::{
        Shutdown, TcpListener as StdTcpListener, TcpStream as StdTcpStream,
        UdpSocket as StdUdpSocket,
    };
    use std::thread;

    fn endpoint(protocol: WatchdogRelayProtocol, target: String) -> WatchdogRelayEndpoint {
        WatchdogRelayEndpoint {
            endpoint_id: "endpoint-1".into(),
            endpoint_name: "Endpoint 1".into(),
            endpoint_type: "tcp".into(),
            protocol,
            target,
        }
    }

    #[test]
    fn tcp_relay_target_preserves_uri_suffix() {
        let target =
            RelayTarget::parse("http://127.0.0.1:8080/api?x=1", WatchdogRelayProtocol::Tcp)
                .expect("target should parse");

        assert_eq!(target.dial.authority, "127.0.0.1:8080");
        assert_eq!(target.relay_target(41234), "http://127.0.0.1:41234/api?x=1");
    }

    #[test]
    fn dial_target_distinguishes_ip_literal_from_hostname() {
        // IP-literal targets are pinned (resolved once, never re-resolved per connection).
        let literal = DialTarget::new("127.0.0.1:8080", None).expect("literal should parse");
        assert_eq!(literal.authority, "127.0.0.1:8080");
        assert!(literal.fixed.is_some());

        let bracketed = DialTarget::new("[::1]:9000", None).expect("v6 literal should parse");
        assert!(bracketed.fixed.is_some());

        // Hostname targets keep their authority and re-resolve per connection (fixed is None).
        let host = DialTarget::new("localhost", Some(80)).expect("hostname should parse");
        assert_eq!(host.authority, "localhost:80");
        assert!(host.fixed.is_none());
    }

    #[test]
    fn tcp_relay_forwards_and_counts_bytes() {
        let listener = StdTcpListener::bind((RELAY_HOST, 0)).expect("tcp echo should bind");
        let target = listener.local_addr().expect("tcp echo address");
        let echo = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("tcp echo should accept");
            let mut buf = [0u8; 32];
            let n = stream.read(&mut buf).expect("tcp echo should read");
            stream
                .write_all(&buf[..n])
                .expect("tcp echo should write response");
        });

        let key = ProcessKey::new("test".into(), "tcp".into());
        let mut manager = RelayManager::default();
        let bindings = manager
            .prepare(
                key.clone(),
                vec![endpoint(WatchdogRelayProtocol::Tcp, target.to_string())],
                &Emitter::new(),
            )
            .expect("relay should start");
        let relay_port = bindings[0].relay_port;
        let mut client = StdTcpStream::connect((RELAY_HOST, relay_port))
            .expect("client should connect to relay");
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("client timeout should set");

        client.write_all(b"hello").expect("client should write");
        let mut response = [0u8; 5];
        client
            .read_exact(&mut response)
            .expect("client should read echo");

        assert_eq!(&response, b"hello");
        echo.join().expect("tcp echo should finish");

        let stats = manager.stats_for(&key).remove(0);
        assert!(stats.upload_bytes >= 5);
        assert!(stats.download_bytes >= 5);
    }

    #[test]
    fn tcp_relay_forwards_http_request_to_real_service() {
        let listener = StdTcpListener::bind((RELAY_HOST, 0)).expect("http service should bind");
        let target = listener.local_addr().expect("http service address");
        let service = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("http service should accept");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("http service timeout should set");
            let mut request = Vec::new();
            let mut buf = [0u8; 64];
            while !request.windows(4).any(|chunk| chunk == b"\r\n\r\n") {
                let n = stream.read(&mut buf).expect("http service should read");
                assert!(n > 0, "http request closed before headers completed");
                request.extend_from_slice(&buf[..n]);
            }
            let request = String::from_utf8_lossy(&request);
            assert!(request.starts_with("GET /health HTTP/1.1\r\n"));
            assert!(request.contains(&format!("Host: {target}\r\n")));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK")
                .expect("http service should write response");
        });

        let key = ProcessKey::new("test".into(), "http".into());
        let mut manager = RelayManager::default();
        let bindings = manager
            .prepare(
                key.clone(),
                vec![endpoint(
                    WatchdogRelayProtocol::Tcp,
                    format!("http://{target}"),
                )],
                &Emitter::new(),
            )
            .expect("relay should start");
        assert_eq!(
            bindings[0].relay_target,
            format!("http://{RELAY_HOST}:{}", bindings[0].relay_port)
        );
        let mut client = StdTcpStream::connect((RELAY_HOST, bindings[0].relay_port))
            .expect("client should connect to relay");
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("client timeout should set");
        client
            .write_all(
                format!("GET /health HTTP/1.1\r\nHost: {target}\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .expect("client should write request");
        let mut response = String::new();
        client
            .read_to_string(&mut response)
            .expect("client should read response");

        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(response.ends_with("\r\n\r\nOK"));
        service.join().expect("http service should finish");

        let stats = manager.stats_for(&key).remove(0);
        assert!(stats.upload_bytes > 0);
        assert!(stats.download_bytes > 0);
    }

    #[test]
    fn tcp_relay_forwards_request_sent_after_idle_connect() {
        // Regression: a client that connects and only later sends its request (HTTP keep-alive /
        // connection pooling, like cloudflared) must still have the delayed request forwarded.
        let listener = StdTcpListener::bind((RELAY_HOST, 0)).expect("service should bind");
        let target = listener.local_addr().expect("service address");
        let service = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("service should accept");
            let mut buf = [0u8; 16];
            let n = stream.read(&mut buf).expect("service should read");
            stream.write_all(&buf[..n]).expect("service should echo");
        });

        let mut manager = RelayManager::default();
        let bindings = manager
            .prepare(
                ProcessKey::new("test".into(), "idle".into()),
                vec![endpoint(WatchdogRelayProtocol::Tcp, target.to_string())],
                &Emitter::new(),
            )
            .expect("relay should start");
        let mut client = StdTcpStream::connect((RELAY_HOST, bindings[0].relay_port))
            .expect("client should connect to relay");
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("client timeout should set");

        // Stay idle past the accept handoff before sending, mimicking a pooled connection.
        thread::sleep(Duration::from_millis(150));
        client.write_all(b"ping").expect("client should write");

        let mut response = [0u8; 4];
        client
            .read_exact(&mut response)
            .expect("relay should forward the delayed request");
        assert_eq!(&response, b"ping");
        service.join().expect("service should finish");
    }

    #[test]
    fn tcp_relay_serves_multiple_sequential_connections() {
        // Guards accept-loop liveness: a single relay must keep serving across many connections.
        let listener = StdTcpListener::bind((RELAY_HOST, 0)).expect("echo should bind");
        let target = listener.local_addr().expect("echo address");
        let service = thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().expect("echo should accept");
                let mut buf = [0u8; 16];
                let n = stream.read(&mut buf).expect("echo should read");
                stream.write_all(&buf[..n]).expect("echo should write");
            }
        });

        let mut manager = RelayManager::default();
        let bindings = manager
            .prepare(
                ProcessKey::new("test".into(), "multi".into()),
                vec![endpoint(WatchdogRelayProtocol::Tcp, target.to_string())],
                &Emitter::new(),
            )
            .expect("relay should start");
        let relay_port = bindings[0].relay_port;

        for i in 0..3u8 {
            let mut client = StdTcpStream::connect((RELAY_HOST, relay_port))
                .expect("client should connect to relay");
            client
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("client timeout should set");
            let payload = [b'a' + i];
            client.write_all(&payload).expect("client should write");
            let mut response = [0u8; 1];
            client
                .read_exact(&mut response)
                .expect("relay should forward each connection");
            assert_eq!(response, payload);
        }

        service.join().expect("echo should finish");
    }

    #[test]
    fn tcp_relay_handles_many_concurrent_connections() {
        // The async reactor must multiplex many simultaneous connections onto its small worker pool
        // (the old thread-per-connection model spent ~2 threads + ~6 fds each).
        const CONNS: usize = 50;
        let listener = StdTcpListener::bind((RELAY_HOST, 0)).expect("echo should bind");
        let target = listener.local_addr().expect("echo address");
        let service = thread::spawn(move || {
            let mut handlers = Vec::new();
            for _ in 0..CONNS {
                let (mut stream, _) = listener.accept().expect("echo should accept");
                handlers.push(thread::spawn(move || {
                    let mut buf = [0u8; 1];
                    if stream.read_exact(&mut buf).is_ok() {
                        let _ = stream.write_all(&buf);
                    }
                }));
            }
            for handler in handlers {
                let _ = handler.join();
            }
        });

        let mut manager = RelayManager::default();
        let bindings = manager
            .prepare(
                ProcessKey::new("test".into(), "concurrent".into()),
                vec![endpoint(WatchdogRelayProtocol::Tcp, target.to_string())],
                &Emitter::new(),
            )
            .expect("relay should start");
        let relay_port = bindings[0].relay_port;

        let clients = (0..CONNS)
            .map(|i| {
                thread::spawn(move || {
                    let mut client = StdTcpStream::connect((RELAY_HOST, relay_port))
                        .expect("client should connect to relay");
                    client
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .expect("client timeout should set");
                    let payload = [i as u8];
                    client.write_all(&payload).expect("client should write");
                    let mut response = [0u8; 1];
                    client
                        .read_exact(&mut response)
                        .expect("relay should forward concurrently");
                    assert_eq!(response, payload);
                })
            })
            .collect::<Vec<_>>();

        for client in clients {
            client.join().expect("client thread should succeed");
        }
        service.join().expect("echo should finish");
    }

    #[test]
    fn tcp_relay_preserves_response_after_client_half_close() {
        // Regression for half-close truncation: a client that finishes its request and shuts down its
        // write side must still receive the full response the server sends back.
        const RESPONSE_LEN: usize = 256 * 1024;
        let listener = StdTcpListener::bind((RELAY_HOST, 0)).expect("service should bind");
        let target = listener.local_addr().expect("service address");
        let service = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("service should accept");
            // Drain the request until the client half-closes (read returns 0).
            let mut buf = [0u8; 1024];
            loop {
                let n = stream.read(&mut buf).expect("service should read request");
                if n == 0 {
                    break;
                }
            }
            // Client is done sending; respond with a large payload, then close.
            let payload = vec![b'z'; RESPONSE_LEN];
            stream
                .write_all(&payload)
                .expect("service should write full response");
        });

        let mut manager = RelayManager::default();
        let bindings = manager
            .prepare(
                ProcessKey::new("test".into(), "halfclose".into()),
                vec![endpoint(WatchdogRelayProtocol::Tcp, target.to_string())],
                &Emitter::new(),
            )
            .expect("relay should start");
        let mut client = StdTcpStream::connect((RELAY_HOST, bindings[0].relay_port))
            .expect("client should connect to relay");
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("client timeout should set");
        client
            .write_all(b"REQUEST")
            .expect("client should write request");
        client
            .shutdown(Shutdown::Write)
            .expect("client should half-close its write side");

        let mut response = Vec::new();
        client
            .read_to_end(&mut response)
            .expect("client should read the full response after half-close");
        assert_eq!(response.len(), RESPONSE_LEN);
        assert!(response.iter().all(|byte| *byte == b'z'));

        service.join().expect("service should finish");
    }

    #[test]
    fn udp_relay_keeps_peer_responses_separate() {
        let target = StdUdpSocket::bind((RELAY_HOST, 0)).expect("udp echo should bind");
        let target_addr = target.local_addr().expect("udp echo address");
        target
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("udp echo timeout should set");
        let echo = thread::spawn(move || {
            let mut buf = [0u8; 32];
            for _ in 0..2 {
                let (n, peer) = target.recv_from(&mut buf).expect("udp echo should read");
                target
                    .send_to(&buf[..n], peer)
                    .expect("udp echo should send");
            }
        });

        let key = ProcessKey::new("test".into(), "udp".into());
        let mut manager = RelayManager::default();
        let bindings = manager
            .prepare(
                key.clone(),
                vec![endpoint(
                    WatchdogRelayProtocol::Udp,
                    target_addr.to_string(),
                )],
                &Emitter::new(),
            )
            .expect("relay should start");
        let relay_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), bindings[0].relay_port);
        let first = StdUdpSocket::bind((RELAY_HOST, 0)).expect("first client should bind");
        let second = StdUdpSocket::bind((RELAY_HOST, 0)).expect("second client should bind");
        first
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("first client timeout should set");
        second
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("second client timeout should set");

        first
            .send_to(b"one", relay_addr)
            .expect("first client should send");
        second
            .send_to(b"two", relay_addr)
            .expect("second client should send");

        let mut first_buf = [0u8; 8];
        let first_len = first
            .recv(&mut first_buf)
            .expect("first client should receive");
        let mut second_buf = [0u8; 8];
        let second_len = second
            .recv(&mut second_buf)
            .expect("second client should receive");

        assert_eq!(&first_buf[..first_len], b"one");
        assert_eq!(&second_buf[..second_len], b"two");
        echo.join().expect("udp echo should finish");

        let stats = manager.stats_for(&key).remove(0);
        assert!(stats.upload_bytes >= 6);
        assert!(stats.download_bytes >= 6);
    }
}
