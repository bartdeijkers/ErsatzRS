use async_trait::async_trait;
use ersatzrs_app::{MediaSourceRuntime, MediaSourceRuntimeError, PlexPinFlow, RemoteServerInfo};
use ersatzrs_core::repositories::{
    JellyfinConnectionConfig, JellyfinService as _, ManagedLibraryOperationRepository,
    ManagedLibraryScanSpec, RemoteMediaSourceKind,
};
use ersatzrs_domain::{ManagedLibraryOperationKind, MediaSourceKind};
use ersatzrs_infra::db::DbPool;

#[derive(Clone)]
pub struct NativeMediaSourceRuntime {
    pool: DbPool,
}

impl NativeMediaSourceRuntime {
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MediaSourceRuntime for NativeMediaSourceRuntime {
    async fn refresh_plex_libraries(&self, source_id: i64) -> Result<(), MediaSourceRuntimeError> {
        ersatzrs_infra::plex::refresh_plex_libraries(&self.pool, source_id)
            .await
            .map_err(|error| MediaSourceRuntimeError::new(error.to_string()))
    }
    async fn validate_jellyfin(
        &self,
        address: &str,
        api_key: &str,
    ) -> Result<RemoteServerInfo, MediaSourceRuntimeError> {
        let info = ersatzrs_infra::jellyfin::fetch_server_information(address, api_key)
            .await
            .map_err(MediaSourceRuntimeError::new)?;
        Ok(RemoteServerInfo {
            server_name: info.server_name,
            operating_system: info.operating_system,
        })
    }

    async fn refresh_jellyfin_libraries(
        &self,
        source_id: i64,
        address: &str,
        api_key: &str,
    ) -> Result<(), MediaSourceRuntimeError> {
        let service = ersatzrs_infra::jellyfin::JellyfinService::new();
        let config = JellyfinConnectionConfig {
            address: address.to_owned(),
            authorization_header: ersatzrs_infra::jellyfin::format_jellyfin_auth_header(
                api_key.to_owned(),
            ),
        };
        let libraries = service
            .list_libraries(&config)
            .await
            .map_err(|error| MediaSourceRuntimeError::new(error.to_string()))?;
        ersatzrs_infra::jellyfin::sync_jellyfin_libraries(&self.pool, source_id, &libraries)
            .await
            .map_err(|error| MediaSourceRuntimeError::new(error.to_string()))
    }

    fn request_library_sync(&self, kind: RemoteMediaSourceKind, library_ids: Vec<i64>) {
        if library_ids.is_empty() {
            return;
        }
        let operations = ersatzrs_infra::repositories::SqliteManagedLibraryOperationRepository(
            self.pool.clone(),
        );
        tokio::spawn(async move {
            let provider = match kind {
                RemoteMediaSourceKind::Jellyfin => MediaSourceKind::Jellyfin,
                RemoteMediaSourceKind::Emby => MediaSourceKind::Emby,
                RemoteMediaSourceKind::Plex => MediaSourceKind::Plex,
            };
            for library_id in library_ids {
                let result = operations
                    .enqueue_scan(
                        ManagedLibraryOperationKind::ProviderRefresh,
                        library_id,
                        ManagedLibraryScanSpec {
                            provider,
                            show_id: None,
                            deep_scan: false,
                        },
                        true,
                    )
                    .await;
                if let Err(error) = result {
                    tracing::warn!(library_id, %error, "remote media library sync admission failed");
                }
            }
        });
    }

    fn request_external_collections_sync(
        &self,
        kind: RemoteMediaSourceKind,
        media_source_id: i64,
        deep_scan: bool,
        guard: ersatzrs_app::operations::OperationGuard,
    ) {
        let pool = self.pool.clone();
        tokio::spawn(async move {
            let _guard = guard;
            let result = match kind {
                RemoteMediaSourceKind::Plex => {
                    ersatzrs_infra::plex::sync_collections_for_media_source(
                        &pool,
                        media_source_id,
                        deep_scan,
                    )
                    .await
                }
                RemoteMediaSourceKind::Jellyfin => {
                    ersatzrs_infra::jellyfin::sync_collections_for_media_source(
                        &pool,
                        media_source_id,
                        deep_scan,
                    )
                    .await
                }
                RemoteMediaSourceKind::Emby => {
                    ersatzrs_infra::emby::sync_collections_for_media_source(
                        &pool,
                        media_source_id,
                        deep_scan,
                    )
                    .await
                }
            };
            if let Err(error) = result {
                tracing::warn!(media_source_id, %error, "external collections sync failed");
            }
        });
    }

    async fn start_plex_pin(
        &self,
        force_new_credentials: bool,
    ) -> Result<PlexPinFlow, MediaSourceRuntimeError> {
        let pin = ersatzrs_infra::plex::PlexService::new()
            .start_account_authentication(&self.pool, force_new_credentials)
            .await
            .map_err(|error| MediaSourceRuntimeError::new(error.to_string()))?;
        Ok(PlexPinFlow {
            id: pin.id,
            code: pin.code,
            auth_url: pin.auth_url,
        })
    }

    async fn complete_plex_pin(&self, pin_id: &str) -> Result<bool, MediaSourceRuntimeError> {
        let service = ersatzrs_infra::plex::PlexService::new();
        let completed = service
            .complete_account_authentication(&self.pool, pin_id)
            .await
            .map_err(|error| MediaSourceRuntimeError::new(error.to_string()))?;
        if completed {
            service
                .refresh_account_servers(&self.pool)
                .await
                .map_err(|error| MediaSourceRuntimeError::new(error.to_string()))?;
        }
        Ok(completed)
    }
}
