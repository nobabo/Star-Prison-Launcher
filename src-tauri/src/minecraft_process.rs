use crate::*;

struct MinecraftLogFilter {
    include_current_entry: bool,
}

impl MinecraftLogFilter {
    fn new() -> Self {
        Self {
            include_current_entry: false,
        }
    }

    fn should_include(&mut self, line: &str) -> bool {
        if let Some(level) = minecraft_log_level(line) {
            self.include_current_entry = matches!(level, "WARN" | "ERROR");
        }

        self.include_current_entry
    }
}

fn minecraft_log_level(line: &str) -> Option<&str> {
    let (_, thread_and_level) = line.split_once("] [")?;
    let header_end = thread_and_level.find("]: ")?;
    let (_, level) = thread_and_level[..header_end].rsplit_once('/')?;

    (!level.is_empty() && level.bytes().all(|byte| byte.is_ascii_uppercase())).then_some(level)
}

fn spawn_filtered_log_reader<R>(reader: R, writer: Arc<Mutex<File>>) -> std::thread::JoinHandle<()>
where
    R: Read + Send + 'static,
{
    std::thread::spawn(move || {
        let mut reader = io::BufReader::new(reader);
        let mut filter = MinecraftLogFilter::new();
        let mut line = String::new();

        loop {
            line.clear();
            match io::BufRead::read_line(&mut reader, &mut line) {
                Ok(0) => break,
                Ok(_) if filter.should_include(&line) => {
                    let Ok(mut writer) = writer.lock() else {
                        break;
                    };
                    if writer.write_all(line.as_bytes()).is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    })
}

pub(crate) fn capture_filtered_minecraft_logs(
    child: &mut Child,
    log_file: File,
) -> Vec<std::thread::JoinHandle<()>> {
    let writer = Arc::new(Mutex::new(log_file));
    let mut readers = Vec::with_capacity(2);

    if let Some(stdout) = child.stdout.take() {
        readers.push(spawn_filtered_log_reader(stdout, Arc::clone(&writer)));
    }
    if let Some(stderr) = child.stderr.take() {
        readers.push(spawn_filtered_log_reader(stderr, writer));
    }

    readers
}

pub(crate) fn monitor_game_process(
    app: tauri::AppHandle,
    mut child: Child,
    process_id: u32,
    process_log_path: PathBuf,
    log_readers: Vec<std::thread::JoinHandle<()>>,
) {
    tauri::async_runtime::spawn_blocking(move || {
        let wait_result = child.wait();
        for reader in log_readers {
            let _ = reader.join();
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minecraft_log_filter_keeps_warn_error_entries_and_continuations() {
        let mut filter = MinecraftLogFilter::new();
        let lines = [
            "[17:32:35] [main/INFO]: Loading Minecraft\n",
            "\t- fabric-api 1.0\n",
            "[17:32:36] [main/WARN]: Optional dependency is missing\n",
            "\tdependency detail\n",
            "[17:32:37] [Render thread/ERROR]: Renderer failed\n",
            "java.lang.IllegalStateException: broken\n",
            "\tat example.Main.run(Main.java:1)\n",
            "[17:32:38] [Render thread/DEBUG]: Retrying\n",
            "debug detail\n",
        ];

        let included: Vec<&str> = lines
            .into_iter()
            .filter(|line| filter.should_include(line))
            .collect();

        assert_eq!(
            included,
            vec![
                "[17:32:36] [main/WARN]: Optional dependency is missing\n",
                "\tdependency detail\n",
                "[17:32:37] [Render thread/ERROR]: Renderer failed\n",
                "java.lang.IllegalStateException: broken\n",
                "\tat example.Main.run(Main.java:1)\n",
            ]
        );
    }

    #[test]
    fn minecraft_log_level_rejects_non_log_lines() {
        assert_eq!(
            minecraft_log_level("[17:32:36] [main/WARN]: warning"),
            Some("WARN")
        );
        assert_eq!(minecraft_log_level("java.lang.Error: failure"), None);
        assert_eq!(minecraft_log_level("[17:32:36] malformed"), None);
    }
}

#[cfg(windows)]
pub(crate) fn terminate_process_tree(process_id: u32) -> Result<(), String> {
    let pid = process_id.to_string();
    let output = Command::new("taskkill.exe")
        .args(["/PID", &pid, "/T", "/F"])
        .output()
        .map_err(|error| {
            contextual_error("Minecraft 프로세스 종료 명령을 실행하지 못했습니다", error)
        })?;

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
pub(crate) fn terminate_process_tree(process_id: u32) -> Result<(), String> {
    let pid = process_id.to_string();
    let output = Command::new("kill")
        .args(["-TERM", &pid])
        .output()
        .map_err(|error| {
            contextual_error("Minecraft 프로세스 종료 명령을 실행하지 못했습니다", error)
        })?;

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

pub(crate) fn terminate_launched_minecraft() -> Result<Value, String> {
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
