use crate::*;

pub(crate) fn create_random_base64url(byte_len: usize) -> String {
    let mut bytes = vec![0; byte_len];
    getrandom::fill(&mut bytes).expect("OS random source must be available for OAuth PKCE");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub(crate) fn create_code_verifier() -> String {
    create_random_base64url(64)
}

pub(crate) fn create_oauth_state() -> String {
    create_random_base64url(24)
}

pub(crate) fn create_code_challenge(code_verifier: &str) -> String {
    let digest = Sha256::digest(code_verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

pub(crate) fn is_matching_redirect_url(target_url: &Url, redirect_url: &Url) -> bool {
    target_url.scheme() == redirect_url.scheme()
        && target_url.username() == redirect_url.username()
        && target_url.password() == redirect_url.password()
        && target_url.host_str() == redirect_url.host_str()
        && target_url.port_or_known_default() == redirect_url.port_or_known_default()
        && target_url.path() == redirect_url.path()
}

pub(crate) fn normalize_auth_error_message(value: Option<&str>, fallback: String) -> String {
    value
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(str::to_string)
        .unwrap_or(fallback)
}

pub(crate) fn extract_error_message(payload: &Value, status: u16) -> String {
    match payload {
        Value::String(value) => normalize_auth_error_message(
            Some(value),
            format!("Request failed with status {status}"),
        ),
        Value::Object(object) => normalize_auth_error_message(
            object
                .get("error_description")
                .or_else(|| object.get("errorMessage"))
                .or_else(|| object.get("error"))
                .or_else(|| object.get("developerMessage"))
                .or_else(|| payload.pointer("/details/errorMessage"))
                .or_else(|| payload.pointer("/details/message"))
                .and_then(Value::as_str),
            format!("Request failed with status {status}"),
        ),
        _ => format!("Request failed with status {status}"),
    }
}

pub(crate) fn classify_auth_error(error: AuthError) -> AuthError {
    if error.code == "AUTH_MINECRAFT_OWNERSHIP_REQUIRED" {
        return AuthError::new(
            "AUTH_MINECRAFT_OWNERSHIP_REQUIRED",
            "마인크래프트를 구매하지 않은 계정입니다.",
        );
    }

    if error.code == "AUTH_REFRESH_REQUIRED"
        || error.message.contains("invalid_grant")
        || error.message.contains("AADSTS70000")
        || error.message.contains("AADSTS700082")
    {
        return AuthError::new(
            "AUTH_REFRESH_EXPIRED",
            "저장된 로그인 세션을 갱신할 수 없습니다. 계정 탭에서 연결 해제 후 다시 로그인해 주세요.",
        );
    }

    if error.status == Some(404)
        && error
            .url
            .as_deref()
            .is_some_and(|url| url.contains("/minecraft/profile"))
    {
        return AuthError::new(
            "AUTH_MINECRAFT_PROFILE_NOT_FOUND",
            "Microsoft 로그인은 완료됐지만 Minecraft Java 프로필을 찾지 못했습니다. 공식 Minecraft Launcher 또는 minecraft.net에서 Java 프로필 이름을 한 번 생성한 뒤 다시 시도해 주세요.",
        );
    }

    if error.message.contains("Invalid app registration")
        || error.message.contains("aka.ms/AppRegInfo")
    {
        return AuthError::new(
            "AUTH_INVALID_APP_REGISTRATION",
            "현재 microsoftClientId는 Microsoft/Minecraft 인증에 사용할 수 있는 app registration으로 승인되지 않았습니다. config/app.config.json의 microsoftClientId와 redirect URI 설정을 확인하고, Minecraft/Xbox 인증이 허용된 앱 ID로 교체해 주세요.",
        );
    }

    if error.message.contains("AADSTS7000218")
        || error.message.contains("client_secret")
        || error.message.contains("client_assertion")
    {
        return AuthError::new(
            "AUTH_PUBLIC_CLIENT_MISCONFIGURED",
            "현재 Microsoft app registration이 public client로 올바르게 설정되지 않아 토큰 교환에 실패했습니다. Azure App registrations > Authentication에서 mobile/desktop redirect URI와 public client flow 설정을 확인해 주세요.",
        );
    }

    if error.message.contains("AADSTS50011") || error.message.contains("redirect_uri") {
        return AuthError::new(
            "AUTH_REDIRECT_URI_MISMATCH",
            "현재 Microsoft app registration에 redirect URI가 일치하지 않습니다. config/app.config.json의 microsoftRedirectUri와 Azure App registrations의 redirect URI를 동일하게 맞춰 주세요.",
        );
    }

    if error.status.is_some_and(|status| status >= 500) {
        return AuthError::new(
            "AUTH_REMOTE_SERVICE_UNAVAILABLE",
            "Microsoft 또는 Minecraft 인증 서버가 현재 올바르게 응답하지 않고 있습니다. 잠시 후 다시 시도해 주세요.",
        );
    }

    error
}

pub(crate) fn request_json(
    request: reqwest::blocking::RequestBuilder,
    url: &str,
) -> Result<Value, AuthError> {
    let response = request
        .send()
        .map_err(|error| AuthError::new("AUTH_NETWORK_ERROR", error.to_string()))?;
    let status = response.status();
    let payload = response.json::<Value>().unwrap_or(Value::Null);

    if !status.is_success() {
        return Err(AuthError::http(url, status.as_u16(), &payload));
    }

    Ok(payload)
}

pub(crate) fn auth_http_client() -> Result<reqwest::blocking::Client, AuthError> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(AUTH_HTTP_CONNECT_TIMEOUT_SECONDS))
        .timeout(Duration::from_secs(AUTH_HTTP_REQUEST_TIMEOUT_SECONDS))
        .build()
        .map_err(|error| AuthError::new("AUTH_CLIENT_FAILED", error.to_string()))
}

pub(crate) fn string_field(payload: &Value, field: &str) -> Result<String, AuthError> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            AuthError::new(
                "AUTH_UNEXPECTED_RESPONSE",
                format!("Missing auth response field: {field}"),
            )
        })
}

