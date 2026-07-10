const STORAGE_ROOT_DIR_NAME: &str = "star-prison-launcher";
const LOCAL_WEBVIEW_DATA_DIR_NAME: &str = "starprison";
const LEGACY_LOCAL_WEBVIEW_DATA_DIR_NAME: &str = "com.nobabo.starprisonlauncher";
const STALE_GAME_LOCK_MS: i64 = 12 * 60 * 60 * 1000;
const AUTH_REFRESH_MARGIN_MS: i64 = 5 * 60 * 1000;
const AUTH_HTTP_CONNECT_TIMEOUT_SECONDS: u64 = 10;
const AUTH_HTTP_REQUEST_TIMEOUT_SECONDS: u64 = 30;
const DEFAULT_MINECRAFT_TOKEN_EXPIRES_IN_SECONDS: i64 = 24 * 60 * 60;
static AUTH_PENDING: AtomicBool = AtomicBool::new(false);
static LAUNCH_PENDING: AtomicBool = AtomicBool::new(false);
static GAME_RUNNING: AtomicBool = AtomicBool::new(false);
static GAME_PROCESS_ID: AtomicU32 = AtomicU32::new(0);
static GAME_TERMINATION_REQUESTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Serialize)]
struct CommandError {
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthSummary {
    signed_in: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    player_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
}

#[derive(Debug, Serialize)]
struct Diagnostic {
    level: String,
    title: String,
    message: String,
    blocking: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreflightReport {
    ready: bool,
    blocking_count: usize,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SelectDirectoryResult {
    canceled: bool,
    path: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct WindowState {
    maximized: bool,
}

#[derive(Clone, Debug)]
struct AuthGrant {
    code: String,
    code_verifier: String,
}

#[derive(Debug)]
struct MinecraftSession {
    access_token: String,
    player_name: String,
    profile_id: String,
    expires_in: i64,
}

#[derive(Debug)]
struct AuthError {
    code: String,
    message: String,
    status: Option<u16>,
    url: Option<String>,
}

impl AuthError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            status: None,
            url: None,
        }
    }

    fn http(url: &str, status: u16, payload: &Value) -> Self {
        Self {
            code: "HTTP_REQUEST_FAILED".to_string(),
            message: extract_error_message(payload, status),
            status: Some(status),
            url: Some(url.to_string()),
        }
    }
}

fn command_error(error: impl std::fmt::Display) -> CommandError {
    CommandError {
        message: error.to_string(),
    }
}
