use crate::model::{Failure, Outcome, io_failure};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::{fs, path::Path};

pub fn private_mode(path: &Path, mode: u32) -> Outcome<()> {
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(io_failure)?;
    #[cfg(windows)]
    let _ = (path, mode);
    Ok(())
}

pub fn is_private(meta: &fs::Metadata) -> bool {
    #[cfg(unix)]
    return meta.uid() == unsafe { libc::geteuid() } && meta.mode() & 0o077 == 0;
    #[cfg(windows)]
    return !is_reparse(meta);
}

pub fn executable(meta: &fs::Metadata) -> bool {
    #[cfg(unix)]
    return meta.is_file() && meta.mode() & 0o111 != 0;
    #[cfg(windows)]
    return meta.is_file();
}

#[cfg(windows)]
pub fn is_reparse(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes() & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
        != 0
}

pub fn unchanged(before: &fs::Metadata, after: &fs::Metadata) -> bool {
    let same = before.len() == after.len() && before.modified().ok() == after.modified().ok();
    #[cfg(unix)]
    return same && before.ctime() == after.ctime() && before.ctime_nsec() == after.ctime_nsec();
    #[cfg(windows)]
    return same && before.created().ok() == after.created().ok();
}

pub fn lock(config: &crate::model::Config) -> Outcome<Option<fs::File>> {
    let mut options = fs::OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(unix)]
    options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options
        .open(config.state_directory.join("execution.lock"))
        .map_err(io_failure)?;
    let meta = file.metadata().map_err(io_failure)?;
    if !meta.is_file() || !is_private(&meta) {
        return Err(Failure::new(
            "STATE_UNSAFE",
            "Execution lock must be a regular file in the private state directory.",
        ));
    }
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(fs::TryLockError::WouldBlock) => Ok(None),
        Err(fs::TryLockError::Error(error)) => Err(io_failure(error)),
    }
}

#[cfg(unix)]
pub struct ProcessGuard(i32);
#[cfg(unix)]
impl ProcessGuard {
    pub fn attach(child: &mut tokio::process::Child) -> Outcome<Self> {
        let pid = child.id().unwrap_or_default() as i32;
        if pid <= 1 {
            return Err(io_failure("process group"));
        }
        Ok(Self(pid))
    }
    fn signal(&self, signal: i32) {
        unsafe { libc::kill(-self.0, signal) };
    }
    pub fn terminate(&self) {
        self.signal(libc::SIGTERM);
    }
    pub fn kill(&self) {
        self.signal(libc::SIGKILL);
    }
}

#[cfg(windows)]
pub struct ProcessGuard(std::os::windows::io::OwnedHandle);
#[cfg(windows)]
impl ProcessGuard {
    pub fn attach(child: &mut tokio::process::Child) -> Outcome<Self> {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::System::JobObjects::*;
        let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if raw.is_null() {
            return Err(io_failure("job creation"));
        }
        let job = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let process = child
            .raw_handle()
            .ok_or_else(|| io_failure("process handle"))?;
        if unsafe {
            SetInformationJobObject(
                job.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            )
        } == 0
        {
            return Err(io_failure("job limits"));
        }
        if unsafe { AssignProcessToJobObject(job.as_raw_handle(), process) } == 0
            && child.try_wait().map_err(io_failure)?.is_none()
        {
            return Err(Failure::new(
                "PROCESS_SUPERVISION_FAILED",
                "Cannot assign the CLI to a supervised Windows job.",
            ));
        }
        Ok(Self(job))
    }
    pub fn terminate(&self) {
        self.kill();
    }
    pub fn kill(&self) {
        use std::os::windows::io::AsRawHandle;
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0.as_raw_handle(), 1);
        }
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        self.kill();
    }
}
