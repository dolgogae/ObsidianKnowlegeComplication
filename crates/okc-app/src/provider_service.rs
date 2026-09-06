//! Provider profile, native credential-store, and capability-test service.

use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use okc_ai::{
    AI_SCHEMA_VERSION, AiRole, Embedder as _, EmbeddingBatchRequest, ProviderClient,
    ProviderConfig, ProviderKind, ProviderProfile, SecretString, StructuredGenerationRequest,
    StructuredGenerator as _,
};
use okc_core::CancellationToken;
use serde_json::json;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::{AppError, Result, set_private_file};

pub const OKC_KEYCHAIN_SERVICE: &str = "org.openai.okc.provider";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialStoreStatus {
    Available,
    Locked,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CredentialError {
    #[error("OS credential store is locked or access was denied")]
    Locked,
    #[error("OS credential store is unavailable on this system")]
    Unavailable,
    #[error("credential entry was not found")]
    NotFound,
    #[error("OS credential store operation failed")]
    Failed,
}

pub trait CredentialStore: Send + Sync {
    fn status(&self) -> CredentialStoreStatus;
    fn store(
        &self,
        service: &str,
        account: &str,
        secret: &str,
    ) -> std::result::Result<(), CredentialError>;
    fn load(
        &self,
        service: &str,
        account: &str,
    ) -> std::result::Result<Zeroizing<String>, CredentialError>;
    fn delete(&self, service: &str, account: &str) -> std::result::Result<(), CredentialError>;
}

#[cfg(feature = "native-keyring")]
#[derive(Debug, Default)]
pub struct NativeCredentialStore;

#[cfg(feature = "native-keyring")]
impl CredentialStore for NativeCredentialStore {
    fn status(&self) -> CredentialStoreStatus {
        match keyring::Entry::store_status() {
            Ok(()) => CredentialStoreStatus::Available,
            Err(keyring::Error::NoStorageAccess(_)) => CredentialStoreStatus::Locked,
            Err(keyring::Error::NoDefaultStore | keyring::Error::NotSupportedByStore(_)) => {
                CredentialStoreStatus::Unavailable
            }
            Err(_) => CredentialStoreStatus::Unavailable,
        }
    }

    fn store(
        &self,
        service: &str,
        account: &str,
        secret: &str,
    ) -> std::result::Result<(), CredentialError> {
        let entry =
            keyring::Entry::new(service, account).map_err(|error| map_keyring_error(&error))?;
        entry
            .set_password(secret)
            .map_err(|error| map_keyring_error(&error))
    }

    fn load(
        &self,
        service: &str,
        account: &str,
    ) -> std::result::Result<Zeroizing<String>, CredentialError> {
        let entry =
            keyring::Entry::new(service, account).map_err(|error| map_keyring_error(&error))?;
        entry
            .get_password()
            .map(Zeroizing::new)
            .map_err(|error| map_keyring_error(&error))
    }

    fn delete(&self, service: &str, account: &str) -> std::result::Result<(), CredentialError> {
        let entry =
            keyring::Entry::new(service, account).map_err(|error| map_keyring_error(&error))?;
        entry
            .delete_credential()
            .map_err(|error| map_keyring_error(&error))
    }
}

#[cfg(feature = "native-keyring")]
fn map_keyring_error(error: &keyring::Error) -> CredentialError {
    match error {
        keyring::Error::NoStorageAccess(_) => CredentialError::Locked,
        keyring::Error::NoDefaultStore | keyring::Error::NotSupportedByStore(_) => {
            CredentialError::Unavailable
        }
        keyring::Error::NoEntry => CredentialError::NotFound,
        _ => CredentialError::Failed,
    }
}

#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct SecretInput(String);

impl SecretInput {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn empty() -> Self {
        Self(String::new())
    }

    pub fn push(&mut self, character: char) {
        self.0.push(character);
    }

    pub fn pop(&mut self) {
        self.0.pop();
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// A fixed-width indicator deliberately reveals neither the secret value nor
    /// its length.
    pub fn masked(&self) -> &'static str {
        if self.0.is_empty() {
            ""
        } else {
            "************"
        }
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl Debug for SecretInput {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("[redacted]")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityTest {
    pub profile_name: String,
    pub generation: bool,
    pub embeddings: bool,
}

#[derive(Clone)]
pub struct ProviderService {
    config_path: PathBuf,
    credential_store: Arc<dyn CredentialStore>,
    fixed_config: Option<Arc<ProviderConfig>>,
}

impl Debug for ProviderService {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderService")
            .field("config_path", &self.config_path)
            .field("fixed_config", &self.fixed_config.is_some())
            .field("credential_store", &"<credential-store>")
            .finish()
    }
}

impl ProviderService {
    #[cfg(feature = "native-keyring")]
    pub fn from_environment() -> Result<Self> {
        Ok(Self::new(
            provider_config_path()?,
            Arc::new(NativeCredentialStore),
        ))
    }

    pub fn new(config_path: PathBuf, credential_store: Arc<dyn CredentialStore>) -> Self {
        Self {
            config_path,
            credential_store,
            fixed_config: None,
        }
    }

    /// Construct an immutable, environment-secret-only provider service for
    /// embedders such as language bindings. It performs no config-file or
    /// native-keyring discovery.
    pub fn from_fixed_config(config: &ProviderConfig) -> Result<Self> {
        // Round-trip validation covers the schema, names, endpoints, limits,
        // and profile secret-reference policy without resolving any secret.
        let config = ProviderConfig::from_toml(&config.to_toml()?)?;
        if config
            .profiles
            .values()
            .any(|profile| profile.os_keychain.is_some())
        {
            return Err(AppError::InvalidProject(
                "embedded provider profiles cannot use os_keychain".into(),
            ));
        }
        Ok(Self {
            config_path: PathBuf::new(),
            credential_store: Arc::new(UnavailableCredentialStore),
            fixed_config: Some(Arc::new(config)),
        })
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn credential_store_status(&self) -> CredentialStoreStatus {
        self.credential_store.status()
    }

    pub fn load_config(&self) -> Result<ProviderConfig> {
        if let Some(config) = &self.fixed_config {
            return Ok((**config).clone());
        }
        if !self.config_path.exists() {
            return Ok(ProviderConfig {
                schema_version: AI_SCHEMA_VERSION,
                profiles: BTreeMap::new(),
            });
        }
        let text = fs::read_to_string(&self.config_path)?;
        Ok(ProviderConfig::from_toml(&text)?)
    }

    pub fn profile(&self, name: &str) -> Result<ProviderProfile> {
        self.load_config()?
            .profiles
            .remove(name)
            .ok_or_else(|| AppError::InvalidProject(format!("unknown provider profile `{name}`")))
    }

    pub fn upsert_profile(
        &self,
        name: &str,
        mut profile: ProviderProfile,
        secret: Option<SecretInput>,
    ) -> Result<()> {
        if self.fixed_config.is_some() {
            return Err(AppError::InvalidProject(
                "embedded provider profiles are immutable".into(),
            ));
        }
        validate_profile_name(name)?;
        if profile.kind == ProviderKind::Command {
            return Err(AppError::InvalidProject(
                "schema-3 command provider profiles are not implemented".into(),
            ));
        }
        if secret.is_some() && profile.os_keychain.is_none() {
            profile.os_keychain = Some(name.to_owned());
        }
        profile.validate()?;
        let old = self.load_config()?.profiles.get(name).cloned();
        if let Some(secret) = secret {
            let account = profile.os_keychain.as_deref().ok_or_else(|| {
                AppError::InvalidProject(
                    "native secret storage requires an os_keychain account reference".into(),
                )
            })?;
            self.credential_store
                .store(OKC_KEYCHAIN_SERVICE, account, secret.expose())?;
        }
        let mut config = self.load_config()?;
        config.profiles.insert(name.to_owned(), profile.clone());
        if let Err(error) = self.save_config(&config) {
            if old
                .as_ref()
                .and_then(|value| value.os_keychain.as_ref())
                .is_none()
                && let Some(account) = profile.os_keychain.as_deref()
            {
                let _ = self.credential_store.delete(OKC_KEYCHAIN_SERVICE, account);
            }
            return Err(error);
        }
        if let Some(old_account) = old.and_then(|value| value.os_keychain)
            && Some(old_account.as_str()) != profile.os_keychain.as_deref()
        {
            let _ = self
                .credential_store
                .delete(OKC_KEYCHAIN_SERVICE, &old_account);
        }
        Ok(())
    }

    pub fn remove_profile(&self, name: &str) -> Result<()> {
        if self.fixed_config.is_some() {
            return Err(AppError::InvalidProject(
                "embedded provider profiles are immutable".into(),
            ));
        }
        let mut config = self.load_config()?;
        let profile = config.profiles.remove(name).ok_or_else(|| {
            AppError::InvalidProject(format!("unknown provider profile `{name}`"))
        })?;
        self.save_config(&config)?;
        if let Some(account) = profile.os_keychain {
            match self.credential_store.delete(OKC_KEYCHAIN_SERVICE, &account) {
                Ok(()) | Err(CredentialError::NotFound) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    pub fn client(&self, name: &str) -> Result<ProviderClient> {
        let profile = self.profile(name)?;
        if let Some(account) = &profile.os_keychain {
            let secret = self.credential_store.load(OKC_KEYCHAIN_SERVICE, account)?;
            if secret.is_empty() {
                return Err(CredentialError::NotFound.into());
            }
            return Ok(ProviderClient::with_api_key(
                profile,
                Some(SecretString::new(secret.to_string())),
            )?);
        }
        Ok(ProviderClient::new(profile)?)
    }

    pub fn test_profile(
        &self,
        name: &str,
        cancellation: &CancellationToken,
    ) -> Result<CapabilityTest> {
        let client = self.client(name)?;
        let schema = json!({
            "type": "object",
            "properties": {"ok": {"type": "boolean"}},
            "required": ["ok"],
            "additionalProperties": false
        });
        let response = client.generate_structured(
            &StructuredGenerationRequest {
                schema_version: AI_SCHEMA_VERSION,
                role: AiRole::Critic,
                task_id: "provider-test-v3".into(),
                system_instruction: "Return {\"ok\":true}. No tools are available.".into(),
                input: json!({"synthetic": true}),
                schema_name: "okc_provider_test".into(),
                output_schema: schema,
                max_output_tokens: 32,
                temperature: Some(0.0),
            },
            cancellation,
        )?;
        if response.output != json!({"ok": true}) {
            return Err(AppError::InvalidProject(
                "provider returned an incorrect synthetic capability result".into(),
            ));
        }
        let embeddings = okc_ai::Embedder::capabilities(&client).embeddings;
        if embeddings {
            client.embed(
                &EmbeddingBatchRequest {
                    schema_version: AI_SCHEMA_VERSION,
                    task_id: "provider-test-embedding-v3".into(),
                    inputs: vec!["synthetic provider capability test".into()],
                    dimensions: None,
                },
                cancellation,
            )?;
        }
        Ok(CapabilityTest {
            profile_name: name.to_owned(),
            generation: true,
            embeddings,
        })
    }

    fn save_config(&self, config: &ProviderConfig) -> Result<()> {
        if self.fixed_config.is_some() {
            return Err(AppError::InvalidProject(
                "embedded provider profiles are immutable".into(),
            ));
        }
        let parent = self.config_path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let content = config.to_toml()?;
        let mut staged = tempfile::NamedTempFile::new_in(parent)?;
        staged.write_all(content.as_bytes())?;
        staged.as_file_mut().sync_all()?;
        staged
            .persist(&self.config_path)
            .map_err(|error| error.error)?;
        set_private_file(&self.config_path)
    }
}

#[derive(Debug)]
struct UnavailableCredentialStore;

impl CredentialStore for UnavailableCredentialStore {
    fn status(&self) -> CredentialStoreStatus {
        CredentialStoreStatus::Unavailable
    }

    fn store(
        &self,
        _service: &str,
        _account: &str,
        _secret: &str,
    ) -> std::result::Result<(), CredentialError> {
        Err(CredentialError::Unavailable)
    }

    fn load(
        &self,
        _service: &str,
        _account: &str,
    ) -> std::result::Result<Zeroizing<String>, CredentialError> {
        Err(CredentialError::Unavailable)
    }

    fn delete(&self, _service: &str, _account: &str) -> std::result::Result<(), CredentialError> {
        Err(CredentialError::Unavailable)
    }
}

pub fn default_endpoint(kind: ProviderKind) -> Option<&'static str> {
    match kind {
        ProviderKind::OpenAi => Some("https://api.openai.com/v1"),
        ProviderKind::Anthropic => Some("https://api.anthropic.com/v1"),
        ProviderKind::Gemini => Some("https://generativelanguage.googleapis.com/v1beta"),
        ProviderKind::Ollama => Some("http://127.0.0.1:11434"),
        ProviderKind::OpenAiCompatible | ProviderKind::Command => None,
    }
}

pub fn provider_config_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("OKC_PROVIDER_CONFIG") {
        return Ok(PathBuf::from(path));
    }
    if let Some(root) = std::env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(root).join("okc/providers.toml"));
    }
    let root = std::env::var_os("HOME").ok_or_else(|| {
        AppError::InvalidProject(
            "set OKC_PROVIDER_CONFIG or XDG_CONFIG_HOME for provider profiles".into(),
        )
    })?;
    Ok(PathBuf::from(root).join(".config/okc/providers.toml"))
}

pub fn validate_profile_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 128
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(AppError::InvalidProject(
            "provider profile name is invalid".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use super::*;

    #[derive(Debug)]
    struct MockCredentialStore {
        status: CredentialStoreStatus,
        values: Mutex<BTreeMap<(String, String), String>>,
    }

    impl CredentialStore for MockCredentialStore {
        fn status(&self) -> CredentialStoreStatus {
            self.status
        }

        fn store(
            &self,
            service: &str,
            account: &str,
            secret: &str,
        ) -> std::result::Result<(), CredentialError> {
            match self.status {
                CredentialStoreStatus::Available => {
                    self.values
                        .lock()
                        .expect("values")
                        .insert((service.into(), account.into()), secret.into());
                    Ok(())
                }
                CredentialStoreStatus::Locked => Err(CredentialError::Locked),
                CredentialStoreStatus::Unavailable => Err(CredentialError::Unavailable),
            }
        }

        fn load(
            &self,
            service: &str,
            account: &str,
        ) -> std::result::Result<Zeroizing<String>, CredentialError> {
            match self.status {
                CredentialStoreStatus::Available => self
                    .values
                    .lock()
                    .expect("values")
                    .get(&(service.into(), account.into()))
                    .cloned()
                    .map(Zeroizing::new)
                    .ok_or(CredentialError::NotFound),
                CredentialStoreStatus::Locked => Err(CredentialError::Locked),
                CredentialStoreStatus::Unavailable => Err(CredentialError::Unavailable),
            }
        }

        fn delete(&self, service: &str, account: &str) -> std::result::Result<(), CredentialError> {
            self.values
                .lock()
                .expect("values")
                .remove(&(service.into(), account.into()))
                .map(|_| ())
                .ok_or(CredentialError::NotFound)
        }
    }

    fn profile(account: Option<&str>) -> ProviderProfile {
        ProviderProfile {
            kind: ProviderKind::OpenAi,
            endpoint: "https://api.openai.com/v1".into(),
            model: "explicit-model".into(),
            api_key_env: None,
            os_keychain: account.map(str::to_owned),
            timeout_ms: 100,
            max_response_bytes: 1_024,
            max_input_bytes: 1_024,
            max_batch_items: 8,
            options: BTreeMap::new(),
        }
    }

    #[test]
    fn mock_keychain_stores_replaces_reads_and_deletes_without_serializing_secret() {
        let temporary = tempfile::tempdir().expect("temporary");
        let store = Arc::new(MockCredentialStore {
            status: CredentialStoreStatus::Available,
            values: Mutex::new(BTreeMap::new()),
        });
        let service = ProviderService::new(temporary.path().join("providers.toml"), store.clone());
        let secret_value = "never-serialize-this-token";
        service
            .upsert_profile(
                "remote",
                profile(Some("remote")),
                Some(SecretInput::new(secret_value.into())),
            )
            .expect("store");
        service
            .upsert_profile(
                "remote",
                profile(Some("remote")),
                Some(SecretInput::new("replacement".into())),
            )
            .expect("replace");
        let config = fs::read_to_string(service.config_path()).expect("config");
        assert!(config.contains("os_keychain"));
        assert!(!config.contains(secret_value));
        assert!(!format!("{:?}", SecretInput::new(secret_value.into())).contains(secret_value));
        assert_eq!(
            store
                .load(OKC_KEYCHAIN_SERVICE, "remote")
                .expect("load")
                .as_str(),
            "replacement"
        );
        service.remove_profile("remote").expect("remove");
        assert!(matches!(
            store.load(OKC_KEYCHAIN_SERVICE, "remote"),
            Err(CredentialError::NotFound)
        ));
    }

    #[test]
    fn locked_or_unavailable_store_never_writes_a_plaintext_fallback() {
        for status in [
            CredentialStoreStatus::Locked,
            CredentialStoreStatus::Unavailable,
        ] {
            let temporary = tempfile::tempdir().expect("temporary");
            let service = ProviderService::new(
                temporary.path().join("providers.toml"),
                Arc::new(MockCredentialStore {
                    status,
                    values: Mutex::new(BTreeMap::new()),
                }),
            );
            assert!(
                service
                    .upsert_profile(
                        "remote",
                        profile(Some("remote")),
                        Some(SecretInput::new("secret".into()))
                    )
                    .is_err()
            );
            assert!(!service.config_path().exists());
        }
    }
}
