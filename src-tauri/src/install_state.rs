use crate::*;

pub(crate) const MINECRAFT_INSTALL_STAGES: [&str; 5] = [
    "minecraft-1-metadata",
    "minecraft-2-libraries",
    "minecraft-3-fabric",
    "minecraft-4-client",
    "minecraft-5-assets",
];

pub(crate) fn migrate_legacy_install_state(data_directory: &Path) -> Result<(), String> {
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

pub(crate) fn state_temp_path(path: &Path) -> PathBuf {
    path.with_extension("json.tmp")
}

pub(crate) fn state_backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

pub(crate) fn recover_json_state_file(path: &Path) -> Result<(), String> {
    ensure_parent_dir(path)?;
    let temp_path = state_temp_path(path);
    let backup_path = state_backup_path(path);

    if temp_path.exists() {
        if read_json_file(&temp_path).is_ok() {
            if path.exists() {
                remove_path_if_exists(path)?;
            }
            fs::rename(&temp_path, path).map_err(|error| {
                contextual_error(
                    &format!(
                        "상태 임시 파일 복구에 실패했습니다 (from: {}, to: {})",
                        display_path(&temp_path),
                        display_path(path)
                    ),
                    error,
                )
            })?;
            remove_path_if_exists(&backup_path)?;
            return Ok(());
        }

        remove_path_if_exists(&temp_path)?;
    }

    if !path.exists() && backup_path.exists() {
        fs::rename(&backup_path, path).map_err(|error| {
            contextual_error(
                &format!(
                    "상태 백업 파일 복구에 실패했습니다 (from: {}, to: {})",
                    display_path(&backup_path),
                    display_path(path)
                ),
                error,
            )
        })?;
        return Ok(());
    }

    if path.exists() {
        remove_path_if_exists(&backup_path)?;
    }

    Ok(())
}

pub(crate) fn load_install_state(data_directory: &Path) -> Result<Option<Value>, String> {
    migrate_legacy_install_state(data_directory)?;
    let path = launcher_install_state_path();
    recover_json_state_file(&path)?;

    if !path.exists() {
        return Ok(None);
    }

    read_json_file(&path).map(Some)
}

pub(crate) fn write_json_state_atomic(
    path: &Path,
    state: &Value,
    label: &str,
) -> Result<(), String> {
    recover_json_state_file(path)?;
    let temp_path = state_temp_path(path);
    let backup_path = state_backup_path(path);
    remove_path_if_exists(&temp_path)?;
    remove_path_if_exists(&backup_path)?;

    let content = format!(
        "{}\n",
        serde_json::to_string_pretty(state).map_err(|error| error.to_string())?
    );
    let mut file = File::create(&temp_path).map_err(|error| {
        io_error(
            &format!("{label} 임시 파일을 만들지 못했습니다"),
            &temp_path,
            error,
        )
    })?;
    file.write_all(content.as_bytes()).map_err(|error| {
        io_error(
            &format!("{label} 임시 파일을 쓰지 못했습니다"),
            &temp_path,
            error,
        )
    })?;
    file.sync_all().map_err(|error| {
        io_error(
            &format!("{label} 임시 파일을 동기화하지 못했습니다"),
            &temp_path,
            error,
        )
    })?;

    if path.exists() {
        fs::rename(path, &backup_path).map_err(|error| {
            contextual_error(
                &format!(
                    "{label} 기존 파일을 백업하지 못했습니다 (from: {}, to: {})",
                    display_path(path),
                    display_path(&backup_path)
                ),
                error,
            )
        })?;
    }

    if let Err(error) = fs::rename(&temp_path, path) {
        if backup_path.exists() && !path.exists() {
            let _ = fs::rename(&backup_path, path);
        }
        return Err(contextual_error(
            &format!(
                "{label} 임시 파일을 적용하지 못했습니다 (from: {}, to: {})",
                display_path(&temp_path),
                display_path(path)
            ),
            error,
        ));
    }

    remove_path_if_exists(&backup_path)
}

pub(crate) fn save_install_state(data_directory: &Path, state: &Value) -> Result<(), String> {
    migrate_legacy_install_state(data_directory)?;
    write_json_state_atomic(&launcher_install_state_path(), state, "설치 상태")
}

pub(crate) fn launcher_install_checkpoint_path() -> PathBuf {
    storage_root_path().join("install-checkpoint.json")
}

pub(crate) fn load_or_create_install_checkpoint(identity: &Value) -> Result<(Value, bool), String> {
    let path = launcher_install_checkpoint_path();
    recover_json_state_file(&path)?;
    let existing = if path.exists() {
        read_json_file(&path).ok()
    } else {
        None
    };

    if let Some(checkpoint) = existing {
        let matches_identity = checkpoint.get("schemaVersion").and_then(Value::as_u64) == Some(1)
            && checkpoint.get("identity") == Some(identity);

        if matches_identity {
            return Ok((checkpoint, false));
        }
    }

    let checkpoint = json!({
        "schemaVersion": 1,
        "identity": identity,
        "completedStages": [],
        "updatedAt": now_ms().to_string()
    });
    write_json_state_atomic(&path, &checkpoint, "설치 체크포인트")?;
    Ok((checkpoint, true))
}

pub(crate) fn install_checkpoint_completed(checkpoint: &Value, stage: &str) -> bool {
    checkpoint
        .get("completedStages")
        .and_then(Value::as_array)
        .is_some_and(|stages| stages.iter().any(|value| value.as_str() == Some(stage)))
}

pub(crate) fn mark_install_checkpoint(checkpoint: &mut Value, stage: &str) -> Result<(), String> {
    let object = checkpoint
        .as_object_mut()
        .ok_or_else(|| "설치 체크포인트 형식이 올바르지 않습니다.".to_string())?;
    let completed_stages = object
        .entry("completedStages".to_string())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| "설치 체크포인트 completedStages 형식이 올바르지 않습니다.".to_string())?;

    if !completed_stages
        .iter()
        .any(|value| value.as_str() == Some(stage))
    {
        completed_stages.push(Value::String(stage.to_string()));
    }

    object.insert("updatedAt".to_string(), Value::String(now_ms().to_string()));
    write_json_state_atomic(
        &launcher_install_checkpoint_path(),
        checkpoint,
        "설치 체크포인트",
    )
}

