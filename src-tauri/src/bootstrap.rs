use crate::*;

pub(crate) fn auth_summary(user_config: &Value) -> AuthSummary {
    let session = user_config.get("authSession").and_then(Value::as_object);

    if session.is_none() {
        return AuthSummary {
            signed_in: false,
            player_name: None,
            profile_id: None,
            expires_at: None,
            source: None,
        };
    }

    let session = session.unwrap();
    AuthSummary {
        signed_in: true,
        player_name: session
            .get("playerName")
            .and_then(Value::as_str)
            .map(str::to_string),
        profile_id: session
            .get("profileId")
            .and_then(Value::as_str)
            .map(str::to_string),
        expires_at: session.get("expiresAt").and_then(Value::as_i64),
        source: session
            .get("source")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

pub(crate) fn auth_account_summaries(user_config: &Value) -> Vec<Value> {
    user_config
        .get("authAccounts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter_map(|session| {
            let player_name = session.get("playerName")?.as_str()?.trim();
            let profile_id = session.get("profileId")?.as_str()?.trim();

            if player_name.is_empty() || profile_id.is_empty() {
                return None;
            }

            Some(json!({
                "playerName": player_name,
                "profileId": profile_id,
                "expiresAt": session.get("expiresAt").cloned().unwrap_or(Value::Null),
                "source": session.get("source").cloned().unwrap_or_else(|| Value::String("microsoft".to_string()))
            }))
        })
        .collect()
}

pub(crate) fn run_preflight(auth_summary: &AuthSummary) -> PreflightReport {
    let mut diagnostics = Vec::new();

    if !auth_summary.signed_in {
        diagnostics.push(Diagnostic {
            level: "warning".to_string(),
            title: "계정 연결".to_string(),
            message: "메인 화면에서 계정을 연결한 뒤 시작할 수 있어요.".to_string(),
            blocking: true,
        });
    }

    let blocking_count = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.blocking)
        .count();

    PreflightReport {
        ready: blocking_count == 0,
        blocking_count,
        diagnostics,
    }
}

pub(crate) fn background_assets() -> Vec<String> {
    [
        "/assets/background/1.webp",
        "/assets/background/2.webp",
        "/assets/background/3.webp",
        "/assets/background/4.webp",
        "/assets/background/5.webp",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

pub(crate) fn app_config_payload(app_config: &Value) -> Value {
    json!({
        "productName": app_config.get("productName").cloned().unwrap_or(Value::String("star-prison".to_string())),
        "supportUrl": app_config.get("supportUrl").cloned().unwrap_or(Value::String(String::new())),
        "allowPrerelease": app_config.get("allowPrerelease").cloned().unwrap_or(Value::Bool(false)),
        "discordNotices": app_config.get("discordNotices").cloned().unwrap_or_else(|| json!({
            "enabled": false,
            "endpointUrl": "",
            "fallbackCards": []
        }))
    })
}

#[cfg(test)]
mod auth_account_summary_tests {
    use super::*;

    #[test]
    fn saved_account_summaries_exclude_tokens() {
        let user_config = json!({
            "authAccounts": [{
                "source": "microsoft",
                "refreshToken": "private-refresh-token",
                "accessToken": "private-access-token",
                "playerName": "Alpha",
                "profileId": "profile-a",
                "expiresAt": 1234
            }]
        });

        let summaries = auth_account_summaries(&user_config);

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0]["playerName"], "Alpha");
        assert!(summaries[0].get("refreshToken").is_none());
        assert!(summaries[0].get("accessToken").is_none());
    }
}
