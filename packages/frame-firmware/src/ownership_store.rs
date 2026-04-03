#[cfg(target_os = "espidf")]
use anyhow::{Context, Result};
#[cfg(target_os = "espidf")]
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs};
#[cfg(target_os = "espidf")]
use frame_core::models::GoogleUser;

#[cfg(target_os = "espidf")]
const OWNER_NAMESPACE: &str = "frame_owner";
#[cfg(target_os = "espidf")]
const KEY_OWNER_EMAIL: &str = "email";
#[cfg(target_os = "espidf")]
const KEY_OWNER_SUBJECT: &str = "subject";
#[cfg(target_os = "espidf")]
const KEY_REFRESH_TOKEN: &str = "refresh";

#[cfg(target_os = "espidf")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredOwnerSession {
    pub owner: GoogleUser,
    pub refresh_token: String,
}

#[cfg(target_os = "espidf")]
pub struct OwnerStore {
    nvs: EspNvs<esp_idf_svc::nvs::NvsDefault>,
}

#[cfg(target_os = "espidf")]
impl OwnerStore {
    pub fn new(partition: EspDefaultNvsPartition) -> Result<Self> {
        let nvs = EspNvs::new(partition, OWNER_NAMESPACE, true)
            .context("failed to open owner storage namespace")?;

        Ok(Self { nvs })
    }

    pub fn load(&self) -> Result<Option<StoredOwnerSession>> {
        let email = self.get_string(KEY_OWNER_EMAIL)?;
        let subject = self.get_string(KEY_OWNER_SUBJECT)?;
        let refresh_token = self.get_string(KEY_REFRESH_TOKEN)?;

        match (email, subject, refresh_token) {
            (Some(email), Some(subject), Some(refresh_token)) => Ok(Some(StoredOwnerSession {
                owner: GoogleUser {
                    email,
                    subject,
                    refresh_token: refresh_token.clone(),
                },
                refresh_token,
            })),
            _ => Ok(None),
        }
    }

    pub fn save(&self, session: &StoredOwnerSession) -> Result<()> {
        self.nvs
            .set_str(KEY_OWNER_EMAIL, &session.owner.email)
            .context("failed to persist owner email")?;
        self.nvs
            .set_str(KEY_OWNER_SUBJECT, &session.owner.subject)
            .context("failed to persist owner subject")?;
        self.nvs
            .set_str(KEY_REFRESH_TOKEN, &session.refresh_token)
            .context("failed to persist owner refresh token")?;

        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        for key in [KEY_OWNER_EMAIL, KEY_OWNER_SUBJECT, KEY_REFRESH_TOKEN] {
            self.nvs
                .remove(key)
                .with_context(|| format!("failed to clear owner storage key '{key}'"))?;
        }

        Ok(())
    }

    fn get_string(&self, key: &str) -> Result<Option<String>> {
        let Some(length) = self
            .nvs
            .str_len(key)
            .with_context(|| format!("failed to read owner storage length for '{key}'"))?
        else {
            return Ok(None);
        };

        let mut buffer = vec![0_u8; length];
        let value = self
            .nvs
            .get_str(key, &mut buffer)
            .with_context(|| format!("failed to read owner storage value for '{key}'"))?;

        Ok(value.map(ToOwned::to_owned))
    }
}
