use crate::{
    audit::Capture,
    model::{Failure, Outcome},
};
use std::{
    ffi::{OsStr, c_void},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions},
    process::Command,
};
use windows_sys::Win32::{
    Foundation::{LocalFree, STILL_ACTIVE},
    Security::{Authorization::*, *},
    System::{Pipes::*, Threading::*},
};

const PREFIX: &str = r"\\.\pipe\antigravity-worker-start-";
const TASK_PREFIX: &str = "antigravity-worker-start-";
const PACKET_LIMIT: usize = 1_048_576;

fn failure(stage: &str) -> Failure {
    Failure {
        code: "BACKGROUND_UNAVAILABLE".into(),
        message: format!(
            "Windows desktop supervisor launch failed during {stage}. An existing interactive sign-in and current-user Task Scheduler permission are required."
        ),
    }
}
fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}
fn sid(process: *mut c_void) -> Outcome<String> {
    let mut raw = std::ptr::null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut raw) } == 0 {
        return Err(failure("identity lookup"));
    }
    let token = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut bytes = 0;
    unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut bytes,
        )
    };
    let mut buffer = vec![0usize; (bytes as usize).div_ceil(std::mem::size_of::<usize>())];
    if bytes == 0
        || unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenUser,
                buffer.as_mut_ptr().cast(),
                bytes,
                &mut bytes,
            )
        } == 0
    {
        return Err(failure("identity lookup"));
    }
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut text = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) } == 0 {
        return Err(failure("identity conversion"));
    }
    let mut length = 0;
    while unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    let result = String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) });
    unsafe {
        LocalFree(text.cast());
    }
    result.map_err(|_| failure("identity conversion"))
}

pub struct Process {
    handle: OwnedHandle,
    pub id: u32,
    startup: bool,
}
impl Drop for Process {
    fn drop(&mut self) {
        if self.startup {
            unsafe {
                TerminateProcess(self.handle.as_raw_handle(), 1);
            }
        }
    }
}
impl Process {
    pub fn exited(&self) -> Outcome<bool> {
        let mut code = 0;
        if unsafe { GetExitCodeProcess(self.handle.as_raw_handle(), &mut code) } == 0 {
            return Err(failure("process status"));
        }
        Ok(code != STILL_ACTIVE as u32)
    }
    pub async fn wait(&self) -> Outcome<()> {
        while !self.exited()? {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(())
    }
    pub async fn stop(&self) -> Outcome<()> {
        if !self.exited()? && unsafe { TerminateProcess(self.handle.as_raw_handle(), 1) } == 0 {
            return Err(failure("startup cleanup"));
        }
        self.wait().await
    }
}
fn peer(id: u32, expected: &Path, terminate: bool) -> Outcome<Process> {
    let rights = PROCESS_QUERY_LIMITED_INFORMATION | if terminate { PROCESS_TERMINATE } else { 0 };
    let raw = unsafe { OpenProcess(rights, 0, id) };
    if raw.is_null() {
        return Err(failure("pipe peer lookup"));
    }
    let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
    if sid(handle.as_raw_handle())? != sid(unsafe { GetCurrentProcess() })? {
        return Err(failure("pipe peer identity"));
    }
    let mut name = vec![0u16; 32768];
    let mut length = name.len() as u32;
    if unsafe {
        QueryFullProcessImageNameW(handle.as_raw_handle(), 0, name.as_mut_ptr(), &mut length)
    } == 0
    {
        return Err(failure("pipe peer executable"));
    }
    let actual = String::from_utf16(&name[..length as usize])
        .map_err(|_| failure("pipe peer executable"))?;
    let actual = std::fs::canonicalize(actual).map_err(|_| failure("pipe peer executable"))?;
    let expected = std::fs::canonicalize(expected).map_err(|_| failure("pipe peer executable"))?;
    if !actual
        .as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&expected.as_os_str().to_string_lossy())
    {
        return Err(failure("pipe peer executable"));
    }
    Ok(Process {
        handle,
        id,
        startup: terminate,
    })
}