#[cfg(test)]
mod install_checkpoint_tests {
    use super::*;

    #[test]
    pub(crate) fn minecraft_install_has_five_ordered_stages() {
        assert_eq!(MINECRAFT_INSTALL_STAGES.len(), 5);
        assert_eq!(MINECRAFT_INSTALL_STAGES[0], "minecraft-1-metadata");
        assert_eq!(MINECRAFT_INSTALL_STAGES[4], "minecraft-5-assets");
    }

    #[test]
    pub(crate) fn completed_stage_lookup_is_exact() {
        let checkpoint = json!({
            "completedStages": ["runtime", "minecraft-1-metadata"]
        });

        assert!(install_checkpoint_completed(&checkpoint, "runtime"));
        assert!(install_checkpoint_completed(
            &checkpoint,
            "minecraft-1-metadata"
        ));
        assert!(!install_checkpoint_completed(
            &checkpoint,
            "minecraft-2-libraries"
        ));
    }

    #[test]
    pub(crate) fn recovers_a_synced_temporary_state_file() {
        let directory = std::env::temp_dir().join(format!(
            "star-prison-checkpoint-test-{}-{}",
            std::process::id(),
            now_ms()
        ));
        fs::create_dir_all(&directory).expect("test directory should be created");
        let path = directory.join("state.json");
        fs::write(&path, "{\"value\":1}\n").expect("old state should be written");
        fs::write(state_temp_path(&path), "{\"value\":2}\n")
            .expect("temporary state should be written");

        recover_json_state_file(&path).expect("temporary state should be recovered");
        let recovered = read_json_file(&path).expect("recovered state should be valid");
        assert_eq!(recovered.get("value").and_then(Value::as_u64), Some(2));

        fs::remove_dir_all(directory).expect("test directory should be removed");
    }
}
