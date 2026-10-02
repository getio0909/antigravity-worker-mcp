use crate::platform;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Failure {
    pub code: String,
    pub message: String,
}
impl Failure {
    pub fn new(code: &'static str, message: &'static str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}
pub type Outcome<T> = Result<T, Failure>;
pub fn io_failure(_: impl std::fmt::Debug) -> Failure {
    Failure::new(
        "IO_FAILED",
        "A local operation failed; raw diagnostics are not exposed.",
    )
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Workspace,
    Analysis,
    #[default]
    Host,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    #[default]
    Fast,
    Deep,
}
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Submission {
    /// Optional caller label. It does not select behavior or change the prompt.
    pub kind: Option<String>,
    pub instructions: String,
    #[serde(default)]
    pub execution_mode: Mode,
    #[serde(default)]
    pub isolation: bool,
    pub root: Option<PathBuf>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub model_profile: Profile,
    /// Optional native model slug; overrides the configured profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Zero disables the deadline. Omitted values use timeoutSeconds, which defaults to zero.
    pub timeout_seconds: Option<u64>,
    /// Reuse a key when retrying the same submission after losing its acknowledgement.
    pub idempotency_key: Option<String>,
}
impl Submission {
    pub fn resolve_mode(&mut self) {
        if self.isolation && self.execution_mode != Mode::Analysis {
            self.execution_mode = Mode::Workspace;
        }
    }
    pub fn resolve_root(&mut self) -> Outcome<()> {
        if self.root.is_none() && (self.execution_mode == Mode::Host || !self.files.is_empty()) {
            self.root = Some(std::env::current_dir().map_err(io_failure)?);
        }
        Ok(())
    }
    pub fn model(&self, config: &Config) -> Outcome<String> {
        match &self.model {
            Some(model) if !model.is_empty() && model.len() <= 128 => Ok(model.clone()),
            Some(_) => Err(Failure::new(
                "INPUT_INVALID",
                "The model slug is empty or too long.",
            )),
            None => config.model(self.model_profile),
        }
    }
    pub fn validate(&self) -> Outcome<()> {
        if self.instructions.is_empty()
            || self.instructions.chars().count() > 16000
            || self.files.len() > 100
            || self.files.iter().any(|p| p.chars().count() > 512)
            || self
                .idempotency_key
                .as_ref()
                .is_some_and(|v| v.is_empty() || v.len() > 128)
        {
            return Err(Failure::new(
                "INPUT_INVALID",
                "Task parameters exceed the documented limits.",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Models {
    pub fast: String,
    pub deep: Option<String>,
}
fn timeout() -> u64 {
    0
}
fn queue() -> usize {
    8
}
fn jobs() -> usize {
    64
}
fn retention() -> u64 {
    0
}
fn host_enabled() -> bool {
    true
}
fn audit_rotation() -> usize {
    16_777_216
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub agy_path: PathBuf,
    #[serde(default)]
    pub allowed_roots: Vec<PathBuf>,
    pub models: Models,
    #[serde(default = "host_enabled")]
    pub allow_host_execution: bool,
    #[serde(default = "host_enabled")]
    pub windows_desktop_fallback: bool,
    #[serde(default)]
    pub runtime_paths: Vec<PathBuf>,
    #[serde(default)]
    pub state_directory: PathBuf,
    #[serde(default = "host_enabled")]
    pub audit_logging: bool,
    #[serde(default)]
    pub audit_directory: PathBuf,
    #[serde(default = "audit_rotation")]
    pub audit_rotate_bytes: usize,
    #[serde(default = "timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "queue")]
    pub max_queue: usize,
    #[serde(default = "jobs")]
    pub max_jobs: usize,
    #[serde(default = "retention")]
    pub retention_seconds: u64,
    #[serde(skip)]
    pub home: PathBuf,
}
impl Config {
    pub fn load(path: &std::path::Path) -> Outcome<Self> {
        let mut c: Self =
            serde_json::from_slice(&fs::read(path).map_err(io_failure)?).map_err(|_| {
                Failure::new(
                    "CONFIG_INVALID",
                    "Configuration does not match the documented schema.",
                )
            })?;
        c.home = PathBuf::from(
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).ok_or_else(
                || {
                    Failure::new(
                        "CONFIG_INVALID",
                        "The current user's home directory must be available.",
                    )
                },
            )?,
        );
        if !c.home.is_absolute()
            || !c.agy_path.is_absolute()
            || c.allowed_roots.len() > 32
            || c.runtime_paths.len() > 16
            || !(1..=32).contains(&c.max_queue)
            || !(c.max_queue..=256).contains(&c.max_jobs)
            || (c.retention_seconds != 0 && !(60..=604800).contains(&c.retention_seconds))
            || !(4096..=268_435_456).contains(&c.audit_rotate_bytes)
        {
            return Err(Failure::new(
                "CONFIG_INVALID",
                "Configuration paths or limits are invalid.",
            ));
        }
        for model in std::iter::once(&c.models.fast).chain(c.models.deep.iter()) {
            if model.is_empty()
                || model.len() > 128
                || !model
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            {
                return Err(Failure::new(
                    "CONFIG_INVALID",
                    "Model profiles require CLI model slugs.",
                ));
            }
        }
        c.agy_path = fs::canonicalize(&c.agy_path).map_err(io_failure)?;
        let binary = fs::metadata(&c.agy_path).map_err(io_failure)?;
        if !platform::executable(&binary)
            || (cfg!(windows)
                && c.agy_path
                    .extension()
                    .is_none_or(|e| !e.eq_ignore_ascii_case("exe")))
        {
            return Err(Failure::new(
                "EXECUTABLE_UNAVAILABLE",
                "Configured CLI must be an executable file.",
            ));
        }
        for p in &mut c.allowed_roots {
            if !p.is_absolute() {
                return Err(Failure::new(
                    "CONFIG_INVALID",
                    "Allowed roots must be absolute.",
                ));
            }
            *p = fs::canonicalize(&*p).map_err(io_failure)?;
            if !p.is_dir() || p.parent().is_none() {
                return Err(Failure::new(
                    "CONFIG_INVALID",
                    "Allowed roots must be specific directories.",
                ));
            }
        }
        for p in &mut c.runtime_paths {
            if !p.is_absolute() {
                return Err(Failure::new(
                    "CONFIG_INVALID",
                    "Runtime paths must be absolute.",
                ));
            }
            *p = fs::canonicalize(&*p).map_err(io_failure)?;
            if !p.is_dir() || p.parent().is_none() || *p == c.home {
                return Err(Failure::new(
                    "CONFIG_INVALID",
                    "Runtime paths must be specific toolchain directories.",
                ));
            }
        }
        if c.state_directory.as_os_str().is_empty() {
            c.state_directory = if cfg!(windows) {
                std::env::var_os("LOCALAPPDATA")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| c.home.join("AppData/Local"))
                    .join("antigravity-worker-mcp")
            } else {
                c.home.join(".local/state/antigravity-worker-mcp")
            };
        }
        if !c.state_directory.is_absolute() {
            return Err(Failure::new(
                "CONFIG_INVALID",
                "State directory must be absolute.",
            ));
        }
        if !c.state_directory.exists() {
            fs::create_dir_all(&c.state_directory).map_err(io_failure)?;
            platform::private_mode(&c.state_directory, 0o700)?;
        }
        let state = fs::symlink_metadata(&c.state_directory).map_err(io_failure)?;
        if !state.is_dir() || !platform::is_private(&state) {
            return Err(Failure::new(
                "STATE_UNSAFE",
                "State must be a private directory, with mode 0700 on Unix and no reparse point on Windows.",
            ));
        }
        if c.audit_directory.as_os_str().is_empty() {
            c.audit_directory = c.state_directory.join("audit");
        }
        if !c.audit_directory.is_absolute() {
            return Err(Failure::new(
                "CONFIG_INVALID",
                "Audit directory must be absolute.",
            ));
        }
        Ok(c)
    }
    pub fn model(&self, profile: Profile) -> Outcome<String> {
        match profile {
            Profile::Fast => Some(&self.models.fast),
            Profile::Deep => self.models.deep.as_ref(),
        }
        .cloned()
        .ok_or_else(|| {
            Failure::new(
                "MODEL_UNAVAILABLE",
                "Requested model profile is not configured.",
            )
        })
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ManifestEntry {
    pub path: String,
    pub sha256: String,
    pub bytes: usize,
    pub lines: usize,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RunResult {
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    pub response: String,
    pub manifest: Vec<ManifestEntry>,
    pub model: String,
    pub actual_model: Option<String>,
    pub usage: Option<serde_json::Value>,
    pub cli_status: Option<String>,
    pub patch: Option<String>,
    pub patch_truncated: bool,
    pub error: Option<Failure>,
}
impl RunResult {
    pub fn empty(model: String) -> Self {
        Self {
            exit_code: None,
            duration_ms: None,
            model,
            response: String::new(),
            manifest: vec![],
            actual_model: None,
            usage: None,
            cli_status: None,
            patch: None,
            patch_truncated: false,
            error: None,
        }
    }
}
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Progress {
    pub events: usize,
    pub steps: usize,
    pub output_bytes: usize,
}

pub fn classify(text: &str) -> Failure {
    let s = text.to_lowercase();
    if [
        "quota",
        "rate limit",
        "rate_limit",
        "resource exhausted",
        "resource_exhausted",
        "capacity",
        "429",
    ]
    .iter()
    .any(|x| s.contains(x))
    {
        Failure::new(
            "QUOTA_EXHAUSTED",
            "Provider quota or capacity is unavailable; use ag_resume after checking the provider.",
        )
    } else if [
        "auth",
        "login",
        "log in",
        "credential",
        "oauth",
        "expired",
        "401",
    ]
    .iter()
    .any(|x| s.contains(x))
    {
        Failure::new(
            "AUTH_REQUIRED",
            "Authenticate the official CLI outside this server, then retry.",
        )
    } else if ["invalid model", "unknown model", "not recognized"]
        .iter()
        .any(|x| s.contains(x))
    {
        Failure::new(
            "MODEL_UNAVAILABLE",
            "The configured model is unavailable; no substitute was selected.",
        )
    } else if ["denied", "permission", "not allowed"]
        .iter()
        .any(|x| s.contains(x))
    {
        Failure::new(
            "PERMISSION_DENIED",
            "A required operation was denied; the task is incomplete.",
        )
    } else {
        Failure::new(
            "CLI_FAILED",
            "CLI did not complete successfully; raw diagnostics are not exposed.",
        )
    }
}
