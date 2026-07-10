fn runtime_current_path(data_directory: &Path, runtime: &Value) -> PathBuf {
    data_directory.join("runtime").join(
        runtime
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("java-runtime"),
    )
}

fn legacy_runtime_current_path(data_directory: &Path, runtime: &Value) -> PathBuf {
    data_directory.join("runtime").join("current").join(
        runtime
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("java-runtime"),
    )
}

fn runtime_download_path(data_directory: &Path, runtime: &Value) -> PathBuf {
    data_directory
        .join("downloads")
        .join("runtime")
        .join(format!(
            "{}.zip",
            runtime
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("runtime")
        ))
}

fn runtime_staged_path(data_directory: &Path, runtime: &Value) -> PathBuf {
    data_directory.join("runtime").join("staged").join(
        runtime
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("runtime"),
    )
}

fn runtime_staged_root_path(data_directory: &Path) -> PathBuf {
    data_directory.join("runtime").join("staged")
}

fn runtime_executable_path(data_directory: &Path, runtime: &Value) -> PathBuf {
    runtime_current_path(data_directory, runtime).join(
        runtime
            .get("executablePath")
            .and_then(Value::as_str)
            .unwrap_or(if cfg!(windows) {
                "bin/javaw.exe"
            } else {
                "bin/java"
            }),
    )
}

fn working_directory_root(data_directory: &Path, server_manifest: &Value) -> PathBuf {
    data_directory.join(manifest_working_directory(server_manifest))
}

fn profile_root_path() -> PathBuf {
    storage_root_path().join("profile")
}

fn legacy_data_profile_path(data_directory: &Path) -> PathBuf {
    data_directory.join("profile")
}

fn legacy_client_current_path(data_directory: &Path, server_manifest: &Value) -> PathBuf {
    working_directory_root(data_directory, server_manifest).join("current")
}

fn profile_staged_path(data_directory: &Path, channel: &Value) -> PathBuf {
    data_directory
        .join("staged")
        .join("profile")
        .join(
            channel
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("client"),
        )
}

fn profile_staged_root_path(data_directory: &Path) -> PathBuf {
    data_directory.join("staged").join("profile")
}

fn legacy_client_staged_root_path(data_directory: &Path, server_manifest: &Value) -> PathBuf {
    working_directory_root(data_directory, server_manifest).join("staged")
}

fn client_download_path(data_directory: &Path, channel: &Value) -> PathBuf {
    data_directory
        .join("downloads")
        .join("client")
        .join(format!(
            "{}.zip",
            channel
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("client")
        ))
}

fn launcher_install_state_path() -> PathBuf {
    storage_root_path().join("install-state.json")
}

fn legacy_install_state_path(data_directory: &Path) -> PathBuf {
    data_directory.join("state").join("install-state.json")
}
