use crate::admin::i18n::{LanguageStrings, TextKey};

pub const SORT_ORDER: u16 = 130;
pub const CODE: &str = "es-es";
pub const LABEL: &str = "español (España)";
pub const ALIASES: &[&str] = &["es", "es-es", "es_es"];

pub struct SpanishStrings;

pub static STRINGS: SpanishStrings = SpanishStrings;

fn episode_display_subtitle(
    show_title: Option<&str>,
    season_number: Option<i64>,
    episode_number: Option<i64>,
) -> Option<String> {
    let show = show_title.unwrap_or("").trim();
    match (season_number, episode_number) {
        (Some(season), Some(episode)) if !show.is_empty() => {
            Some(format!("{show} - temporada {season}, episodio {episode}"))
        }
        (Some(season), Some(episode)) => {
            Some(format!("temporada {season}, episodio {episode}"))
        }
        _ => (!show.is_empty()).then(|| show.to_owned()),
    }
}

impl LanguageStrings for SpanishStrings {
    fn text(&self, key: TextKey) -> &'static str {
        text(key)
    }

    fn selected_count(&self, count: usize) -> String {
        format!("{count} seleccionados")
    }

    fn item_count(&self, count: u64) -> String {
        if count == 1 {
            "1 elemento".to_owned()
        } else {
            format!("{count} elementos")
        }
    }

    fn range_of(&self, start: u32, end: u32, total: u64) -> String {
        format!("{start}-{end} de {total}")
    }

    fn match_status(&self, matched: u64, total: u64) -> String {
        format!("{matched} de {total}")
    }

    fn rows_per_page(&self, page_size: u32) -> String {
        format!("Filas por página: {page_size}")
    }

    fn page_of(&self, page: u32, total: u32) -> String {
        format!("Página {page} de {total}")
    }

    fn save_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("No se ha podido guardar: {error}")
    }

    fn delete_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("No se ha podido eliminar: {error}")
    }

    fn empty_trash_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("No se ha podido vaciar la papelera: {error}")
    }

    fn search_count(&self, total: u32, label: &'static str) -> String {
        format!("{total} {label}")
    }

    fn selected_items_count(&self, count: usize) -> String {
        if count == 1 {
            "1 elemento seleccionado".to_owned()
        } else {
            format!("{count} elementos seleccionados")
        }
    }

    fn delete_selected_count(&self, count: usize) -> String {
        format!("Eliminar selección ({count})")
    }

    fn deleted_items_from_database(&self, count: u64) -> String {
        if count == 1 {
            "Se ha eliminado 1 elemento de la base de datos.".to_owned()
        } else {
            format!("Se han eliminado {count} elementos de la base de datos.")
        }
    }

    fn season_display_name(&self, number: i64) -> String {
        if number == 0 {
            "Especiales".to_owned()
        } else {
            format!("Temporada {number}")
        }
    }

    fn episode_display_subtitle(
        &self,
        show_title: Option<&str>,
        season_number: Option<i64>,
        episode_number: Option<i64>,
    ) -> Option<String> {
        episode_display_subtitle(show_title, season_number, episode_number)
    }

    fn media_item_fallback_label(&self, id: i64) -> String {
        format!("Elemento multimedia n.º {id}")
    }

    fn schedule_item_fallback_label(&self, id: i64) -> String {
        format!("Elemento de programación n.º {id}")
    }

    fn schedule_error_message(&self, error: &str) -> String {
        schedule_error_message(error)
    }

    fn scheduling_content_error_message(&self, error: &str) -> String {
        scheduling_content_error_message(error)
    }

    fn watermark_error_message(&self, error: &str) -> String {
        watermark_error_message(error)
    }

    fn channel_streaming_mode_label(&self, mode: i32) -> String {
        match mode {
            1 => "MPEG-TS (heredado)".to_owned(),
            2 => "HLS directo".to_owned(),
            4 => "Segmentador HLS".to_owned(),
            5 => "MPEG-TS".to_owned(),
            _ => format!("Desconocido ({mode})"),
        }
    }

    fn channel_language_label(&self, code: &str) -> String {
        match code.trim().to_ascii_lowercase().as_str() {
            "" => String::new(),
            "eng" | "en" => "Inglés".to_owned(),
            "nld" | "dut" | "nl" => "Neerlandés".to_owned(),
            "deu" | "ger" | "de" => "Alemán".to_owned(),
            "fra" | "fre" | "fr" => "Francés".to_owned(),
            "spa" | "es" => "Español".to_owned(),
            "ita" | "it" => "Italiano".to_owned(),
            "jpn" | "ja" => "Japonés".to_owned(),
            "kor" | "ko" => "Coreano".to_owned(),
            "zho" | "chi" | "zh" => "Chino".to_owned(),
            value => value.to_owned(),
        }
    }

    fn media_kind_label(&self, kind: i64) -> String {
        match kind {
            1 => text(TextKey::Movies),
            2 => text(TextKey::Shows),
            3 => text(TextKey::MusicVideos),
            4 => text(TextKey::OtherVideos),
            5 => text(TextKey::Songs),
            6 => text(TextKey::Images),
            7 => text(TextKey::RemoteStreams),
            _ => "Desconocido",
        }
        .to_owned()
    }

    fn playout_schedule_kind_label(&self, kind: i32) -> String {
        match kind {
            1 => text(TextKey::ScheduleKindClassic),
            2 => text(TextKey::ScheduleKindBlock),
            3 => text(TextKey::ScheduleKindSequential),
            4 => text(TextKey::ScheduleKindScripted),
            20 => text(TextKey::ScheduleKindExternalJson),
            _ => "",
        }
        .to_owned()
    }

    fn library_kind_label(&self, kind: &str) -> String {
        match kind {
            "Local" => text(TextKey::Local).to_owned(),
            "Plex" | "Jellyfin" | "Emby" => kind.to_owned(),
            _ => kind.to_owned(),
        }
    }

    fn hardware_acceleration_option_label(&self, label: &str, supported: bool) -> String {
        if supported {
            label.to_owned()
        } else {
            format!("{label} (no disponible en este sistema)")
        }
    }

    fn ffmpeg_tonemap_algorithm_label(&self, algorithm: i32) -> String {
        match algorithm {
            0 => "Lineal",
            1 => "Recorte",
            2 => "Gamma",
            3 => "Reinhard",
            4 => "Mobius",
            5 => "Hable",
            _ => return format!("Desconocido ({algorithm})"),
        }
        .to_owned()
    }

    fn ffmpeg_output_dynamic_range_label(&self, range: i32) -> String {
        match range {
            0 => "SDR (BT.709)",
            1 => "HDR10 (BT.2020 PQ)",
            _ => return format!("Desconocido ({range})"),
        }
        .to_owned()
    }

    fn ffmpeg_loudness_mode_label(&self, mode: i32) -> String {
        match mode {
            0 => text(TextKey::Off),
            1 => "loudnorm",
            _ => return format!("Desconocido ({mode})"),
        }
        .to_owned()
    }

    fn ffmpeg_profile_video_format_label(&self, format: i32, hardware_acceleration: i32) -> String {
        let label = ffmpeg_profile_video_format_label(format);
        if hardware_acceleration == 0 {
            label
        } else {
            format!(
                "{} / {}",
                label,
                ffmpeg_profile_hardware_acceleration_label(hardware_acceleration)
            )
        }
    }

    fn ffmpeg_profile_audio_format_label(&self, format: i32) -> String {
        ffmpeg_profile_audio_format_label(format)
    }

    fn ffmpeg_profile_error_message(&self, error: &str) -> String {
        ffmpeg_profile_error_message(error)
    }

    fn settings_error_message(&self, error: &str) -> String {
        settings_error_message(error)
    }

    fn media_action_error_message(&self, error: &str) -> String {
        media_action_error_message(error)
    }

    fn filler_error_message(&self, error: &str) -> String {
        filler_error_message(error)
    }

    fn media_browser_error_message(&self, error: &str) -> String {
        media_browser_error_message(error)
    }

    fn media_source_error_message(&self, error: &str) -> String {
        media_source_error_message(error)
    }

    fn addon_media_list_error_message(&self, error: &str) -> String {
        addon_media_list_error_message(error)
    }

    fn addon_operation_error_message(&self, code: &str, detail: &str) -> String {
        addon_operation_error_message(code, detail)
    }

    fn upload_error_message(&self, error: &str) -> String {
        upload_error_message(error)
    }

    fn emptied_trash_count(&self, count: u64) -> String {
        format!("Papelera vaciada: se han eliminado {count} elementos.")
    }

    fn select_collection_to_add_selected_items(&self, count: usize) -> String {
        format!("Selecciona la colección a la que añadir los {count} elementos seleccionados")
    }

    fn select_playlist_to_add_selected_items(&self, count: usize) -> String {
        format!(
            "Selecciona la lista de reproducción a la que añadir los {count} elementos seleccionados"
        )
    }

    fn added_items_to_collection(&self, count: usize) -> String {
        format!("Se han añadido {count} elementos a la colección")
    }

    fn added_items_to_playlist(&self, count: usize) -> String {
        format!("Se han añadido {count} elementos a la lista de reproducción")
    }

    fn add_to_collection_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("No se ha podido añadir a la colección: {error}")
    }

    fn add_to_playlist_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("No se ha podido añadir a la lista de reproducción: {error}")
    }

    fn no_media_source_configured(&self, provider: &'static str) -> String {
        format!(
            "No hay ninguna fuente de {provider} configurada. Haz clic en Configurar conexión para añadir una."
        )
    }

    fn provider_connection_title(&self, provider: &'static str) -> String {
        format!("Conexión de {provider}")
    }

    fn provider_libraries_title(&self, provider: &'static str) -> String {
        format!("Bibliotecas de {provider}")
    }

    fn provider_path_replacements_title(&self, provider: &'static str) -> String {
        format!("Sustituciones de rutas de {provider}")
    }

    fn provider_path_label(&self, provider: &'static str) -> String {
        format!("Ruta de {provider}")
    }

    fn health_summary(&self, errors: u32, warnings: u32) -> String {
        format!(
            "{} error{}, {} advertencia{}",
            errors,
            if errors == 1 { "" } else { "es" },
            warnings,
            if warnings == 1 { "" } else { "s" },
        )
    }

    fn classic_playout_state_message(&self, playouts: &str) -> String {
        format!(
            "El estado del enumerador de reproducción clásica está incompleto para: {playouts}. El programador debe actualizar estas reproducciones antes de que puedan continuar de forma segura."
        )
    }

    fn unable_to_load_playback_troubleshooting(&self, error: &dyn std::fmt::Display) -> String {
        format!("No se ha podido cargar el contexto de diagnóstico de reproducción: {error}")
    }

    fn playout_reset_prompt(&self, channel: &str, schedule: &str) -> String {
        format!(
            "Introduce la hora a la que debe reiniciarse cada día la reproducción del canal {channel} con la programación {schedule}"
        )
    }
}

