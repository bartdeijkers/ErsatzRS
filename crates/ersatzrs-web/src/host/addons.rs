use async_trait::async_trait;
use ersatzrs_app::{
    AddonIconRead, AddonOverviewRead, AddonRepositoryRead, AddonRuntime, AddonRuntimeError,
    AddonSettingEdit, AddonSettingRead, AvailableAddonRead, InstalledAddonRead, SourceAddonRouting,
};
use ersatzrs_infra::addon_source_routing::{resolve_source_addon, ResolvedSourceAddon};
use ersatzrs_infra::addons::{
    check_addon, disable_addon, enable_addon, ensure_addon_checkouts_folder,
    install_addon_from_local_source, install_addon_from_package, install_local_repository_bundle,
    list_installed_addons, list_local_addon_sources, list_local_available_addons,
    load_addon_settings, load_installed_addon, load_local_addon_icon,
    remove_local_repository_bundle, save_addon_setting, scan_local_addon_repositories,
    set_default_remote_stream_addon, set_unverified_sources_enabled, uninstall_addon,
    unverified_sources_enabled, AddonRuntimeContext, AddonSecretReferenceKind, AddonSettingValue,
    LocalAddonSource,
};
use ersatzrs_infra::db::DbPool;

#[derive(Clone)]
pub struct NativeAddonRuntime {
    pool: DbPool,
}

impl NativeAddonRuntime {
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AddonRuntime for NativeAddonRuntime {
    async fn overview(&self) -> Result<AddonOverviewRead, AddonRuntimeError> {
        let repositories = list_local_addon_sources(&self.pool)
            .await
            .map_err(runtime_error)?
            .into_iter()
            .map(map_repository)
            .collect();
        let available = list_local_available_addons(&self.pool)
            .await
            .map_err(runtime_error)?
            .into_iter()
            .map(|value| {
                let icon_url = value.has_local_icon.then(|| {
                    format!(
                        "/api/addons/source-icon/{}/{}",
                        value.repository_id, value.addon.id
                    )
                });
                AvailableAddonRead {
                    icon_url,
                    repository_id: value.repository_id,
                    addon: value.addon,
                    host_compatible: value.host_compatible,
                    package_available: value.package_available,
                }
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
            let is_legacy = value.source_id.is_none() && value.repository_id.is_some();
            let provenance_id = value.source_id.clone().or(value.repository_id);
            installed.push(InstalledAddonRead {
                icon_url: manifest
                    .icon
                    .as_ref()
                    .map(|_| format!("/api/addons/installed-icon/{}", value.addon_id)),
                addon_id: value.addon_id,
                repository_id: provenance_id,
                version: value.version.to_string(),
                enabled: value.enabled,
                trust_tier: if is_legacy {
                    "legacy".to_owned()
                } else {
                    value.trust_tier.as_str().to_owned()
                },
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
            checkout_path: ensure_addon_checkouts_folder()
                .await
                .map_err(runtime_error)?
                .to_string_lossy()
                .into_owned(),
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

    async fn source_icon(
        &self,
        source_id: String,
        addon_id: String,
    ) -> Result<Option<AddonIconRead>, AddonRuntimeError> {
        load_local_addon_icon(&self.pool, &source_id, &addon_id)
            .await
            .map(|icon| {
                icon.map(|icon| AddonIconRead {
                    bytes: icon.bytes,
                    content_type: icon.media_type.as_mime_type().to_owned(),
                })
            })
            .map_err(runtime_error)
    }

    async fn scan_repositories(&self) -> Result<(), AddonRuntimeError> {
        scan_local_addon_repositories(&self.pool)
            .await
            .map(|_| ())
            .map_err(runtime_error)
    }

    async fn remove_repository(&self, repository_id: String) -> Result<(), AddonRuntimeError> {
        remove_local_repository_bundle(&self.pool, &repository_id)
            .await
            .map_err(runtime_error)
    }

    async fn install(
        &self,
        repository_id: String,
        addon_id: String,
        adopt_legacy: bool,
    ) -> Result<(), AddonRuntimeError> {
        install_addon_from_local_source(
            &self.pool,
            &repository_id,
            &addon_id,
            adopt_legacy,
            &runtime_context(),
        )
        .await
        .map(|_| ())
        .map_err(runtime_error)
    }

    async fn set_unverified_sources(&self, enabled: bool) -> Result<(), AddonRuntimeError> {
        set_unverified_sources_enabled(&self.pool, enabled)
            .await
            .map_err(runtime_error)
    }

    async fn resolve_source_addon(
        &self,
        url: String,
        required_capabilities: Vec<String>,
    ) -> Result<SourceAddonRouting, AddonRuntimeError> {
        let required_capabilities = required_capabilities
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let resolved = resolve_source_addon(&self.pool, &url, &required_capabilities)
            .await
            .map_err(runtime_error)?;
        Ok(match resolved {
            ResolvedSourceAddon::Declared(addon_id) => SourceAddonRouting::Declared(addon_id),
            ResolvedSourceAddon::Default(addon_id) => SourceAddonRouting::Default(addon_id),
            ResolvedSourceAddon::None => SourceAddonRouting::None,
            ResolvedSourceAddon::Ambiguous(addon_ids) => SourceAddonRouting::Ambiguous(addon_ids),
        })
    }

    async fn default_remote_stream_addon(&self) -> Result<Option<String>, AddonRuntimeError> {
        ersatzrs_infra::addons::default_remote_stream_addon(&self.pool)
            .await
            .map_err(runtime_error)
    }

    async fn set_default_remote_stream_addon(
        &self,
        addon_id: Option<String>,
    ) -> Result<(), AddonRuntimeError> {
        set_default_remote_stream_addon(&self.pool, addon_id.as_deref())
            .await
            .map_err(runtime_error)
    }

    async fn install_from_package(&self, archive: Vec<u8>) -> Result<(), AddonRuntimeError> {
        install_addon_from_package(&self.pool, archive, &runtime_context())
            .await
            .map(|_| ())
            .map_err(runtime_error)
    }

    async fn install_repository_bundle(&self, archive: Vec<u8>) -> Result<(), AddonRuntimeError> {
        install_local_repository_bundle(&self.pool, archive)
            .await
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

fn map_repository(value: LocalAddonSource) -> AddonRepositoryRead {
    AddonRepositoryRead {
        id: value.id,
        name: value.name,
        kind: value.kind,
        root_path: value.root_path.to_string_lossy().into_owned(),
        last_scan: value.last_scan,
        last_error: value.last_error,
    }
}

fn runtime_error(error: impl std::fmt::Display) -> AddonRuntimeError {
    AddonRuntimeError::new(error.to_string())
}
