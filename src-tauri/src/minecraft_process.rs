fn monitor_game_process(
    app: tauri::AppHandle,
    mut child: Child,
    process_id: u32,
    process_log_path: PathBuf,
) {
    tauri::async_runtime::spawn_blocking(move || {
        let wait_result = child.wait();
        let termination_requested = GAME_TERMINATION_REQUESTED.swap(false, Ordering::SeqCst);
        GAME_PROCESS_ID.store(0, Ordering::SeqCst);
        release_game_lock();
        focus_launcher_window(&app);

        let payload = match wait_result {
            Ok(status) => json!({
                "stage": "game-ended",
                "label": "게임 종료",
                "progress": 1.0,
                "processId": process_id,
                "exitCode": status.code(),
                "terminationRequested": termination_requested,
                "logPath": launcher_storage_relative_path(&process_log_path)
            }),
            Err(error) => json!({
                "stage": "game-ended",
                "label": "게임 종료",
                "progress": 1.0,
                "processId": process_id,
                "errorDetail": error.to_string(),
                "terminationRequested": termination_requested,
                "logPath": launcher_storage_relative_path(&process_log_path)
            }),
        };

        let _ = app.emit("launcher:launch-state-changed", payload);
    });
}

#[cfg(windows)]
fn terminate_process_tree(process_id: u32) -> Result<(), String> {
    let pid = process_id.to_string();
    let output = Command::new("taskkill.exe")
        .args(["/PID", &pid, "/T", "/F"])
        .output()
        .map_err(|error| contextual_error("Minecraft 프로세스 종료 명령을 실행하지 못했습니다", error))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() { stderr } else { stdout };

    Err(if detail.is_empty() {
        format!("Minecraft 프로세스를 종료하지 못했습니다. PID: {process_id}")
    } else {
        format!("Minecraft 프로세스를 종료하지 못했습니다. PID: {process_id}\n{detail}")
    })
}

#[cfg(not(windows))]
fn terminate_process_tree(process_id: u32) -> Result<(), String> {
    let pid = process_id.to_string();
    let output = Command::new("kill")
        .args(["-TERM", &pid])
        .output()
        .map_err(|error| contextual_error("Minecraft 프로세스 종료 명령을 실행하지 못했습니다", error))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("Minecraft 프로세스를 종료하지 못했습니다. PID: {process_id}")
        } else {
            format!("Minecraft 프로세스를 종료하지 못했습니다. PID: {process_id}\n{stderr}")
        })
    }
}

fn terminate_launched_minecraft() -> Result<Value, String> {
    let process_id = GAME_PROCESS_ID.load(Ordering::SeqCst);

    if process_id == 0 {
        return Ok(json!({
            "ok": true,
            "mode": "not-running",
            "message": "런처로 실행 중인 Minecraft가 없습니다."
        }));
    }

    GAME_TERMINATION_REQUESTED.store(true, Ordering::SeqCst);
    if let Err(error) = terminate_process_tree(process_id) {
        GAME_TERMINATION_REQUESTED.store(false, Ordering::SeqCst);
        return Err(error);
    }

    Ok(json!({
        "ok": true,
        "mode": "terminating",
        "message": "Minecraft 종료 명령을 보냈습니다.",
        "processId": process_id
    }))
}
