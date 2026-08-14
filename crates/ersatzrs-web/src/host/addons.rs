use async_trait::async_trait;
use ersatzrs_app::{
    AddonIconRead, AddonOverviewRead, AddonRepositoryEdit, AddonRepositoryRead, AddonRuntime,
    AddonRuntimeError, AddonSettingEdit, AddonSettingRead, AvailableAddonRead, InstalledAddonRead,
};
use ersatzrs_infra::addons::{
    check_addon, disable_addon, enable_addon, install_addon, install_addon_from_package,
    install_repository_from_package, list_available_addons, list_installed_addons,
    list_trusted_repositories, load_addon_settings, load_installed_addon, refresh_repository,
    remove_trusted_repository, repositories_are_deployment_managed, save_addon_setting,
    save_trusted_repository, set_unverified_sources_enabled, uninstall_addon,
    unverified_sources_enabled, AddonRuntimeContext, AddonSecretReferenceKind, AddonSettingValue,
    TrustedAddonRepository,
};
use ersatzrs_infra::db::DbPool;

#[derive(Clone)]
pub struct NativeAddonRuntime {
    pool: DbPool,
    client: reqwest::Client,
}

impl NativeAddonRuntime {
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self {
            pool,
            client: addon_repository_client(),
        }
    }
}

pub(crate) fn addon_repository_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

#[async_trait]
impl AddonRuntime for NativeAddonRuntime {
    async fn overview(&self) -> Result<AddonOverviewRead, AddonRuntimeError> {
        let repositories = list_trusted_repositories(&self.pool)
            .await
            .map_err(runtime_error)?
            .into_iter()
            .map(map_repository)
            .collect();
        let available = list_available_addons(&self.pool)
            .await
            .map_err(runtime_error)?
            .into_iter()
            .map(|value| AvailableAddonRead {
                icon_url: value.addon.icon.as_ref().map(|_| {
                    format!(
                        "/api/addons/catalog-icon/{}/{}",
                        value.repository_id, value.addon.id
                    )
                }),
                repository_id: value.repository_id,
                addon: value.addon,
                host_compatible: value.host_compatible,
                package_available: value.package_available,
            })
            .collect();
        let mut installed = Vec::new();
        for value in list_installed_addons(&self.pool)
            .await
            .map_err(runtime_error)?
        {
            let stored_settings = load_addon_settings(&self.pool, &value.addon_id)
                .await
                .map_err(runtime_error)?;
            let settings = stored_settings
                .iter()
                .cloned()
                .map(|setting| AddonSettingRead {
                    key: setting.key,
                    value: setting.value,
                    reference_kind: setting.reference_kind.map(|kind| match kind {
                        AddonSecretReferenceKind::Environment => "environment".to_owned(),
                        AddonSecretReferenceKind::File => "file".to_owned(),
                    }),
                    reference: setting.reference,
                })
                .collect();
            let mut manifest = value.manifest;
            ersatzrs_infra::addon_media_storage::apply_effective_storage_default(
                &mut manifest,
                &stored_settings,
            )
            .map_err(runtime_error)?;
            installed.push(InstalledAddonRead {
                icon_url: manifest
                    .icon
                    .as_ref()
                    .map(|_| format!("/api/addons/installed-icon/{}", value.addon_id)),
                addon_id: value.addon_id,
                repository_id: value.repository_id,
                version: value.version.to_string(),
                enabled: value.enabled,
                trust_tier: value.trust_tier.as_str().to_owned(),
                manifest,
                settings,
                last_check_code: value.last_check_code,
                last_check_message: value.last_check_message,
                last_check_at: value.last_check_at,
                installed_at: value.installed_at,
                updated_at: value.updated_at,
            });
        }
        Ok(AddonOverviewRead {
            unverified_sources_enabled: unverified_sources_enabled(&self.pool)
                .await
                .map_err(runtime_error)?,
            repositories_are_deployment_managed: repositories_are_deployment_managed(),
            repositories,
            available,
            installed,
        })
    }

    async fn installed_icon(
        &self,
        addon_id: String,
    ) -> Result<Option<AddonIconRead>, AddonRuntimeError> {
        let Some(installed) = load_installed_addon(&self.pool, &addon_id)
            .await
            .map_err(runtime_error)?
        else {
            return Ok(None);
        };
        let Some(icon) = installed.manifest.icon else {
            return Ok(None);
        };
        let bytes = match tokio::fs::read(installed.install_path.join(icon.path)).await {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(AddonRuntimeError::new(error.to_string())),
        };
        if bytes.len() > 2 * 1024 * 1024 || !valid_icon_bytes(&bytes, icon.media_type) {
            return Err(AddonRuntimeError::new(
                "installed add-on icon does not match its manifest",
            ));
        }
        Ok(Some(AddonIconRead {
            bytes,
            content_type: icon.media_type.as_mime_type().to_owned(),
        }))
    }

