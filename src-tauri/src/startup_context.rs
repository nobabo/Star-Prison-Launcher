use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};
use windows_sys::Win32::System::SystemInformation::GetTickCount64;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StartupContext {
    boot_session_id: String,
}

pub(crate) fn startup_context_script() -> String {
    let context = StartupContext {
        boot_session_id: boot_session_id(),
    };
    let serialized = serde_json::to_string(&context)
        .unwrap_or_else(|_| r#"{"bootSessionId":"unknown"}"#.to_owned());

    format!("window.__STAR_PRISON_STARTUP__ = Object.freeze({serialized});")
}

fn boot_session_id() -> String {
    let current_time_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let uptime_ms = unsafe { GetTickCount64() };
    let boot_time_ms = current_time_ms.saturating_sub(uptime_ms);

    format!("windows-boot-{}", (boot_time_ms + 30_000) / 60_000)
}
