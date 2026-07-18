use crate::*;

pub(crate) const PROTECTED_SECRET_PREFIX: &str = "dpapi:v1:";

pub(crate) fn protect_secret(value: &str) -> Result<String, String> {
    if value.is_empty() || value.starts_with(PROTECTED_SECRET_PREFIX) {
        return Ok(value.to_string());
    }

    #[cfg(not(target_os = "windows"))]
    {
        return Ok(value.to_string());
    }

    #[cfg(target_os = "windows")]
    protect_secret_bytes(value.as_bytes()).map(|bytes| {
        format!(
            "{PROTECTED_SECRET_PREFIX}{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
        )
    })
}

pub(crate) fn unprotect_secret(value: &str) -> Result<String, String> {
    let Some(encoded) = value.strip_prefix(PROTECTED_SECRET_PREFIX) else {
        return Ok(value.to_string());
    };

    let encrypted = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|error| format!("보호된 토큰 인코딩을 읽지 못했습니다: {error}"))?;
    let decrypted = unprotect_secret_bytes(&encrypted)?;

    String::from_utf8(decrypted)
        .map_err(|error| format!("보호된 토큰을 UTF-8로 읽지 못했습니다: {error}"))
}

pub(crate) fn protect_auth_session_for_storage(config: &Value) -> Result<Value, String> {
    let mut next_config = config.clone();

    if let Some(session) = next_config
        .get_mut("authSession")
        .and_then(Value::as_object_mut)
    {
        protect_auth_field(session, "refreshToken")?;
        protect_auth_field(session, "accessToken")?;
    }

    if let Some(accounts) = next_config
        .get_mut("authAccounts")
        .and_then(Value::as_array_mut)
    {
        for session in accounts.iter_mut().filter_map(Value::as_object_mut) {
            protect_auth_field(session, "refreshToken")?;
            protect_auth_field(session, "accessToken")?;
        }
    }

    Ok(next_config)
}

pub(crate) fn unprotect_auth_session_from_storage(config: &mut Value) -> Result<(), String> {
    if let Some(session) = config.get_mut("authSession").and_then(Value::as_object_mut) {
        unprotect_auth_field(session, "refreshToken")?;
        unprotect_auth_field(session, "accessToken")?;
    }

    if let Some(accounts) = config.get_mut("authAccounts").and_then(Value::as_array_mut) {
        for session in accounts.iter_mut().filter_map(Value::as_object_mut) {
            unprotect_auth_field(session, "refreshToken")?;
            unprotect_auth_field(session, "accessToken")?;
        }
    }

    Ok(())
}

pub(crate) fn protect_auth_field(
    session: &mut Map<String, Value>,
    field: &str,
) -> Result<(), String> {
    let Some(value) = session.get(field).and_then(Value::as_str) else {
        return Ok(());
    };

    session.insert(field.to_string(), Value::String(protect_secret(value)?));
    Ok(())
}

pub(crate) fn unprotect_auth_field(
    session: &mut Map<String, Value>,
    field: &str,
) -> Result<(), String> {
    let Some(value) = session.get(field).and_then(Value::as_str) else {
        return Ok(());
    };

    session.insert(field.to_string(), Value::String(unprotect_secret(value)?));
    Ok(())
}

#[cfg(target_os = "windows")]
pub(crate) fn protect_secret_bytes(bytes: &[u8]) -> Result<Vec<u8>, String> {
    dpapi_protect(bytes, true)
}

#[cfg(target_os = "windows")]
pub(crate) fn unprotect_secret_bytes(bytes: &[u8]) -> Result<Vec<u8>, String> {
    dpapi_protect(bytes, false)
}

#[cfg(target_os = "windows")]
pub(crate) fn dpapi_protect(bytes: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
    use std::ptr;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes
            .len()
            .try_into()
            .map_err(|_| "토큰이 너무 커서 보호 저장할 수 없습니다.".to_string())?,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };

    let ok = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                ptr::null(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };

    if ok == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }

    let protected = unsafe {
        let slice = std::slice::from_raw_parts(output.pbData, output.cbData as usize);
        let protected = slice.to_vec();
        LocalFree(output.pbData as *mut core::ffi::c_void);
        protected
    };

    Ok(protected)
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn protect_secret_bytes(bytes: &[u8]) -> Result<Vec<u8>, String> {
    Ok(bytes.to_vec())
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn unprotect_secret_bytes(bytes: &[u8]) -> Result<Vec<u8>, String> {
    Ok(bytes.to_vec())
}
