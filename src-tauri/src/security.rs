
#[cfg(windows)]
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
};

#[derive(Clone)]
pub struct SecurityManager;

impl SecurityManager {
    pub fn new() -> Self {
        Self
    }

    /// Encrypt a secret for storage on disk.
    ///
    /// Sensitive clips used to be held in memory for sixty seconds and never
    /// written down, so they simply vanished and left a dead row behind. They
    /// are now kept like any other clip, but sealed with a key the operating
    /// key the operating system or this user account owns: DPAPI on Windows,
    /// where the key belongs to the Windows account and never touches disk, and
    /// AES-256-GCM on macOS with the key in an owner-only file beside the
    /// database. Neither asks the user for a password. That keeps the clips
    /// readable to the user and unreadable in the database file and in backups.
    ///
    /// It does NOT defend against malware already running as this user: it can
    /// ask the OS to unseal too. Nothing stored locally can.
    ///
    /// Fails closed. If sealing is unavailable the caller must store nothing
    /// rather than fall back to plain text.
    pub fn seal(&self, plaintext: &str) -> Result<Vec<u8>, String> {
        #[cfg(windows)]
        {
            Self::encrypt_dpapi(plaintext.as_bytes())
        }

        #[cfg(target_os = "macos")]
        {
            use aes_gcm::aead::{Aead, KeyInit};
            use aes_gcm::{Aes256Gcm, Nonce};
            use rand::RngCore;

            let key_bytes = Self::local_key()?;
            let cipher = Aes256Gcm::new_from_slice(&key_bytes).map_err(|e| e.to_string())?;

            let mut nonce_bytes = [0u8; 12];
            rand::thread_rng().fill_bytes(&mut nonce_bytes);
            let nonce = Nonce::from_slice(&nonce_bytes);

            let mut sealed = cipher
                .encrypt(nonce, plaintext.as_bytes())
                .map_err(|_| "Could not encrypt the clip".to_string())?;

            // The nonce is not a secret and must travel with the ciphertext.
            let mut out = nonce_bytes.to_vec();
            out.append(&mut sealed);
            Ok(out)
        }

        #[cfg(all(not(windows), not(target_os = "macos")))]
        {
            let _ = plaintext;
            Err("No secure store is available on this platform".to_string())
        }
    }

    /// Reverse of [`seal`].
    pub fn open(&self, sealed: &[u8]) -> Result<String, String> {
        #[cfg(windows)]
        {
            let plain = Self::decrypt_dpapi(sealed)?;
            String::from_utf8(plain).map_err(|_| "Stored secret is not valid text".to_string())
        }

        #[cfg(target_os = "macos")]
        {
            use aes_gcm::aead::{Aead, KeyInit};
            use aes_gcm::{Aes256Gcm, Nonce};

            if sealed.len() < 12 {
                return Err("Stored secret is corrupt".to_string());
            }
            let (nonce_bytes, ciphertext) = sealed.split_at(12);

            let key_bytes = Self::local_key()?;
            let cipher = Aes256Gcm::new_from_slice(&key_bytes).map_err(|e| e.to_string())?;
            let plain = cipher
                .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
                .map_err(|_| "Could not decrypt this clip".to_string())?;
            String::from_utf8(plain).map_err(|_| "Stored secret is not valid text".to_string())
        }

        #[cfg(all(not(windows), not(target_os = "macos")))]
        {
            let _ = sealed;
            Err("No secure store is available on this platform".to_string())
        }
    }