pub(crate) fn request_microsoft_tokens(
    client: &reqwest::blocking::Client,
    client_id: &str,
    redirect_uri: &str,
    form: &[(&str, &str)],
) -> Result<Value, AuthError> {
    let url = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
    let mut body = vec![
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("scope", "XboxLive.signin offline_access"),
    ];
    body.extend_from_slice(form);

    request_json(
        client
            .post(url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&body),
        url,
    )
}

pub(crate) fn exchange_authorization_code(
    client: &reqwest::blocking::Client,
    client_id: &str,
    redirect_uri: &str,
    code: &str,
    code_verifier: &str,
) -> Result<Value, AuthError> {
    request_microsoft_tokens(
        client,
        client_id,
        redirect_uri,
        &[
            ("code", code),
            ("grant_type", "authorization_code"),
            ("code_verifier", code_verifier),
        ],
    )
}

pub(crate) fn exchange_refresh_token(
    client: &reqwest::blocking::Client,
    client_id: &str,
    redirect_uri: &str,
    refresh_token: &str,
) -> Result<Value, AuthError> {
    request_microsoft_tokens(
        client,
        client_id,
        redirect_uri,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ],
    )
}

pub(crate) fn exchange_microsoft_tokens_for_minecraft(
    client: &reqwest::blocking::Client,
    microsoft_access_token: &str,
) -> Result<MinecraftSession, AuthError> {
    let xbox_url = "https://user.auth.xboxlive.com/user/authenticate";
    let xbox_response = request_json(
        client
            .post(xbox_url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&json!({
                "Properties": {
                    "AuthMethod": "RPS",
                    "SiteName": "user.auth.xboxlive.com",
                    "RpsTicket": format!("d={microsoft_access_token}")
                },
                "RelyingParty": "http://auth.xboxlive.com",
                "TokenType": "JWT"
            })),
        xbox_url,
    )?;
    let xbox_token = string_field(&xbox_response, "Token")?;
    let user_hash = xbox_response
        .pointer("/DisplayClaims/xui/0/uhs")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| AuthError::new("AUTH_UNEXPECTED_RESPONSE", "Missing Xbox user hash"))?;

    let xsts_url = "https://xsts.auth.xboxlive.com/xsts/authorize";
    let xsts_response = request_json(
        client
            .post(xsts_url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&json!({
                "Properties": {
                    "SandboxId": "RETAIL",
                    "UserTokens": [xbox_token]
                },
                "RelyingParty": "rp://api.minecraftservices.com/",
                "TokenType": "JWT"
            })),
        xsts_url,
    )?;
    let xsts_token = string_field(&xsts_response, "Token")?;

    let minecraft_login_url = "https://api.minecraftservices.com/authentication/login_with_xbox";
    let minecraft_login_response = request_json(
        client
            .post(minecraft_login_url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&json!({
                "identityToken": format!("XBL3.0 x={user_hash};{xsts_token}")
            })),
        minecraft_login_url,
    )?;
    let minecraft_access_token = string_field(&minecraft_login_response, "access_token")?;
    let minecraft_expires_in = minecraft_login_response
        .get("expires_in")
        .and_then(Value::as_i64)
        .unwrap_or(DEFAULT_MINECRAFT_TOKEN_EXPIRES_IN_SECONDS);

    let ownership_url = "https://api.minecraftservices.com/entitlements/mcstore";
    let ownership_response = request_json(
        client
            .get(ownership_url)
            .header("Authorization", format!("Bearer {minecraft_access_token}")),
        ownership_url,
    )?;

    let has_ownership = ownership_response
        .get("items")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|item| {
                matches!(
                    item.get("name").and_then(Value::as_str),
                    Some("game_minecraft") | Some("product_minecraft")
                )
            })
        });

    if !has_ownership {
        return Err(AuthError::new(
            "AUTH_MINECRAFT_OWNERSHIP_REQUIRED",
            "Minecraft entitlement not found for this account.",
        ));
    }

    let profile_url = "https://api.minecraftservices.com/minecraft/profile";
    let profile_response = request_json(
        client
            .get(profile_url)
            .header("Authorization", format!("Bearer {minecraft_access_token}")),
        profile_url,
    )?;
    let profile_id = string_field(&profile_response, "id")?;
    let player_name = string_field(&profile_response, "name")?;

    Ok(MinecraftSession {
        access_token: minecraft_access_token,
        player_name,
        profile_id,
        expires_in: minecraft_expires_in,
    })
}