pub struct Handoff {
    pipe: NamedPipeServer,
    pub name: String,
    task: String,
}
impl Handoff {
    pub fn new() -> Outcome<Self> {
        let id = uuid::Uuid::new_v4();
        let name = format!("{PREFIX}{id}");
        let task = format!("{TASK_PREFIX}{id}");
        let user = sid(unsafe { GetCurrentProcess() })?;
        let descriptor = wide(OsStr::new(&format!("D:P(A;;GA;;;{user})")));
        let mut security = std::ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                descriptor.as_ptr(),
                SDDL_REVISION_1,
                &mut security,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(failure("private pipe permissions"));
        }
        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: security,
            bInheritHandle: 0,
        };
        let result = unsafe {
            ServerOptions::new()
                .first_pipe_instance(true)
                .reject_remote_clients(true)
                .create_with_security_attributes_raw(
                    &name,
                    (&mut attributes as *mut SECURITY_ATTRIBUTES).cast(),
                )
        };
        unsafe {
            LocalFree(security.cast());
        }
        let pipe = result.map_err(|_| failure("private pipe creation"))?;
        Ok(Self { pipe, name, task })
    }
    pub async fn deliver(&mut self, executable: &Path, bytes: &[u8]) -> Outcome<Process> {
        if bytes.len() > PACKET_LIMIT {
            return Err(failure("input limit"));
        }
        self.pipe
            .connect()
            .await
            .map_err(|_| failure("private pipe connection"))?;
        let mut id = 0;
        if unsafe { GetNamedPipeClientProcessId(self.pipe.as_raw_handle(), &mut id) } == 0 {
            return Err(failure("pipe peer lookup"));
        }
        let mut process = peer(id, executable, true)?;
        if let Err(error) = async {
            self.pipe
                .write_u32_le(bytes.len() as u32)
                .await
                .map_err(|_| failure("input delivery"))?;
            self.pipe
                .write_all(bytes)
                .await
                .map_err(|_| failure("input delivery"))?;
            self.pipe
                .flush()
                .await
                .map_err(|_| failure("input delivery"))
        }
        .await
        {
            let _ = process.stop().await;
            return Err(error);
        }
        process.startup = false;
        Ok(process)
    }
}