pub fn text(key: TextKey) -> &'static str {
    match key {
        TextKey::Addon => "Complemento",
        TextKey::ChangeAddon => "Cambiar complemento",
        TextKey::ChangeAddonHelp => "Elige otro complemento para volver a importar la lista con él.",
        TextKey::ChangeAddonPrompt => "La lista se vuelve a importar mediante el complemento seleccionado. Los metadatos de los elementos, los reemplazos y las correcciones de capítulos se derivan otra vez del nuevo proveedor.",
        TextKey::AddonAutoDetect => "Determinar a partir de la URL",
        TextKey::AddonAutoDetected => "Seleccionado automáticamente a partir de la URL.",
        TextKey::AddonMediaListNoAddonForUrl => "Ningún complemento instalado procesa esta URL. Elige uno o define un procesador predeterminado de transmisiones remotas.",
        TextKey::DefaultRemoteStreamAddon => "Procesador predeterminado de transmisiones remotas",
        TextKey::DefaultRemoteStreamAddonHelp => "Se usa para orígenes HTTP y HTTPS que ningún complemento instalado reclama. El complemento aún debe tener permiso para contactar al host.",
        TextKey::Addons => "Complementos",
        TextKey::AddonOverview => "Resumen",
        TextKey::AddonUpdates => "Actualizaciones",
        TextKey::AddonSources => "Fuentes",
        TextKey::AddonNotFound => "No se encontró el complemento",
        TextKey::BackToAddons => "Volver a complementos",
        TextKey::AvailableVersion => "Versión disponible",
        TextKey::BrowseAddons => "Explorar complementos",
        TextKey::CheckForUpdates => "Buscar actualizaciones",
        TextKey::InstallFromFile => "Instalar desde archivo",
        TextKey::InstallRepositoryFromFile => "Instalar repositorio desde archivo",
        TextKey::License => "Licencia",
        TextKey::UnknownSources => "Orígenes desconocidos",
        TextKey::UnknownSourcesHelp => "Permite repositorios sin firma y paquetes locales. Los complementos se ejecutan con el acceso del usuario del servicio ErsatzRS.",
        TextKey::Verified => "Verificado",
        TextKey::Unverified => "Sin verificar",
        TextKey::Sideloaded => "Instalado localmente",
        TextKey::Enabled => "Activado",
        TextKey::Disabled => "Desactivado",
        TextKey::Source => "Fuente",
        TextKey::ViewSourceCode => "Ver el código fuente",
        TextKey::Version => "Versión",
        TextKey::NoAddonsMatch => "Ningún complemento coincide con estos filtros.",
        TextKey::AddonsUpToDate => "Todo está actualizado.",
        TextKey::AddonRepositories => "Repositorios de complementos",
        TextKey::AvailableAddons => "Complementos disponibles",
        TextKey::InstalledAddons => "Complementos instalados",
        TextKey::AddRepository => "Añadir repositorio",
        TextKey::RepositoryUrl => "URL del repositorio",
        TextKey::PublicSigningKey => "Clave pública de firma",
        TextKey::DeploymentManagedRepositories => {
            "Los repositorios se administran mediante la configuración del despliegue."
        }
        TextKey::Refresh => "Actualizar",
        TextKey::Install => "Instalar",
        TextKey::UpdateAddon => "Actualizar",
        TextKey::Uninstall => "Desinstalar",
        TextKey::Configuration => "Configuración",
        TextKey::SecretReference => "Referencia de secreto",
        TextKey::EnvironmentVariable => "Variable de entorno",
        TextKey::SecretFile => "Archivo de secreto",
        TextKey::Incompatible => "Incompatible",
        TextKey::PackageUnavailable => "Paquete no disponible",
        TextKey::Official => "Oficial",
        TextKey::ThirdParty => "Repositorio de terceros",
        TextKey::Permissions => "Permisos",
        TextKey::Capabilities => "Capacidades",
        TextKey::Dependencies => "Dependencias",
        TextKey::Optional => "Opcional",
        TextKey::MediaReadAccess => "Acceso de lectura a archivos multimedia",
        TextKey::ExternalCommands => "Comandos externos",
        TextKey::ThirdPartyRepositoryWarning => {
            "Añade únicamente repositorios de confianza; los complementos firmados se ejecutan como el usuario del servicio ErsatzRS."
        }
        TextKey::AddCustomResolution => "Añadir resolución personalizada",
        TextKey::AddBlock => "Añadir bloque",
        TextKey::AddBlockGroup => "Añadir grupo de bloques",
        TextKey::AddBlockItem => "Añadir elemento al bloque",
        TextKey::AddBlockToTemplate => "Añadir bloque a la plantilla",
        TextKey::AddCollection => "Añadir colección",
        TextKey::AddContent => "Añadir contenido",
        TextKey::AddFailed => "No se ha podido añadir",
        TextKey::AddFillerPreset => "Añadir preajuste de relleno",
        TextKey::AddMultiCollection => "Añadir multicolección",
        TextKey::AddPlaylist => "Añadir lista de reproducción",
        TextKey::AddPlaylistGroup => "Añadir grupo de listas de reproducción",
        TextKey::AddPlaylistItem => "Añadir elemento a la lista de reproducción",
        TextKey::AddRerunCollection => "Añadir colección de reposiciones",
        TextKey::AddTemplate => "Añadir plantilla",
        TextKey::AddTemplateGroup => "Añadir grupo de plantillas",
        TextKey::AddAddonMediaList => "Añadir lista de add-on",
        TextKey::AddToCollection => "Añadir a la colección",
        TextKey::AddToEllipsis => "Añadir a...",
        TextKey::AddToPlaylist => "Añadir a la lista de reproducción",
        TextKey::AddToSchedule => "Añadir a la programación",
        TextKey::Actions => "Acciones",
        TextKey::All => "Todo",
        TextKey::AreYouSureDelete => "¿Seguro que quieres eliminar",
        TextKey::Actors => "Intérpretes",
        TextKey::Artist => "Artista",
        TextKey::ArtistNotFound => "No se ha encontrado el artista",
        TextKey::AfterDurationEnd => "Después del final de la duración",
        TextKey::AutomaticRefresh => "Actualización automática",
        TextKey::Back => "Volver",
        TextKey::BackToSettings => "Volver a los ajustes",
        TextKey::BackToArtists => "Volver a los artistas",
        TextKey::BackToCollections => "Volver a las colecciones",
        TextKey::BackToMovies => "Volver a las películas",
        TextKey::BackToOtherVideos => "Volver a otros vídeos",
        TextKey::BackToTvShows => "Volver a las series",
        TextKey::BeforeDurationEnd => "Antes del final de la duración",
        TextKey::BackToTemplates => "Volver a las plantillas",
        TextKey::Block => "Bloque",
        TextKey::BlockGroup => "Grupo de bloques",
        TextKey::BlockCannotBeMoved => {
            "El bloque no puede moverse ahí porque se solaparía con otro o cruzaría la medianoche."
        }
        TextKey::BlockCopied => "Bloque copiado.",
        TextKey::BlockCreated => "Bloque creado.",
        TextKey::BlockDeleted => "Bloque eliminado.",
        TextKey::BlockGroupCreated => "Grupo de bloques creado.",
        TextKey::BlockGroupDeleted => "Grupo de bloques eliminado.",
        TextKey::BlockGroupName => "Nombre del grupo de bloques",
        TextKey::BlockGroups => "Grupos de bloques",
        TextKey::BlockItem => "Elemento de bloque",
        TextKey::BlockItems => "Elementos del bloque",
        TextKey::BlockName => "Nombre del bloque",
        TextKey::BlockNotFound => "No se ha encontrado el bloque.",
        TextKey::BlockPreviewFailed => {
            "No se ha podido generar la vista previa del bloque. Comprueba sus elementos e inténtalo de nuevo."
        }
        TextKey::Blocks => "Bloques",
        TextKey::BlockToRemove => "Bloque que se va a quitar",
        TextKey::BlockWillNotFit => {
            "El bloque no cabe en la hora de inicio seleccionada ni después."
        }
        TextKey::Cancel => "Cancelar",
        TextKey::Clear => "Limpiar",
        TextKey::Available => "Disponible",
        TextKey::UnknownAvailability => "Desconocido",
        TextKey::LocalReplacement => "Sustitución local",
        TextKey::NoReplacement => "Sin sustitución",
        TextKey::InvalidReplacement => "Sustitución no válida",
        TextKey::Channels => "Canales",
        TextKey::Close => "Cerrar",
        TextKey::Collections => "Colecciones",
        TextKey::CollectionDeleted => "Colección eliminada.",
        TextKey::CollectionSaved => "Colección guardada.",
        TextKey::SaveCollection => "Guardar colección",
        TextKey::ComputingPreview => "Calculando la vista previa...",
        TextKey::Confirm => "Confirmar",
        TextKey::ConfirmDelete => "Confirmar eliminación",
        TextKey::CommentaryTracks => "Pistas de comentarios",
        TextKey::Content => "Contenido",
        TextKey::ContentVariantAcoustic => "Acústico",
        TextKey::ContentVariantAlternateVersion => "Versión",
        TextKey::ContentVariantBehindTheScenes => "Entre bastidores",
        TextKey::ContentVariantDeletedScene => "Escena eliminada",
        TextKey::ContentVariantDirectorsCut => "Montaje del director",
        TextKey::ContentVariantExtended => "Versión extendida",
        TextKey::ContentVariantExtra => "Extra",
        TextKey::ContentVariantFeaturette => "Reportaje",
        TextKey::ContentVariantInterview => "Entrevista",
        TextKey::ContentVariantLive => "En directo",
        TextKey::ContentVariantOfficialVideo => "Vídeo oficial",
        TextKey::ContentVariantOther => "Otro",
        TextKey::ContentVariantRemaster => "Remasterización",
        TextKey::ContentVariantRemix => "Remezcla",
        TextKey::ContentVariantScene => "Escena",
        TextKey::ContentVariantSearchHelp => "Tokens de variante: content_kind:extra, content_kind:alternate_version, sub_kind:featurette, sub_kind:unplugged, parent_title:«Título de película», audio_title:commentary, subtitle_title:commentary, has_commentary:true",
        TextKey::ContentVariantShort => "Cortometraje",
        TextKey::ContentVariantSpecialFeatures => "Extras / Contenido adicional",
        TextKey::ContentVariantTrailer => "Tráiler",
        TextKey::ContentVariantUnplugged => "Acústico",
        TextKey::ContentVariantVersions => "Versiones",
        TextKey::ContentRatings => "Clasificaciones por edades",
        TextKey::CustomResolutionHeight => "Altura de la resolución personalizada",
        TextKey::CustomResolutionWidth => "Anchura de la resolución personalizada",
        TextKey::CustomResolutions => "Resoluciones personalizadas",
        TextKey::Dark => "Oscuro",
        TextKey::DaysToBuild => "Días que se deben generar",
        TextKey::AddBreakContent => "Añadir contenido de pausa",
        TextKey::AddDeco => "Añadir decoración",
        TextKey::AddDecoGroup => "Añadir grupo de decoraciones",
        TextKey::AddDecoTemplate => "Añadir plantilla de decoración",
        TextKey::AddDecoTemplateGroup => "Añadir grupo de plantillas de decoración",
        TextKey::AtChapterMarkers => "En los marcadores de capítulo",
        TextKey::BackToDecos => "Volver a las decoraciones",
        TextKey::BackToDecoTemplates => "Volver a las plantillas de decoración",
        TextKey::BetweenBlockItems => "Entre elementos del bloque",
        TextKey::BlockFinish => "Final del bloque",
        TextKey::BlockStart => "Inicio del bloque",
        TextKey::BreakContent => "Contenido de pausa",
        TextKey::BreakContentBody => {
            "El contenido de pausa permite reproducir elementos concretos (mediante listas de reproducción) y completar los bloques hasta su duración configurada."
        }
        TextKey::BreakContentItem => "Elemento de contenido de pausa",
        TextKey::BreakContentMode => "Modo de contenido de pausa",
        TextKey::DeadAirFallback => "Contenido alternativo para interrupciones",
        TextKey::DeadAirFallbackBody => "Cuando no haya ningún elemento del playout para la hora actual, se elegirá uno al azar de esta colección, se repetirá y recortará para que encaje exactamente hasta el inicio del siguiente elemento. Sustituye a la imagen «El canal está desconectado» que se mostraría de otro modo.",
        TextKey::DeadAirFallbackCollectionType => {
            "Tipo de colección alternativa para interrupciones"
        }
        TextKey::DeadAirFallbackMode => "Modo alternativo para interrupciones",
        TextKey::Deco => "Decoración",
        TextKey::DecoCannotBeAdded => {
            "La decoración no puede añadirse porque se solaparía con otra o cruzaría la medianoche."
        }
        TextKey::DecoCannotBeMoved => {
            "La decoración no puede moverse ahí porque se solaparía con otra o cruzaría la medianoche."
        }
        TextKey::DecoCreated => "Decoración creada.",
        TextKey::DecoDeleted => "Decoración eliminada.",
        TextKey::DecoGroup => "Grupo de decoraciones",
        TextKey::DecoGroupCreated => "Grupo de decoraciones creado.",
        TextKey::DecoGroupDeleted => "Grupo de decoraciones eliminado.",
        TextKey::DecoGroupName => "Nombre del grupo de decoraciones",
        TextKey::DecoGroups => "Grupos de decoraciones",
        TextKey::DecoName => "Nombre de la decoración",
        TextKey::DecoNotFound => "No se ha encontrado la decoración.",
        TextKey::DecoSaved => "Decoración guardada.",
        TextKey::Decos => "Decoraciones",
        TextKey::DecoTemplate => "Plantilla de decoración",
        TextKey::DecoTemplateCreated => "Plantilla de decoración creada.",
        TextKey::DecoTemplateDeleted => "Plantilla de decoración eliminada.",
        TextKey::DecoTemplateGroup => "Grupo de plantillas de decoración",
        TextKey::DecoTemplateGroupCreated => "Grupo de plantillas de decoración creado.",
        TextKey::DecoTemplateGroupDeleted => "Grupo de plantillas de decoración eliminado.",
        TextKey::DecoTemplateGroupName => "Nombre del grupo de plantillas de decoración",
        TextKey::DecoTemplateGroups => "Grupos de plantillas de decoración",
        TextKey::DecoTemplateName => "Nombre de la plantilla de decoración",
        TextKey::DecoTemplateNotFound => "No se ha encontrado la plantilla de decoración.",
        TextKey::DecoTemplateSaved => "Plantilla de decoración guardada.",
        TextKey::AddDecoToTemplate => "Añadir decoración a la plantilla",
        TextKey::DecoToRemove => "Decoración que se va a quitar",
        TextKey::DefaultFiller => "Contenido de relleno predeterminado",
        TextKey::DefaultFillerBody => {
            "Cuando se hayan programado todos los bloques, se hará una segunda pasada para completar el tiempo sin programar con elementos al azar de esta colección."
        }
        TextKey::DefaultFillerCollectionType => "Tipo de colección de relleno predeterminada",
        TextKey::DefaultFillerMode => "Modo de relleno predeterminado",
        TextKey::DeleteDeco => "Eliminar decoración",
        TextKey::DeleteDecoGroup => "Eliminar grupo de decoraciones",
        TextKey::DeleteDecoTemplate => "Eliminar plantilla de decoración",
        TextKey::DeleteDecoTemplateGroup => "Eliminar grupo de plantillas de decoración",
        TextKey::Disable => "Desactivar",
        TextKey::EditDeco => "Editar decoración",
        TextKey::EditDecoTemplate => "Editar plantilla de decoración",
        TextKey::GraphicsElementsMode => "Modo de elementos gráficos",
        TextKey::Inherit => "Heredar",
        TextKey::Merge => "Combinar",
        TextKey::NoDecosFound => "No se han encontrado decoraciones.",
        TextKey::NoDecoTemplatesFound => "No se han encontrado plantillas de decoración.",
        TextKey::Override => "Reemplazar",
        TextKey::RemoveBreakContent => "Quitar contenido de pausa",
        TextKey::RemoveDecoFromTemplate => "Quitar decoración de la plantilla",
        TextKey::Replace => "Sustituir",
        TextKey::SavedDecos => "Decoraciones guardadas",
        TextKey::SavedDecoTemplates => "Plantillas de decoración guardadas",
        TextKey::SearchForDecos => "Buscar decoraciones",
        TextKey::SearchForDecoTemplates => "Buscar plantillas de decoración",
        TextKey::TrimToFit => "Recortar para ajustar",
        TextKey::UseGraphicsElementsDuringFiller => "Usar elementos gráficos durante el relleno",
        TextKey::UseWatermarkDuringFiller => "Usar la marca de agua durante el relleno",
        TextKey::WatermarkMode => "Modo de marca de agua",
        TextKey::DecoTemplates => "Plantillas de decoración",
        TextKey::DefaultFfmpegProfile => "Perfil de FFmpeg predeterminado",
        TextKey::DefaultFfmpegProfileHelp => "Perfil de FFmpeg que se usará al crear canales",
        TextKey::DefaultMinimumLogLevel => "Nivel mínimo de registro predeterminado",
        TextKey::DefaultMpegTsScript => "Script MPEG-TS predeterminado",
        TextKey::DefaultMpegTsScriptHelp => "Script MPEG-TS que se usará para emitir",
        TextKey::Decorations => "Decoraciones",
        TextKey::Debug => "Depuración",
        TextKey::Delete => "Eliminar",
        TextKey::DeleteAll => "Eliminar todo",
        TextKey::DeleteBlock => "Eliminar bloque",
        TextKey::DeleteBlockGroup => "Eliminar grupo de bloques",
        TextKey::DeleteCustomResolution => "Eliminar resolución personalizada",
        TextKey::DeleteCollection => "Eliminar colección",
        TextKey::DeleteFillerPreset => "Eliminar preajuste de relleno",
        TextKey::DeleteMultiCollection => "Eliminar multicolección",
        TextKey::DeletePlaylist => "Eliminar lista de reproducción",
        TextKey::DeletePlaylistGroup => "Eliminar grupo de listas de reproducción",
        TextKey::DeleteRerunCollection => "Eliminar colección de reposiciones",
        TextKey::DeleteTemplate => "Eliminar plantilla",
        TextKey::DeleteTemplateGroup => "Eliminar grupo de plantillas",
        TextKey::DeleteAddonMediaList => "Eliminar lista de add-on",
        TextKey::DisabledCollectionsAlreadyPresent => {
            "Las colecciones desactivadas ya están presentes en esta multicolección"
        }
        TextKey::EmptyTrash => "Vaciar papelera",
        TextKey::ExtractEmbeddedSubtitles => "Extraer subtítulos de texto incrustados",
        TextKey::ErasedPlayoutItemsAndHistory => "Se han borrado los elementos y el historial del playout.",
        TextKey::EraseFailed => "No se ha podido borrar",
        TextKey::EraseItemsAndHistory => "Borrar elementos e historial",
        TextKey::ErasePlayoutItemsAndHistoryConfirm => "Esto eliminará todos los elementos y el historial de este playout secuencial. ¿Quieres continuar?",
        TextKey::Episode => "Episodio",
        TextKey::FanArt => "fan art",
        TextKey::Ffmpeg => "FFmpeg",
        TextKey::FileNotFound => "No se ha encontrado el archivo",
        TextKey::Finish => "Finalizar",
        TextKey::Fatal => "Fatal",
        TextKey::FfmpegPath => "Ruta de FFmpeg",
        TextKey::FfmpegPathHelp => "Ruta completa del ejecutable ffmpeg",
        TextKey::FfmpegProfiles => "Perfiles de FFmpeg",
        TextKey::FfprobePath => "Ruta de FFprobe",
        TextKey::FfprobePathHelp => "Ruta completa del ejecutable ffprobe",
        TextKey::FillerPreset => "Preajuste de relleno",
        TextKey::FillerPresetActionFailed => "La acción del preajuste de relleno ha fallado.",
        TextKey::FillerPresetCouldNotBeSaved => {
            "No se ha podido guardar el preajuste de relleno. Comprueba los valores del formulario e inténtalo de nuevo."
        }
        TextKey::FillerPresetDeleted => "Preajuste de relleno eliminado.",
        TextKey::FillerPresetExpressionHelp => {
            "En el relleno intermedio, solo se añadirá contenido cuando esta expresión sea verdadera para un punto intermedio concreto."
        }
        TextKey::FillerPresetSaved => "Preajuste de relleno guardado.",
        TextKey::FillerPresets => "Preajustes de relleno",
        TextKey::Flags => "Indicadores",
        TextKey::GlobalFallbackFiller => "Relleno alternativo global",
        TextKey::GlobalWatermark => "Marca de agua global",
        TextKey::GoHome => "Ir al inicio",
        TextKey::HdHomeRun => "HDHomeRun",
        TextKey::HealthChecks => "Comprobaciones de estado",
        TextKey::Height => "Altura",
        TextKey::HlsDirectOutputFormat => "Formato de salida de HLS Direct",
        TextKey::HlsSegmenterIdleTimeout => "Tiempo de espera por inactividad del segmentador HLS",
        TextKey::HlsSegmenterIdleTimeoutHelp => {
            "Número de segundos durante los que se seguirá transcodificando un canal sin recibir solicitudes de ningún cliente"
        }
        TextKey::HlsSegmenterInitialSegmentCount => {
            "Número inicial de segmentos del segmentador HLS"
        }
        TextKey::HlsSegmenterInitialSegmentCountHelp => {
            "Retrasa el inicio de la emisión hasta que se haya transcodificado el número indicado de segmentos (de 4 segundos); un número mayor ralentiza el inicio, pero puede reducir las pausas de almacenamiento en búfer"
        }
        TextKey::HomeRunning => "El servidor IPTV está en funcionamiento.",
        TextKey::HttpMinimumLogLevel => "Nivel mínimo del registro de solicitudes",
        TextKey::Id => "Id",
        TextKey::Images => "Imágenes",
        TextKey::Item => "Elemento",
        TextKey::ItemActions => "Acciones del elemento",
        TextKey::ItemType => "Tipo de elemento",
        TextKey::Items => "Elementos",
        TextKey::Language => "Idioma",
        TextKey::Languages => "Idiomas",
        TextKey::MusicLabels => "Sellos discográficos",
        TextKey::LeaveEmptyToAutoDetect => "Déjalo vacío para detectarlo automáticamente",
        TextKey::Libraries => "Bibliotecas",
        TextKey::LibraryRefreshInterval => "Intervalo de actualización de la biblioteca",
        TextKey::Light => "Claro",
        TextKey::Lists => "Listas",
        TextKey::Loading => "Cargando...",
        TextKey::LoadingMembers => "Cargando miembros...",
        TextKey::Local => "Local",
        TextKey::Logging => "Registro",
        TextKey::M3u => "M3U",
        TextKey::Media => "Contenido multimedia",
        TextKey::MediaInfo => "Información multimedia",
        TextKey::MediaSources => "Fuentes multimedia",
        TextKey::Movie => "Película",
        TextKey::MovieNotFound => "No se ha encontrado la película",
        TextKey::Movies => "Películas",
        TextKey::MusicVideo => "Vídeo musical",
        TextKey::MultiCollections => "Multicolecciones",
        TextKey::ManualCollections => "Colecciones manuales",
        TextKey::MatchFailed => "No se ha podido encontrar una coincidencia",
        TextKey::MatchStatus => "Estado de coincidencia",
        TextKey::MatchAddonMediaListItems => {
            "Buscar coincidencias para los elementos de la lista de add-on"
        }
        TextKey::Members => "Miembros",
        TextKey::MultiCollection => "Multicolección",
        TextKey::MultiCollectionDeleted => "Multicolección eliminada.",
        TextKey::MultiCollectionItems => "Elementos de la multicolección",
        TextKey::MultiCollectionNameRequired => "El nombre de la multicolección es obligatorio.",
        TextKey::MultiCollectionNameTooLong => {
            "El nombre de la multicolección no puede superar los 50 caracteres."
        }
        TextKey::MultiCollectionSaved => "Multicolección guardada.",
        TextKey::Music => "Música",
        TextKey::Name => "Nombre",
        TextKey::NameRequired => "El nombre es obligatorio",
        TextKey::Networks => "Cadenas",
        TextKey::NewCollection => "(Nueva colección)",
        TextKey::NewCollectionTitle => "Nueva colección",
        TextKey::NewFillerPreset => "Nuevo preajuste de relleno",
        TextKey::NewMultiCollection => "Nueva multicolección",
        TextKey::NewTemplateGroup => "Nuevo grupo de plantillas",
        TextKey::NewTemplateName => "Nuevo nombre de plantilla",
        TextKey::Next24Hours => "Próximas 24 h",
        TextKey::NextPage => "Página siguiente",
        TextKey::NoFillerPresets => "Aún no hay preajustes de relleno.",
        TextKey::NoBlockItems => "No hay elementos en el bloque.",
        TextKey::NoItems => "No hay elementos.",
        TextKey::NoItemsToDisplay => "No hay elementos que mostrar.",
        TextKey::NoAddonMediaListsYet => "Aún no hay listas de add-on.",
        TextKey::No => "No",
        TextKey::NoCollections => "Aún no hay colecciones.",
        TextKey::NoMembers => "Aún no hay miembros.",
        TextKey::NoMultiCollections => "Aún no hay multicolecciones.",
        TextKey::NoTemplatesFound => "No se han encontrado plantillas.",
        TextKey::NotFound => "No se ha encontrado la página.",
        TextKey::OtherVideo => "Otro vídeo",
        TextKey::OtherVideoNotFound => "No se ha encontrado el vídeo",
        TextKey::OtherVideos => "Otros vídeos",
        TextKey::OpenBrowser => "Abrir navegador",
        TextKey::Playout => "Playout",
        TextKey::PlayoutItemsAndHistory => "Elementos e historial del playout",
        TextKey::PlayoutNotFound => "No se ha encontrado el playout.",
        TextKey::Playouts => "Playouts",
        TextKey::AddPlayout => "Añadir playout",
        TextKey::ClassicScheduleMenu => "Programación clásica",
        TextKey::BlockScheduleMenu => "Programación por bloques",
        TextKey::SequentialScheduleMenu => "Programación secuencial",
        TextKey::ScriptedScheduleMenu => "Programación guionizada",
        TextKey::JsonDizqueTvScheduleMenu => "Programación JSON (dizqueTV)",
        TextKey::ResetAllPlayouts => "Reiniciar todos los playouts",
        TextKey::PlayoutBuildQueued => "Reconstrucción del playout en cola",
        TextKey::PlayoutBuildsQueued => "reconstrucciones de playouts en cola",
        TextKey::SearchForPlayouts => "Buscar playouts",
        TextKey::PlayoutDetail => "Detalle del playout",
        TextKey::ShowFiller => "Mostrar relleno",
        TextKey::EditPlayout => "Editar playout",
        TextKey::RefreshPlayout => "Actualizar playout",
        TextKey::ResetPlayout => "Reiniciar playout",
        TextKey::ScheduleReset => "Reinicio de programación",
        TextKey::SchedulePlayoutReset => "Programar el reinicio del playout",
        TextKey::EditExternalJsonFile => "Editar archivo JSON externo",
        TextKey::DeletePlayout => "Eliminar playout",
        TextKey::Troubleshoot => "Solucionar problemas",
        TextKey::DailyResetTime => "Hora de reinicio diario",
        TextKey::DoNotAutomaticallyReset => "No reiniciar automáticamente",
        TextKey::SaveChanges => "Guardar cambios",
        TextKey::ExternalJsonFileLabel => "Archivo JSON externo",
        TextKey::EditExternalJsonFilePrompt => "Editar el archivo JSON externo del playout",
        TextKey::UnscheduledPlayoutItem => "NO PROGRAMADO",
        TextKey::DisabledChannelsAlreadyHavePlayout => "Los canales deshabilitados ya tienen un playout",
        TextKey::ProgramSchedule => "Programación de programas",
        TextKey::SequentialScheduleFileHelp => {
            "La ruta completa al archivo de programación secuencial (YAML)"
        }
        TextKey::BlockTemplatesAddedLater => "Las plantillas de bloques se añaden posteriormente",
        TextKey::UnsupportedPlayoutKind => "Tipo de playout no soportado.",
        TextKey::ClassicPlayout => "Playout clásico",
        TextKey::ClassicPlayoutState => "Estado del playout clásico",
        TextKey::BlockPlayout => "Playout por bloques",
        TextKey::ScriptedPlayout => "Programación guionizada",
        TextKey::AlternateSchedules => "Programaciones alternativas",
        TextKey::EditAlternateSchedules => "Editar programaciones alternativas",
        TextKey::PlayoutTemplates => "Plantillas de playout",
        TextKey::EditTemplates => "Editar plantillas",
        TextKey::DefaultDeco => "Deco por defecto",
        TextKey::EnableDefaultDeco => "Activar deco por defecto",
        TextKey::SaveDefaultDeco => "Guardar deco por defecto",
        TextKey::SaveScriptedSchedule => "Guardar programación guionizada",
        TextKey::EraseItems => "Borrar elementos",
        TextKey::ErasedPlayoutItems => "Elementos del playout borrados.",
        TextKey::SavedDefaultDeco => "Deco por defecto guardado.",
        TextKey::SavedScriptedSchedule => "Programación guionizada guardada.",
        TextKey::Maintenance => "Mantenimiento",
        TextKey::SaveAlternateSchedules => "Guardar programaciones alternativas",
        TextKey::AddAlternateSchedule => "Añadir programación alternativa",
        TextKey::SaveTemplates => "Guardar plantillas",
        TextKey::PreviewCalendar => "Vista previa del calendario",
        TextKey::EditContent => "Editar contenido",
        TextKey::PlayoutNestedCatchAllHelp => {
            "En orden de prioridad de arriba a abajo. La entrada inferior coincidirá siempre con todos los días y todos los meses como una red de seguridad."
        }
        TextKey::PlayoutNestedPriorityHelp => "En orden de prioridad de arriba a abajo.",
        TextKey::DaysOfWeek => "Días de la semana",
        TextKey::DaysOfMonth => "Días del mes",
        TextKey::Months => "Meses",
        TextKey::LimitToDateRange => "Limitar al rango de fechas",
        TextKey::StartDay => "Día de inicio",
        TextKey::EndDay => "Día de fin",
        TextKey::Enable => "Activar",
        TextKey::Up => "Arriba",
        TextKey::Down => "Abajo",
        TextKey::Weekdays => "Días laborables",
        TextKey::Weekends => "Fines de semana",
        TextKey::Sunday => "Domingo",
        TextKey::Monday => "Lunes",
        TextKey::Tuesday => "Martes",
        TextKey::Wednesday => "Miércoles",
        TextKey::Thursday => "Jueves",
        TextKey::Friday => "Viernes",
        TextKey::Saturday => "Sábado",
        TextKey::January => "Enero",
        TextKey::February => "Febrero",
        TextKey::March => "Marzo",
        TextKey::April => "Abril",
        TextKey::May => "Mayo",
        TextKey::June => "Junio",
        TextKey::July => "Julio",
        TextKey::August => "Agosto",
        TextKey::September => "Septiembre",
        TextKey::October => "Octubre",
        TextKey::November => "Noviembre",
        TextKey::December => "Diciembre",
        TextKey::JanuaryShort => "Ene",
        TextKey::FebruaryShort => "feb",
        TextKey::MarchShort => "mar",
        TextKey::AprilShort => "Abr",
        TextKey::MayShort => "may",
        TextKey::JuneShort => "jun",
        TextKey::JulyShort => "jul",
        TextKey::AugustShort => "Ago",
        TextKey::SeptemberShort => "sept",
        TextKey::OctoberShort => "oct",
        TextKey::NovemberShort => "nov",
        TextKey::DecemberShort => "Dic",
        TextKey::PlayAll => "Reproducir todo",
        TextKey::Playlist => "Lista de reproducción",
        TextKey::PlaylistAdded => "Lista de reproducción añadida.",
        TextKey::PlaylistDeleted => "Lista de reproducción eliminada.",
        TextKey::PlaylistGroup => "Grupo de listas de reproducción",
        TextKey::PlaylistGroupAdded => "Grupo de listas de reproducción añadido.",
        TextKey::PlaylistGroupDeleted => "Grupo de listas de reproducción eliminado.",
        TextKey::PlaylistGroupName => "Nombre del grupo de listas de reproducción",
        TextKey::PlaylistGroups => "Grupos de listas de reproducción",
        TextKey::PlaylistItem => "Elemento de lista de reproducción",
        TextKey::PlaylistItems => "Elementos de lista de reproducción",
        TextKey::PlaylistName => "Nombre de la lista de reproducción",
        TextKey::PlaylistNotFound => "Lista de reproducción no encontrada.",
        TextKey::PlaylistPlayoutPreview => "Vista previa del playout de la lista de reproducción",
        TextKey::PlaylistPreviewEmpty => "La vista previa del playout de la lista de reproducción no tiene elementos",
        TextKey::Playlists => "Listas de reproducción",
        TextKey::ProbeForInterlacedFrames => "Búsqueda de tramas entrelazadas",
        TextKey::PreferredAudioLanguage => "Idioma de audio preferido",
        TextKey::DefaultSubtitleLanguage => "Idioma de subtítulos por defecto",
        TextKey::FallbackSubtitleLanguage => "Idioma de subtítulos de respaldo",
        TextKey::OriginalLanguageOption => "Original (und)",
        TextKey::UndefinedLanguageOption => "No definido (und)",
        TextKey::Previous24Hours => "Anterior 24h",
        TextKey::PreviousPage => "Página anterior",
        TextKey::RerunCollections => "Colecciones de retransmisiones",
        TextKey::RerunCollection => "Colección de retransmisiones",
        TextKey::RerunCollectionActionFailed => {
            "La acción de la colección de retransmisiones ha fallado."
        }
        TextKey::RerunCollectionDeleted => "Colección de retransmisiones eliminada.",
        TextKey::RerunCollectionNameRequired => {
            "¡El nombre de la colección de retransmisiones es obligatorio!"
        }
        TextKey::RerunCollectionSaved => "Colección de retransmisiones guardada.",
        TextKey::RerunPlaybackOrder => "Orden de reproducción de retransmisiones",
        TextKey::RemoteStreams => "Flujos remotos",
        TextKey::RemoteStream => "Flujo remoto",
        TextKey::RemoteStreamPlaylists => "Listas de reproducción de flujos remotos",
        TextKey::PlaylistUrl => "URL de la lista de reproducción",
        TextKey::Extractor => "Extractor",
        TextKey::DestinationFolder => "Carpeta de destino",
        TextKey::SyncInterval => "Intervalo de sincronización",
        TextKey::MaxItems => "Máximo de elementos",
        TextKey::RemovedEntries => "Entradas eliminadas",
        TextKey::CreateOrderedPlaylist => "Crear una lista de reproducción ordenada",
        TextKey::SyncNow => "Sincronizar ahora",
        TextKey::LastSync => "Última sincronización",
        TextKey::NeverSynced => "Nunca sincronizado",
        TextKey::ManagedManifestReadOnly => {
            "Este manifiesto creado manualmente es de solo lectura aquí."
        }
        TextKey::Truncated => "Truncado",
        TextKey::MissingDuration => "Falta duración",
        TextKey::Orphaned => "Huérfano",
        TextKey::Materialized => "Materializado",
        TextKey::State => "Estado",
        TextKey::Upstream => "Flujo ascendente",
        TextKey::NewestFirst => "Más reciente primero",
        TextKey::Keep => "Mantener",
        TextKey::ResetChannels => "Reiniciar canales",
        TextKey::RowsPerPage => "Filas por página",
        TextKey::Save => "Guardar",
        TextKey::SaveMultiCollection => "Guardar colección multi",
        TextKey::SavePlaylist => "Guardar lista de reproducción",
        TextKey::SaveTroubleshootingReportsToDisk => {
            "Guardar informes de solución de problemas en disco"
        }
        TextKey::SaveAsSmartCollection => "Guardar como colección inteligente",
        TextKey::SearchQueryRequired => "La consulta de búsqueda es obligatoria",
        TextKey::SaveSettings => "Guardar configuración",
        TextKey::Scanner => "Escáner",
        TextKey::ScanningMinimumLogLevel => "Nivel mínimo de registro de escaneo",
        TextKey::Scheduling => "Programación",
        TextKey::SchedulingMinimumLogLevel => "Nivel mínimo de registro de programación",
        TextKey::StartWithWindows => "Iniciar ErsatzRS cuando me conecto a Windows",
        TextKey::StartWithWindowsHelp => {
            "Iniciar automáticamente el lanzador de la bandeja para el usuario actual de Windows."
        }
        TextKey::Schedules => "Programaciones",
        TextKey::AddSchedule => "Añadir programación",
        TextKey::AddScheduleItem => "Añadir elemento de programación",
        TextKey::AddSmartCollection => "Añadir colección inteligente",
        TextKey::Album => "Álbum",
        TextKey::Chronological => "Cronológico",
        TextKey::CollectionSize => "Tamaño de colección",
        TextKey::CollectionType => "Tipo de colección",
        TextKey::Count => "Recuento",
        TextKey::CopySchedule => "Copiar programación",
        TextKey::CopyScheduleItem => "Copiar elemento de programación",
        TextKey::CustomTitle => "Título personalizado",
        TextKey::DeleteSchedule => "Eliminar programación",
        TextKey::DeleteScheduleItem => "Eliminar elemento de programación",
        TextKey::DeleteSmartCollection => "Eliminar colección inteligente",
        TextKey::DetailRowsPerPage => "Filas de detalle por página",
        TextKey::Director => "Dirección",
        TextKey::Directors => "Dirección",
        TextKey::DiscardToFillAttempts => "Intentos de descarte para completar",
        TextKey::Duration => "Duración",
        TextKey::Dynamic => "Dinámico",
        TextKey::EditFillerPreset => "Editar preajuste de relleno",
        TextKey::EditProperties => "Editar propiedades",
        TextKey::EditScheduleItems => "Editar elementos de programación",
        TextKey::EditSmartCollection => "Editar colección inteligente",
        TextKey::EnterNameForNewSchedule => "Introduce un nombre para la nueva programación",
        TextKey::Fixed => "Fijo",
        TextKey::FixedStartTimeBehavior => "Comportamiento de la hora de inicio fija",
        TextKey::Flood => "Rellenar",
        TextKey::Flexible => "Flexible",
        TextKey::FlexibleStartSoonAsPossible => "Flexible: iniciar lo antes posible",
        TextKey::FlexibleStartSoonAsPossibleAfterStartTime => {
            "Flexible: iniciar lo antes posible después de la hora de inicio"
        }
        TextKey::FillWithGroupMode => "Modo Completar con grupo (serie o artista)",
        TextKey::GraphicsElements => "Elementos gráficos",
        TextKey::GraphicsIds => "Id. de elementos gráficos",
        TextKey::Genres => "Géneros",
        TextKey::GroupName => "Nombre del grupo",
        TextKey::GroupMultiPartEpisodesHelp => "Agrupar episodios de varias partes al mezclar",
        TextKey::GuideMode => "Modo de guía",
        TextKey::KeepMultiPartEpisodesTogether => "Mantener juntos los episodios de varias partes",
        TextKey::Marathon => "Maratón",
        TextKey::MarathonBatchSize => "Tamaño del lote de maratón",
        TextKey::MarathonBatchSizeHelp => {
            "Número de elementos que se reproducirán de cada grupo antes de avanzar; si se deja vacío o se indica cero, se reproducirán todos"
        }
        TextKey::MarathonGroupBy => "Agrupar maratón por",
        TextKey::MarathonShuffleGroups => "Mezclar grupos del maratón",
        TextKey::MarathonShuffleItems => "Mezclar elementos del maratón",
        TextKey::MidRollFiller => "Relleno intermedio",
        TextKey::MidRoll => "Intermedio",
        TextKey::MoveDown => "Bajar",
        TextKey::MoveUp => "Subir",
        TextKey::MultiEpisodeGroupSize => "Tamaño del grupo de varios episodios",
        TextKey::MultiEpisodeShuffle => "Mezcla de varios episodios",
        TextKey::Multiple => "Múltiple",
        TextKey::MultipleCount => "Recuento múltiple",
        TextKey::MultipleCountHelp => {
            "Puede ser una expresión con 'count' (tamaño de la colección) y 'random' (de 0 a count-1)"
        }
        TextKey::MultipleMode => "Modo múltiple",
        TextKey::NewScheduleName => "Nuevo nombre de programación",
        TextKey::NewSmartCollection => "Nueva colección inteligente",
        TextKey::NoScheduleItems => "No hay elementos de programación.",
        TextKey::NoSchedules => "No hay programaciones.",
        TextKey::Normal => "Normal",
        TextKey::Offline => "Sin conexión",
        TextKey::One => "Uno",
        TextKey::OrderedGroups => "Grupos ordenados",
        TextKey::Playback => "Reproducción",
        TextKey::PlaybackOrder => "Orden de reproducción",
        TextKey::PlaybackOrderUpdateFailed => "No se ha podido actualizar el orden de reproducción",
        TextKey::PlaybackOrderUpdated => "Orden de reproducción actualizado.",
        TextKey::PlaylistItemSize => "Tamaño del elemento de lista de reproducción",
        TextKey::PlayoutDurationHours => "Horas de duración del playout",
        TextKey::PlayoutDurationMinutes => "Minutos de duración del playout",
        TextKey::PostRollFiller => "Relleno posterior",
        TextKey::PostRoll => "Posterior",
        TextKey::PreRollFiller => "Relleno previo",
        TextKey::PreRoll => "Previo",
        TextKey::Random => "Aleatorio",
        TextKey::RandomCount => "Recuento aleatorio",
        TextKey::RandomRotation => "Rotación aleatoria",
        TextKey::RemainingDurationHelp => {
            "duración del contenido después de este punto intermedio, en segundos"
        }
        TextKey::RandomStartPoint => "Punto de inicio aleatorio",
        TextKey::RemoveFailed => "No se ha podido quitar",
        TextKey::RemovedFromCollection => "Quitado de la colección.",
        TextKey::RemoveFromCollection => "Quitar de la colección",
        TextKey::Rename => "Renombrar",
        TextKey::RenameFailed => "No se ha podido renombrar",
        TextKey::ReorderFailed => "No se ha podido reordenar",
        TextKey::RerunFirstRun => "Reposición del primer pase",
        TextKey::RerunRerun => "Reposición repetida",
        TextKey::ReadOnlyTwentyFourHourPlayoutView => "Vista de solo lectura de 24 horas de los elementos del playout programados.",
        TextKey::SaveSchedule => "Guardar programación",
        TextKey::SaveScheduleItems => "Guardar elementos de programación",
        TextKey::SaveSmartCollection => "Guardar colección inteligente",
        TextKey::Schedule => "Programación",
        TextKey::ScheduleActionFailed => "La acción de programación ha fallado.",
        TextKey::ScheduleAsGroup => "Programar como grupo",
        TextKey::ScheduleKind => "Tipo de programación",
        TextKey::ScheduleKindBlock => "Bloque",
        TextKey::ScheduleKindClassic => "Clásica",
        TextKey::ScheduleKindExternalJson => "JSON (dizqueTV)",
        TextKey::ScheduleKindScripted => "Mediante script",
        TextKey::ScheduleKindSequential => "Secuencial",
        TextKey::SchedulingContext => "Contexto de programación",
        TextKey::ScheduleItem => "Elemento de programación",
        TextKey::ScheduleItemActionFailed => "La acción del elemento de programación ha fallado.",
        TextKey::ScheduleItemFiller => "Relleno del elemento de programación",
        TextKey::ScheduleItemFillerHelp => {
            "El relleno previo, intermedio y posterior se aplica a todos los elementos multimedia seleccionados por este elemento de programación. Para reproducir contenido una vez después de un bloque Múltiple, añádelo como siguiente elemento de programación."
        }
        TextKey::ScheduleItemOverrides => "Anulaciones del elemento de programación",
        TextKey::ScheduleItems => "Elementos de programación",
        TextKey::ScheduleNotFound => "No se ha encontrado la programación.",
        TextKey::ScheduleSaveFailed => "No se ha podido guardar la programación.",
        TextKey::SmartShuffle => "Mezcla inteligente",
        TextKey::SmartShufflePreset => "Preajuste de separación",
        TextKey::Balanced => "Equilibrado",
        TextKey::SmartShuffleScope => "Compartir separación entre",
        TextKey::SmartShuffleScopePlayout => "Playout",
        TextKey::SmartShuffleScopeScheduleItem => "Elemento de programación",
        TextKey::SmartShuffleValidation => {
            "Elige un umbral de separación y al menos una regla de similitud."
        }
        TextKey::SmartShuffleScopeContentSource => "Origen del contenido",
        TextKey::SmartShuffleAfterItems => "Vídeos entre elementos similares",
        TextKey::SmartShuffleAfterDurationMinutes => "Minutos entre elementos similares",
        TextKey::SmartShuffleIncludeFiller => "Incluir relleno",
        TextKey::SmartShuffleInheritedBlock => "Este elemento no tiene ninguna anulación de Mezcla inteligente y conserva cualquier política de playout heredada.",
        TextKey::SmartShuffleInactive => {
            "La Mezcla inteligente está inactiva porque este orden de reproducción no es aleatorio."
        }
        TextKey::SmartShuffleOverrideBlock => "Sustituir para este elemento de bloque",
        TextKey::SimilarityRules => "Reglas de similitud",
        TextKey::AlbumArtist => "Artista del álbum",
        TextKey::Apply => "Aplicar",
        TextKey::ContentParent => "Elemento principal o variante",
        TextKey::SmartShuffleFilterRules => "Reglas de filtrado",
        TextKey::SearchForSchedules => "Buscar programaciones",
        TextKey::SearchForManualCollections => "Buscar colecciones manuales",
        TextKey::SearchForRerunCollections => "Buscar colecciones de reposiciones",
        TextKey::SearchForSmartCollections => "Buscar colecciones inteligentes",
        TextKey::SearchForTemplates => "Buscar plantillas",
        TextKey::SearchForBlocks => "Buscar bloques",
        TextKey::SearchForMultiCollections => "Buscar multicolecciones",
        TextKey::SearchQuery => "Consulta de búsqueda",
        TextKey::BlockSearchQueryPlaceholder => "p. ej. genre:action",
        TextKey::SearchTitle => "Título de búsqueda",
        TextKey::SearchTitleDisplay => "Título de búsqueda (visualización)",
        TextKey::SearchSmartCollection => "Buscar colección inteligente",
        TextKey::Season => "Temporada",
        TextKey::SeasonEpisode => "Temporada, episodio",
        TextKey::SeasonNotFound => "No se ha encontrado la temporada",
        TextKey::Select => "Seleccionar",
        TextKey::SelectCollection => "Seleccionar colección",
        TextKey::SelectGroupForNewTemplate => "Selecciona un grupo para la nueva plantilla",
        TextKey::SelectScheduleItemPrompt => "Selecciona un elemento de programación para editarlo o añade uno nuevo.",
        TextKey::SelectSchedule => "Selecciona una programación",
        TextKey::SelectSmartCollection => "Seleccionar colección inteligente",
        TextKey::Shuffle => "Mezclar",
        TextKey::ShuffleInOrder => "Mezclar en orden",
        TextKey::ShuffleScheduleItems => "Mezclar elementos de programación",
        TextKey::ShuffleScheduleItemsHelp => "Nota: esto desactiva las horas de inicio fijas y el modo de relleno",
        TextKey::ShuffledGroups => "Grupos mezclados",
        TextKey::Show => "Serie",
        TextKey::ShowNotFound => "No se ha encontrado la serie",
        TextKey::Shows => "Series",
        TextKey::Start => "Iniciar",
        TextKey::StartTime => "Hora de inicio",
        TextKey::StartTimeOnOrAfter => "Hora de inicio en o después de",
        TextKey::StartType => "Tipo de inicio",
        TextKey::Strict => "Estricto",
        TextKey::StrictAlwaysWaitExactStartTime => "Estricto: esperar siempre a la hora de inicio exacta",
        TextKey::StrictWaitExactStartTime => "Estricto: esperar a la hora de inicio exacta",
        TextKey::TailFiller => "Relleno de cola",
        TextKey::TailMode => "Modo de cola",
        TextKey::Tail => "Cola",
        TextKey::TelevisionSeason => "Temporada de televisión",
        TextKey::TelevisionShow => "Serie de televisión",
        TextKey::TreatCollectionsAsShows => "Tratar las colecciones como series",
        TextKey::TreatCollectionsAsShowsHelp => "Resulta útil para episodios cruzados de varias partes",
        TextKey::Type => "Tipo",
        TextKey::TypeToSearch => "Escribe para buscar...",
        TextKey::Search => "Buscar",
        TextKey::SearchMedia => "Buscar contenido multimedia",
        TextKey::SearchMoviesOrShows => "Buscar películas o series...",
        TextKey::SearchAddonMediaList => "Buscar lista de add-on",
        TextKey::SearchingMinimumLogLevel => "Nivel mínimo de registro de búsquedas",
        TextKey::Settings => "Ajustes",
        TextKey::SettingsSaveFailed => "No se han podido guardar los ajustes",
        TextKey::ShowMediaInfo => "Mostrar información multimedia",
        TextKey::ScriptedScheduleTimeout => "Tiempo de espera de programación mediante script",
        TextKey::SkipMissingItems => "Omitir elementos ausentes",
        TextKey::SkipMissingItemsHelp => "Controla si los elementos cuyo archivo no se encuentra o que no están disponibles deben incluirse en los playouts",
        TextKey::SmartCollections => "Colecciones inteligentes",
        TextKey::SmartCollection => "Colección inteligente",
        TextKey::SmartCollectionDeleted => "Colección inteligente eliminada.",
        TextKey::SmartCollectionQueryPlaceholder => "p. ej. genre:action OR genre:comedy",
        TextKey::SmartCollectionSaved => "Colección inteligente guardada.",
        TextKey::SaveAddonMediaList => "Guardar lista de add-on",
        TextKey::Songs => "Canciones",
        TextKey::SplitTimeEvenly => "Dividir el tiempo de forma uniforme",
        TextKey::StreamingMinimumLogLevel => "Nivel mínimo de registro de emisiones",
        TextKey::Studios => "Estudios",
        TextKey::Support => "Compatibilidad",
        TextKey::Template => "Plantilla",
        TextKey::TemplateCopied => "Plantilla copiada.",
        TextKey::TemplateCreated => "Plantilla creada.",
        TextKey::TemplateDeleted => "Plantilla eliminada.",
        TextKey::TemplateGroup => "Grupo de plantillas",
        TextKey::TemplateGroupCreated => "Grupo de plantillas creado.",
        TextKey::TemplateGroupDeleted => "Grupo de plantillas eliminado.",
        TextKey::TemplateGroupName => "Nombre del grupo de plantillas",
        TextKey::TemplateGroups => "Grupos de plantillas",
        TextKey::TemplateName => "Nombre de la plantilla",
        TextKey::TemplateNotFound => "No se ha encontrado la plantilla.",
        TextKey::Templates => "Plantillas",
        TextKey::Theme => "Tema",
        TextKey::ThisActionCannotBeUndone => "Esta acción no se puede deshacer.",
        TextKey::ThisCollectionHasNoItemsYet => "Esta colección aún no contiene elementos.",
        TextKey::Thumbnail => "miniatura",
        TextKey::Title => "Título",
        TextKey::ToggleNavigation => "Mostrar u ocultar la navegación",
        TextKey::TotalDurationHelp => "duración total del contenido, en segundos",
        TextKey::TotalPointsHelp => "número total de posibles puntos intermedios",
        TextKey::TotalProgressHelp => "posición normalizada de 0 a 1",
        TextKey::AddonMediaList => "Lista de add-on",
        TextKey::AddonMediaListActionFailed => "La acción de la lista de add-on ha fallado.",
        TextKey::AddonMediaListAdded => "Lista de add-on añadida.",
        TextKey::AddonMediaListDeleted => "Lista de add-on eliminada.",
        TextKey::AddonMediaListMatched => "Se han encontrado coincidencias para la lista de add-on.",
        TextKey::AddonMediaListNotFound => "No se ha encontrado la lista de add-on.",
        TextKey::AddonMediaListSaveFailed => "No se ha podido guardar la lista de add-on.",
        TextKey::AddonMediaListSaved => "Lista de add-on guardada.",
        TextKey::AddonMediaLists => "Listas de add-on",
        TextKey::AddonMediaListUrl => "URL de la lista de add-on",
        TextKey::AddonMediaListFolder => "Carpeta de almacenamiento",
        TextKey::AddonMediaListFolderHelp => "Opcional; si se omite, se deriva del nombre de la lista",
        TextKey::ChapterInput => "Capítulos",
        TextKey::ChapterInputHelp => "Opcional, uno por línea: 0:00 Introducción",
        TextKey::RestoreProviderChapters => "Restaurar capítulos del proveedor",
        TextKey::ManagedByAddon => "Gestionado por un complemento",
        TextKey::Trash => "Papelera",
        TextKey::TrashAlreadyEmpty => "La papelera ya está vacía.",
        TextKey::TrashEmpty => "La papelera está vacía.",
        TextKey::TrashEmptyConfirm => "¿Eliminar de la base de datos todos los elementos cuyo archivo no se encuentra?",
        TextKey::TrashEmptyConfirmHelp => {
            "Esto elimina de forma permanente los elementos multimedia que faltan actualmente y sus filas de metadatos dependientes."
        }
        TextKey::TrashSubtitle => {
            "Elementos cuyos archivos de origen no se encuentran actualmente en el disco o en el proveedor."
        }
        TextKey::Troubleshooting => "Diagnóstico",
        TextKey::TunerCount => "Número de sintonizadores",
        TextKey::TvShows => "Series",
        TextKey::Ui => "UI",
        TextKey::Unavailable => "No disponible",
        TextKey::UseActualTimes => "Usar horas reales",
        TextKey::UseCustomPlaybackOrder => "Usar orden de reproducción personalizado",
        TextKey::UseChaptersAsMediaItems => "Usar capítulos como elementos multimedia",
        TextKey::UseChaptersAsMediaItemsHelp => {
            "Programar capítulos individuales en lugar de archivos completos"
        }
        TextKey::UseEmbeddedSubtitles => "Usar subtítulos incrustados",
        TextKey::Uuid => "UUID",
        TextKey::UntitledItem => "Elemento sin título",
        TextKey::WindowsStartup => "Inicio con Windows",
        TextKey::WindowsStartupNeedsRepair => {
            "El comando de inicio guardado apunta a otra ubicación. Aplica este ajuste para repararlo."
        }
        TextKey::WindowsStartupUnavailable => {
            "El inicio con Windows no está disponible porque no se ha encontrado el iniciador incluido o no se ha podido leer el registro."
        }
        TextKey::ActiveTranscodeLimit => "Máximo de transcodificaciones de emisión simultáneas",
        TextKey::ActiveTranscodeLimitHelp => {
            "Límite estricto de transcodificaciones de emisión de FFmpeg. Cuando todas las ranuras están ocupadas, un nuevo espectador recibe una respuesta de que todas las emisiones están en uso hasta que se libera una; las emisiones activas no eluden el límite."
        }
        TextKey::ActiveTranscodeSuggestionHardware => {
            "Se ha detectado un codificador de hardware; es un valor recomendado, no un cambio automático."
        }
        TextKey::ActiveTranscodeSuggestionSoftware => {
            "Solo hay codificación por software; es un valor recomendado, no un cambio automático."
        }
        TextKey::Peak => "pico",
        TextKey::StreamsInUse => "Emisiones en uso",
        TextKey::SuggestedTranscodeLimit => "Valor recomendado",
        TextKey::Watermarks => "Marcas de agua",
        TextKey::Width => "Anchura",
        TextKey::WorkAheadSegmenterLimit => "Transcodificaciones HLS anticipadas",
        TextKey::WorkAheadSegmenterLimitHelp => {
            "Número de ranuras de transcodificación HLS que pueden adelantarse a máxima velocidad para crear búfer. El trabajo anticipado siempre cede ante las emisiones que se están viendo."
        }
        TextKey::AccelerationKind => "Tipo de aceleración",
        TextKey::AddFfmpegProfile => "Añadir perfil de FFmpeg",
        TextKey::AddProfile => "Añadir perfil",
        TextKey::AllowBFrames => "Permitir fotogramas B",
        TextKey::AllowWatermarks => "Permitir marcas de agua",
        TextKey::Audio => "Audio",
        TextKey::AudioBitrate => "Tasa de bits de audio (kbit/s)",
        TextKey::AudioBufferSize => "Tamaño del búfer de audio (kbit)",
        TextKey::AudioChannels => "Canales",
        TextKey::AudioFormat => "Formato de audio",
        TextKey::BitDepth => "Profundidad de bits",
        TextKey::Copy => "Copiar",
        TextKey::CopyBlock => "Copiar bloque",
        TextKey::CopyFailed => "No se ha podido copiar",
        TextKey::CopyNameSuffix => "Copiar",
        TextKey::CopyProfile => "Copiar perfil",
        TextKey::CopyTemplate => "Copiar plantilla",
        TextKey::Crop => "Recortar",
        TextKey::DeinterlaceVideo => "Desentrelazar vídeo",
        TextKey::Edit => "Editar",
        TextKey::EditBlock => "Editar bloque",
        TextKey::EditCollection => "Editar colección",
        TextKey::EditFfmpegProfile => "Editar perfil de FFmpeg",
        TextKey::EditMultiCollection => "Editar multicolección",
        TextKey::EditRerunCollection => "Editar colección de reposiciones",
        TextKey::EditTemplate => "Editar plantilla",
        TextKey::Expression => "Expresión",
        TextKey::Fallback => "Alternativo",
        TextKey::FillerKind => "Tipo de relleno",
        TextKey::General => "General",
        TextKey::HardwareAcceleration => "Aceleración por hardware",
        TextKey::HardwareIfPossible => "Hardware si es posible",
        TextKey::NewProfileName => "Nuevo nombre de perfil",
        TextKey::NoneOption => "(Ninguno)",
        TextKey::NumHelp => "número del punto intermedio, empezando por 1",
        TextKey::OptionalDisplayTitle => "Título de visualización opcional...",
        TextKey::TranscodeAudio => "Transcodificar audio",
        TextKey::NormalizeColors => "Normalizar colores",
        TextKey::NormalizeFramerate => "Normalizar frecuencia de fotogramas",
        TextKey::NormalizeLoudness => "Normalizar sonoridad",
        TextKey::TranscodeVideo => "Transcodificar vídeo",
        TextKey::Off => "Desactivado",
        TextKey::Pad => "Completar",
        TextKey::PadNearestFifteenMinutes => "15 (:00, :15, :30, :45)",
        TextKey::PadNearestFiveMinutes => "5 (:00, :05, :10, :15, :20, etc.)",
        TextKey::PadNearestTenMinutes => "10 (:00, :10, :20, :30, :40, :50)",
        TextKey::PadNearestThirtyMinutes => "30 (:00, :30)",
        TextKey::PadMode => "Modo de ajuste",
        TextKey::PadToNearestMinute => "Completar hasta el minuto más cercano",
        TextKey::PointHelp => "posición del punto intermedio, en segundos",
        TextKey::ProfileCopied => "Perfil copiado.",
        TextKey::ProfileNotFound => "No se ha encontrado el perfil.",
        TextKey::QsvExtraHardwareFrames => "Fotogramas de hardware adicionales de QSV",
        TextKey::Resolution => "Resolución",
        TextKey::SampleRateKhz => "Frecuencia de muestreo (kHz)",
        TextKey::SaveDecoTemplate => "Guardar plantilla de decoración",
        TextKey::SaveProfile => "Guardar perfil",
        TextKey::SaveFillerPreset => "Guardar preajuste de relleno",
        TextKey::SaveRerunCollection => "Guardar colección de reposiciones",
        TextKey::SaveSequentialSchedule => "Guardar programación secuencial",
        TextKey::SaveTemplate => "Guardar plantilla",
        TextKey::SaveBlock => "Guardar bloque",
        TextKey::ScaleAndPad => "Escalar y completar",
        TextKey::ScalingBehavior => "Comportamiento de escalado",
        TextKey::Software => "Software",
        TextKey::Stretch => "Estirar",
        TextKey::TargetLoudnessLufs => "Sonoridad objetivo (LUFS)",
        TextKey::ThreadCountAuto => "Número de hilos (0 = automático)",
        TextKey::OutputDynamicRange => "Rango dinámico de salida",
        TextKey::OutputDynamicRangeCopyHint => {
            "El vídeo se copia, por lo que la salida conserva el rango dinámico del origen."
        }
        TextKey::HdrStaticMetadataHint => {
            "Déjalo vacío para usar los valores predeterminados de HDR10 documentados. Solo se aplican cuando el origen no contiene metadatos HDR propios; la pantalla de masterización y los niveles de luz del origen siempre tienen prioridad."
        }
        TextKey::HdrMasteringDisplay => "Pantalla de masterización",
        TextKey::HdrMaxCll => "MaxCLL (cd/m²)",
        TextKey::HdrMaxFall => "MaxFALL (cd/m²)",
        TextKey::TonemapAlgorithm => "Algoritmo de mapeo de tonos",
        TextKey::VaapiDevicePath => "Ruta del dispositivo VAAPI",
        TextKey::VaapiDisplay => "Pantalla VAAPI",
        TextKey::VaapiDriver => "Controlador VAAPI",
        TextKey::Video => "Video",
        TextKey::VideoBitrate => "Tasa de bits de vídeo (kbit/s)",
        TextKey::VideoBufferSize => "Tamaño del búfer de vídeo (kbit)",
        TextKey::VideoFormat => "Formato de vídeo",
        TextKey::VideoPreset => "Preajuste de vídeo",
        TextKey::VideoTune => "Ajuste de vídeo",
        TextKey::VideoProfile => "Perfil de vídeo",
        TextKey::ViewCollection => "Ver colección",
        TextKey::ViewPlaylist => "Ver lista de reproducción",
        TextKey::AddAll => "Añadir todo",
        TextKey::AddedItemToPlaylist => "Elemento añadido a la lista de reproducción.",
        TextKey::AddedArtistToSchedule => "Artista añadido a la programación",
        TextKey::AddedMovieToSchedule => "Película añadida a la programación",
        TextKey::AddedSeasonToSchedule => "Temporada añadida a la programación",
        TextKey::AddedShowToSchedule => "Serie añadida a la programación",
        TextKey::AddingItem => "Añadiendo elemento...",
        TextKey::AddingItemToPlaylist => "Añadiendo elemento a la lista de reproducción...",
        TextKey::Artists => "Artistas",
        TextKey::ClearSelection => "Borrar selección",
        TextKey::Collection => "Colección",
        TextKey::CollectionNameRequired => "El nombre de la colección es obligatorio",
        TextKey::CollectionNotFound => "No se ha encontrado la colección",
        TextKey::CollectionRenamed => "Colección renombrada.",
        TextKey::EditAddonMediaListProperties => "Editar propiedades de la lista de add-on",
        TextKey::EnterAddonMediaListUrl => "Introduce la URL de la lista de add-on",
        TextKey::EnterSearchQuery => "Introduce una consulta de búsqueda.",
        TextKey::Episodes => "Episodios",
        TextKey::LoadingCollections => "Cargando colecciones...",
        TextKey::LoadingPlaylistGroups => "Cargando grupos de listas de reproducción...",
        TextKey::LoadingPlaylists => "Cargando listas de reproducción...",
        TextKey::GeneratePlaylist => "Generar lista de reproducción",
        TextKey::GeneratePlaylistFromSortedAddonMediaListItems => {
            "Generar una lista de reproducción a partir de los elementos ordenados de la lista de add-on"
        }
        TextKey::MusicVideos => "Vídeos musicales",
        TextKey::NewCollectionName => "Nuevo nombre de colección",
        TextKey::NoMediaMatched => "No se han encontrado coincidencias multimedia.",
        TextKey::NoItemsToAdd => "No hay elementos que añadir.",
        TextKey::NoItemsToPreview => "No hay elementos que previsualizar.",
        TextKey::NoPlaylistsInGroup => "No hay listas de reproducción en este grupo.",
        TextKey::PleaseChoosePlaylist => "Elige una lista de reproducción.",
        TextKey::RequestFailed => "La solicitud ha fallado",
        TextKey::Released => "Publicado",
        TextKey::SaveAs => "Guardar como",
        TextKey::Seasons => "Temporadas",
        TextKey::SeeAll => "Ver todo >>",
        TextKey::SelectAll => "Seleccionar todo",
        TextKey::SelectGroup => "(Selecciona un grupo)",
        TextKey::SelectPlaylist => "(Selecciona una lista de reproducción)",
        TextKey::SelectSearchCollectionHelp => {
            "Selecciona la colección a la que añadir los resultados de búsqueda."
        }
        TextKey::SelectSearchPlaylistHelp => "Selecciona la lista de reproducción a la que añadir los resultados de búsqueda.",
        TextKey::Selected => "Seleccionado",
        TextKey::SelectCollectionToAddThisMovie => "Selecciona una colección a la que añadir esta película.",
        TextKey::SelectPlaylistToAddThisMovie => "Selecciona una lista de reproducción a la que añadir esta película.",
        TextKey::Add => "Añadir",
        TextKey::AddLibrary => "Añadir biblioteca",
        TextKey::Address => "Dirección",
        TextKey::AddressRequired => "La dirección es obligatoria.",
        TextKey::ApiKey => "Clave de API",
        TextKey::ApiKeyRequired => "La clave de API es obligatoria.",
        TextKey::AuthorizedRedirecting => "Autorizado; redirigiendo...",
        TextKey::CheckingEveryTwoSeconds => "Comprobando cada 2 segundos...",
        TextKey::ClickAuthorizePlex => "Haz clic aquí para autorizar en Plex.tv",
        TextKey::ClickPlayoutItemHelp => "Haz clic en un elemento del playout para consultar su título, horarios y contexto de programación.",
        TextKey::ConfigureConnection => "Configurar conexión",
        TextKey::Connection => "Conexión",
        TextKey::DeepScanCollections => "Analizar colecciones exhaustivamente",
        TextKey::DeepScanLibrary => "Analizar biblioteca exhaustivamente",
        TextKey::DeepScan => "Análisis exhaustivo",
        TextKey::EditLibrary => "Editar biblioteca",
        TextKey::EditPlaylist => "Editar lista de reproducción",
        TextKey::EnterOnePathPerLine => "Introduce una ruta por línea.",
        TextKey::Error => "Error",
        TextKey::ExternalCollections => "Colecciones externas",
        TextKey::Information => "Información",
        TextKey::Kind => "Tipo",
        TextKey::Library => "Biblioteca",
        TextKey::LibraryKind => "Tipo de biblioteca",
        TextKey::LibraryPaths => "Rutas de la biblioteca",
        TextKey::LocalLibraries => "Bibliotecas locales",
        TextKey::LocalPath => "Ruta local",
        TextKey::MediaKind => "Tipo de contenido multimedia",
        TextKey::NoLibrariesConfigured => "No hay bibliotecas configuradas. Añade primero una fuente multimedia.",
        TextKey::NoSchedulingContext => "No hay contexto de programación",
        TextKey::NoPlayoutItemsInWindow => "No hay elementos del playout en este intervalo de 24 horas",
        TextKey::NoPlayoutsFound => "No se han encontrado playouts.",
        TextKey::NoPathReplacements => "No hay sustituciones de rutas.",
        TextKey::Paths => "Rutas",
        TextKey::PathReplacements => "Sustituciones de rutas",
        TextKey::PlexSignInHelp => "Inicia sesión en Plex para añadir una fuente multimedia.",
        TextKey::Remote => "Remoto",
        TextKey::RemotePath => "Ruta remota",
        TextKey::Remove => "Quitar",
        TextKey::RemoveBlockFromTemplate => "Quitar bloque de la plantilla",
        TextKey::RemoveContent => "Quitar contenido",
        TextKey::Retry => "Reintentar",
        TextKey::Saved => "Guardado.",
        TextKey::Saving => "Guardando...",
        TextKey::QuickScan => "Análisis rápido",
        TextKey::ScanCollections => "Analizar colecciones",
        TextKey::ScanFailed => "El análisis ha fallado",
        TextKey::ScanLibrary => "Analizar biblioteca",
        TextKey::ScanQueued => "Análisis en cola",
        TextKey::Scanning => "Analizando...",
        TextKey::Server => "Servidor",
        TextKey::ServerAddress => "Dirección del servidor",
        TextKey::SignInToPlex => "Iniciar sesión en Plex",
        TextKey::SignOutOfPlex => "Cerrar sesión en Plex",
        TextKey::Sync => "Sincronizar",
        TextKey::WaitingForPlexAuthorization => "Esperando la autorización de Plex.",
        TextKey::AddChannel => "Añadir canal",
        TextKey::Any => "Cualquiera",
        TextKey::BlockPlayoutTroubleshooting => "Diagnóstico del playout por bloques",
        TextKey::BlockPlayoutHistory => "Historial del playout por bloques",
        TextKey::Categories => "Categorías",
        TextKey::CategoriesHelp => "Lista de categorías separadas por comas",
        TextKey::Channel => "Canal",
        TextKey::ChannelCouldNotBeSaved => {
            "No se ha podido guardar el canal. Comprueba los valores del formulario e inténtalo de nuevo."
        }
        TextKey::ChannelDisabled => "Canal desactivado",
        TextKey::ChannelHiddenFromEpg => "Canal oculto en la EPG",
        TextKey::ChannelLogo => "Logotipo del canal",
        TextKey::ChannelLogoPreview => "Vista previa del logotipo del canal",
        TextKey::ChannelLogoUploadFailed => {
            "No se ha podido cargar el logotipo del canal. Comprueba el archivo e inténtalo de nuevo."
        }
        TextKey::Check => "Comprobar",
        TextKey::Continuous => "Continuo",
        TextKey::DateAndTime => "Fecha y hora",
        TextKey::DefaultOption => "Predeterminado",
        TextKey::DefaultSchedule => "Programación predeterminada",
        TextKey::DeleteChannel => "Eliminar canal",
        TextKey::DownloadMediaSample => "Descargar muestra multimedia",
        TextKey::DownloadResults => "Descargar resultados",
        TextKey::DisableWatermarks => "Desactivar marcas de agua",
        TextKey::EditChannel => "Editar canal",
        TextKey::EditChannelNumbers => "Editar números de canal",
        TextKey::EditNumbers => "Editar números",
        TextKey::ExternalLogoUrl => "URL externa del logotipo",
        TextKey::Fail => "Fallo",
        TextKey::FallbackFiller => "Relleno alternativo",
        TextKey::FfmpegProfile => "Perfil de FFmpeg",
        TextKey::FirstPage => "Primera página",
        TextKey::FirstRunPlaybackOrder => "Orden de reproducción del primer pase",
        TextKey::Forced => "Forzado",
        TextKey::GenerateSubtitles => "Generar subtítulos",
        TextKey::Generated => "Generado",
        TextKey::Details => "Detalles",
        TextKey::Full => "Completo",
        TextKey::Group => "Grupo",
        TextKey::FfmpegRuntime => "Entorno de ejecución de FFmpeg",
        TextKey::FfmpegRuntimeDownloaded => "Entorno de ejecución de FFmpeg descargado",
        TextKey::FfmpegRuntimeLatestRelease => "Última versión",
        TextKey::FfmpegRuntimeRelease => "Versión instalada",
        TextKey::FfmpegRuntimeSource => "Origen",
        TextKey::FfmpegRuntimeSourceDownloaded => "Descargado",
        TextKey::FfmpegRuntimeSourceManaged => "Gestionado",
        TextKey::FfmpegRuntimeSourceOverride => "Sustituir",
        TextKey::FfmpegRuntimeSourcePath => "PATH",
        TextKey::FfmpegRuntimeSourceUnknown => "Desconocido",
        TextKey::FfmpegRuntimeUpdateAvailable => "Actualización disponible",
        TextKey::FfmpegRuntimeUpdateCheckFailed => "La comprobación de actualizaciones ha fallado",
        TextKey::FfmpegRuntimeUpdateFailed => "La actualización del entorno de ejecución de FFmpeg ha fallado",
        TextKey::FfmpegRuntimeUpToDate => "Actualizado",
        TextKey::FfmpegRuntimeUpdated => "Entorno de ejecución de FFmpeg actualizado",
        TextKey::FfmpegRuntimeVersion => "Versión",
        TextKey::HealthNoPersistedRun => "No hay comprobaciones de estado aplicables en este momento.",
        TextKey::HealthPersistedRunHelp => {
            "Resultados actuales de las comprobaciones de estado de paridad. Esta página se actualiza cuando los análisis multimedia cambian el estado de los elementos."
        }
        TextKey::HistoryDetails => "Detalles del historial",
        TextKey::HlsDirect => "HLS directo",
        TextKey::HlsSegmenter => "Segmentador HLS",
        TextKey::Hours => "horas",
        TextKey::IdleBehavior => "Comportamiento en inactividad",
        TextKey::IdleBehaviorHelp => {
            "Controla cómo se comporta el proceso de transcodificación después de que se desconecten todos los clientes"
        }
        TextKey::Info => "Información",
        TextKey::Import => "Importar",
        TextKey::IsEnabled => "Está activado",
        TextKey::Json => "JSON",
        TextKey::KeepRunning => "Mantener en ejecución",
        TextKey::Key => "Clave",
        TextKey::LastMidFillerHelp => "segundos desde el último relleno intermedio",
        TextKey::LastPage => "Última página",
        TextKey::Level => "Nivel",
        TextKey::LoadingMediaInfo => "Cargando información multimedia...",
        TextKey::Logs => "Registros",
        TextKey::Logo => "Logotipo",
        TextKey::MediaItem => "Elemento multimedia",
        TextKey::Message => "Mensaje",
        TextKey::MissingMediaItemQueryParameter => "Falta el parámetro de consulta mediaItem",
        TextKey::Mirror => "Espejo",
        TextKey::MirrorOptionsHelp => "Solo está disponible cuando el origen del playout está establecido en Espejo.",
        TextKey::MirrorSourceChannel => "Canal de origen del espejo",
        TextKey::MusicVideoCreditsMode => "Modo de créditos de vídeos musicales",
        TextKey::MusicVideoCreditsTemplate => "Plantilla de créditos de vídeos musicales",
        TextKey::NoMatchingRecordsFound => "No se han encontrado registros coincidentes",
        TextKey::NoFfmpegProfiles => "No se han encontrado perfiles de FFmpeg. Crea uno primero.",
        TextKey::NoBlockPlayoutSelected => "Selecciona un playout por bloques para inspeccionar sus bloques.",
        TextKey::NoBlocks => "No se han encontrado bloques.",
        TextKey::NoHistory => "No se ha encontrado historial.",
        TextKey::NoMessages => "No hay mensajes.",
        TextKey::NvidiaCapabilitiesNotCollected => {
            "Aún no se han recopilado las capacidades de NVIDIA."
        }
        TextKey::Number => "Número",
        TextKey::OnDemand => "Bajo demanda",
        TextKey::MatchedPointsHelp => {
            "número de puntos intermedios que ya han cumplido la expresión"
        }
        TextKey::Pass => "Correcto",
        TextKey::ParityDebt => "Deuda de paridad",
        TextKey::PlaybackTroubleshooting => "Diagnóstico de reproducción",
        TextKey::Play => "Reproducir",
        TextKey::PlayoutTimeline => "Cronología del playout",
        TextKey::PlayoutMode => "Modo del playout",
        TextKey::PlayoutModeHelp => "Controla el avance del playout del canal",
        TextKey::PlayoutOffset => "Desplazamiento del playout",
        TextKey::PlayoutSource => "Origen del playout",
        TextKey::PlayoutSourceHelp => "Controla el origen del contenido del canal. No puede cambiarse si el canal ya tiene un playout.",
        TextKey::Preview => "Vista previa",
        TextKey::PreviewBlockPlayout => "Vista previa del playout por bloques",
        TextKey::PreviewChannel => "Previsualizar canal",
        TextKey::ChannelPreview => "Vista previa del canal",
        TextKey::ReloadStream => "Recargar emisión",
        TextKey::StopPreview => "Detener vista previa",
        TextKey::OpenFullPlayer => "Abrir reproductor completo",
        TextKey::BackToChannels => "Volver a los canales",
        TextKey::NowPlaying => "En reproducción",
        TextKey::UpNext => "A continuación",
        TextKey::ProgrammeGuide => "Guía de programación",
        TextKey::EpgPreview => "Vista previa de la EPG",
        TextKey::EpgPreviewDescription => {
            "Consulta la guía publicada con dos horas de historial y las 24 horas siguientes."
        }
        TextKey::EpgZoom => "Ampliación",
        TextKey::EpgNow => "Ahora",
        TextKey::NoEpgChannels => "No hay canales activados visibles en la EPG.",
        TextKey::ScheduleGap => "Hueco de programación",
        TextKey::Filler => "Relleno",
        TextKey::NoProgramming => "No hay programación prevista en este intervalo.",
        TextKey::ChannelDisabledPreview => "Este canal está desactivado.",
        TextKey::ChannelNotAiring => "Este canal no está emitiendo en este momento.",
        TextKey::ChannelNoPlayout => "Este canal no tiene ningún playout.",
        TextKey::PreviewIdle => "La vista previa se inicia automáticamente sin sonido.",
        TextKey::PreviewConnecting => "Conectando con la emisión en directo…",
        TextKey::PreviewLive => "La reproducción en directo está activa.",
        TextKey::PreviewPaused => "Vista previa en pausa.",
        TextKey::PreviewRecovering => "Recuperando la reproducción…",
        TextKey::PreviewStopped => "Vista previa detenida.",
        TextKey::PreviewUnsupported => "Este navegador no puede reproducir vídeo HLS.",
        TextKey::PreviewAuthorizationFailed => "Ha fallado la autorización de la vista previa.",
        TextKey::PreviewUnavailable => "La emisión en directo no está disponible.",
        TextKey::PreviewCapacityReached => "Todas las ranuras de transcodificación están ocupadas.",
        TextKey::PreviewNetworkError => "La vista previa ha perdido la conexión de red.",
        TextKey::PreviewMediaError => {
            "El navegador no ha podido decodificar la salida configurada de este canal."
        }
        TextKey::PreviewDerivativeNote => {
            "La vista previa de administración usa una versión H.264/AAC compatible con el navegador; la salida configurada del canal no cambia."
        }
        TextKey::JavascriptRequiredPlayback => "JavaScript es necesario para la reproducción interactiva.",
        TextKey::ConfirmStopTranscodeSession => {
            "¿Detener la sesión de transcodificación compartida de este canal?"
        }
        TextKey::PreviewPlaylistPlayout => "Vista previa del playout de la lista de reproducción",
        TextKey::PreferredAudioTitle => "Título de audio preferido",
        TextKey::PreferredSubtitleLanguage => "Idioma de subtítulos preferido",
        TextKey::QsvCapabilitiesNotCollected => "Aún no se han recopilado las capacidades de QSV.",
        TextKey::RemoveStoredLogo => "Quitar logotipo guardado",
        TextKey::SaveChannel => "Guardar canal",
        TextKey::SaveChannels => "Guardar canales",
        TextKey::Second => "segundo",
        TextKey::Seconds => "segundos",
        TextKey::SelectProfile => "- Selecciona un perfil -",
        TextKey::SequentialSchedule => "Programación secuencial",
        TextKey::SequentialScheduleValidation => "Validación de la programación secuencial",
        TextKey::SeekSeconds => "Segundos de desplazamiento",
        TextKey::Schema => "Esquema",
        TextKey::ShowDisabled => "Mostrar desactivados",
        TextKey::ShowInEpg => "Mostrar en la EPG",
        TextKey::SlugSeconds => "Segundos de separación",
        TextKey::SlugSecondsHelp => "Segundos de vídeo negro y audio en silencio que se insertarán entre cada elemento del playout",
        TextKey::Song => "Canción",
        TextKey::SongVideoMode => "Modo de vídeo de canciones",
        TextKey::SortByLevel => "Ordenar por nivel",
        TextKey::SortByTimestamp => "Ordenar por marca de tiempo",
        TextKey::StopOnDisconnect => "Detener al desconectar",
        TextKey::StopScheduling => "Detener programación",
        TextKey::StopSchedulingBlockItems => "Dejar de programar elementos de bloque",
        TextKey::StopTranscodeSession => "Detener sesión de transcodificación",
        TextKey::StartFromBeginning => "Empezar desde el principio",
        TextKey::StreamSelector => "Selector de pistas",
        TextKey::StreamSelectorMode => "Modo del selector de pistas",
        TextKey::StreamingMode => "Modo de emisión",
        TextKey::Styles => "Estilos",
        TextKey::Subtitle => "Subtítulo",
        TextKey::SubtitleMode => "Modo de subtítulos",
        TextKey::SubtitleModeHelp => "«Forzado» solo muestra las pistas de narrativa forzada. Para mostrar siempre los subtítulos en tu idioma preferido, elige «Cualquiera».",
        TextKey::Timestamp => "Marca de tiempo",
        TextKey::Timeline => "Cronología",
        TextKey::Target => "Objetivo",
        TextKey::Tags => "Etiquetas",
        TextKey::Tools => "Herramientas",
        TextKey::TranscodeMode => "Modo de transcodificación",
        TextKey::TranscodeModeHelp => "Controla cuándo está activo el proceso de transcodificación",
        TextKey::TroubleshootPlayback => "Diagnosticar reproducción",
        TextKey::TroubleshootChannel => "Diagnosticar canal",
        TextKey::TroubleshootingReports => "Informes de diagnóstico",
        TextKey::MpegTsTroubleshooting => "Diagnóstico de MPEG-TS",
        TextKey::InternalMpegTs => "MPEG-TS interno",
        TextKey::PublicMpegTs => "MPEG-TS público",
        TextKey::RetainRawSample => "Conservar muestra sin procesar",
        TextKey::TsdDuckVersion => "Versión de TSDuck",
        TextKey::Report => "Informe",
        TextKey::RunId => "Id. de ejecución",
        TextKey::Status => "Estado",
        TextKey::Summary => "Resumen",
        TextKey::SampleBytes => "Bytes de muestra",
        TextKey::RawSampleArchived => "Muestra sin procesar archivada",
        TextKey::ArchivePath => "Ruta del archivo",
        TextKey::SampleUrl => "URL de muestra",
        TextKey::StabilityMeter => "Medidor de estabilidad",
        TextKey::Stable => "Estable",
        TextKey::Packets => "paquetes",
        TextKey::Services => "Servicios",
        TextKey::Pids => "PIDs",
        TextKey::ContinuityFindings => "Hallazgos de continuidad",
        TextKey::PcrFindings => "Hallazgos de PCR",
        TextKey::Bitrate => "Tasa de bits",
        TextKey::AnalysisJson => "JSON de análisis",
        TextKey::StandardOutput => "Salida estándar",
        TextKey::StandardError => "Error estándar",
        TextKey::NotAvailable => "No disponible",
        TextKey::NotApplicable => "No aplicable",
        TextKey::Completed => "Completado",
        TextKey::Failed => "Fallido",
        TextKey::TroubleshootingPlaybackStartDebt => {
            "La vista previa de reproducción de elementos multimedia y canales ahora escribe informes de FFmpeg, registros, media_info.json, troubleshooting_info.json, el YAML del selector de pistas elegido y archivos de capacidades; además, ejecuta las reglas del selector de pistas y redirige a /iptv/session/.troubleshooting/live.m3u8."
        }
        TextKey::UnableToLoadMediaInfo => "No se ha podido cargar la información multimedia",
        TextKey::UnableToLoadPlaybackTroubleshooting => {
            "No se ha podido cargar el contexto de diagnóstico de reproducción"
        }
        TextKey::UpdateAddonMediaListDaily => "Actualizar la lista desde el proveedor una vez al día",
        TextKey::UploadLogo => "Cargar logotipo",
        TextKey::UploadLogoHelp => {
            "Elige una imagen local para sustituir el logotipo guardado del canal al guardar"
        }
        TextKey::Warning => "Advertencia",
        TextKey::Watermark => "Marca de agua",
        TextKey::WatermarkIds => "Id. de marcas de agua",
        TextKey::Window => "Ventana",
        TextKey::WithProgress => "Con progreso",
        TextKey::Writers => "Guionistas",
        TextKey::Validate => "Validar",
        TextKey::Yaml => "YAML",
        TextKey::YamlFile => "Archivo YAML",
        TextKey::Messages => "Mensajes",
        TextKey::CollectionName => "Nombre de la colección",
        TextKey::MediaItemType => "Tipo de elemento multimedia",
        TextKey::MediaItemTitle => "Título del elemento multimedia",
        TextKey::ActualSize => "Tamaño real",
        TextKey::AddWatermark => "Añadir marca de agua",
        TextKey::AspectRatioGuides => "Guías de relación de aspecto",
        TextKey::BottomLeft => "Abajo a la izquierda",
        TextKey::BottomMiddle => "Abajo en el centro",
        TextKey::BottomRight => "Abajo a la derecha",
        TextKey::CopyWatermark => "Copiar marca de agua",
        TextKey::CopyOf => "Copia de",
        TextKey::CurrentImage => "Imagen actual",
        TextKey::Custom => "Personalizado",
        TextKey::ChannelLogoPreviewUnavailable => {
            "Las vistas previas del logotipo requieren un contexto de canal. Elige aquí una imagen personalizada para previsualizarla."
        }
        TextKey::DeleteWatermark => "Eliminar marca de agua",
        TextKey::DurationSeconds => "Duración (segundos)",
        TextKey::EditWatermark => "Editar marca de agua",
        TextKey::EnableWatermarkToPreview => "Activa la marca de agua para ver su posición.",
        TextKey::Frequency => "Frecuencia",
        TextKey::GeneratingPreview => "Generando vista previa...",
        TextKey::HorizontalMarginPercent => "Margen horizontal (%)",
        TextKey::GuideFourThree => "Marco seguro 4:3",
        TextKey::Identity => "Identidad",
        TextKey::Image => "Imagen",
        TextKey::ImageSource => "Origen de la imagen",
        TextKey::Intermittent => "Intermitente",
        TextKey::IntermittentTiming => "Temporización intermitente",
        TextKey::LeftMiddle => "Centro izquierda",
        TextKey::Location => "Ubicación",
        TextKey::MiddleCenter => "Centro",
        TextKey::Mode => "Modo",
        TextKey::Minutes => "minutos",
        TextKey::Moods => "Estados de ánimo",
        TextKey::NewName => "Nuevo nombre",
        TextKey::NativeAssetDimensions => "Tamaño nativo y renderizado",
        TextKey::NextWatermark => "Marca de agua siguiente",
        TextKey::Opacity => "Opacidad",
        TextKey::OpacityExpression => "Expresión de opacidad",
        TextKey::OpacityOneToHundred => "Opacidad (1-100)",
        TextKey::Permanent => "Permanente",
        TextKey::PerPass => "por pasada",
        TextKey::PlaceWithinSourceContent => "Ubicación en el contenido de origen",
        TextKey::Placement => "Posición",
        TextKey::PlacementPreview => "Vista previa de la posición",
        TextKey::PreviewFailed => "La vista previa ha fallado",
        TextKey::PreviewReferenceFullHd => "Marco de referencia Full HD · 1920×1080",
        TextKey::PreviousWatermark => "Marca de agua anterior",
        TextKey::RightMiddle => "Centro derecha",
        TextKey::SaveWatermark => "Guardar marca de agua",
        TextKey::Scaled => "Escalado",
        TextKey::SelectCustomImageToPreview => "Selecciona o carga una imagen personalizada para previsualizarla.",
        TextKey::Size => "Tamaño",
        TextKey::TopLeft => "Arriba a la izquierda",
        TextKey::TopMiddle => "Arriba en el centro",
        TextKey::TopRight => "Arriba a la derecha",
        TextKey::UploadFailed => "La carga ha fallado.",
        TextKey::UploadImage => "Cargar imagen",
        TextKey::UnsavedChanges => "Cambios sin guardar",
        TextKey::UnsavedWatermarkChangesMessage => {
            "¿Descartar los cambios de la marca de agua y abrir la marca seleccionada?"
        }
        TextKey::VerticalMarginPercent => "Margen vertical (%)",
        TextKey::WatermarkCopied => "Marca de agua copiada.",
        TextKey::WatermarkNotFound => "No se ha encontrado la marca de agua.",
        TextKey::WatermarkPlacementPreview => "vista previa de la posición de la marca de agua",
        TextKey::WatermarkPreview => "vista previa de la marca de agua",
        TextKey::WatermarkNavigation => "Navegación de marcas de agua",
        TextKey::WidthPercent => "Anchura (%)",
        TextKey::ZIndex => "Índice Z",
        TextKey::DiscardChanges => "Descartar cambios",
        TextKey::XmltvBlockBehavior => "Comportamiento de bloques XMLTV",
        TextKey::XmltvTimeZone => "Zona horaria de XMLTV",
        TextKey::Xmltv => "XMLTV",
        TextKey::Badge => "Insignia",
        TextKey::Buttons => "Botones",
        TextKey::CheckValue => "Comprobar",
        TextKey::ChannelName => "Nombre del canal",
        TextKey::ChannelNumber => "Número de canal",
        TextKey::ComponentGallery => "Galería de componentes de ErsatzRS",
        TextKey::DangerButton => "Peligro",
        TextKey::DeleteDialog => "Diálogo de eliminación",
        TextKey::Decrement => "Reducir",
        TextKey::Dialogs => "Diálogos",
        TextKey::DisabledButton => "Desactivado",
        TextKey::EnterName => "Introduce el nombre",
        TextKey::ErrorOccurred => "Se ha producido un error.",
        TextKey::ErrorToast => "Notificación de error",
        TextKey::ExampleModal => "Diálogo de ejemplo",
        TextKey::ExampleModalBody => "Este es el contenido de un diálogo de ejemplo.",
        TextKey::FormInputs => "Campos de formulario",
        TextKey::Icons => "Iconos",
        TextKey::IncrementBadge => "Aumentar insignia",
        TextKey::InfoToast => "Notificación informativa",
        TextKey::JustFyi => "Para tu información.",
        TextKey::LinkButton => "Botón de enlace",
        TextKey::ModalCancelled => "Cancelado.",
        TextKey::ModalConfirmed => "Confirmado.",
        TextKey::NumValue => "N.º",
        TextKey::OpenModal => "Abrir diálogo",
        TextKey::OperationSucceeded => "La operación se ha completado correctamente.",
        TextKey::PagedTableDemo => "PagedTable (5 filas por página, 7 en total)",
        TextKey::PrimaryButton => "Principal",
        TextKey::SecondaryButton => "Secundario",
        TextKey::SelectValue => "Seleccionar",
        TextKey::SomethingMightBeWrong => "Puede que algo no funcione correctamente.",
        TextKey::SuccessToast => "Notificación de éxito",
        TextKey::TextButton => "Texto",
        TextKey::TextValue => "Texto",
        TextKey::Toasts => "Notificaciones",
        TextKey::WarningToast => "Notificación de advertencia",
        TextKey::YamlScript => "Script YAML",
        TextKey::Yes => "Sí",
    }
}

