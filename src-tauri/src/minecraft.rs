fn resolve_game_directory(bundle_root: &Path, launch_plan: Option<&Value>) -> PathBuf {
    launch_plan
        .and_then(|plan| plan.get("gameDirectory"))
        .and_then(Value::as_str)
        .map(|path| bundle_root.join(path))
        .unwrap_or_else(|| bundle_root.to_path_buf())
}

fn current_launcher_version() -> String {
    serde_json::from_str::<Value>(include_str!("../../package.json"))
        .ok()
        .and_then(|package| {
            package
                .get("version")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .expect("package.json version must be set")
}

fn installed_launcher_version(install_state: Option<&Value>) -> Option<&str> {
    install_state
        .and_then(|state| state.get("launcherVersion"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn launcher_content_reinstall_required(install_state: Option<&Value>, launcher_version: &str) -> bool {
    install_state.is_some_and(|state| installed_launcher_version(Some(state)) != Some(launcher_version))
}

fn runtime_install_signature(runtime: &Value) -> Value {
    json!({
        "id": runtime.get("id").cloned().unwrap_or(Value::Null),
        "version": runtime.get("version").cloned().unwrap_or(Value::Null),
        "url": runtime.get("url").cloned().unwrap_or(Value::Null),
        "size": runtime.get("size").cloned().unwrap_or(Value::Null),
        "sha256": runtime.get("sha256").cloned().unwrap_or(Value::Null),
        "executablePath": runtime.get("executablePath").cloned().unwrap_or(Value::Null)
    })
}

fn client_install_signature(server_manifest: &Value, channel: &Value) -> Result<Value, String> {
    let client_bundle = channel
        .get("clientBundle")
        .ok_or_else(|| "배포 manifest에 clientBundle 정보가 없습니다.".to_string())?;

    Ok(json!({
        "minecraftVersion": server_manifest.get("minecraftVersion").cloned().unwrap_or(Value::Null),
        "clientBundle": client_bundle
    }))
}

fn install_signature_matches(
    install_state: Option<&Value>,
    field: &str,
    expected: &Value,
) -> bool {
    install_state
        .and_then(|state| state.get(field))
        .is_some_and(|actual| actual == expected)
}

fn show_launcher_content_reinstall_notice(installed_version: Option<&str>, launcher_version: &str) {
    let installed_version = installed_version.unwrap_or("알 수 없음");
    let _ = rfd::MessageDialog::new()
        .set_title("런처 업데이트")
        .set_description(format!(
            "런처 버전이 변경되어 모드와 쉐이더 파일을 다시 확인합니다.\n\n설치된 버전: {installed_version}\n현재 버전: {launcher_version}\n\n프로필과 게임 설정은 유지됩니다."
        ))
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

fn launcher_storage_relative_path(path: &Path) -> Option<String> {
    path.strip_prefix(storage_root_path())
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
}

fn minecraft_process_log_path() -> Result<PathBuf, String> {
    let logs_directory = storage_root_path().join("logs");
    fs::create_dir_all(&logs_directory).map_err(|error| {
        io_error(
            "Minecraft 프로세스 로그 폴더를 만들지 못했습니다",
            &logs_directory,
            error,
        )
    })?;
    Ok(logs_directory.join(format!("minecraft-process-{}.log", now_ms())))
}

fn nbt_write_name(buffer: &mut Vec<u8>, name: &str) -> Result<(), String> {
    let bytes = name.as_bytes();
    let length = u16::try_from(bytes.len())
        .map_err(|_| format!("NBT 이름이 너무 깁니다: {name}"))?;
    buffer.extend_from_slice(&length.to_be_bytes());
    buffer.extend_from_slice(bytes);
    Ok(())
}

fn nbt_write_string_payload(buffer: &mut Vec<u8>, value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    let length = u16::try_from(bytes.len())
        .map_err(|_| "서버 목록 문자열이 너무 깁니다.".to_string())?;
    buffer.extend_from_slice(&length.to_be_bytes());
    buffer.extend_from_slice(bytes);
    Ok(())
}

fn nbt_write_string(buffer: &mut Vec<u8>, name: &str, value: &str) -> Result<(), String> {
    buffer.push(8);
    nbt_write_name(buffer, name)?;
    nbt_write_string_payload(buffer, value)
}

fn build_single_server_list_nbt(server_name: &str, server_address: &str) -> Result<Vec<u8>, String> {
    let mut buffer = Vec::new();

    buffer.push(10);
    nbt_write_name(&mut buffer, "")?;
    buffer.push(9);
    nbt_write_name(&mut buffer, "servers")?;
    buffer.push(10);
    buffer.extend_from_slice(&1_i32.to_be_bytes());
    nbt_write_string(&mut buffer, "name", server_name)?;
    nbt_write_string(&mut buffer, "ip", server_address)?;
    buffer.push(1);
    nbt_write_name(&mut buffer, "acceptTextures")?;
    buffer.push(1);
    buffer.push(0);
    buffer.push(0);

    Ok(buffer)
}

fn ensure_default_server_list(
    game_directory: &Path,
    server_manifest: &Value,
) -> Result<(), String> {
    let servers_dat_path = game_directory.join("servers.dat");
    let server_address = server_manifest
        .get("address")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|address| !address.is_empty())
        .ok_or_else(|| "config/server.manifest.json에 address 값이 없습니다.".to_string())?;
    let server_name = server_manifest
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("StarPrison");
    let content = build_single_server_list_nbt(server_name, server_address)?;
    let tmp_path = servers_dat_path.with_extension("dat.tmp");

    fs::write(&tmp_path, content)
        .map_err(|error| io_error("Minecraft 서버 목록 임시 파일을 쓰지 못했습니다", &tmp_path, error))?;
    fs::rename(&tmp_path, &servers_dat_path).map_err(|error| {
        io_error(
            "Minecraft 서버 목록 파일을 적용하지 못했습니다",
            &servers_dat_path,
            error,
        )
    })?;

    Ok(())
}

fn launch_minecraft(app: &tauri::AppHandle) -> Result<Value, String> {
    emit_launch_state(app, "앱 설정을 읽는 중", 0.10);

    let app_config = load_app_config()?;
    emit_launch_state(app, "서버 정보를 읽는 중", 0.14);
    let server_manifest = load_server_manifest()?;
    emit_launch_state(app, "GitHub 배포 정보를 확인하는 중", 0.18);
    let distribution = read_distribution_manifest()?;
    let launcher_version = current_launcher_version();
    emit_launch_state(app, "사용자 설정을 읽는 중", 0.22);
    let mut user_config = load_or_create_user_config()?;
    emit_launch_state(app, "로그인 세션 갱신 확인", 0.24);
    if let Err(error) = refresh_auth_session_if_needed(&mut user_config, &app_config) {
        let classified_error = classify_auth_error(error);
        let recovery_message = if classified_error.code == "AUTH_REFRESH_EXPIRED" {
            "계정 탭에서 연결 해제 후 다시 로그인해 주세요."
        } else {
            "네트워크 상태를 확인한 뒤 다시 시작해 주세요. 문제가 계속되면 계정을 다시 연결해 주세요."
        };

        return Ok(json!({
            "ok": false,
            "mode": "blocked",
            "message": "로그인 세션을 갱신하지 못했습니다.",
            "code": classified_error.code,
            "errorDetail": classified_error.message,
            "preflight": {
                "ready": false,
                "blockingCount": 1,
                "diagnostics": [
                    {
                        "level": "warning",
                        "title": "계정 세션",
                        "message": recovery_message,
                        "blocking": true
                    }
                ]
            }
        }));
    }

    emit_launch_state(app, "계정 상태를 확인 중", 0.26);
    let auth_summary = auth_summary(&user_config);
    let preflight = run_preflight(&auth_summary);

    if preflight.blocking_count > 0 {
        return Ok(json!({
            "ok": false,
            "mode": "blocked",
            "message": "게임 시작 전에 필요한 항목을 확인해 주세요.",
            "preflight": preflight
        }));
    }

    emit_launch_state(app, "설치 폴더 확인", 0.30);
    let data_directory = user_config
        .get("settings")
        .and_then(Value::as_object)
        .ok_or_else(|| "사용자 설정을 찾지 못했습니다.".to_string())?
        .get("dataDirectory")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(default_data_directory);

    if let Some(settings) = user_config
        .get_mut("settings")
        .and_then(Value::as_object_mut)
    {
        settings.insert(
            "dataDirectory".to_string(),
            Value::String(data_directory.to_string_lossy().into_owned()),
        );
    }

    emit_launch_state(app, "배포 채널 선택", 0.42);
    let (channel_name, channel) =
        selected_distribution_channel(&distribution, &user_config, &app_config)?;
    let install_state = load_install_state(&data_directory)?;
    let installed_launcher_manifest_version = installed_launcher_version(install_state.as_ref());
    let should_reinstall_launcher_content =
        launcher_content_reinstall_required(install_state.as_ref(), &launcher_version);
    let runtime = channel
        .get("runtime")
        .ok_or_else(|| "배포 manifest에 Java runtime 정보가 없습니다.".to_string())?;
    let runtime_signature = runtime_install_signature(runtime);
    let client_signature = client_install_signature(&server_manifest, channel)?;
    let force_runtime_install = !install_signature_matches(
        install_state.as_ref(),
        "runtimeSignature",
        &runtime_signature,
    );
    let force_client_install = !install_signature_matches(
        install_state.as_ref(),
        "clientSignature",
        &client_signature,
    );
    let java_executable = ensure_runtime_installed(
        app,
        &data_directory,
        runtime,
        force_runtime_install,
    )
    .map_err(|install_error| {
        format!(
            "게임 실행용 Java 21 런타임을 설치하지 못했습니다.\n설치 대상: {}\n원인: {install_error}",
            display_path(&runtime_executable_path(&data_directory, runtime))
        )
    })?;

    let profile_root = ensure_client_bundle_installed(
        app,
        &data_directory,
        &server_manifest,
        channel,
        force_client_install,
    )?;

    emit_launch_state(app, "실행 계획 파일 확인", 0.50);
    let launch_plan = load_instance_launch_plan(&profile_root);
    emit_launch_state(app, "번들 루트 경로 확인", 0.54);
    let bundle_root = launch_plan
        .as_ref()
        .map(|(_, root)| root.clone())
        .unwrap_or_else(|| profile_root.clone());
    let launch_plan_value = launch_plan.as_ref().map(|(plan, _)| plan);
    emit_launch_state(app, "게임 폴더 확인", 0.58);
    let game_directory = resolve_game_directory(&bundle_root, launch_plan_value);
    let quick_play_dir = game_directory.join("quickPlay");
    fs::create_dir_all(&quick_play_dir)
        .map_err(|error| io_error("Minecraft quickPlay 폴더를 만들지 못했습니다", &quick_play_dir, error))?;
    ensure_default_server_list(&game_directory, &server_manifest)?;

    if should_reinstall_launcher_content {
        show_launcher_content_reinstall_notice(installed_launcher_manifest_version, &launcher_version);
    }

    let force_release_archive_keys: &[&str] = if should_reinstall_launcher_content {
        &["mods", "shaderpacks"]
    } else {
        &[]
    };
    ensure_release_archives_synchronized(
        app,
        &data_directory,
        &bundle_root,
        channel.get("releaseArchives"),
        force_release_archive_keys,
    )?;

    let (modpack_manifest, modpack_manifest_path, _modpack_changed_count) =
        ensure_modpack_synchronized(
            app,
            &data_directory,
            &bundle_root,
            &server_manifest,
            launch_plan_value,
            channel.get("modpackManifest"),
        )?;

    emit_launch_state(app, "실행 인자값 확인", 0.62);
    let args = build_launch_arguments(
        &bundle_root,
        &server_manifest,
        &user_config,
        launch_plan_value,
    )?;
    emit_launch_state(app, "실행 정보 최종 저장", 0.80);
    let client_bundle = channel
        .get("clientBundle")
        .ok_or_else(|| "배포 manifest에 clientBundle 정보가 없습니다.".to_string())?;
    save_install_state(
        &data_directory,
        &json!({
            "schemaVersion": 1,
            "channel": channel_name,
            "launcherVersion": launcher_version,
            "runtimeId": runtime.get("id").cloned().unwrap_or(Value::Null),
            "runtimeVersion": runtime.get("version").cloned().unwrap_or(Value::Null),
            "runtimeSha256": runtime.get("sha256").cloned().unwrap_or(Value::Null),
            "runtimeExecutablePath": runtime.get("executablePath").cloned().unwrap_or(Value::Null),
            "runtimeSignature": runtime_signature,
            "currentVersion": channel.get("version").cloned().unwrap_or(Value::Null),
            "bundleId": client_bundle.get("bundleId").cloned().unwrap_or(Value::Null),
            "bundleSha256": client_bundle.get("sha256").cloned().unwrap_or(Value::Null),
            "clientSignature": client_signature,
            "installedAt": install_state
                .as_ref()
                .and_then(|state| state.get("installedAt"))
                .and_then(Value::as_str)
                .map(|value| Value::String(value.to_string()))
                .unwrap_or_else(|| Value::String(now_ms().to_string())),
            "verifiedAt": now_ms().to_string(),
            "launchProfilePath": launch_plan
                .as_ref()
                .map(|(_, root)| root.join("launch-profile.json"))
                .and_then(|path| launcher_storage_relative_path(&path))
                .unwrap_or_default(),
            "modpackVersion": modpack_manifest
                .as_ref()
                .and_then(|manifest| manifest.get("version"))
                .cloned()
                .unwrap_or(Value::String(String::new())),
            "modpackManifestSha256": channel
                .get("modpackManifest")
                .and_then(|descriptor| descriptor.get("sha256"))
                .cloned()
                .unwrap_or(Value::String(String::new())),
            "modpackManifestPath": modpack_manifest_path
                .as_ref()
                .and_then(|path| path.strip_prefix(&data_directory).ok())
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            "modpackVerifiedAt": if modpack_manifest_path.is_some() {
                now_ms().to_string()
            } else {
                String::new()
            },
            "lastKnownGood": {
                "version": channel.get("version").cloned().unwrap_or(Value::Null),
                "bundleSha256": client_bundle.get("sha256").cloned().unwrap_or(Value::Null),
                "runtimeVersion": runtime.get("version").cloned().unwrap_or(Value::Null),
                "runtimeSha256": runtime.get("sha256").cloned().unwrap_or(Value::Null),
                "modpackVersion": modpack_manifest
                    .as_ref()
                    .and_then(|manifest| manifest.get("version"))
                    .cloned()
                    .unwrap_or(Value::String(String::new())),
                "modpackManifestSha256": channel
                    .get("modpackManifest")
                    .and_then(|descriptor| descriptor.get("sha256"))
                    .cloned()
                    .unwrap_or(Value::String(String::new()))
            }
        }),
    )?;
    save_user_config(&user_config)?;

    emit_launch_state(app, "자바 실행 명령 구성", 0.86);
    let process_log_path = minecraft_process_log_path()?;
    let process_stdout = File::create(&process_log_path).map_err(|error| {
        io_error(
            "Minecraft 프로세스 로그 파일을 만들지 못했습니다",
            &process_log_path,
            error,
        )
    })?;
    let process_stderr = process_stdout.try_clone().map_err(|error| {
        io_error(
            "Minecraft 프로세스 로그 파일 핸들을 복제하지 못했습니다",
            &process_log_path,
            error,
        )
    })?;
    let mut command = Command::new(&java_executable);
    command
        .args(&args)
        .current_dir(&game_directory)
        .stdout(Stdio::from(process_stdout))
        .stderr(Stdio::from(process_stderr));

    emit_launch_state(app, "자바 프로세스 실행", 0.92);
    let mut child = command.spawn().map_err(|error| {
        contextual_error(
            &format!(
                "Java 프로세스를 시작하지 못했습니다 (java: {}, cwd: {}, log: {})",
                display_path(&java_executable),
                display_path(&game_directory),
                display_path(&process_log_path)
            ),
            error,
        )
    })?;
    let process_id = child.id();

    if let Err(lock_error) = update_game_lock_process_id(process_id) {
        let termination_error = terminate_process_tree(process_id).err();
        let _ = child.wait();
        return Err(match termination_error {
            Some(termination_error) => format!(
                "게임 실행 잠금에 PID를 기록하지 못했고 시작된 프로세스 종료도 실패했습니다: {lock_error}; {termination_error}"
            ),
            None => format!(
                "게임 실행 잠금에 PID를 기록하지 못해 시작된 프로세스를 종료했습니다: {lock_error}"
            ),
        });
    }

    GAME_PROCESS_ID.store(process_id, Ordering::SeqCst);
    GAME_TERMINATION_REQUESTED.store(false, Ordering::SeqCst);
    minimize_launcher_window(app);
    emit_launch_state(app, "게임 창으로 전환합니다", 1.0);
    monitor_game_process(app.clone(), child, process_id, process_log_path.clone());

    Ok(json!({
        "ok": true,
        "mode": "launched",
        "message": "Minecraft 프로세스를 시작했습니다.",
        "processId": process_id,
        "logPath": process_log_path
    }))
}