pub(crate) fn auth_session_payload(
    refresh_token: String,
    minecraft_session: MinecraftSession,
    refreshed: bool,
) -> Value {
    json!({
        "source": "microsoft",
        "refreshToken": refresh_token,
        "accessToken": minecraft_session.access_token,
        "playerName": minecraft_session.player_name,
        "profileId": minecraft_session.profile_id,
        "expiresAt": now_ms() + (minecraft_session.expires_in * 1000),
        "refreshedAt": if refreshed { Value::from(now_ms()) } else { Value::Null }
    })
}

pub(crate) fn microsoft_auth_config(app_config: &Value) -> Result<(String, String), AuthError> {
    let client_id = app_config
        .get("microsoftClientId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let redirect_uri = app_config
        .get("microsoftRedirectUri")
        .and_then(Value::as_str)
        .unwrap_or("https://login.microsoftonline.com/common/oauth2/nativeclient")
        .trim()
        .to_string();

    if client_id.is_empty() {
        return Err(AuthError::new(
            "AUTH_NOT_CONFIGURED",
            "Set microsoftClientId in config/app.config.json before using sign-in.",
        ));
    }

    Ok((client_id, redirect_uri))
}

pub(crate) fn auth_session_needs_refresh(session: &Map<String, Value>) -> bool {
    session
        .get("expiresAt")
        .and_then(Value::as_i64)
        .is_none_or(|expires_at| expires_at <= now_ms() + AUTH_REFRESH_MARGIN_MS)
}

pub(crate) fn refresh_auth_session_if_needed(
    user_config: &mut Value,
    app_config: &Value,
) -> Result<bool, AuthError> {
    let _mutation_guard =
        lock_user_config_mutation().map_err(|error| AuthError::new("AUTH_CONFIG_FAILED", error))?;
    *user_config = load_or_create_user_config()
        .map_err(|error| AuthError::new("AUTH_CONFIG_FAILED", error))?;
    let previous = user_config.clone();
    let Some(session) = user_config.get("authSession").and_then(Value::as_object) else {
        return Ok(false);
    };

    if !auth_session_needs_refresh(session) {
        return Ok(false);
    }

    let refresh_token = session
        .get("refreshToken")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            AuthError::new(
                "AUTH_REFRESH_REQUIRED",
                "Saved auth session does not include a refresh token.",
            )
        })?;
    let (client_id, redirect_uri) = microsoft_auth_config(app_config)?;
    let client = auth_http_client()?;
    let microsoft_tokens =
        exchange_refresh_token(&client, &client_id, &redirect_uri, &refresh_token)?;
    let microsoft_access_token = string_field(&microsoft_tokens, "access_token")?;
    let next_refresh_token = microsoft_tokens
        .get("refresh_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .unwrap_or(refresh_token);
    let minecraft_session =
        exchange_microsoft_tokens_for_minecraft(&client, &microsoft_access_token)?;
    let refreshed_session = auth_session_payload(next_refresh_token, minecraft_session, true);

    if let Some(config) = user_config.as_object_mut() {
        config.insert("authSession".to_string(), refreshed_session);
    }

    save_user_config_if_changed(&previous, user_config)
        .map_err(|error| AuthError::new("AUTH_CONFIG_FAILED", error))?;
    Ok(true)
}