fn schedule_error_message(error: &str) -> String {
    match error.trim() {
        "Schedule name is required!" => "El nombre de la programación es obligatorio.".to_owned(),
        "New Schedule Name is required." => "El nuevo nombre de la programación es obligatorio.".to_owned(),
        "Schedule name must be 50 characters or less." => {
            "El nombre de la programación no puede superar los 50 caracteres.".to_owned()
        }
        "Schedule name must be unique" => "El nombre de la programación debe ser único".to_owned(),
        "Schedule does not exist" => "La programación no existe".to_owned(),
        "[ProgramScheduleId] does not exist." => "[ProgramScheduleId] no existe.".to_owned(),
        "A collection must use the same playback order in every schedule item." => {
            "Una colección debe usar el mismo orden de reproducción en todos los elementos de la programación.".to_owned()
        }
        "Shuffle In Order cannot be used with Fill With Group Mode." => {
            "Mezclar en orden no puede usarse con el modo Completar con grupo.".to_owned()
        }
        "Fixed schedule items require a start time." => {
            "Los elementos fijos de la programación requieren una hora de inicio.".to_owned()
        }
        "Multi Collection items can only use Shuffle or Shuffle In Order." => {
            "Los elementos de una multicolección solo pueden usar Mezclar o Mezclar en orden.".to_owned()
        }
        "Playlist Item Size can only be used with Playlist schedule items." => {
            "El tamaño de elemento de lista solo puede usarse con elementos de programación de tipo lista de reproducción.".to_owned()
        }
        "Multiple playout mode with Count requires a count expression." => {
            "El modo de reproducción múltiple con recuento requiere una expresión de recuento.".to_owned()
        }
        "Duration playout mode requires a duration between 1 minute and 24 hours." => {
            "El modo de reproducción por duración requiere una duración de entre 1 minuto y 24 horas.".to_owned()
        }
        "Duration playout mode requires discard-to-fill attempts." => {
            "El modo de reproducción por duración requiere intentos de descarte para completar.".to_owned()
        }
        "Tail Mode Filler requires a tail filler preset." => {
            "El modo de cola Relleno requiere un preajuste de relleno de cola.".to_owned()
        }
        "Tail filler can only be set when Tail Mode is Filler." => {
            "El relleno de cola solo puede configurarse cuando el modo de cola es Relleno.".to_owned()
        }
        "Collection is required." => "La colección es obligatoria.".to_owned(),
        "Media item is required." => "El elemento multimedia es obligatorio.".to_owned(),
        "Multi Collection is required." => "La multicolección es obligatoria.".to_owned(),
        "Smart Collection is required." => "La colección inteligente es obligatoria.".to_owned(),
        "Playlist is required." => "La lista de reproducción es obligatoria.".to_owned(),
        "Rerun Collection is required." => "La colección de reposiciones es obligatoria.".to_owned(),
        "Search Query is required." => "La consulta de búsqueda es obligatoria.".to_owned(),
        "Only one pre-, mid-, or post-roll filler can pad to the nearest minute." => {
            "Solo un relleno previo, intermedio o posterior puede completar hasta el minuto más cercano.".to_owned()
        }
        "Padding filler requires a fallback filler." => {
            "El relleno de ajuste requiere un relleno alternativo.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn scheduling_content_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("Block group name \"") && trimmed.ends_with("\" is invalid") {
        return "El nombre del grupo de bloques debe tener 50 caracteres o menos.".to_owned();
    }
    if trimmed.starts_with("Block name \"") && trimmed.ends_with("\" is invalid") {
        return "El nombre del bloque debe tener 50 caracteres o menos.".to_owned();
    }
    if trimmed.starts_with("Template name \"") && trimmed.ends_with("\" is invalid") {
        return "El nombre de la plantilla debe tener 50 caracteres o menos.".to_owned();
    }
    if trimmed.starts_with("A template named \"")
        && trimmed.ends_with("\" already exists in that template group")
    {
        return "Ya existe una plantilla con ese nombre en ese grupo de plantillas.".to_owned();
    }
    if trimmed.starts_with("Block from ") && trimmed.contains(" intersects block from ") {
        return "El bloque se solapa con otro bloque.".to_owned();
    }
    if trimmed.starts_with("TemplateGroup ") && trimmed.ends_with(" does not exist.") {
        return "El grupo de plantillas no existe.".to_owned();
    }
    if trimmed.starts_with("Template ") && trimmed.ends_with(" does not exist.") {
        return "La plantilla no existe.".to_owned();
    }
    if trimmed.starts_with("DecoGroup ") && trimmed.ends_with(" does not exist.") {
        return "El grupo de decoraciones no existe.".to_owned();
    }
    if trimmed.starts_with("Deco ") && trimmed.ends_with(" does not exist.") {
        return "La decoración no existe.".to_owned();
    }
    if trimmed.starts_with("DecoTemplateGroup ") && trimmed.ends_with(" does not exist.") {
        return "El grupo de plantillas de decoración no existe.".to_owned();
    }
    if trimmed.starts_with("DecoTemplate ") && trimmed.ends_with(" does not exist.") {
        return "La plantilla de decoración no existe.".to_owned();
    }
    if trimmed.starts_with("Deco name \"") && trimmed.ends_with("\" is invalid") {
        return "El nombre de la decoración no puede superar los 50 caracteres.".to_owned();
    }
    if trimmed.starts_with("A deco named \"")
        && trimmed.ends_with("\" already exists in that deco group")
    {
        return "Ya existe una decoración con ese nombre en el grupo.".to_owned();
    }
    if trimmed.starts_with("Deco template name \"") && trimmed.ends_with("\" is invalid") {
        return "El nombre de la plantilla de decoración no puede superar los 50 caracteres."
            .to_owned();
    }
    if trimmed.starts_with("A deco template named \"")
        && trimmed.ends_with("\" already exists in that deco template group")
    {
        return "Ya existe una plantilla de decoración con ese nombre en ese grupo de plantillas de decoración."
            .to_owned();
    }
    if trimmed.starts_with("Watermark ") && trimmed.ends_with(" does not exist.") {
        return "La marca de agua no existe.".to_owned();
    }
    if trimmed.starts_with("Graphics element ") && trimmed.ends_with(" does not exist.") {
        return "El elemento gráfico no existe.".to_owned();
    }
    if trimmed.starts_with("Deco from ") && trimmed.contains(" intersects deco from ") {
        return "La decoración se solapa con otra.".to_owned();
    }

    match trimmed {
        "Block does not exist." => "El bloque no existe.".to_owned(),
        "Block group name is required" => {
            "El nombre del grupo de bloques es obligatorio".to_owned()
        }
        "Block group name must be unique" => {
            "El nombre del grupo de bloques debe ser único".to_owned()
        }
        "Block name is required" => "El nombre del bloque es obligatorio".to_owned(),
        "A block named with that value already exists in that block group" => {
            "Ya existe un bloque con ese nombre en el grupo de bloques".to_owned()
        }
        "Block duration must be between 5 minutes and 24 hours" => {
            "La duración del bloque debe estar entre 5 minutos y 24 horas".to_owned()
        }
        "[Collection] is required for collection type 'Collection'" => {
            "[Collection] es obligatorio para el tipo de colección 'Collection'".to_owned()
        }
        "[MediaItem] is required for collection type 'TelevisionShow'" => {
            "[MediaItem] es obligatorio para el tipo de colección 'TelevisionShow'".to_owned()
        }
        "[MediaItem] is required for collection type 'TelevisionSeason'" => {
            "[MediaItem] es obligatorio para el tipo de colección 'TelevisionSeason'".to_owned()
        }
        "[MultiCollection] is required for collection type 'MultiCollection'" => {
            "[MultiCollection] es obligatorio para el tipo de colección 'MultiCollection'"
                .to_owned()
        }
        "[SmartCollection] is required for collection type 'SmartCollection'" => {
            "[SmartCollection] es obligatorio para el tipo de colección 'SmartCollection'"
                .to_owned()
        }
        "[SearchQuery] is required for collection type 'SearchQuery'" => {
            "[SearchQuery] es obligatorio para el tipo de colección 'SearchQuery'".to_owned()
        }
        "[CollectionType] is invalid" => "[CollectionType] no es válido".to_owned(),
        "[BlockId] does not exist." => "[BlockId] no existe.".to_owned(),
        "Template does not exist." => "La plantilla no existe.".to_owned(),
        "Template group name is required" => {
            "El nombre del grupo de plantillas es obligatorio".to_owned()
        }
        "Template group name must be 50 characters or fewer" => {
            "El nombre del grupo de plantillas no puede superar los 50 caracteres".to_owned()
        }
        "Template group name must be unique" => {
            "El nombre del grupo de plantillas debe ser único".to_owned()
        }
        "Template group does not exist." => "El grupo de plantillas no existe.".to_owned(),
        "[TemplateId] does not exist." => "[TemplateId] no existe.".to_owned(),
        "Template name is required" => "El nombre de la plantilla es obligatorio".to_owned(),
        "Template items cannot cross midnight." => {
            "Los elementos de la plantilla no pueden cruzar la medianoche.".to_owned()
        }
        "Deco group name is required" => {
            "El nombre del grupo de decoraciones es obligatorio".to_owned()
        }
        "Deco group name must be 50 characters or fewer" => {
            "El nombre del grupo de decoraciones no puede superar los 50 caracteres".to_owned()
        }
        "Deco group name must be unique" => {
            "El nombre del grupo de decoraciones debe ser único".to_owned()
        }
        "Deco group does not exist." => "El grupo de decoraciones no existe.".to_owned(),
        "Deco name is required" => "El nombre de la decoración es obligatorio".to_owned(),
        "Deco does not exist" => "La decoración no existe.".to_owned(),
        "Default filler has unsupported collection type." => {
            "El relleno predeterminado tiene un tipo de colección no compatible.".to_owned()
        }
        "Dead air fallback has unsupported collection type." => {
            "El contenido alternativo para interrupciones tiene un tipo de colección no compatible."
                .to_owned()
        }
        "Break content has unsupported collection type." => {
            "El contenido de pausa tiene un tipo de colección no compatible.".to_owned()
        }
        "Default filler must have valid item" => {
            "El relleno predeterminado debe tener un elemento válido".to_owned()
        }
        "Dead air fallback must have valid item" => {
            "El contenido alternativo para interrupciones debe tener un elemento válido".to_owned()
        }
        "Break content collection must have valid item" => {
            "La colección de contenido de pausa debe tener un elemento válido".to_owned()
        }
        "Break content multi collection must have valid item" => {
            "La multicolección de contenido de pausa debe tener un elemento válido".to_owned()
        }
        "Break content smart collection must have valid item" => {
            "La colección inteligente de contenido de pausa debe tener un elemento válido"
                .to_owned()
        }
        "Break content media item must have valid item" => {
            "El contenido multimedia de pausa debe tener un elemento válido".to_owned()
        }
        "Break content playlist must have valid item" => {
            "La lista de reproducción de contenido de pausa debe tener un elemento válido"
                .to_owned()
        }
        "Default filler item does not exist." => {
            "El elemento de relleno predeterminado no existe.".to_owned()
        }
        "Dead air fallback item does not exist." => {
            "El elemento alternativo para interrupciones no existe.".to_owned()
        }
        "Break content collection item does not exist." => {
            "El elemento de colección del contenido de pausa no existe.".to_owned()
        }
        "Break content multi collection item does not exist." => {
            "El elemento de multicolección del contenido de pausa no existe.".to_owned()
        }
        "Break content smart collection item does not exist." => {
            "El elemento de colección inteligente del contenido de pausa no existe.".to_owned()
        }
        "Break content media item item does not exist." => {
            "El elemento multimedia del contenido de pausa no existe.".to_owned()
        }
        "Break content playlist item does not exist." => {
            "La lista de reproducción del contenido de pausa no existe.".to_owned()
        }
        "Deco may only contain one [Block Start] break content" => {
            "La decoración solo puede contener un contenido de pausa de Inicio de bloque."
                .to_owned()
        }
        "Deco may only contain one [Between Block Items] break content" => {
            "La decoración solo puede contener un contenido de pausa Entre elementos del bloque."
                .to_owned()
        }
        "Deco may only contain one [At Chapter Markers] break content" => {
            "La decoración solo puede contener un contenido de pausa En marcadores de capítulo."
                .to_owned()
        }
        "Deco may only contain one [Block Finish] break content" => {
            "La decoración solo puede contener un contenido de pausa de Final de bloque.".to_owned()
        }
        "Deco template group name is required" => {
            "El nombre del grupo de plantillas de decoración es obligatorio".to_owned()
        }
        "Deco template group name must be 50 characters or fewer" => {
            "El nombre del grupo de plantillas de decoración no puede superar los 50 caracteres"
                .to_owned()
        }
        "Deco template group name must be unique" => {
            "El nombre del grupo de plantillas de decoración debe ser único".to_owned()
        }
        "Deco template group does not exist." => {
            "El grupo de plantillas de decoración no existe.".to_owned()
        }
        "[DecoTemplateId] does not exist." => "La plantilla de decoración no existe.".to_owned(),
        "Deco template name is required" => {
            "El nombre de la plantilla de decoración es obligatorio".to_owned()
        }
        "[DecoId] does not exist." => "La decoración no existe.".to_owned(),
        "Deco template items cannot be empty." => {
            "Los elementos de la plantilla de decoración no pueden estar vacíos.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn watermark_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("Watermark ") && trimmed.ends_with(" does not exist.") {
        return "No se ha encontrado la marca de agua.".to_owned();
    }

    match trimmed {
        "Name is required" => "El nombre es obligatorio".to_owned(),
        "Name must be 50 characters or fewer" => {
            "El nombre no puede superar los 50 caracteres".to_owned()
        }
        "Watermark image is required" | "Watermark image is required!" => {
            "La imagen de la marca de agua es obligatoria".to_owned()
        }
        "Width must be between 0 and 100%" => {
            "La anchura debe estar entre el 0 y el 100 %".to_owned()
        }
        "Horizontal margin must be between 0 and 50%" => {
            "El margen horizontal debe estar entre el 0 y el 50 %".to_owned()
        }
        "Vertical margin must be between 0 and 50%" => {
            "El margen vertical debe estar entre el 0 y el 50 %".to_owned()
        }
        "Duration must be positive and less than frequency window" => {
            "La duración debe ser positiva e inferior al intervalo de frecuencia".to_owned()
        }
        "Opacity must be between 1 and 100" => "La opacidad debe estar entre 1 y 100".to_owned(),
        "Watermark name must be unique" | "ChannelWatermark name must be unique" => {
            "El nombre de la marca de agua debe ser único".to_owned()
        }
        "Watermark not found" | "Source watermark not found" | "Watermark does not exist." => {
            "No se ha encontrado la marca de agua.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn ffmpeg_profile_hardware_acceleration_label(value: i32) -> String {
    match value {
        1 => "QSV",
        2 => "NVENC",
        3 => "VAAPI",
        4 => "VideoToolbox",
        5 => "AMF",
        6 => "V4L2M2M",
        7 => "RKMPP",
        _ => return format!("Desconocido ({value})"),
    }
    .to_owned()
}

fn ffmpeg_profile_video_format_label(format: i32) -> String {
    match format {
        0 => "Ninguno",
        1 => "H264",
        2 => "H265 (HEVC)",
        3 => "MPEG-2",
        4 => "AV1",
        99 => text(TextKey::Copy),
        _ => return format!("Desconocido ({format})"),
    }
    .to_owned()
}

fn ffmpeg_profile_audio_format_label(format: i32) -> String {
    match format {
        0 => "Ninguno",
        1 => "AAC",
        2 => "AC3",
        3 => "AAC (LATM)",
        99 => text(TextKey::Copy),
        _ => return format!("Desconocido ({format})"),
    }
    .to_owned()
}

fn settings_error_message(error: &str) -> String {
    let trimmed = error.trim();
    let trimmed = trimmed
        .strip_prefix("update Windows startup registration is unavailable: ")
        .unwrap_or(trimmed);

    if let Some((current, limit)) = xmltv_days_error_parts(trimmed) {
        return format!(
            "Los días de XMLTV que se generarán ({current}) no pueden superar los días de reproducción que se generarán ({limit})"
        );
    }
    if let Some(name) = trimmed.strip_suffix(" path does not exist") {
        return format!("La ruta de {name} no existe");
    }
    if let Some(name) = trimmed
        .strip_prefix("Unable to verify ")
        .and_then(|value| value.strip_suffix(" version"))
    {
        return format!("No se ha podido verificar la versión de {name}");
    }
    if let Some(value) = trimmed.strip_prefix("Invalid option: ") {
        return format!("Opción no válida: {value}");
    }
    if trimmed.starts_with("Resolution ") && trimmed.ends_with(" does not exist.") {
        return "La resolución no existe.".to_owned();
    }

    match trimmed {
        "Windows startup registration is not supported on this platform" => {
            "El registro de inicio de Windows no es compatible con esta plataforma".to_owned()
        }
        "ErsatzRS.exe was not found next to ersatzrs-web.exe" => {
            "No se ha encontrado ErsatzRS.exe junto a ersatzrs-web.exe".to_owned()
        }
        "Unable to update Windows startup registration" => {
            "No se ha podido actualizar el registro de inicio de Windows".to_owned()
        }
        "Tuner count must be greater than zero" => {
            "El número de sintonizadores debe ser mayor que cero".to_owned()
        }
        "Library refresh interval must be 0 (do not refresh) or greater than zero" => {
            "El intervalo de actualización de la biblioteca debe ser 0 (no actualizar) o mayor que cero".to_owned()
        }
        "Library refresh interval must be less than 1,000,000. Use 0 to disable automatic refresh" => {
            "El intervalo de actualización de la biblioteca debe ser inferior a 1 000 000. Usa 0 para desactivar la actualización automática".to_owned()
        }
        "Playout days to build must be greater than zero" => {
            "Los días de reproducción que se generarán deben ser mayores que cero".to_owned()
        }
        "Scripted schedule timeout seconds must be at least 10" => {
            "El tiempo de espera de la programación mediante script debe ser de al menos 10 segundos".to_owned()
        }
        "XMLTV days to build must be greater than zero" => {
            "Los días de XMLTV que se generarán deben ser mayores que cero".to_owned()
        }
        "Resolution width or height is invalid" => {
            "La anchura o la altura de la resolución no es válida".to_owned()
        }
        "Resolution width and height must be unique" => {
            "La combinación de anchura y altura de la resolución debe ser única".to_owned()
        }
        "HLS Segmenter idle timeout must be greater than or equal to 30" => {
            "El tiempo de espera por inactividad del segmentador HLS debe ser mayor o igual que 30".to_owned()
        }
        "Work-Ahead HLS Segmenter limit must be greater than or equal to 0" => {
            "El límite anticipado del segmentador HLS debe ser mayor o igual que 0".to_owned()
        }
        "Active HLS transcode limit must be greater than or equal to 1" => {
            "El límite de transcodificaciones HLS activas debe ser mayor o igual que 1".to_owned()
        }
        "HLS Segmenter initial segment count must be greater than or equal to 1" => {
            "El número inicial de segmentos del segmentador HLS debe ser mayor o igual que 1".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn media_action_error_message(error: &str) -> String {
    match error.trim() {
        "Collection name is required" => "El nombre de la colección es obligatorio".to_owned(),
        "Collection name must be 50 characters or less" => {
            "El nombre de la colección no puede superar los 50 caracteres".to_owned()
        }
        "Collection name must be unique" => "El nombre de la colección debe ser único".to_owned(),
        "Media item does not exist" => "El elemento multimedia no existe".to_owned(),
        "Collection does not exist" | "Collection does not exist." => {
            "La colección no existe.".to_owned()
        }
        "Playlist does not exist" | "Playlist does not exist." => {
            "La lista de reproducción no existe.".to_owned()
        }
        "Unable to locate media item" => {
            "No se ha podido localizar el elemento multimedia".to_owned()
        }
        "Media action failed" => "La acción multimedia ha fallado".to_owned(),
        _ => error.to_owned(),
    }
}

fn filler_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("FillerPreset ") && trimmed.ends_with(" does not exist.") {
        return "El preajuste de relleno no existe.".to_owned();
    }

    match trimmed {
        "Filler preset does not exist" => "El preajuste de relleno no existe.".to_owned(),
        "Filler preset name must be unique" => "El nombre del preajuste de relleno debe ser único".to_owned(),
        "Name must not be empty" => "El nombre no puede estar vacío".to_owned(),
        "Name must be 50 characters or fewer" => "El nombre no puede superar los 50 caracteres".to_owned(),
        "Filler kind must not be None" => "El tipo de relleno no puede ser Ninguno".to_owned(),
        "Filler mode must not be None" => "El modo de relleno no puede ser Ninguno".to_owned(),
        "Count must be greater than or equal to 0" => {
            "El recuento debe ser mayor o igual que 0".to_owned()
        }
        "Duration must not be empty" => "La duración no puede estar vacía".to_owned(),
        "Pad To Nearest Minute must not be empty" => {
            "Completar hasta el minuto más cercano no puede estar vacío".to_owned()
        }
        "Use Chapters As Media Items must not be enabled for fallback filler" => {
            "Usar capítulos como elementos multimedia no puede activarse para el relleno alternativo".to_owned()
        }
        "Collection must not be empty" => "La colección no puede estar vacía".to_owned(),
        "Multi Collection must not be empty" => "La multicolección no puede estar vacía".to_owned(),
        "Smart Collection must not be empty" => "La colección inteligente no puede estar vacía".to_owned(),
        "Media item must not be empty" => "El elemento multimedia no puede estar vacío".to_owned(),
        _ => error.to_owned(),
    }
}

fn media_browser_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed == "Pinned stream selection is invalid or no longer available." {
        return "La selección de pistas fijada no es válida o ya no está disponible.".to_owned();
    }
    if trimmed == "Movie was not found." {
        return "No se ha encontrado la película.".to_owned();
    }
    if trimmed.starts_with("Collection ") && trimmed.ends_with(" is already present.") {
        return "La colección ya está presente.".to_owned();
    }
    if trimmed.starts_with("Collection ") && trimmed.ends_with(" does not exist.") {
        return "La colección no existe.".to_owned();
    }
    if trimmed.starts_with("Smart collection ") && trimmed.ends_with(" is already present.") {
        return "La colección inteligente ya está presente.".to_owned();
    }
    if trimmed.starts_with("Smart collection ") && trimmed.ends_with(" does not exist.") {
        return "La colección inteligente no existe.".to_owned();
    }
    if trimmed.starts_with("MultiCollection ") && trimmed.ends_with(" does not exist.") {
        return "La multicolección no existe.".to_owned();
    }
    if trimmed.starts_with("Rerun collection ") && trimmed.ends_with(" does not exist.") {
        return "La colección de reposiciones no existe.".to_owned();
    }
    if trimmed.starts_with("Referenced item ") && trimmed.ends_with(" does not exist.") {
        return "El elemento referenciado no existe.".to_owned();
    }
    if trimmed.starts_with("PlaylistGroup ") && trimmed.ends_with(" does not exist.") {
        return "El grupo de listas de reproducción no existe.".to_owned();
    }
    if trimmed.starts_with("Playlist ") && trimmed.ends_with(" does not exist.") {
        return "La lista de reproducción no existe.".to_owned();
    }
    if trimmed.starts_with("Playlist name \"") && trimmed.ends_with("\" is invalid") {
        return "El nombre de la lista de reproducción no es válido.".to_owned();
    }
    if let Some(error) = trimmed.strip_prefix("Playlist preview failed: ") {
        return format!("La vista previa de la lista de reproducción ha fallado: {error}");
    }
    if trimmed.starts_with("Media item ") && trimmed.ends_with(" does not exist.") {
        return "El elemento multimedia no existe.".to_owned();
    }

    match trimmed {
        "Collection name is required" => "El nombre de la colección es obligatorio".to_owned(),
        "Collection name must be 50 characters or less" => {
            "El nombre de la colección no puede superar los 50 caracteres".to_owned()
        }
        "Collection name must be unique" => "El nombre de la colección debe ser único".to_owned(),
        "Collection does not exist." => "La colección no existe.".to_owned(),
        "Smart collection name is required" => {
            "El nombre de la colección inteligente es obligatorio".to_owned()
        }
        "Smart collection query is required" => {
            "La consulta de la colección inteligente es obligatoria".to_owned()
        }
        "SmartCollection name must be unique" => {
            "El nombre de la colección inteligente debe ser único".to_owned()
        }
        "MultiCollection does not exist." => "La multicolección no existe.".to_owned(),
        "Multi-collection name is required!" => {
            "El nombre de la multicolección es obligatorio.".to_owned()
        }
        "Multi-collection name cannot be longer than 50 characters." => {
            "El nombre de la multicolección no puede superar los 50 caracteres.".to_owned()
        }
        "MultiCollection name must be unique" => {
            "El nombre de la multicolección debe ser único".to_owned()
        }
        "Each multi-collection item must reference exactly one collection." => {
            "Cada elemento de la multicolección debe hacer referencia exactamente a una colección."
                .to_owned()
        }
        "Rerun collection does not exist." => "La colección de reposiciones no existe.".to_owned(),
        "[Name] is an empty string" | "[Name] must not be empty." => {
            "El nombre no puede estar vacío.".to_owned()
        }
        "[Name] must not be longer than 50" | "[Name] must not be longer than 50." => {
            "El nombre no puede superar los 50 caracteres.".to_owned()
        }
        "Rerun collection name must be unique" => {
            "El nombre de la colección de reposiciones debe ser único".to_owned()
        }
        "Unsupported collection type." => "Tipo de colección no compatible.".to_owned(),
        "[ProgramScheduleId] does not exist." => "La programación no existe.".to_owned(),
        "[MediaItem] does not exist." => "El elemento multimedia no existe.".to_owned(),
        "[PlaylistGroupId] does not exist." => {
            "El grupo de listas de reproducción no existe.".to_owned()
        }
        "Cannot delete system playlist group" => {
            "No se puede eliminar el grupo de listas de reproducción del sistema".to_owned()
        }
        "Cannot delete system (generated) playlist" => {
            "No se puede eliminar la lista de reproducción del sistema (generada)".to_owned()
        }
        "[PlaylistId] does not exist." => "La lista de reproducción no existe.".to_owned(),
        "Cannot save system (generated) playlist" => {
            "No se puede guardar la lista de reproducción del sistema (generada)".to_owned()
        }
        "Playlist playout preview has no items" => {
            "La vista previa de reproducción de la lista no contiene elementos".to_owned()
        }
        "Playlist name is required" => {
            "El nombre de la lista de reproducción es obligatorio".to_owned()
        }
        "[Collection] is required for collection type 'Collection'" => {
            "La colección es obligatoria.".to_owned()
        }
        "[MultiCollection] is required for collection type 'MultiCollection'" => {
            "La multicolección es obligatoria.".to_owned()
        }
        "[SmartCollection] is required for collection type 'SmartCollection'" => {
            "La colección inteligente es obligatoria.".to_owned()
        }
        "[CollectionType] is invalid" => "El tipo de colección no es válido.".to_owned(),
        _ => error.to_owned(),
    }
}

fn media_source_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if let Some(error) = trimmed.strip_prefix("Could not reach Jellyfin: ") {
        return format!("No se ha podido contactar con Jellyfin: {error}");
    }

    match trimmed {
        "Address is required." => "La dirección es obligatoria.".to_owned(),
        "API key is required." => "La clave de API es obligatoria.".to_owned(),
        "'Address' must be a valid URL" => "'Address' debe ser una URL válida".to_owned(),
        _ => error.to_owned(),
    }
}

fn addon_operation_error_message(code: &str, detail: &str) -> String {
    let summary = match code {
        "missing-setting" => "Configura el ajuste obligatorio del add-on.",
        "missing-secret" => "Configura la referencia secreta obligatoria en los ajustes del add-on.",
        "provider-unreachable" => "El add-on no pudo contactar con su proveedor.",
        "provider-rejected" => "El proveedor rechazó la solicitud del add-on.",
        "provider-rate-limited" => "El proveedor limitó temporalmente el add-on.",
        "not-found" => "No se encontró el elemento solicitado del add-on.",
        "unsupported-url" => "El add-on no admite esta URL.",
        _ => "El add-on notificó un fallo.",
    };
    if detail.trim().is_empty() { summary.to_owned() } else { format!("{summary} {detail}") }
}

fn addon_media_list_error_message(error: &str) -> String {
    let trimmed = error.trim();
    let normalized = trimmed.strip_prefix("conflict: ").unwrap_or(trimmed);
    if let Some(error) = normalized.strip_prefix("failed to decode media-list add-on output: ") {
        return format!("El add-on devolvió datos de lista no válidos: {error}");
    }
    if normalized.starts_with("add-on media list with id ") && normalized.ends_with(" not found") {
        return "La lista de add-on no existe.".to_owned();
    }

    if let Some(folder) =
        normalized.strip_prefix("another list of this add-on already uses the folder ")
    {
        return format!("Otra lista de este complemento ya usa la carpeta \"{folder}\".");
    }
    if let Some(folder) = normalized.strip_suffix(" is not usable as a folder name") {
        return format!("\"{folder}\" no se puede usar como nombre de carpeta.");
    }

    match normalized {
        "Invalid add-on list url" => "La URL de la lista de add-on no es válida.".to_owned(),
        "add-on API client is not configured for this operation" => {
            "El cliente de la API de add-on no está configurado para esta operación.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn upload_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if let Some(error) = trimmed.strip_prefix("Cannot create upload directory: ") {
        return format!("No se puede crear el directorio de carga: {error}");
    }
    if let Some(error) = trimmed.strip_prefix("Write failed: ") {
        return format!("Error de escritura: {error}");
    }
    if let Some(error) = trimmed.strip_prefix("Read field: ") {
        return format!("No se ha podido leer el campo: {error}");
    }

    match trimmed {
        "Upload exceeds maximum allowed size" => {
            "La carga supera el tamaño máximo permitido.".to_owned()
        }
        "No file field" => "No se ha encontrado ningún campo de archivo.".to_owned(),
        _ => error.to_owned(),
    }
}

fn xmltv_days_error_parts(error: &str) -> Option<(&str, &str)> {
    let rest = error.strip_prefix("XMLTV days to build (")?;
    let (current, rest) = rest.split_once(") cannot be greater than Playout days to build (")?;
    let limit = rest.strip_suffix(')')?;
    Some((current, limit))
}

fn ffmpeg_profile_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("An FFmpeg profile named \"") && trimmed.ends_with("\" already exists.")
    {
        let name = trimmed
            .strip_prefix("An FFmpeg profile named \"")
            .and_then(|value| value.strip_suffix("\" already exists."))
            .unwrap_or("");
        return format!("Ya existe un perfil de FFmpeg llamado \"{name}\".");
    }

    match trimmed {
        "Profile name is required." => "El nombre del perfil es obligatorio.".to_owned(),
        "Profile name must be 50 characters or fewer." => {
            "El nombre del perfil no puede superar los 50 caracteres.".to_owned()
        }
        "Thread count must be 0 or greater." => "El número de hilos debe ser 0 o mayor.".to_owned(),
        "QSV supports formats (h264, hevc, mpeg2video)" => {
            "QSV admite los formatos (h264, hevc, mpeg2video).".to_owned()
        }
        "NVENC supports formats (h264, hevc, av1)" => {
            "NVENC admite los formatos (h264, hevc, av1).".to_owned()
        }
        "VAAPI supports formats (h264, hevc, av1, mpeg2video)" => {
            "VAAPI admite los formatos (h264, hevc, av1, mpeg2video).".to_owned()
        }
        "VideoToolbox supports formats (h264, hevc)" => {
            "VideoToolbox admite los formatos (h264, hevc).".to_owned()
        }
        "Amf supports formats (h264, hevc)" => "AMF admite los formatos (h264, hevc).".to_owned(),
        "V4L2 M2M supports formats (h264, hevc)" => {
            "V4L2 M2M admite los formatos (h264, hevc).".to_owned()
        }
        "Rkmpp supports formats (h264, hevc)" => "RKMPP admite los formatos (h264, hevc).".to_owned(),
        "HDR10 output requires hevc or av1; h264 and mpeg2video cannot carry HDR10 static metadata" => {
            "La salida HDR10 requiere hevc o av1; h264 y mpeg2video no pueden transportar metadatos estáticos HDR10.".to_owned()
        }
        "HDR10 output requires 10-bit; 8-bit cannot represent a PQ signal without banding" => {
            "La salida HDR10 requiere 10 bits; 8 bits no pueden representar una señal PQ sin bandas de color.".to_owned()
        }
        "Mastering display must look like G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1), with chromaticities up to 50000 and max luminance above min" => {
            "La pantalla de masterización debe tener el formato G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1), con cromaticidades de hasta 50000 y una luminancia máxima superior a la mínima.".to_owned()
        }
        "MaxCLL and MaxFALL must both be set (or both left empty), at most 65535, with MaxFALL no greater than MaxCLL" => {
            "MaxCLL y MaxFALL deben definirse ambos (o dejarse ambos vacíos), con un máximo de 65535 y MaxFALL no superior a MaxCLL.".to_owned()
        }
        "Mpeg2Video does not support 10-bit content" => {
            "Mpeg2Video no admite contenido de 10 bits.".to_owned()
        }
        "VideoProfile must be high10 with 10-bit h264" => {
            "El perfil de vídeo debe ser high10 con h264 de 10 bits.".to_owned()
        }
        "VideoProfile must be high444p with NVIDIA 10-bit h264" => {
            "El perfil de vídeo debe ser high444p con h264 de 10 bits de NVIDIA.".to_owned()
        }
        "VideoProfile cannot be high10 with 8-bit h264" => {
            "El perfil de vídeo no puede ser high10 con h264 de 8 bits.".to_owned()
        }
        "Source profile not found." => "No se ha encontrado el perfil de origen.".to_owned(),
        "No FFmpeg resolutions are available; database bootstrap is incomplete" => {
            "No hay resoluciones de FFmpeg disponibles; la inicialización de la base de datos está incompleta".to_owned()
        }
        _ => error.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_matches_spanish_spain_contract() {
        assert_eq!(SORT_ORDER, 130);
        assert_eq!(CODE, "es-es");
        assert_eq!(LABEL, "español (España)");
        assert_eq!(ALIASES, &["es", "es-es", "es_es"]);
    }

    #[test]
    fn representative_product_text_is_localized() {
        assert_eq!(STRINGS.text(TextKey::Addons), "Complementos");
        assert_eq!(
            STRINGS.text(TextKey::AddDecoToTemplate),
            "Añadir decoración a la plantilla"
        );
        assert_eq!(STRINGS.season_display_name(0), "Especiales");
        assert_eq!(STRINGS.season_display_name(2), "Temporada 2");
        assert_eq!(
            STRINGS.media_item_fallback_label(42),
            "Elemento multimedia n.º 42"
        );
    }

    #[test]
    fn dynamic_text_preserves_values_and_technical_identifiers() {
        assert_eq!(STRINGS.selected_count(3), "3 seleccionados");
        assert_eq!(STRINGS.page_of(2, 5), "Página 2 de 5");
        assert_eq!(STRINGS.channel_streaming_mode_label(2), "HLS directo");
        assert_eq!(
            STRINGS.hardware_acceleration_option_label("NVENC", false),
            "NVENC (no disponible en este sistema)"
        );
    }

    #[test]
    fn glossary_terms_are_used_consistently() {
        // Playout keeps the product noun so it cannot collide with Playback,
        // which is the term that owns "reproducción".
        assert_eq!(STRINGS.text(TextKey::Playout), "Playout");
        assert_eq!(STRINGS.text(TextKey::BlockPlayout), "Playout por bloques");
        assert_eq!(STRINGS.text(TextKey::PlayoutMode), "Modo del playout");
        assert_eq!(STRINGS.text(TextKey::Playback), "Reproducción");
        assert_eq!(STRINGS.text(TextKey::PlaybackOrder), "Orden de reproducción");
        assert_eq!(STRINGS.text(TextKey::StreamingMode), "Modo de emisión");
    }

    #[test]
    fn backend_error_guards_return_localized_text() {
        assert_eq!(
            STRINGS.scheduling_content_error_message("Block from 1 intersects block from 2"),
            "El bloque se solapa con otro bloque."
        );
        assert_eq!(
            STRINGS.scheduling_content_error_message("TemplateGroup 3 does not exist."),
            "El grupo de plantillas no existe."
        );
        assert_eq!(
            STRINGS.scheduling_content_error_message("Block name \"x\" is invalid"),
            "El nombre del bloque debe tener 50 caracteres o menos."
        );
    }

    #[test]
    fn episode_display_subtitles_are_localized() {
        assert_eq!(
            STRINGS.episode_display_subtitle(Some("Serie"), Some(2), Some(5)),
            Some("Serie - temporada 2, episodio 5".to_owned())
        );
        assert_eq!(
            STRINGS.episode_display_subtitle(None, Some(2), Some(5)),
            Some("temporada 2, episodio 5".to_owned())
        );
    }

    #[test]
    fn unknown_server_errors_pass_through_unchanged() {
        let error = "opaque backend detail";
        assert_eq!(STRINGS.settings_error_message(error), error);
        assert_eq!(STRINGS.media_browser_error_message(error), error);
    }

    #[test]
    fn stable_server_errors_are_localized() {
        assert_eq!(
            STRINGS.ffmpeg_profile_error_message("Source profile not found."),
            "No se ha encontrado el perfil de origen."
        );
        assert_eq!(
            STRINGS.schedule_error_message("Schedule name is required!"),
            "El nombre de la programación es obligatorio."
        );
    }
}