    #[cfg(target_os = "macos")]
    /// The local encryption key, created once and kept in the app's data
    /// directory with owner-only permissions.
    ///
    /// The Keychain would be the stronger store, but it prompts for the login
    /// password whenever the binary's signature changes — every update — which
    /// is not something to put in front of someone who just copied a password.
    /// A 0600 key file protects the clips from anything that reads the database
    /// file (backups, file sharing, another account on the machine); it does not
    /// protect against code already running as this user.
    fn local_key() -> Result<[u8; 32], String> {
        use rand::RngCore;
        use std::fs;
        use std::io::Write;

        let dir = crate::db::app_data_dir()
            .ok_or_else(|| "Could not work out where to keep the encryption key.".to_string())?;
        fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
        let key_path = dir.join("secret.key");

        if let Ok(existing) = fs::read(&key_path) {
            if existing.len() == 32 {
                let mut key = [0u8; 32];
                key.copy_from_slice(&existing);
                return Ok(key);
            }
        }

        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);

        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&key_path)
            .map_err(|e| format!("Could not create the encryption key: {e}"))?;
        file.write_all(&key)
            .map_err(|e| format!("Could not write the encryption key: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("Could not save the encryption key: {e}"))?;

        Ok(key)
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
        if trimmed.is_empty() {
            return false;
        }

        // Documented vendor prefixes. Each one is an unambiguous marker of a
        // credential, so matching on them will not mask ordinary text.
        const SECRET_PREFIXES: &[&str] = &[
            "sk-",                                                 // OpenAI and friends
            "sk_live_", "sk_test_", "rk_live_", "whsec_",          // Stripe
            "ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_", // GitHub
            "glpat-",                                              // GitLab
            "xoxb-", "xoxp-", "xoxa-", "xoxs-",                    // Slack
            "AKIA", "ASIA",                                        // AWS
            "AIza", "ya29.",                                       // Google
            "SG.",                                                 // SendGrid
            "dop_v1_",                                             // DigitalOcean
            "npm_",                                                // npm
            "hf_",                                                 // Hugging Face
            "eyJ",                                                 // JWT
        ];
        if SECRET_PREFIXES.iter().any(|prefix| trimmed.starts_with(prefix)) {
            return true;
        }

        if trimmed.starts_with("-----BEGIN") && trimmed.contains("PRIVATE KEY") {
            return true;
        }

        // Long opaque tokens written as hex with separators, such as session
        // ids. Unseparated hex is deliberately excluded: commit hashes and
        // checksums are not secrets, and masking them only gets in the way.
        if trimmed.len() > 30
            && trimmed.contains('-')
            && trimmed.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
        {
            return true;
        }

        false
    }

    #[cfg(windows)]
    fn encrypt_dpapi(data: &[u8]) -> Result<Vec<u8>, String> {
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
    fn decrypt_dpapi(encrypted_data: &[u8]) -> Result<Vec<u8>, String> {
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
}

#[cfg(test)]
mod tests {
    use super::SecurityManager;

    #[test]
    fn recognises_vendor_credentials() {
        for secret in [
            "sk-proj-abc123",
            "sk_live_51Hxxxxxxxxxxxxxxxx",
            "ghp_16CharactersOfTokenHere",
            "github_pat_11ABCDE",
            "glpat-xxxxxxxxxxxxxxxxxxxx",
            "xoxb-123-456-abcdef",
            "AKIAIOSFODNN7EXAMPLE",
            "AIzaSyA-ExampleKeyValue",
            "-----BEGIN OPENSSH PRIVATE KEY-----",
            "eyJhbGciOiJIUzI1NiJ9.e30.abc",
            "550e8400-e29b-41d4-a716-446655440000",
        ] {
            assert!(
                SecurityManager::is_sensitive_pattern(secret),
                "should be treated as sensitive: {secret}"
            );
        }
    }

    #[test]
    fn leaves_ordinary_text_alone() {
        for ordinary in [
            "",
            "https://example.com/pricing",
            "Remember to call the bank tomorrow",
            // A git commit hash: long hex, but not a secret.
            "e83c5163316f89bfbde7d9ab23ca2e25604af290",
            // A sha256 checksum.
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
        ] {
            assert!(
                !SecurityManager::is_sensitive_pattern(ordinary),
                "should not be masked: {ordinary}"
            );
        }
    }
}