pub(crate) async fn capture_authorization_code(
    app: &tauri::AppHandle,
    client_id: &str,
    redirect_uri: &str,
) -> Result<AuthGrant, AuthError> {
    let code_verifier = create_code_verifier();
    let oauth_state = create_oauth_state();
    let mut auth_url =
        Url::parse("https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize")
            .map_err(|error| AuthError::new("AUTH_URL_INVALID", error.to_string()))?;

    auth_url
        .query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_mode", "query")
        .append_pair("scope", "XboxLive.signin offline_access")
        .append_pair("prompt", "select_account")
        .append_pair("state", &oauth_state)
        .append_pair("code_challenge", &create_code_challenge(&code_verifier))
        .append_pair("code_challenge_method", "S256");

    let (sender, receiver) = mpsc::channel::<Result<AuthGrant, AuthError>>();
    let redirect_url = Url::parse(redirect_uri)
        .map_err(|error| AuthError::new("AUTH_REDIRECT_INVALID", error.to_string()))?;
    let navigation_sender = sender.clone();
    let close_sender = sender.clone();
    let oauth_state_for_navigation = oauth_state.clone();
    let code_verifier_for_navigation = code_verifier.clone();

    let auth_window =
        WebviewWindowBuilder::new(app, "microsoft-auth", WebviewUrl::External(auth_url))
            .data_directory(
                local_webview_data_directory()
                    .map_err(|error| AuthError::new("AUTH_WINDOW_DATA_FAILED", error))?,
            )
            .title("Microsoft Sign-In")
            .inner_size(540.0, 720.0)
            .resizable(true)
            .center()
            .on_navigation(move |target_url| {
                if !is_matching_redirect_url(target_url, &redirect_url) {
                    return true;
                }

                let code = target_url
                    .query_pairs()
                    .find(|(key, _)| key == "code")
                    .map(|(_, value)| value.into_owned());
                let error = target_url
                    .query_pairs()
                    .find(|(key, _)| key == "error")
                    .map(|(_, value)| value.into_owned());
                let error_description = target_url
                    .query_pairs()
                    .find(|(key, _)| key == "error_description")
                    .map(|(_, value)| value.into_owned());
                let state = target_url
                    .query_pairs()
                    .find(|(key, _)| key == "state")
                    .map(|(_, value)| value.into_owned());

                let result = if state.as_deref() != Some(oauth_state_for_navigation.as_str()) {
                    Err(AuthError::new(
                        "AUTH_STATE_MISMATCH",
                        "로그인 상태를 확인하지 못했습니다. 다시 시도해 주세요.",
                    ))
                } else if let Some(code) = code {
                    Ok(AuthGrant {
                        code,
                        code_verifier: code_verifier_for_navigation.clone(),
                    })
                } else if error.as_deref() == Some("access_denied") {
                    Err(AuthError::new("AUTH_CANCELLED", "로그인을 취소했습니다."))
                } else {
                    Err(AuthError::new(
                        "AUTH_FAILED",
                        normalize_auth_error_message(
                            error_description.as_deref().or(error.as_deref()),
                            "Microsoft sign-in was cancelled.".to_string(),
                        ),
                    ))
                };

                let _ = navigation_sender.send(result);
                false
            })
            .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
            .build()
            .map_err(|error| AuthError::new("AUTH_WINDOW_FAILED", error.to_string()))?;

    auth_window.on_window_event(move |event| {
        if matches!(
            event,
            WindowEvent::Destroyed | WindowEvent::CloseRequested { .. }
        ) {
            let _ = close_sender.send(Err(AuthError::new(
                "AUTH_CANCELLED",
                "로그인을 취소했습니다.",
            )));
        }
    });

    let result = tauri::async_runtime::spawn_blocking(move || {
        receiver
            .recv()
            .map_err(|_| AuthError::new("AUTH_CANCELLED", "로그인을 취소했습니다."))
    })
    .await
    .map_err(|error| AuthError::new("AUTH_TASK_FAILED", error.to_string()))??;
    let _ = auth_window.close();
    result
}

