fn migrate_legacy_install_state(data_directory: &Path) -> Result<(), String> {
    let path = launcher_install_state_path();
    let legacy_path = legacy_install_state_path(data_directory);

    if path.exists() || !legacy_path.exists() {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| io_error("설치 상태 상위 폴더를 만들지 못했습니다", parent, error))?;
    }

    fs::rename(&legacy_path, &path).map_err(|error| {
        contextual_error(
            &format!(
                "설치 상태 파일을 런처 루트로 이동하지 못했습니다 (from: {}, to: {})",
                display_path(&legacy_path),
                display_path(&path)
            ),
            error,
        )
    })?;

    let _ = fs::remove_dir(data_directory.join("state"));
    Ok(())
}

fn load_install_state(data_directory: &Path) -> Result<Option<Value>, String> {
    migrate_legacy_install_state(data_directory)?;
    let path = launcher_install_state_path();

    if !path.exists() {
        return Ok(None);
    }

    read_json_file(&path).map(Some)
}

fn save_install_state(data_directory: &Path, state: &Value) -> Result<(), String> {
    migrate_legacy_install_state(data_directory)?;
    let path = launcher_install_state_path();
    ensure_parent_dir(&path)?;
    fs::write(
        &path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(state).map_err(|error| error.to_string())?
        ),
    )
    .map_err(|error| io_error("설치 상태 파일을 쓰지 못했습니다", &path, error))
}
