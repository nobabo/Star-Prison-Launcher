use crate::*;

pub(crate) const STORAGE_ROOT_DIR_NAME: &str = "star-prison-launcher";
pub(crate) const LOCAL_WEBVIEW_DATA_DIR_NAME: &str = "starprison";
pub(crate) const LEGACY_LOCAL_WEBVIEW_DATA_DIR_NAME: &str = "com.nobabo.starprisonlauncher";
pub(crate) const STALE_GAME_LOCK_MS: i64 = 12 * 60 * 60 * 1000;
pub(crate) const AUTH_REFRESH_MARGIN_MS: i64 = 5 * 60 * 1000;
pub(crate) const AUTH_HTTP_CONNECT_TIMEOUT_SECONDS: u64 = 10;
pub(crate) const AUTH_HTTP_REQUEST_TIMEOUT_SECONDS: u64 = 30;
pub(crate) const DEFAULT_MINECRAFT_TOKEN_EXPIRES_IN_SECONDS: i64 = 24 * 60 * 60;
pub(crate) static AUTH_PENDING: AtomicBool = AtomicBool::new(false);
pub(crate) static LAUNCH_PENDING: AtomicBool = AtomicBool::new(false);
pub(crate) static GAME_RUNNING: AtomicBool = AtomicBool::new(false);
pub(crate) static GAME_PROCESS_ID: AtomicU32 = AtomicU32::new(0);
pub(crate) static GAME_TERMINATION_REQUESTED: AtomicBool = AtomicBool::new(false);
pub(crate) static USER_CONFIG_WRITE_LOCK: Mutex<()> = Mutex::new(());
pub(crate) static USER_CONFIG_MUTATION_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Serialize)]
pub(crate) struct CommandError {
    pub(crate) message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AuthSummary {
    pub(crate) signed_in: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) player_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) expires_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) source: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Diagnostic {
    pub(crate) level: String,
    pub(crate) title: String,
    pub(crate) message: String,
    pub(crate) blocking: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreflightReport {
    pub(crate) ready: bool,
    pub(crate) blocking_count: usize,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SelectDirectoryResult {
    pub(crate) canceled: bool,
    pub(crate) path: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct WindowState {
    pub(crate) maximized: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct AuthGrant {
    pub(crate) code: String,
    pub(crate) code_verifier: String,
}

#[derive(Debug)]
pub(crate) struct MinecraftSession {
    pub(crate) access_token: String,
    pub(crate) player_name: String,
    pub(crate) profile_id: String,
    pub(crate) expires_in: i64,
}

#[derive(Debug)]
pub(crate) struct AuthError {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) status: Option<u16>,
    pub(crate) url: Option<String>,
}

impl AuthError {
    pub(crate) fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            status: None,
            url: None,
        }
    }

    pub(crate) fn http(url: &str, status: u16, payload: &Value) -> Self {
        Self {
            code: "HTTP_REQUEST_FAILED".to_string(),
            message: extract_error_message(payload, status),
            status: Some(status),
            url: Some(url.to_string()),
        }
    }
}

pub(crate) fn command_error(error: impl std::fmt::Display) -> CommandError {
    CommandError {
        message: error.to_string(),
    }
}