    async fn save_repository(
        &self,
        repository: AddonRepositoryEdit,
    ) -> Result<(), AddonRuntimeError> {
        save_trusted_repository(
            &self.pool,
            &TrustedAddonRepository {
                id: repository.id,
                name: repository.name,
                index_url: repository.index_url,
                public_key: repository.public_key,
                is_official: false,
                is_enabled: repository.is_enabled,
                last_sequence: None,
                last_refresh: None,
                last_error: None,
            },
        )
        .await
        .map_err(runtime_error)
    }

    async fn remove_repository(&self, repository_id: String) -> Result<(), AddonRuntimeError> {
        remove_trusted_repository(&self.pool, &repository_id)
            .await
            .map_err(runtime_error)
    }

    async fn refresh_repository(&self, repository_id: String) -> Result<(), AddonRuntimeError> {
        refresh_repository(&self.pool, &self.client, &repository_id)
            .await
            .map(|_| ())
            .map_err(runtime_error)
    }

    async fn install(
        &self,
        repository_id: String,
        addon_id: String,
    ) -> Result<(), AddonRuntimeError> {
        install_addon(&self.pool, &self.client, &repository_id, &addon_id)
            .await
            .map(|_| ())
            .map_err(runtime_error)
    }

    async fn set_unverified_sources(&self, enabled: bool) -> Result<(), AddonRuntimeError> {
        set_unverified_sources_enabled(&self.pool, enabled)
            .await
            .map_err(runtime_error)
    }

    async fn install_from_package(&self, archive: Vec<u8>) -> Result<(), AddonRuntimeError> {
        install_addon_from_package(&self.pool, archive)
            .await
            .map(|_| ())
            .map_err(runtime_error)
    }

    async fn install_repository_from_package(
        &self,
        archive: Vec<u8>,
    ) -> Result<(), AddonRuntimeError> {
        install_repository_from_package(&self.pool, archive)
            .await
            .map(|_| ())
            .map_err(runtime_error)
    }

    async fn save_setting(&self, setting: AddonSettingEdit) -> Result<(), AddonRuntimeError> {
        let reference_kind = setting
            .reference_kind
            .as_deref()
            .map(|kind| match kind {
                "environment" => Ok(AddonSecretReferenceKind::Environment),
                "file" => Ok(AddonSecretReferenceKind::File),
                _ => Err(AddonRuntimeError::new("unknown secret reference kind")),
            })
            .transpose()?;
        save_addon_setting(
            &self.pool,
            &setting.addon_id,
            &AddonSettingValue {
                key: setting.key,
                value: setting.value,
                reference_kind,
                reference: setting.reference,
            },
        )
        .await
        .map_err(runtime_error)
    }

    async fn check(&self, addon_id: String) -> Result<String, AddonRuntimeError> {
        check_addon(&self.pool, &addon_id, &runtime_context())
            .await
            .map(|result| format!("{}: {}", result.code, result.message))
            .map_err(runtime_error)
    }

    async fn enable(&self, addon_id: String) -> Result<String, AddonRuntimeError> {
        enable_addon(&self.pool, &addon_id, &runtime_context())
            .await
            .map(|result| format!("{}: {}", result.code, result.message))
            .map_err(runtime_error)
    }

    async fn disable(&self, addon_id: String) -> Result<(), AddonRuntimeError> {
        disable_addon(&self.pool, &addon_id)
            .await
            .map_err(runtime_error)
    }

    async fn uninstall(&self, addon_id: String, force: bool) -> Result<(), AddonRuntimeError> {
        uninstall_addon(&self.pool, &addon_id, force)
            .await
            .map_err(runtime_error)
    }
}

fn runtime_context() -> AddonRuntimeContext {
    AddonRuntimeContext {
        ffmpeg_path: Some(ersatzrs_ffmpeg::ffmpeg_path()),
        ffprobe_path: Some(ersatzrs_ffmpeg::ffprobe_path()),
    }
}

fn valid_icon_bytes(bytes: &[u8], media_type: ersatzrs_addon_contract::AddonIconMediaType) -> bool {
    match media_type {
        ersatzrs_addon_contract::AddonIconMediaType::Png => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        ersatzrs_addon_contract::AddonIconMediaType::Webp => {
            bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP"
        }
    }
}

fn map_repository(value: TrustedAddonRepository) -> AddonRepositoryRead {
    let trust_tier = value.trust_tier().as_str().to_owned();
    AddonRepositoryRead {
        id: value.id,
        name: value.name,
        index_url: value.index_url,
        public_key: value.public_key,
        is_official: value.is_official,
        is_enabled: value.is_enabled,
        trust_tier,
        last_sequence: value.last_sequence,
        last_refresh: value.last_refresh,
        last_error: value.last_error,
    }
}

fn runtime_error(error: impl std::fmt::Display) -> AddonRuntimeError {
    AddonRuntimeError::new(error.to_string())
}
