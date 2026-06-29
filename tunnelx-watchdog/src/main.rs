//! tunnelx-watchdog
//!
//! TunnelX 的平台级进程看门狗。主程序只启动一个 sidecar，由它通过行分隔 JSON
//! 协议托管 provider 进程。主程序正常退出时会主动下发停止，主程序异常退出时
//! watchdog 通过父进程检测兜底清理本地进程，并执行 provider 启动时下发的通用回收计划。

mod args;
mod cleanup;
mod emitter;
mod platform;
mod process;
mod relay;
mod session;
mod supervisor;

fn main() {
    let args = match args::parse_args() {
        Ok(args) => args,
        Err(error) => {
            eprintln!("[tunnelx-watchdog] argument error: {error}");
            std::process::exit(2);
        }
    };

    supervisor::run(args);
}