pub(crate) async fn run_sign_in(app: &tauri::AppHandle) -> Result<Value, AuthError> {
    let app_config =
        load_app_config().map_err(|error| AuthError::new("AUTH_CONFIG_FAILED", error))?;
    let (client_id, redirect_uri) = microsoft_auth_config(&app_config)?;

    let auth_grant = capture_authorization_code(app, &client_id, &redirect_uri).await?;

    tauri::async_runtime::spawn_blocking(move || {
        let client = auth_http_client()?;
        let microsoft_tokens = exchange_authorization_code(
            &client,
            &client_id,
            &redirect_uri,
            &auth_grant.code,
            &auth_grant.code_verifier,
        )?;
        let microsoft_access_token = string_field(&microsoft_tokens, "access_token")?;
        let refresh_token = microsoft_tokens
            .get("refresh_token")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let minecraft_session =
            exchange_microsoft_tokens_for_minecraft(&client, &microsoft_access_token)?;

        let _mutation_guard = lock_user_config_mutation()
            .map_err(|error| AuthError::new("AUTH_CONFIG_FAILED", error))?;
        let mut user_config = load_or_create_user_config()
            .map_err(|error| AuthError::new("AUTH_CONFIG_FAILED", error))?;
        let previous = user_config.clone();
        let session = auth_session_payload(refresh_token, minecraft_session, false);

        if let Some(config) = user_config.as_object_mut() {
            config.insert("authSession".to_string(), session.clone());
        }

        save_user_config_if_changed(&previous, &user_config)
            .map_err(|error| AuthError::new("AUTH_CONFIG_FAILED", error))?;

        Ok(json!({
            "ok": true,
            "session": {
                "signedIn": true,
                "playerName": session.get("playerName").cloned().unwrap_or(Value::Null),
                "profileId": session.get("profileId").cloned().unwrap_or(Value::Null),
                "expiresAt": session.get("expiresAt").cloned().unwrap_or(Value::Null),
                "source": "microsoft"
            }
        }))
    })
    .await
    .map_err(|error| AuthError::new("AUTH_TASK_FAILED", error.to_string()))?
}

pub(crate) fn build_bootstrap_payload() -> Result<Value, String> {
    let app_config = load_app_config()?;
    let server_manifest = load_server_manifest()?;
    let user_config = load_or_create_user_config()?;
    let auth_summary = auth_summary(&user_config);
    let preflight = run_preflight(&auth_summary);
    let settings = user_config
        .get("settings")
        .cloned()
        .unwrap_or_else(|| default_user_config()["settings"].clone());

    Ok(json!({
        "appConfig": app_config_payload(&app_config),
        "backgroundAssets": background_assets(),
        "serverManifest": server_manifest,
        "userConfig": {
            "settings": settings
        },
        "authSummary": auth_summary,
        "preflight": preflight,
        "fatalError": null
    }))
}