const START: &str = r#"
$ErrorActionPreference='Stop'
$service=New-Object -ComObject Schedule.Service
$service.Connect()
$folder=$service.GetFolder('\')
$definition=$service.NewTask(0)
$user=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$definition.Principal.UserId=$user
$definition.Principal.LogonType=3
$definition.Principal.RunLevel=0
$definition.Settings.ExecutionTimeLimit='PT0S'
$definition.Settings.DisallowStartIfOnBatteries=$false
$definition.Settings.StopIfGoingOnBatteries=$false
$definition.Settings.Hidden=$true
$definition.Settings.MultipleInstances=2
$definition.Settings.DeleteExpiredTaskAfter='PT1M'
$trigger=$definition.Triggers.Create(1)
$trigger.StartBoundary=(Get-Date).ToString('s')
$trigger.EndBoundary=(Get-Date).AddMinutes(1).ToString('s')
$trigger.Enabled=$false
$action=$definition.Actions.Create(0)
$action.Path=$env:AG_MCP_START_EXE
$action.Arguments='--run-job-pipe "'+$env:AG_MCP_START_PIPE+'"'
$registered=$false
try {
 $task=$folder.RegisterTaskDefinition($env:AG_MCP_START_TASK,$definition,2,$user,$null,3,('D:P(A;;GA;;;SY)(A;;GA;;;'+$user+')'))
 $registered=$true
 $task.Run($null)|Out-Null
} catch {
 if($registered){$folder.DeleteTask($env:AG_MCP_START_TASK,0)}
 exit 1
}
"#;
const REMOVE: &str = r#"
$ErrorActionPreference='Stop'
$service=New-Object -ComObject Schedule.Service
$service.Connect()
try {$service.GetFolder('\').DeleteTask($env:AG_MCP_START_TASK,0)} catch {if($_.Exception.HResult-ne -2147024894){exit 1}}
"#;
async fn helper(
    script: &str,
    task: &str,
    executable: Option<&Path>,
    pipe: Option<&str>,
    capture: Option<&Capture>,
) -> Outcome<()> {
    let root = std::env::var_os("SystemRoot").ok_or_else(|| failure("Windows runtime lookup"))?;
    let powershell = Path::new(&root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut command = Command::new(powershell);
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .env("AG_MCP_START_TASK", task)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .creation_flags(0x08000000)
        .kill_on_drop(true);
    if let Some(executable) = executable {
        command.env("AG_MCP_START_EXE", executable);
    }
    if let Some(pipe) = pipe {
        command.env("AG_MCP_START_PIPE", pipe);
    }
    let output = tokio::time::timeout(Duration::from_secs(10), command.output())
        .await
        .map_err(|_| failure("Task Scheduler timeout"))?
        .map_err(|_| failure("Task Scheduler invocation"))?;
    if let Some(capture) = capture {
        capture.bytes("stdout", &output.stdout)?;
        capture.bytes("stderr", &output.stderr)?;
    }
    if !output.status.success() {
        return Err(failure("Task Scheduler registration"));
    }
    Ok(())
}
pub async fn launch(executable: &Path, bytes: &[u8], capture: Capture) -> Outcome<Process> {
    let mut handoff = Handoff::new()?;
    let task = handoff.task.clone();
    let name = handoff.name.clone();
    capture.record(
        "windows.desktop.launch",
        serde_json::json!({"task":task,"pipe":name,"interactive_token":true,"elevation":false}),
    )?;
    helper(
        START,
        &task,
        Some(executable),
        Some(&name),
        Some(&capture.nested("start")),
    )
    .await?;
    let delivered =
        tokio::time::timeout(Duration::from_secs(10), handoff.deliver(executable, bytes))
            .await
            .map_err(|_| failure("desktop startup timeout"));
    let cleanup = helper(REMOVE, &task, None, None, Some(&capture.nested("cleanup"))).await;
    match delivered {
        Ok(Ok(process)) => {
            if capture.failure_token().is_cancelled() {
                let _ = process.stop().await;
                return Err(Failure::new(
                    "AUDIT_FAILED",
                    "Desktop-launch audit logging failed; startup was stopped.",
                ));
            }
            if let Err(error) = capture.record(
                "windows.desktop.registration_cleanup",
                serde_json::json!({"removed":cleanup.is_ok(),"expiry_fallback":cleanup.is_err()}),
            ) {
                let _ = process.stop().await;
                return Err(error);
            }
            Ok(process)
        }
        Ok(Err(error)) | Err(error) => Err(error),
    }
}
pub async fn receive(name: &OsStr) -> Outcome<Vec<u8>> {
    let name = name.to_str().ok_or_else(|| failure("pipe name"))?;
    let id = name
        .strip_prefix(PREFIX)
        .ok_or_else(|| failure("pipe name"))?;
    uuid::Uuid::parse_str(id).map_err(|_| failure("pipe name"))?;
    let task = format!("{TASK_PREFIX}{id}");
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let mut pipe = ClientOptions::new()
            .open(name)
            .map_err(|_| failure("private pipe connection"))?;
        let mut server = 0;
        if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut server) } == 0 {
            return Err(failure("pipe peer lookup"));
        }
        let executable = std::env::current_exe().map_err(|_| failure("pipe peer executable"))?;
        let _server = peer(server, &executable, false)?;
        let length = pipe
            .read_u32_le()
            .await
            .map_err(|_| failure("input delivery"))? as usize;
        if length > PACKET_LIMIT {
            return Err(failure("input limit"));
        }
        let mut bytes = vec![0u8; length];
        pipe.read_exact(&mut bytes)
            .await
            .map_err(|_| failure("input delivery"))?;
        Ok(bytes)
    })
    .await
    .map_err(|_| failure("input delivery timeout"));
    let _ = helper(REMOVE, &task, None, None, None).await;
    result?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn private_pipe_transfers_bytes_between_verified_processes() {
        let mut handoff = Handoff::new().unwrap();
        let executable = std::env::current_exe().unwrap();
        let mut child = Command::new(&executable)
            .args([
                "--ignored",
                "--exact",
                "windows_background::tests::pipe_child_receives_private_input",
            ])
            .env("AG_MCP_TEST_PIPE", &handoff.name)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let process = tokio::time::timeout(
            Duration::from_secs(10),
            handoff.deliver(&executable, b"\0private-input\xff"),
        )
        .await
        .unwrap()
        .unwrap();
        let status = child.wait().await.unwrap();
        assert!(status.success());
        assert!(process.exited().unwrap());
    }

    #[tokio::test]
    #[ignore = "Invoked as the verified child of the private pipe transport check."]
    async fn pipe_child_receives_private_input() {
        let name = std::env::var_os("AG_MCP_TEST_PIPE").unwrap();
        assert_eq!(receive(&name).await.unwrap(), b"\0private-input\xff");
    }
}
