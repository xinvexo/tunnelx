use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
const TERM_GRACE: Duration = Duration::from_secs(3);

pub(crate) struct ProcessGroup {
    #[cfg(windows)]
    job: Option<windows_impl::Job>,
}

impl ProcessGroup {
    pub(crate) fn new() -> Self {
        Self {
            #[cfg(windows)]
            job: windows_impl::create_kill_on_close_job(),
        }
    }

    pub(crate) fn assign(&self, child: &Child) {
        #[cfg(windows)]
        if let Some(job) = self.job.as_ref() {
            windows_impl::assign(job, child);
        }

        #[cfg(not(windows))]
        {
            let _ = child;
        }
    }
}

pub(crate) fn configure_command(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows_impl::CREATE_NO_WINDOW);
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: pre_exec 在 fork 之后、exec 之前的子进程里运行，只调用
        // async-signal-safe 的系统调用（setsid / prctl）。
        unsafe {
            command.pre_exec(|| {
                libc::setsid();
                #[cfg(target_os = "linux")]
                libc::prctl(
                    libc::PR_SET_PDEATHSIG,
                    libc::SIGKILL as libc::c_ulong,
                    0 as libc::c_ulong,
                    0 as libc::c_ulong,
                    0 as libc::c_ulong,
                );
                Ok(())
            });
        }
    }
}

pub(crate) fn wait_for(child: &mut Child, grace: Duration) -> bool {
    let deadline = Instant::now() + grace;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => {}
            Err(_) => return false,
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(unix)]
pub(crate) fn force_kill(child: &mut Child) {
    let pid = child.id() as i32;
    unsafe {
        libc::kill(-pid, libc::SIGTERM);
    }
    if !wait_for(child, TERM_GRACE) {
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
        let _ = child.kill();
    }
    let _ = child.wait();
}

#[cfg(windows)]
pub(crate) fn force_kill(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
pub(crate) fn parent_alive(pid: u32) -> bool {
    if unsafe { libc::kill(pid as i32, 0) } == 0 {
        return true;
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ESRCH) => false,
        Some(libc::EPERM) => true,
        _ => true,
    }
}

#[cfg(windows)]
pub(crate) fn parent_alive(pid: u32) -> bool {
    windows_impl::process_alive(pid)
}

#[cfg(windows)]
mod windows_impl {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };

    pub(super) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub(super) struct Job(HANDLE);

    impl Drop for Job {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CloseHandle(self.0) };
            }
        }
    }

    pub(super) fn create_kill_on_close_job() -> Option<Job> {
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return None;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok == 0 {
                CloseHandle(job);
                return None;
            }
            Some(Job(job))
        }
    }

    pub(super) fn assign(job: &Job, child: &Child) -> bool {
        unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle() as HANDLE) != 0 }
    }

    pub(super) fn process_alive(pid: u32) -> bool {
        unsafe {
            let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if handle.is_null() {
                return false;
            }
            let result = WaitForSingleObject(handle, 0);
            CloseHandle(handle);
            result != WAIT_OBJECT_0
        }
    }
}
