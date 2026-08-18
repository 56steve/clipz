use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

#[cfg(windows)]
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
};

#[derive(Clone)]
pub struct SensitiveItem {
    pub id: String,
    pub content: Zeroizing<String>,
    pub source_app: String,
    pub created_at: Instant,
    pub ttl_secs: u64,
}

#[derive(Clone)]
pub struct SecurityManager {
    transient_store: Arc<Mutex<HashMap<String, SensitiveItem>>>,
}

impl SecurityManager {
    pub fn new() -> Self {
        Self {
            transient_store: Arc::new(Mutex::new(HashMap::<String, SensitiveItem>::new())),
        }
    }

    pub fn start_cleanup_task(&self) {
        let store_clone = Arc::clone(&self.transient_store);
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                let mut guard = store_clone.lock().unwrap();
                let now = Instant::now();
                guard.retain(|_, item| now.duration_since(item.created_at).as_secs() < item.ttl_secs);
            }
        });
    }

    pub fn is_sensitive_source(app_name: &str) -> bool {
        let name_lower = app_name.to_lowercase();
        let password_managers = [
            "1password",
            "bitwarden",
            "keepass",
            "keepassxc",
            "dashlane",
            "lastpass",
            "enpass",
            "keeper",
            "roboform",
            "authenticator",
        ];

        for pm in &password_managers {
            if name_lower.contains(pm) {
                return true;
            }
        }
        false
    }

    pub fn is_sensitive_pattern(text: &str) -> bool {
        let trimmed = text.trim();

        // API Key & Token patterns
        if trimmed.starts_with("sk-")
            || trimmed.starts_with("ghp_")
            || trimmed.starts_with("AKIA")
            || trimmed.starts_with("eyJ") // JWT token start
            || (trimmed.len() > 30 && trimmed.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
        {
            return true;
        }

        false
    }

    pub fn store_transient(&self, id: String, content: String, source_app: String) {
        let mut guard = self.transient_store.lock().unwrap();
        guard.insert(
            id.clone(),
            SensitiveItem {
                id,
                content: Zeroizing::new(content),
                source_app,
                created_at: Instant::now(),
                ttl_secs: 60,
            },
        );
    }

    pub fn get_transient(&self, id: &str) -> Option<String> {
        let guard = self.transient_store.lock().unwrap();
        if let Some(item) = guard.get(id) {
            let now = Instant::now();
            if now.duration_since(item.created_at).as_secs() < item.ttl_secs {
                return Some((*item.content).clone());
            }
        }
        None
    }

    pub fn clear_transient(&self, id: &str) {
        let mut guard = self.transient_store.lock().unwrap();
        guard.remove(id);
    }

    /// Windows DPAPI Encryption for persistent sensitive items
    #[cfg(windows)]
    pub fn encrypt_dpapi(data: &[u8]) -> Result<Vec<u8>, String> {
        unsafe {
            let mut input_blob = CRYPT_INTEGER_BLOB {
                cbData: data.len() as u32,
                pbData: data.as_ptr() as *mut u8,
            };
            let mut output_blob = CRYPT_INTEGER_BLOB {
                cbData: 0,
                pbData: std::ptr::null_mut(),
            };

            let res = CryptProtectData(
                &mut input_blob,
                windows::core::PCWSTR::null(),
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output_blob,
            );

            if res.is_ok() && !output_blob.pbData.is_null() {
                let result = std::slice::from_raw_parts(output_blob.pbData, output_blob.cbData as usize).to_vec();
                let _ = windows::Win32::Foundation::LocalFree(windows::Win32::Foundation::HLOCAL(output_blob.pbData as _));
                Ok(result)
            } else {
                Err("DPAPI encryption failed".to_string())
            }
        }
    }

    /// Windows DPAPI Decryption
    #[cfg(windows)]
    pub fn decrypt_dpapi(encrypted_data: &[u8]) -> Result<Vec<u8>, String> {
        unsafe {
            let mut input_blob = CRYPT_INTEGER_BLOB {
                cbData: encrypted_data.len() as u32,
                pbData: encrypted_data.as_ptr() as *mut u8,
            };
            let mut output_blob = CRYPT_INTEGER_BLOB {
                cbData: 0,
                pbData: std::ptr::null_mut(),
            };

            let res = CryptUnprotectData(
                &mut input_blob,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output_blob,
            );

            if res.is_ok() && !output_blob.pbData.is_null() {
                let result = std::slice::from_raw_parts(output_blob.pbData, output_blob.cbData as usize).to_vec();
                let _ = windows::Win32::Foundation::LocalFree(windows::Win32::Foundation::HLOCAL(output_blob.pbData as _));
                Ok(result)
            } else {
                Err("DPAPI decryption failed".to_string())
            }
        }
    }

    #[cfg(not(windows))]
    pub fn encrypt_dpapi(data: &[u8]) -> Result<Vec<u8>, String> {
        Ok(data.to_vec())
    }

    #[cfg(not(windows))]
    pub fn decrypt_dpapi(encrypted_data: &[u8]) -> Result<Vec<u8>, String> {
        Ok(encrypted_data.to_vec())
    }
}
