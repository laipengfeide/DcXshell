use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use rand::RngCore;
use std::{fs, path::Path};

/// 本地密码保险箱：使用随机生成的 256 位密钥对连接密码进行 AES-256-GCM 加密。
pub struct SecretBox {
    key: [u8; 32],
}

impl SecretBox {
    pub fn load_or_create(path: &Path) -> Result<Self, String> {
        if path.exists() {
            let raw = fs::read(path).map_err(|e| format!("读取密钥文件失败: {e}"))?;
            if raw.len() != 32 {
                return Err("密钥文件已损坏，请删除后重新启动应用".into());
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&raw);
            return Ok(Self { key });
        }
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("创建目录失败: {e}"))?;
        }
        let mut key = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut key);
        fs::write(path, key).map_err(|e| format!("写入密钥文件失败: {e}"))?;
        Ok(Self { key })
    }

    pub fn encrypt(&self, plain: &str) -> Result<String, String> {
        if plain.is_empty() {
            return Ok(String::new());
        }
        let cipher = Aes256Gcm::new_from_slice(&self.key).map_err(|e| e.to_string())?;
        let mut nonce = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), plain.as_bytes())
            .map_err(|e| format!("加密失败: {e}"))?;
        let mut out = nonce.to_vec();
        out.extend_from_slice(&ciphertext);
        Ok(STANDARD.encode(out))
    }

    pub fn decrypt(&self, encoded: &str) -> Result<String, String> {
        if encoded.is_empty() {
            return Ok(String::new());
        }
        let raw = STANDARD
            .decode(encoded)
            .map_err(|e| format!("解密失败: {e}"))?;
        if raw.len() < 12 {
            return Err("密文格式无效".into());
        }
        let (nonce, ciphertext) = raw.split_at(12);
        let cipher = Aes256Gcm::new_from_slice(&self.key).map_err(|e| e.to_string())?;
        let plain = cipher
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|_| "密文校验失败，可能是密钥文件已更换".to_string())?;
        String::from_utf8(plain).map_err(|e| format!("解密内容无效: {e}"))
    }
}
