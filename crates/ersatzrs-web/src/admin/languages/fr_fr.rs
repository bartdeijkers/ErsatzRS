use crate::admin::i18n::{LanguageStrings, TextKey};

pub const SORT_ORDER: u16 = 50;
pub const CODE: &str = "fr-fr";
pub const LABEL: &str = "français (France)";
pub const ALIASES: &[&str] = &["fr", "fr-fr", "fr_fr"];

pub struct FrenchStrings;

pub static STRINGS: FrenchStrings = FrenchStrings;

fn episode_display_subtitle(
    show_title: Option<&str>,
    season_number: Option<i64>,
    episode_number: Option<i64>,
) -> Option<String> {
    let show = show_title.unwrap_or("").trim();
    match (season_number, episode_number) {
        (Some(season), Some(episode)) if !show.is_empty() => {
            Some(format!("{show} - S{season:02}E{episode:02}"))
        }
        (Some(season), Some(episode)) => Some(format!("S{season:02}E{episode:02}")),
        _ => (!show.is_empty()).then(|| show.to_owned()),
    }
}

impl LanguageStrings for FrenchStrings {
    fn text(&self, key: TextKey) -> &'static str {
        text(key)
    }

    fn selected_count(&self, count: usize) -> String {
        format!("{count} sélectionné(s)")
    }

    fn item_count(&self, count: u64) -> String {
        if count == 1 {
            "1 élément".to_owned()
        } else {
            format!("{count} éléments")
        }
    }

    fn range_of(&self, start: u32, end: u32, total: u64) -> String {
        format!("{start}-{end} sur {total}")
    }

    fn match_status(&self, matched: u64, total: u64) -> String {
        format!("{matched} sur {total}")
    }

    fn rows_per_page(&self, page_size: u32) -> String {
        format!("Lignes par page : {page_size}")
    }

    fn page_of(&self, page: u32, total: u32) -> String {
        format!("Page {page} sur {total}")
    }

    fn save_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Échec de l'enregistrement : {error}")
    }

    fn delete_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Échec de la suppression : {error}")
    }

    fn empty_trash_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Échec du vidage de la corbeille : {error}")
    }

    fn search_count(&self, total: u32, label: &'static str) -> String {
        format!("{total} {label}")
    }

    fn selected_items_count(&self, count: usize) -> String {
        if count == 1 {
            "1 élément sélectionné".to_owned()
        } else {
            format!("{count} éléments sélectionnés")
        }
    }

    fn delete_selected_count(&self, count: usize) -> String {
        format!("Supprimer la sélection ({count})")
    }

    fn deleted_items_from_database(&self, count: u64) -> String {
        if count == 1 {
            "1 élément supprimé de la base de données.".to_owned()
        } else {
            format!("{count} éléments supprimés de la base de données.")
        }
    }

    fn season_display_name(&self, number: i64) -> String {
        if number == 0 {
            "Épisodes spéciaux".to_owned()
        } else {
            format!("Saison {number}")
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
        format!("Élément multimédia #{id}")
    }

    fn schedule_item_fallback_label(&self, id: i64) -> String {
        format!("Élément de grille #{id}")
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
            1 => "MPEG-TS (hérité)".to_owned(),
            2 => "HLS direct".to_owned(),
            4 => "Segmenteur HLS".to_owned(),
            5 => "MPEG-TS".to_owned(),
            _ => format!("Inconnu ({mode})"),
        }
    }

    fn channel_language_label(&self, code: &str) -> String {
        match code.trim().to_ascii_lowercase().as_str() {
            "" => String::new(),
            "eng" | "en" => "Anglais".to_owned(),
            "nld" | "dut" | "nl" => "Néerlandais".to_owned(),
            "deu" | "ger" | "de" => "Allemand".to_owned(),
            "fra" | "fre" | "fr" => "Français".to_owned(),
            "spa" | "es" => "Espagnol".to_owned(),
            "ita" | "it" => "Italien".to_owned(),
            "jpn" | "ja" => "Japonais".to_owned(),
            "kor" | "ko" => "Coréen".to_owned(),
            "zho" | "chi" | "zh" => "Chinois".to_owned(),
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
            _ => "Inconnu",
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
            format!("{label} (non disponible sur ce système)")
        }
    }

    fn ffmpeg_tonemap_algorithm_label(&self, algorithm: i32) -> String {
        match algorithm {
            0 => "Linéaire",
            1 => "Écrêtage",
            2 => "Gamma",
            3 => "Reinhard",
            4 => "Mobius",
            5 => "Hable",
            _ => return format!("Inconnu ({algorithm})"),
        }
        .to_owned()
    }

    fn ffmpeg_output_dynamic_range_label(&self, range: i32) -> String {
        match range {
            0 => "SDR (BT.709)",
            1 => "HDR10 (BT.2020 PQ)",
            _ => return format!("Inconnu ({range})"),
        }
        .to_owned()
    }

    fn ffmpeg_loudness_mode_label(&self, mode: i32) -> String {
        match mode {
            0 => text(TextKey::Off),
            1 => "loudnorm",
            _ => return format!("Inconnu ({mode})"),
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
        format!("Corbeille vidée : {count} éléments supprimés.")
    }

    fn select_collection_to_add_selected_items(&self, count: usize) -> String {
        format!("Sélectionnez la collection où ajouter les {count} éléments sélectionnés")
    }

    fn select_playlist_to_add_selected_items(&self, count: usize) -> String {
        format!("Sélectionnez la liste de lecture où ajouter les {count} éléments sélectionnés")
    }

    fn added_items_to_collection(&self, count: usize) -> String {
        format!("{count} éléments ajoutés à la collection")
    }

    fn added_items_to_playlist(&self, count: usize) -> String {
        format!("{count} éléments ajoutés à la liste de lecture")
    }

    fn add_to_collection_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Échec de l'ajout à la collection : {error}")
    }

    fn add_to_playlist_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Échec de l'ajout à la liste de lecture : {error}")
    }

    fn no_media_source_configured(&self, provider: &'static str) -> String {
        format!(
            "Aucune source {provider} configurée. Cliquez sur Configurer la connexion pour en ajouter une."
        )
    }

    fn provider_connection_title(&self, provider: &'static str) -> String {
        format!("Connexion {provider}")
    }

    fn provider_libraries_title(&self, provider: &'static str) -> String {
        format!("Bibliothèques {provider}")
    }

    fn provider_path_replacements_title(&self, provider: &'static str) -> String {
        format!("Remplacements de chemins {provider}")
    }

    fn provider_path_label(&self, provider: &'static str) -> String {
        format!("Chemin {provider}")
    }

    fn health_summary(&self, errors: u32, warnings: u32) -> String {
        format!("{errors} erreur(s), {warnings} avertissement(s)")
    }

    fn classic_playout_state_message(&self, playouts: &str) -> String {
        format!(
            "L’état de la diffusion classique ne permet pas une actualisation sûre pour : {playouts}. Utilisez Réinitialiser la diffusion pour un état ancien ou incompatible ; l’actualisation n’efface jamais une fenêtre sans point de contrôle complet."
        )
    }

    fn scan_refresh_state_message(
        &self,
        unresolved_resolutions: u64,
        stale_resolutions: u64,
        failed_intents: u64,
        stale_intents: u64,
    ) -> String {
        format!(
            "L’état de l’actualisation après analyse nécessite une intervention : {unresolved_resolutions} résolution(s) d’impact non terminée(s), {stale_resolutions} résolution(s) d’impact bloquée(s), {failed_intents} demande(s) d’actualisation de diffusion en échec et {stale_intents} demande(s) d’actualisation de diffusion bloquée(s). Consultez les journaux de l’analyseur et de la planification, puis relancez l’analyse ou l’actualisation concernée."
        )
    }

    fn hls_live_delivery_message(
        &self,
        recent: u64,
        minutes: u64,
        active: u64,
        fallbacks: u64,
        fallback_hours: u64,
    ) -> String {
        format!(
            "{recent} chaîne(s) ont fourni le média HLS en direct plus lentement que le temps réel au cours des {minutes} dernières minutes ; {active} n'avaient pas récupéré lors de la dernière observation ; {fallbacks} chaîne(s) utilisent un repli vérifié vers l'encodage logiciel et/ou une qualité inférieure pendant au plus {fallback_hours} heures. Consultez les journaux de diffusion et exécutez le test de lecture TSDuck sensible au débit avant de modifier les paramètres du fournisseur."
        )
    }

    fn artwork_cardinality_message(
        &self,
        duplicate_rows: [i64; 3],
        projection_counts: [i64; 5],
    ) -> String {
        let [show_duplicate_rows, season_duplicate_rows, episode_duplicate_rows] = duplicate_rows;
        let [published_items, projection_rows, distinct_episodes, complete_hierarchy_rows, hierarchy_episodes] =
            projection_counts;
        format!(
            "La cardinalité des illustrations nécessite une intervention : {show_duplicate_rows} ligne(s) d’illustration de série en double, {season_duplicate_rows} ligne(s) d’illustration de saison en double et {episode_duplicate_rows} ligne(s) d’illustration d’épisode en double ; les décomptes de la projection télévisuelle sont {published_items} élément(s) d’épisode publié(s) dans les listes d’extensions, {projection_rows} ligne(s) de projection, {distinct_episodes} épisode(s) projeté(s) distinct(s), {complete_hierarchy_rows} projection(s) de hiérarchie complète(s) et {hierarchy_episodes} épisode(s) dans la hiérarchie. Actualisez ou republiez les listes télévisées d’extensions concernées."
        )
    }

    fn unable_to_load_playback_troubleshooting(&self, error: &dyn std::fmt::Display) -> String {
        format!("Impossible de charger le contexte de dépannage de lecture : {error}")
    }

    fn playout_reset_prompt(&self, channel: &str, schedule: &str) -> String {
        format!(
            "Saisissez l'heure à laquelle la diffusion de la chaîne {channel} avec la grille {schedule} doit être réinitialisée chaque jour"
        )
    }
}

pub fn text(key: TextKey) -> &'static str {
    match key {
        TextKey::ItemDetailsNotRequested => "Détails non récupérés",
        TextKey::ItemDetailsFailed => "Échec de récupération des détails",
        TextKey::ItemDetailsRetry => "Détails en attente d’une nouvelle tentative",
        TextKey::ItemProviderPlayback => "Lecture du fournisseur",
        TextKey::ItemPosition => "Position",
        TextKey::ItemPrevious => "Précédent",
        TextKey::ItemNext => "Suivant",
        TextKey::ItemBack => "Retour aux éléments",
        TextKey::ItemTools => "Outils de l’élément",
        TextKey::ItemEpisodePosition => "Position de l’épisode",
        TextKey::ItemNumbered => "Saison et épisode",
        TextKey::ItemUnnumberedHelp => "Cet élément n’aura pas de numéro de saison ou d’épisode après l’enregistrement.",
        TextKey::ItemNumberingHelp => "Un élément numéroté exige les deux valeurs. La saison 0 est réservée aux épisodes spéciaux.",
        TextKey::ItemMetadata => "Résumé, genres et étiquettes",
        TextKey::ItemMetadataHelp => "Autres titres, langues et crédits",
        TextKey::ItemSaveHelp => "Enregistrez vos modifications pour les appliquer automatiquement.",
        TextKey::ItemEditNumbering => "Modifier la numérotation dans la liste",
        TextKey::ItemReplacementHelp => "Choisir ou retirer un remplacement modifie ce brouillon. Utilisez Enregistrer l’élément pour le conserver.",
        TextKey::ItemReplacementPending => "Le changement de remplacement n’est pas enregistré.",
        TextKey::ItemSearchReplacement => "Rechercher des médias locaux",
        TextKey::ItemRemoveReplacement => "Retirer le remplacement",
        TextKey::ItemInformation => "Informations et provenance du média",
        TextKey::ItemInformationHelp => "Audio, sous-titres, détails du flux et options du fournisseur",
        TextKey::ItemSavedToolsHelp => "Ces actions utilisent l’élément enregistré. Les modifications non enregistrées sont exclues.",
        TextKey::ItemViewMedia => "Voir les détails du média",
        TextKey::ItemSavedValue => "Valeur enregistrée",
        TextKey::AddonWorkspaceSelectFirst => "Sélectionnez d’abord les éléments à modifier.",
        TextKey::AddonsNoneInstalled => "Aucun module n’est encore installé.",
        TextKey::AddonsNoneAvailable => "Vos sources ne proposent encore aucun module.",
        TextKey::AddonsSetupHelp => "Ouvrez Sources pour ajouter un dépôt ou installer depuis un fichier.",
        TextKey::AddonPackageHelp => "Utilisez cette ligne pour un ZIP contenant un seul module.",
        TextKey::AddonRepositoryPackageHelp => "Utilisez cette ligne pour un ZIP de dépôt, tel que ErsatzRS-addons.zip, contenant un catalogue de modules.",
        TextKey::AddonPackageUploadHelp => "Impossible d’installer ce module. Pour un ZIP de dépôt, utilisez la ligne Paquet de dépôt ci-dessous.",
        TextKey::AddonRepositoryUploadHelp => "Impossible d’ajouter ce dépôt. Pour un ZIP contenant un seul module, utilisez la ligne Paquet de module ci-dessus.",
        TextKey::AddonConfigurationSavedRecheck => "Enregistré. Le module est désactivé jusqu’à sa nouvelle vérification et activation.",
        TextKey::AddonCheckRequired => "Configuration modifiée. Vérifiez à nouveau le module avant de l’activer.",
        TextKey::AddonCheckPending => "Vérification du module…",
        TextKey::AddonWorkspaceIncludeSelected => "Inclure les éléments sélectionnés",
        TextKey::AddonWorkspaceExcludeSelected => "Exclure les éléments sélectionnés",
        TextKey::AddonWorkspaceInclusionHelp => "Enregistrez les changements de sélection pour les appliquer automatiquement.",
        TextKey::AddonWorkspacePublicationFailed => "Échec de la publication",
        TextKey::AddonWorkspacePublicationRestoredHelp => "La version précédente a été restaurée. Les modifications enregistrées sont conservées et réessayées automatiquement. Les erreurs techniques figurent dans Activité.",

        TextKey::AddonWorkspaceParentMetadataPreserved => "Le traitement est arrêté pour préserver les métadonnées NFO parentes modifiées ou invalides. Corrigez le fichier avant de réessayer.",

        TextKey::AddonWorkspaceImporting => "Importation de la liste…",
        TextKey::AddonWorkspaceImportingHelp => "La liste s’ouvrira automatiquement après la découverte. Sans JavaScript, utilisez Actualiser.",
        TextKey::AddonWorkspaceImportSteps => "Importez la liste. Les métadonnées et les médias nécessaires sont traités et publiés automatiquement.",
        TextKey::AddonWorkspaceChooseProvider => "Choisissez un module ci-dessous si aucun n’est détecté pour cette URL.",
        TextKey::AddonWorkspaceExcluded => "Exclu de la publication",
        TextKey::ItemFromList => "De la liste",
        TextKey::ItemUseListSetting => "Utiliser le réglage de la liste",
        TextKey::ItemOverride => "Valeur propre à l’élément",
        TextKey::ItemPackageDefault => "Valeur du paquet ; pas encore activée",
        TextKey::ItemPreviewOptions => "Aperçu des modifications d’options",
        TextKey::ItemAccessActive => "Une vérification d’accès est déjà en cours.",
        TextKey::OpenItemInNewTab => "Ouvrir dans un nouvel onglet",
        TextKey::AddonWorkspaceLengthFilter => "Filtrer par durée",
        TextKey::AddonWorkspaceMinimumLength => "Durée minimale",
        TextKey::AddonWorkspaceMaximumLength => "Durée maximale",
        TextKey::AddonWorkspaceUnknownLength => "Inclure les durées inconnues",
        TextKey::AddonWorkspaceLengthHelp => "Minutes:secondes. Laisser vide pour ne pas limiter.",
        TextKey::AddonWorkspaceReviewLatest => "Vérifier la dernière version enregistrée",
        TextKey::AddonWorkspaceReviewRetained => "Votre saisie est conservée. Comparez-la aux dernières valeurs enregistrées avant de sauvegarder. Vérifiez séparément les autres tâches ouvertes.",
        TextKey::AddonWorkspaceStreamHelp => "Lire directement depuis la source.",
        TextKey::AddonWorkspaceDownloadHelp => "Préparer chaque vidéo avant sa lecture.",
        TextKey::AddonWorkspaceNoOptions => "Cette extension ne propose aucun réglage d’accès ou de segments.",
        TextKey::AddonWorkspaceNewHelp => "Télécharger les nouvelles vidéos importées ; diffuser les vidéos existantes.",
        TextKey::AddonWorkspaceSizeLimit => "Ce formulaire est trop volumineux. Revenez à votre saisie et enregistrez une tâche plus petite.",
        TextKey::AddonWorkspaceItems => "Éléments",
        TextKey::AddonWorkspaceSettings => "Paramètres de la liste",
        TextKey::AddonWorkspacePlayback => "Lecture et accès",
        TextKey::AddonWorkspaceActivity => "Activité et récupération",
        TextKey::AddonWorkspaceSections => "Sections de la liste",
        TextKey::AddonWorkspaceSaveSettings => "Enregistrer les paramètres",
        TextKey::AddonWorkspaceSaveItem => "Enregistrer l’élément",
        TextKey::AddonWorkspaceSaveNumbering => "Enregistrer la numérotation",
        TextKey::AddonWorkspacePublish => "Publier les modifications enregistrées",
        TextKey::AddonWorkspacePublished => "La liste publiée est à jour",
        TextKey::AddonWorkspacePending => "Les modifications enregistrées sont appliquées automatiquement",
        TextKey::AddonWorkspacePreparationRequired => "Les médias nécessaires sont préparés automatiquement",
        TextKey::AddonWorkspacePreparing => "Préparation des médias pour la publication",
        TextKey::AddonWorkspaceAddIncomplete => "La liste a été ajoutée, mais certains paramètres n’ont pas été enregistrés. Ouvrez Paramètres de la liste, vérifiez-les et enregistrez à nouveau.",
        TextKey::AddonWorkspacePreparationFailed => "La préparation des médias a échoué",
        TextKey::RetryPreparation => "Réessayer la préparation",
        TextKey::PreparationFailedItems => "Tâches de préparation",
        TextKey::PreparationFailureFailed => "L’extension n’a pas pu préparer les médias. Réessayez la préparation et vérifiez l’extension si l’échec persiste.",
        TextKey::PreparationFailureInputChanged => "La demande de préparation ne correspondait plus à la liste enregistrée ni à l’extension installée. Réessayez la préparation.",
        TextKey::PreparationFailureInvalidRequest => "Les options enregistrées ne correspondent pas à l’extension installée. Vérifiez Lecture et accès, enregistrez, puis réessayez la préparation.",
        TextKey::PreparationFailureCancelled => "La préparation a été annulée. Réessayez la préparation pour recommencer.",
        TextKey::PreparationFailureAttemptsExhausted => "La préparation s’est arrêtée après plusieurs tentatives échouées. Réessayez la préparation et vérifiez l’extension si l’échec persiste.",
        TextKey::PreparationFailureStorageFull => "L’espace de stockage était insuffisant pour préparer les médias. Libérez de l’espace, puis réessayez la préparation.",
        TextKey::PreparationFailureUnsupported => "L’extension ne peut pas préparer ces médias.",
        TextKey::PreviousSavedRevision => "Révision enregistrée précédente",
        TextKey::AddonWorkspacePreparationActivity => "Préparation des médias",
        TextKey::AddonWorkspaceBulk => "Modifier plusieurs éléments",
        TextKey::AddonWorkspaceSelection => "Sélectionnés pour modification",
        TextKey::AddonWorkspaceOffPage => "Sur d’autres pages",
        TextKey::AddonWorkspaceDiscard => "Abandonner les modifications",
        TextKey::AddonWorkspaceSelected => "Éléments sélectionnés",
        TextKey::AddonWorkspaceMatching => "Tous les éléments correspondants",
        TextKey::AddonWorkspacePage => "Page actuelle",
        TextKey::AddonWorkspaceAccess => "Accès et segments",
        TextKey::AddonWorkspaceSaveConflict => "Les paramètres enregistrés ont changé. Vérifiez la version actuelle avant de sauvegarder.",
        TextKey::AddonWorkspaceSaveInvalid => "Ces paramètres ne sont pas valides. Vérifiez votre saisie et réessayez.",
        TextKey::AddonWorkspaceSaveStorage => "Les paramètres n’ont pas pu être enregistrés. Votre saisie est conservée ; réessayez.",
        TextKey::AddonWorkspaceLoadIncomplete => "Certaines informations de l’éditeur n’ont pas pu être chargées de manière cohérente. Votre saisie est conservée. Vérifiez la version actuelle pour réessayer.",
        TextKey::TechnicalDetails => "Détails techniques",
        TextKey::AddonPermissionNetworkHelp => "Se connecter aux hôtes réseau indiqués",
        TextKey::AddonPermissionMediaHelp => "Lire les médias disponibles pour ErsatzRS",
        TextKey::AddonPermissionFfmpegHelp => "Exécuter la version gérée de FFmpeg",
        TextKey::AddonPermissionCommandHelp => "Exécuter les commandes externes indiquées",
        TextKey::PreparationTasks => "Tâches de préparation",
        TextKey::PreparationAffectedItems => "Éléments concernés",
        TextKey::PreparationIntervals => "Tâches d’intervalles / SponsorBlock",
        TextKey::PreparationDownloads => "Tâches de téléchargement",
        TextKey::PreparationSubtitles => "Tâches de sous-titres",
        TextKey::PreparationQueued => "En file d’attente",
        TextKey::PreparationRunning => "En cours",
        TextKey::PreparationWaiting => "En attente",
        TextKey::PreparationNotQueued => "Non planifiées",
        TextKey::LastActivity => "Dernière activité",
        TextKey::AddonAccessUnknownFailure => "La cause de cet échec d’accès est inconnue. Vérifiez les paramètres d’accès enregistrés, testez la source dans votre navigateur, puis relancez la vérification.",
        TextKey::AddonWorkspaceRetained => "Impossible d’enregistrer les modifications. Votre saisie est conservée ; vérifiez les erreurs ou rechargez la révision actuelle.",
        TextKey::AddonWorkspaceNavigation => "La navigation conserve les modifications non enregistrées. Les modifications enregistrées sont appliquées automatiquement.",
        TextKey::AddonWorkspaceSeasonOnly => "Modifier uniquement la saison",
        TextKey::AddonWorkspaceSeasonPerYear => "Nouvelle saison par année civile",
        TextKey::AddonWorkspacePageSize => "Éléments par page",

        TextKey::AddonAccessTest => "Tester l’accès",
        TextKey::AddonAccessHelp => {
            "Vérifiez cet élément avec les options enregistrées du brouillon. Les modifications non enregistrées sont exclues ; la lecture publiée reste inchangée."
        }
        TextKey::AddonAccessSaveFirst => {
            "Enregistrez d’abord une option pour activer les paramètres du brouillon avant de tester l’accès."
        }
        TextKey::AddonAccessError => {
            "Impossible de charger ou de modifier le contrôle d’accès. Actualisez l’élément et réessayez."
        }
        TextKey::AddonAccessNotChecked => "Non vérifié",
        TextKey::AddonAccessQueued => "Contrôle d’accès en attente",
        TextKey::AddonAccessRunning => "Vérification de l’accès",
        TextKey::AddonAccessWaiting => "En attente d’une nouvelle tentative",
        TextKey::AddonAccessCancelled => "Contrôle d’accès annulé",
        TextKey::AddonAccessStale => "Résultat d’accès expiré ou paramètres modifiés",
        TextKey::AddonAccessAccessible => "Accessible",
        TextKey::AddonAccessProfileUnavailable => "Le profil de navigateur sélectionné est inaccessible au serveur. Vérifiez le profil enregistré, puis relancez la vérification de l’accès.",
        TextKey::AddonAccessProfileLocked => "Le profil du navigateur est verrouillé. Fermez le navigateur qui utilise ce profil, puis relancez la vérification de l’accès.",
        TextKey::AddonAccessDecryptionFailed => "Le serveur n’a pas pu déchiffrer les identifiants du navigateur. Vérifiez le profil et le compte qui exécute le serveur, puis réessayez.",
        TextKey::AddonDerivedMetadataLimited => "Les métadonnées de la série ont atteint leur taille maximale. Celles des épisodes restent complètes.",
        TextKey::AddonLocalPlaybackAvailable => "Copie locale lisible",
        TextKey::AddonAccessRestricted => "Le fournisseur a restreint l’accès. Ouvrez la source dans votre navigateur pour vérifier les restrictions de compte ou de région, puis réessayez.",
        TextKey::AddonAccessNextAttempt => "Prochaine tentative :",
        TextKey::AddonAccessExpires => "Expiration du résultat :",
        TextKey::AddonAccessRefresh => "Actualiser l’état",
        TextKey::AddonOptionsCurrentFilter => "Filtre actuel",
        TextKey::AddonOptions => "Options du fournisseur",
        TextKey::AddonOptionsDraftHelp => "Les options enregistrées sont appliquées automatiquement. La lecture actuelle reste disponible pendant le traitement.",
        TextKey::AddonOptionsProvisional => {
            "Ces valeurs par défaut du paquet sont provisoires. Enregistrer une option les active pour ce brouillon."
        }
        TextKey::AddonOptionsListDefault => "Valeur par défaut de la liste",
        TextKey::AddonOptionsInherited => "Héritée",
        TextKey::AddonOptionsOverride => "Personnalisée",
        TextKey::AddonOptionsFiniteOnly => {
            "Indisponible pour les éléments en direct ou de durée inconnue : cette option nécessite un média de durée finie. La lecture directe reste disponible."
        }
        TextKey::AddonOptionsResetPackage => "Rétablir la valeur du paquet",
        TextKey::AddonOptionsResetList => "Rétablir la valeur de la liste",
        TextKey::AddonOptionsMixed => "Valeurs différentes — choisissez une valeur",
        TextKey::AddonOptionsPreviewHelp => {
            "Vérifiez ces éléments précis avant d’appliquer. Une modification intermédiaire du brouillon ou du schéma nécessite un nouvel aperçu."
        }
        TextKey::AddonOptionsBefore => "Avant",
        TextKey::AddonOptionsAfter => "Après",
        TextKey::AddonOptionsConflict => {
            "Impossible d’enregistrer ou de charger les options. Le brouillon, la sélection ou le schéma du paquet a peut-être changé. Rechargez l’éditeur et réessayez."
        }
        TextKey::AddonOptionsNone => "Ce paquet ne définit aucune option.",
        TextKey::Addon => "Extension",
        TextKey::PrimaryFeature => "Film principal",
        TextKey::MovieExtra => "Bonus du film",
        TextKey::NeedReview => "Avis",

        TextKey::ReviewRecommended => "Vérification recommandée",
        TextKey::PublicationReadiness => "État de préparation à la publication",
        TextKey::PublicationBlockers => "Blocages",
        TextKey::PublicationActivity => "Activité",
        TextKey::PublicationJobs => "tâches de publication",
        TextKey::PublicationEvents => "événements",
        TextKey::NoPublicationActivity => {
            "Aucune activité de publication n'a encore été enregistrée."
        }
        TextKey::CopyRedactedDiagnostics => "Copier le diagnostic expurgé",
        TextKey::DownloadRedactedDiagnostics => "Télécharger le diagnostic expurgé",
        TextKey::RedactedDiagnosticsCopied => "Diagnostic expurgé copié.",
        TextKey::PublicationGeneration => "Génération",
        TextKey::CorrelationId => "ID de corrélation",
        TextKey::TargetPublishedRevision => "Révision publiée cible",
        TextKey::RelatedOperation => "Opération associée",
        TextKey::FilesystemTransition => "Transition du système de fichiers",
        TextKey::RetryClass => "Classe de nouvelle tentative",
        TextKey::RecoveryAction => "Action de récupération",
        TextKey::OperatingSystemErrorCode => "Code d'erreur du système d'exploitation",
        TextKey::PublicationOutcome => "Résultat",
        TextKey::PublicationExpired => "Expirée",
        TextKey::EventExistingJobAdopted => "Publication existante reprise",
        TextKey::EventEnqueued => "Publication mise en file d'attente",
        TextKey::EventLeaseWaitStarted => "Attente de l'accès à la bibliothèque",
        TextKey::EventLeaseAcquired => "Accès à la bibliothèque obtenu",
        TextKey::EventLeaseReclaimed => {
            "Publication reprise après une interruption ou un bail expiré"
        }
        TextKey::EventPhaseAdvanced => "Phase de publication avancée",
        TextKey::EventChildScanStarted => "Analyse de vérification démarrée",
        TextKey::EventChildScanCompleted => "Analyse de vérification terminée",
        TextKey::EventChildScanFailed => "Échec de l'analyse de vérification",
        TextKey::EventOriginalFailure => "Échec initial de publication",
        TextKey::EventRecoveryFailure => "Échec de récupération",
        TextKey::EventRetryScheduled => "Nouvelle tentative de récupération planifiée",
        TextKey::EventRecoveryRequested => "Récupération demandée",
        TextKey::EventCompleted => "Publication terminée",
        TextKey::EventHistoryTruncated => "L'activité plus ancienne a été tronquée",
        TextKey::TriggerAutomaticRefresh => "Actualisation planifiée",
        TextKey::TriggerOperator => "Demandée par un opérateur",
        TextKey::TriggerRestoreRevision => "Demandée par la restauration d’une révision",
        TextKey::TestSource => "Tester la source",
        TextKey::TestSourceHelp => {
            "Vérifiez la source enregistrée sans importer ni modifier la liste."
        }
        TextKey::SourceTestQueued => "Test de source en attente",
        TextKey::SourceTestRunning => "Test de source en cours…",
        TextKey::SourceTestStale => {
            "Le test est obsolète car ses entrées enregistrées ne correspondent plus à la liste, au paquet d’extension ou aux valeurs par défaut actuels."
        }
        TextKey::SourceAvailable => "La source est disponible",
        TextKey::SourceUnsupported => "La source n’est pas prise en charge",
        TextKey::SourceTestFailed => "Échec du test de source",
        TextKey::SourceRevision => "Révision de la source",
        TextKey::FirstPageComplete => "Première page complète",
        TextKey::ProviderIdentity => "Identité du fournisseur",
        TextKey::ReturnedFirstPageItems => "Éléments renvoyés sur la première page",
        TextKey::ProviderTotalItemCount => "Nombre total d’éléments annoncé par le fournisseur",
        TextKey::DraftChanges => "Modifications du brouillon enregistré",
        TextKey::ExactDraftDiffHelp => {
            "Différences exactes entre le brouillon enregistré et la révision publiée actuelle."
        }
        TextKey::DraftDiffUnavailable => {
            "Une comparaison exacte sera disponible après la publication d’une révision complète."
        }
        TextKey::MetadataChanges => "Modifications des métadonnées",
        TextKey::ExcludedItems => "Éléments exclus",
        TextKey::Conflicts => "Conflits",
        TextKey::PendingRemovals => "Suppressions en attente",
        TextKey::PendingRemovalHelp => {
            "Absent une fois lors d’un rapprochement complet. Confirmez la suppression maintenant ou attendez une seconde observation complète au moins 24 heures plus tard."
        }
        TextKey::Reappearances => "Réapparitions",
        TextKey::RemovalCandidates => "Candidats à la suppression",
        TextKey::ConfirmRemoval => "Confirmer la suppression",
        TextKey::RemovalConfirmed => "Suppression confirmée.",
        TextKey::RemovalConfirmationFailed => "Échec de la confirmation de la suppression.",
        TextKey::RevisionHistory => "Historique des révisions",
        TextKey::RevisionHistoryHelp => {
            "La restauration remplace le brouillon actuel et publie l’instantané choisi comme nouvelle révision."
        }
        TextKey::NoRevisionHistory => "Aucune révision publiée n’est encore conservée.",
        TextKey::Revision => "Révision",
        TextKey::CompareRevisions => "Comparer les révisions",
        TextKey::RestoreRevision => "Restaurer la révision",
        TextKey::RestoreRevisionWarning => {
            "Ceci remplace le brouillon actuel et publie la révision"
        }
        TextKey::RestoreRevisionQueued => "Restauration de révision en attente",
        TextKey::RestoreRevisionFailed => "Impossible de restaurer la révision.",
        TextKey::PublishedItems => "Éléments publiés",
        TextKey::GeneratedPlaylists => "Listes de lecture générées",
        TextKey::ActiveOrFuturePlayouts => "Diffusions actives ou futures",
        TextKey::EffectiveOptions => "Options effectives",
        TextKey::Files => "Fichiers",
        TextKey::Artifacts => "Artefacts",
        TextKey::Added => "Ajouté",
        TextKey::Removed => "Supprimé",
        TextKey::Changed => "Modifié",
        TextKey::FilesystemPreviousBackedUp => "Révision précédente sauvegardée",
        TextKey::FilesystemCandidateCommitted => "Révision candidate validée",
        TextKey::FilesystemPreviousRestored => "Révision précédente restaurée",
        TextKey::FilesystemProjectionRestored => "Projection de base de données restaurée",
        TextKey::FilesystemRevisionVerified => "Révision vérifiée",
        TextKey::FilesystemCleanupCompleted => "Nettoyage de publication terminé",
        TextKey::RetryTransientRecovery => "Récupération transitoire",
        TextKey::ErrorFilesystemIo => "E/S du système de fichiers",
        TextKey::ErrorPermissionDenied => "Autorisation refusée",
        TextKey::ErrorSharingViolation => "Violation de partage de fichier",
        TextKey::ErrorUnsafeFilesystemState => "État non sûr du système de fichiers",
        TextKey::ErrorInvalidPublicationMarker => "Marqueur de publication non valide",
        TextKey::ErrorPublicationIntegrity => "Contrôle d'intégrité de publication",
        TextKey::OutcomePublished => "Publiée",
        TextKey::OutcomeRestoredAfterFailure => "Révision précédente restaurée après l'échec",
        TextKey::ReviewRecommendations => "Recommandations de vérification",
        TextKey::OptionalDetails => "Détails facultatifs",
        TextKey::DraftSaved => "Brouillon enregistré",
        TextKey::SavedRevision => "Révision enregistrée",
        TextKey::PublishList => "Publier la liste",
        TextKey::PreparePublication => "Préparer la publication",
        TextKey::ProcessedMediaRequired => "Tâches de préparation",
        TextKey::ProcessedMediaQueued => "Préparation des médias mise en file d’attente",
        TextKey::PreparingProcessedMedia => "Préparation des fichiers multimédias",
        TextKey::PreparingPublication => "Préparation de la publication",
        TextKey::Publishing => "Publication…",
        TextKey::PublicationQueued => "Publication en attente",
        TextKey::RestoringPublication => "Restauration de la révision précédente…",
        TextKey::Published => "Publié",
        TextKey::PublishSavedRevisionHelp => {
            "La publication utilise exactement la révision enregistrée. Enregistrez d’abord les modifications de la liste."
        }
        TextKey::ChangesSavedImmediately => {
            "Les modifications d’éléments et groupées sont enregistrées immédiatement et peuvent être annulées."
        }
        TextKey::ReviewItem => "Vérifier l’élément",
        TextKey::ShowFullDescription => "Afficher la description complète",
        TextKey::ShowLess => "Afficher moins",
        TextKey::ImportAList => "Importer une liste",
        TextKey::ImportOverview => "Importer l’aperçu",
        TextKey::AddonContextSelected => "Extension sélectionnée",
        TextKey::CompletedIn => "Terminé en",
        TextKey::NoPublicationBlockers => "Aucun blocage de publication.",
        TextKey::BlockerOverviewIncomplete => "L’aperçu complet doit être vérifié.",
        TextKey::BlockerPublicationInProgress => "La publication est déjà en cours.",
        TextKey::BlockerRecoveryRequired => {
            "Une publication précédente doit encore être restaurée."
        }
        TextKey::BlockerCollectionTitleMissing => "La collection doit avoir un titre.",
        TextKey::BlockerInvalidArtwork => {
            "Les images doivent utiliser une URL HTTPS valide et limitée."
        }
        TextKey::BlockerNoIncludedItems => "Incluez au moins un élément.",
        TextKey::BlockerMoviePrimaryCount => {
            "La publication de films exige exactement un film principal."
        }
        TextKey::BlockerPlaybackDecisionRequired => "Choisissez une lecture finie ou en direct.",
        TextKey::BlockerPlaybackDetailsUnavailable => {
            "Cette extension ne fournit pas les détails de lecture requis."
        }
        TextKey::BlockerPlaybackDetailsFailed => {
            "Les détails du fournisseur ne contiennent pas de durée positive."
        }
        TextKey::BlockerUnavailableWithoutReplacement => {
            "L’élément indisponible nécessite un remplacement local valide."
        }
        TextKey::BlockerItemTitleMissing => "L’élément doit avoir un titre.",
        TextKey::BlockerSeasonEpisodeMissing => "Saisissez une saison et un épisode valides.",
        TextKey::BlockerArtistMissing => "Saisissez au moins un artiste.",
        TextKey::PublishUnsavedChanges => {
            "Enregistrez les modifications avant de publier cette révision."
        }
        TextKey::PublicationBlocked => "La publication est bloquée.",
        TextKey::PublishBlocked => "Publication bloquée",
        TextKey::RetryRecovery => "Réessayer la récupération",
        TextKey::RestorePreviousRevision => "Restaurer la révision précédente",
        TextKey::PublicationPhase => "Phase de publication",
        TextKey::PublicationAttempts => "Tentatives",
        TextKey::LastProgress => "Dernière progression",
        TextKey::FailureReason => "Motif initial",
        TextKey::RestrictedItemsExcluded => "Éléments restreints exclus de cette révision",
        TextKey::RecoveryQueued => "Récupération mise en attente",
        TextKey::RecoveryFailed => "La récupération n’a pas pu être mise en attente.",
        TextKey::PhaseAwaitingLease => "En attente de l’accès à la médiathèque",
        TextKey::PhaseStaging => "Préparation de la révision",
        TextKey::PhaseScanning => "Analyse de la révision préparée",
        TextKey::PhaseCommitting => "Validation de la révision",
        TextKey::PhaseRestoringFiles => "Restauration des fichiers",
        TextKey::PhaseRestoringProjection => "Restauration de la projection de la base de données",
        TextKey::PhaseReconciling => "Rapprochement de la révision restaurée",
        TextKey::PhaseVerifying => "Vérification de la révision",
        TextKey::PhaseCleaningUp => "Nettoyage des fichiers de publication",
        TextKey::PhaseComplete => "Terminée",
        TextKey::InstallLeavesDisabled => {
            "Après l’installation, cette extension reste désactivée jusqu’à son activation."
        }
        TextKey::EnableActivatesPermissions => {
            "L’activation applique les autorisations déclarées de l’extension."
        }
        TextKey::FirstAired => "Première diffusion",
        TextKey::LoadItemDetailsAfterPublishing => "Charger automatiquement les détails",
        TextKey::LoadItemDetailsHelp => {
            "Importez d'abord la vue d'ensemble. Les détails techniques requis pour la lecture sont chargés avant la première publication ; les autres métadonnées détaillées suivent à un rythme contrôlé."
        }
        TextKey::OverviewOnlyManualDetailsHelp => {
            "Importez uniquement la vue d'ensemble et demandez manuellement les détails descriptifs. Les détails techniques requis pour la lecture sont préparés automatiquement avant publication."
        }
        TextKey::PlaybackDetailsQueued => "Détails de lecture mis en file d'attente",
        TextKey::PublishAgainAfterDetails => "Les détails obtenus sont appliqués automatiquement.",
TextKey::FetchDetails => "Récupérer les détails",
        TextKey::DetailsQueued => "Détails en attente",
        TextKey::DetailsLoading => "Chargement des détails",
        TextKey::RetryDetails => "Réessayer les détails",
        TextKey::DetailsComplete => "Détails complets",
        TextKey::DetailsSkipped => "Détails ignorés",
        TextKey::FindingProgrammes => "Recherche des programmes",
        TextKey::CheckingOverview => "Vérification de la vue d'ensemble",
        TextKey::ReadyToReview => "Traitement automatique",

        TextKey::LoadingItemDetails => "Chargement des détails des éléments",
        TextKey::WaitingForProvider => "En attente du fournisseur",
        TextKey::OverviewOnly => "Vue d'ensemble uniquement",
        TextKey::PauseImport => "Suspendre l'importation",
        TextKey::ResumeImport => "Reprendre l'importation",
        TextKey::RetryImportNow => "Réessayer l'importation maintenant",
        TextKey::EmptyImportNeedsRetry => "L'importation vide doit être relancée",
        TextKey::EmptyImportRetryHelp => {
            "Cette importation terminée ne contient aucun élément. Mettez à jour l'extension, puis réessayez ; les paramètres et modifications de la liste seront conservés."
        }
        TextKey::CancelImport => "Annuler l'importation",
        TextKey::OtherPageItemsAffected => {
            "D'autres éléments concernés se trouvent sur d'autres pages"
        }
        TextKey::ElapsedTime => "Temps écoulé",
        TextKey::LastSuccessfulProgress => "Dernière progression réussie",
        TextKey::EstimatedRemainingTime => "Temps restant estimé",
        TextKey::PlaybackBehavior => "Comportement de lecture",
        TextKey::UseProviderValue => "Utiliser la valeur du fournisseur",
        TextKey::FiniteMedia => "Média fini",
        TextKey::LiveSource => "Source en direct",
        TextKey::ProviderValueChanged => "Valeur du fournisseur modifiée",
        TextKey::ApplyToSelected => "Appliquer à la sélection",
        TextKey::ApplyToAllRemoteStreams => "Appliquer à tous les flux distants",
        TextKey::SelectItem => "Sélectionner l’élément",
        TextKey::NumberEpisodes => "Numéroter les épisodes",
        TextKey::NumberEpisodesHelp => {
            "Attribuez une saison et des numéros d’épisode consécutifs dans l’ordre d’origine de l’extension. Les éléments exclus et les fragments de chapitre sont ignorés."
        }
        TextKey::FirstEpisode => "Premier épisode",
        TextKey::NumberToAssign => "Nombre à attribuer",
        TextKey::AllRemaining => "Tous les éléments restants",
        TextKey::ReplaceExistingEpisodeNumbers => "Remplacer les numéros d’épisode existants",
        TextKey::AutoDetectSeasons => "Détecter les saisons selon les dates de première diffusion",
        TextKey::AutoDetectSeasonsHelp => {
            "Une nouvelle saison commence lorsque l’écart entre deux dates connues atteint le minimum configuré. Les éléments sans date restent dans la saison actuelle."
        }
        TextKey::MinimumSeasonGapDays => "Écart minimal entre les saisons (jours)",
        TextKey::NumberingStartsAt => "La numérotation commence à",
        TextKey::FirstIncludedItem => "Premier élément inclus",
        TextKey::StartWithFirstItem => "Commencer au premier élément",
        TextKey::StartHere => "Commencer ici",
        TextKey::SaveSeasonEpisode => "Enregistrer la saison et l’épisode",
        TextKey::BulkEdit => "Modification en masse",
        TextKey::Scope => "Portée",
        TextKey::AllItems => "Tous les éléments",
        TextKey::CurrentPage => "Page actuelle",
        TextKey::ItemsNeedingReview => "Éléments avec avis",

        TextKey::KeepCurrentValue => "Conserver la valeur actuelle",
        TextKey::UndoLastChange => "Annuler la dernière modification",
        TextKey::Undo => "Annuler",
        TextKey::ChangeUndone => "Modification annulée",
        TextKey::MetadataConflicts => "Conflits de métadonnées",
        TextKey::MetadataConflictsHelp => "Le fournisseur a modifié des champs personnalisés. Vos valeurs enregistrées restent utilisées.",
TextKey::PreviousProviderValue => "Ancienne valeur du fournisseur",
        TextKey::NewProviderValue => "Nouvelle valeur du fournisseur",
        TextKey::OperatorValue => "Valeur de l’opérateur",
        TextKey::Discovered => "Détectés",
        TextKey::Examined => "Examinés",
        TextKey::ArchivedSkipped => "Ignorés car archivés",
        TextKey::Emitted => "Émis",
        TextKey::EditMetadata => "Modifier les métadonnées",
        TextKey::OneTagPerLine => "Un tag par ligne",
        TextKey::OriginalTitle => "Titre original",
        TextKey::ReleaseDate => "Date de sortie",
        TextKey::Year => "Année",
        TextKey::SortTitle => "Titre de tri",
        TextKey::MovieFilesAndExtras => "Fichiers du film et bonus",
        TextKey::MovieFilesAndExtrasHelp => {
            "Attribuez exactement un film principal. L’ordre des sources ne détermine pas le rôle : une bande-annonce peut apparaître en premier."
        }
        TextKey::EpisodesMetadataHelp => {
            "Vérifiez l’identité et la numérotation des épisodes ; l’ordre du fournisseur reste inchangé."
        }
        TextKey::MusicVideosMetadataHelp => {
            "L’artiste, l’album et la piste appartiennent à chaque vidéo ; les listes multi-artistes restent valides."
        }
        TextKey::OtherVideosMetadataHelp => {
            "Utilisez ce type pour des vidéos indépendantes plutôt que pour un film avec bonus."
        }
        TextKey::SongsMetadataHelp => {
            "Piste, disque, artiste et album sont modifiés par morceau ; l’ordre de la liste ne remplace pas le numéro de piste."
        }
        TextKey::ImagesMetadataHelp => {
            "La légende et l’auteur sont modifiables ; format, dimensions et identité de source restent factuels."
        }
        TextKey::RemoteStreamsMetadataHelp => {
            "Modifiez ici les métadonnées de présentation ; connexion et protocole restent des faits de la source."
        }
        TextKey::CastAndCredits => "Distribution et équipe",
        TextKey::OneCreditPerLine => "Une personne par ligne : nom | rôle",
        TextKey::Poster => "Affiche",
        TextKey::Artwork => "Illustrations",
        TextKey::PublishMetadata => "Publier",
        TextKey::IncludeItem => "Inclure",
        TextKey::IncludeInPublication => "Inclure dans la publication",
        TextKey::MetadataDraftNotice => "Les métadonnées et sélections enregistrées sont appliquées automatiquement.",
TextKey::ApplyToAllItemsPrompt => {
            "Chaque élément est reclassé avec le type de média sélectionné, remplaçant les valeurs déduites par l'extension ou saisies manuellement."
        }
        TextKey::TelevisionEpisode => "Épisode de télévision",
        TextKey::CollectionTypeAuto => "Déterminer d'après les éléments",
        TextKey::CurrentLibraryType => "Type actuel de la médiathèque",
        TextKey::CollectionTypeChangeHelp => "Enregistrez pour réorganiser automatiquement les éléments existants. Leur ordre, leur sélection et les modifications de métadonnées sont conservés.",
TextKey::NonPlayableItems => "Éléments non lisibles",
        TextKey::UnknownItems => "Éléments inconnus",
        TextKey::PlayableItems => "Éléments lisibles",
        TextKey::ApplyToAllItems => "Appliquer à tous les éléments",
        TextKey::ChangeAddon => "Changer d'extension",
        TextKey::ChangeAddonHelp => {
            "Sélectionnez une autre extension pour réimporter la liste avec celle-ci."
        }
        TextKey::ChangeAddonPrompt => {
            "La liste est réimportée via l'extension sélectionnée. Les métadonnées des éléments, les remplacements et les corrections de chapitres sont de nouveau dérivés du nouveau fournisseur."
        }
        TextKey::AddonAutoDetect => "Déterminer d'après l'URL",
        TextKey::AddonAutoDetected => "Sélectionnée automatiquement d'après l'URL.",
        TextKey::AddonMediaListNoAddonForUrl => {
            "Aucune extension installée ne traite cette URL. Choisissez-en une ou définissez un processeur de flux distant par défaut."
        }
        TextKey::DefaultRemoteStreamAddon => "Processeur de flux distant par défaut",
        TextKey::DefaultRemoteStreamAddonHelp => {
            "Utilisé pour les sources HTTP et HTTPS qu'aucune extension installée ne revendique. L'extension doit toujours être autorisée à contacter l'hôte."
        }
        TextKey::AddonDetailWait => "Rythme des détails des éléments",
        TextKey::MinimumDetailWait => "Attente minimale des détails",
        TextKey::MaximumDetailWait => "Attente maximale des détails",
        TextKey::AddonDetailWaitHelp => {
            "Après chaque tentative de chargement des détails d’un élément, ErsatzRS attend un nouveau nombre aléatoire de secondes dans cette plage globale avant qu’une liste puisse réutiliser le même hôte fournisseur."
        }
        TextKey::AddonDetailWaitInvalid => {
            "Saisissez un nombre entier positif de secondes et vérifiez que le minimum ne dépasse pas le maximum."
        }
        TextKey::Addons => "Extensions",
        TextKey::AddonOverview => "Vue d’ensemble",
        TextKey::AddonUpdates => "Mises à jour",
        TextKey::AddonSources => "Sources",
        TextKey::AddonNotFound => "Extension introuvable",
        TextKey::BackToAddons => "Retour aux extensions",
        TextKey::AvailableVersion => "Version disponible",
        TextKey::BrowseAddons => "Parcourir les extensions",
        TextKey::CheckForUpdates => "Rechercher des mises à jour",
        TextKey::InstallFromFile => "Installer depuis un fichier",
        TextKey::InstallRepositoryFromFile => "Installer un dépôt depuis un fichier",
        TextKey::License => "Licence",
        TextKey::UnknownSources => "Sources inconnues",
        TextKey::UnknownSourcesHelp => {
            "Autorise les extensions issues de copies de travail Git locales et de fichiers ZIP. Elles s’exécutent avec les droits du compte de service ErsatzRS."
        }
        TextKey::Verified => "Vérifié",
        TextKey::Unverified => "Non vérifié",
        TextKey::Sideloaded => "Installé localement",
        TextKey::GitCheckout => "Extraction Git",
        TextKey::OfflineBundle => "Archive hors ligne",
        TextKey::LegacyUnmanaged => "Ancien catalogue · non géré",
        TextKey::LegacyAdoptionHelp => {
            "Cette action remplace l’installation de catalogue non gérée par la source locale sélectionnée tout en conservant ses réglages et ses données."
        }
        TextKey::Enabled => "Activé",
        TextKey::Disabled => "Désactivé",
        TextKey::Source => "Source",
        TextKey::ViewSourceCode => "Voir le code source",
        TextKey::Version => "Version",
        TextKey::NoAddonsMatch => "Aucune extension ne correspond à ces filtres.",
        TextKey::AddonsUpToDate => "Tout est à jour.",
        TextKey::AddonRepositories => "Dépôts d’extensions",
        TextKey::AvailableAddons => "Extensions disponibles",
        TextKey::InstalledAddons => "Extensions installées",
        TextKey::AddRepository => "Ajouter un dépôt",
        TextKey::LastRefresh => "Dernière analyse",
        TextKey::NeverRefreshed => "Jamais",
        TextKey::ChooseFile => "Choisir un fichier…",
        TextKey::NoFileSelected => "Aucun fichier sélectionné",
        TextKey::EditRepository => "Modifier le dépôt",
        TextKey::OfficialRepositoryProtected => "Le dépôt officiel ne peut pas être supprimé.",
        TextKey::UnknownSourcesRequired => "Activez d’abord les sources inconnues.",
        TextKey::NoRepositories => {
            "Aucun dépôt local trouvé. Clonez-en un dans le dossier de copie indiqué ci-dessus, puis actualisez."
        }
        TextKey::AddonPackage => "Paquet d’extension",
        TextKey::RepositoryPackage => "Paquet de dépôt",
        TextKey::NoSigningKeyPinned => "Aucune clé de signature publique n’est épinglée.",
        TextKey::RepositoryUrl => "URL du dépôt",
        TextKey::PublicSigningKey => "Clé publique de signature",
        TextKey::DeploymentManagedRepositories => {
            "Les dépôts sont gérés par la configuration du déploiement."
        }
        TextKey::Refresh => "Actualiser",
        TextKey::Install => "Installer",
        TextKey::UpdateAddon => "Mettre à jour",
        TextKey::Uninstall => "Désinstaller",
        TextKey::Configuration => "Configuration",
        TextKey::SecretReference => "Référence du secret",
        TextKey::EnvironmentVariable => "Variable d’environnement",
        TextKey::SecretFile => "Fichier secret",
        TextKey::Incompatible => "Incompatible",
        TextKey::PackageUnavailable => "Paquet indisponible",
        TextKey::Official => "Officiel",
        TextKey::ThirdParty => "Dépôt tiers",
        TextKey::Permissions => "Autorisations",
        TextKey::Capabilities => "Capacités",
        TextKey::Dependencies => "Dépendances",
        TextKey::Optional => "Facultatif",
        TextKey::MediaReadAccess => "Accès en lecture aux médias",
        TextKey::ExternalCommands => "Commandes externes",
        TextKey::ThirdPartyRepositoryWarning => {
            "Ne chargez que des dépôts locaux et fichiers ZIP de confiance ; les extensions s’exécutent avec le compte de service ErsatzRS."
        }
        TextKey::AddCustomResolution => "Ajouter une résolution personnalisée",
        TextKey::AddBlock => "Ajouter un bloc",
        TextKey::AddBlockGroup => "Ajouter un groupe de blocs",
        TextKey::AddBlockItem => "Ajouter un élément de bloc",
        TextKey::AddBlockToTemplate => "Ajouter un bloc au modèle",
        TextKey::AddCollection => "Ajouter une collection",
        TextKey::AddContent => "Ajouter du contenu",
        TextKey::AddFailed => "Échec de l'ajout",
        TextKey::AddFillerPreset => "Ajouter un préréglage de filler",
        TextKey::AddMultiCollection => "Ajouter une multi-collection",
        TextKey::AddPlaylist => "Ajouter une liste de lecture",
        TextKey::AddPlaylistGroup => "Ajouter un groupe de listes de lecture",
        TextKey::AddPlaylistItem => "Ajouter un élément de liste de lecture",
        TextKey::AddRerunCollection => "Ajouter une collection de rediffusions",
        TextKey::AddTemplate => "Ajouter un modèle",
        TextKey::AddTemplateGroup => "Ajouter un groupe de modèles",
        TextKey::AddAddonMediaList => "Ajouter une liste add-on",
        TextKey::AddToCollection => "Ajouter à une collection",
        TextKey::AddToEllipsis => "Ajouter à...",
        TextKey::AddToPlaylist => "Ajouter à une liste de lecture",
        TextKey::AddToSchedule => "Ajouter à une grille",
        TextKey::Actions => "Actions",
        TextKey::All => "Tout",
        TextKey::AreYouSureDelete => "Voulez-vous vraiment supprimer",
        TextKey::Actors => "Acteurs",
        TextKey::Artist => "Artiste",
        TextKey::ArtistNotFound => "Artiste introuvable",
        TextKey::AfterDurationEnd => "Après la fin de la durée",
        TextKey::AutomaticRefresh => "Actualisation automatique",
        TextKey::AddonDeliveryMode => "Diffusion",
        TextKey::AddonDeliveryStream => "Diffuser en streaming",
        TextKey::AddonDeliveryDownloadAhead => "Télécharger à l'avance",
        TextKey::AddonDeliveryNewVideos => "Télécharger à l'avance — nouvelles vidéos uniquement",
        TextKey::AddonDeliveryHelp => "Les modifications prennent effet après la prochaine publication. Seules les vidéos importées pour la première fois après la date limite enregistrée sont téléchargées ; les vidéos déjà connues sont diffusées en streaming. Réactiver ce mode définit une nouvelle date limite. Seuls les médias de durée finie peuvent être téléchargés à l'avance. Les contenus en direct et ceux dont la durée est inconnue restent diffusés en streaming.",
        TextKey::AddonDeliveryUnavailable => "Le téléchargement nécessite une extension compatible et activée et des médias de durée finie.",
        TextKey::AddonDeliveryCutoff => "Date limite des nouvelles vidéos",
        TextKey::AddonDeliveryFreshCutoff => "Une nouvelle date limite est enregistrée lors de la sauvegarde.",
        TextKey::AdaptiveRefreshUnavailable => {
            "L’actualisation adaptative est indisponible, car cette extension ne prend plus en charge les contrôles de source. Choisissez une autre fréquence pour reprendre l’actualisation automatique."
        }
        TextKey::RefreshPublicationPolicy => "Publication après actualisation",
        TextKey::RefreshPublicationReview => "Vérifier chaque actualisation",
        TextKey::RefreshPublicationAutomatic => "Appliquer automatiquement les modifications enregistrées",
        TextKey::RefreshPublicationPolicyHelp => "Les imports, actualisations, métadonnées et modifications enregistrées sont appliqués automatiquement.",
        TextKey::AutoPublicationHeld => "Le traitement automatique nécessite votre attention",
        TextKey::PreparationWorkerUnavailable => {
            "Le processus de préparation des médias est indisponible. Redémarrez le serveur avant de réessayer la préparation."
        }
        TextKey::PreparationProcessRecoveryRequired => {
            "La préparation des médias nécessite une récupération des processus. Vérifiez que les processus précédents du fournisseur sont arrêtés avant de récupérer le stockage réservé et de réessayer la préparation."
        }
        TextKey::AutoPublicationBaselineUnavailable => "Aucune référence publiée disponible.",
        TextKey::AutoPublicationExistingChanges => "Des éléments existants ont été modifiés.",
        TextKey::AutoPublicationConflicts => {
            "Des conflits de métadonnées ou de remplacement doivent être vérifiés."
        }
        TextKey::AutoPublicationRemovals => "Des éléments ont été supprimés.",
        TextKey::AutoPublicationRestricted => "Certains ajouts ont des restrictions d’accès.",
        TextKey::AutoPublicationUnavailable => "Certains ajouts sont indisponibles.",
        TextKey::AutoPublicationOptionsChanged => "Les options de publication ont changé.",
        TextKey::AutoPublicationPreparationFailed => "La préparation de la publication a échoué.",
        TextKey::AutoPublicationConcurrentChange => "La liste a changé pendant la préparation.",
        TextKey::RefreshCadenceAdaptive => "Adaptative",
        TextKey::RefreshCadenceDaily => "Quotidienne",
        TextKey::RefreshCadenceWeekly => "Hebdomadaire",
        TextKey::RefreshCadenceMonthly => "Mensuelle",
        TextKey::RefreshCadenceYearly => "Annuelle",
        TextKey::RefreshCadenceNever => "Jamais",
        TextKey::LastSuccessfulRefresh => "Dernière actualisation réussie",
        TextKey::NextScheduledRefresh => "Prochaine actualisation planifiée",
        TextKey::RefreshRetryAt => "Nouvelle tentative possible à partir de",
        TextKey::Back => "Retour",
        TextKey::BackToSettings => "Retour aux paramètres",
        TextKey::BackToArtists => "Retour aux artistes",
        TextKey::BackToCollections => "Retour aux collections",
        TextKey::BackToMovies => "Retour aux films",
        TextKey::BackToOtherVideos => "Retour aux autres vidéos",
        TextKey::BackToTvShows => "Retour aux séries TV",
        TextKey::BeforeDurationEnd => "Avant la fin de la durée",
        TextKey::BackToTemplates => "Retour aux modèles",
        TextKey::Block => "Bloc",
        TextKey::BlockGroup => "Groupe de blocs",
        TextKey::BlockCannotBeMoved => {
            "Le bloc ne peut pas être déplacé ici car il chevaucherait un autre bloc ou franchirait minuit."
        }
        TextKey::BlockCopied => "Bloc copié.",
        TextKey::BlockCreated => "Bloc créé.",
        TextKey::BlockDeleted => "Bloc supprimé.",
        TextKey::BlockGroupCreated => "Groupe de blocs créé.",
        TextKey::BlockGroupDeleted => "Groupe de blocs supprimé.",
        TextKey::BlockGroupName => "Nom du groupe de blocs",
        TextKey::BlockGroups => "Groupes de blocs",
        TextKey::BlockItem => "Élément de bloc",
        TextKey::BlockItems => "Éléments de bloc",
        TextKey::BlockName => "Nom du bloc",
        TextKey::BlockNotFound => "Bloc introuvable.",
        TextKey::BlockPreviewFailed => {
            "Impossible de générer l'aperçu du bloc. Vérifiez les éléments du bloc et réessayez."
        }
        TextKey::Blocks => "Blocs",
        TextKey::BlockToRemove => "Bloc à retirer",
        TextKey::BlockWillNotFit => {
            "Le bloc ne tient pas à partir de l'heure de début sélectionnée."
        }
        TextKey::Cancel => "Annuler",
        TextKey::Clear => "Effacer",
        TextKey::Available => "Disponible",
        TextKey::UnknownAvailability => "Inconnu",
        TextKey::LocalReplacement => "Remplacement local",
        TextKey::NoReplacement => "Aucun remplacement",
        TextKey::InvalidReplacement => "Remplacement non valide",
        TextKey::Channels => "Chaînes",
        TextKey::Close => "Fermer",
        TextKey::Collections => "Collections",
        TextKey::CollectionDeleted => "Collection supprimée.",
        TextKey::CollectionSaved => "Collection enregistrée.",
        TextKey::SaveCollection => "Enregistrer la collection",
        TextKey::ComputingPreview => "Calcul de l'aperçu...",
        TextKey::Confirm => "Confirmer",
        TextKey::ConfirmDelete => "Confirmer la suppression",
        TextKey::CommentaryTracks => "Pistes de commentaire",
        TextKey::Content => "Contenu",
        TextKey::ContentVariantAcoustic => "Acoustique",
        TextKey::ContentVariantAlternateVersion => "Version",
        TextKey::ContentVariantBehindTheScenes => "Coulisses",
        TextKey::ContentVariantDeletedScene => "Scène coupée",
        TextKey::ContentVariantDirectorsCut => "Version du réalisateur",
        TextKey::ContentVariantExtended => "Version longue",
        TextKey::ContentVariantExtra => "Bonus",
        TextKey::ContentVariantFeaturette => "Featurette",
        TextKey::ContentVariantInterview => "Interview",
        TextKey::ContentVariantLive => "Live",
        TextKey::ContentVariantOfficialVideo => "Clip officiel",
        TextKey::ContentVariantOther => "Autre",
        TextKey::ContentVariantRemaster => "Remasterisé",
        TextKey::ContentVariantRemix => "Remix",
        TextKey::ContentVariantScene => "Scène",
        TextKey::ContentVariantSearchHelp => {
            "Jetons de variante : content_kind:extra, content_kind:alternate_version, sub_kind:featurette, sub_kind:unplugged, parent_title:\"Titre du film\", audio_title:commentary, subtitle_title:commentary, has_commentary:true"
        }
        TextKey::ContentVariantShort => "Court-métrage",
        TextKey::ContentVariantSpecialFeatures => "Bonus / suppléments",
        TextKey::ContentVariantTrailer => "Bande-annonce",
        TextKey::ContentVariantUnplugged => "Unplugged",
        TextKey::ContentVariantVersions => "Versions",
        TextKey::ContentRatings => "Classifications",
        TextKey::CustomResolutionHeight => "Hauteur de la résolution personnalisée",
        TextKey::CustomResolutionWidth => "Largeur de la résolution personnalisée",
        TextKey::CustomResolutions => "Résolutions personnalisées",
        TextKey::Dark => "Sombre",
        TextKey::DaysToBuild => "Jours à générer",
        TextKey::AddBreakContent => "Ajouter un contenu de coupure",
        TextKey::AddDeco => "Ajouter une déco",
        TextKey::AddDecoGroup => "Ajouter un groupe de décos",
        TextKey::AddDecoTemplate => "Ajouter un modèle de déco",
        TextKey::AddDecoTemplateGroup => "Ajouter un groupe de modèles de déco",
        TextKey::AtChapterMarkers => "Aux marqueurs de chapitre",
        TextKey::BackToDecos => "Retour aux décos",
        TextKey::BackToDecoTemplates => "Retour aux modèles de déco",
        TextKey::BetweenBlockItems => "Entre les éléments de bloc",
        TextKey::BlockFinish => "Fin de bloc",
        TextKey::BlockStart => "Début de bloc",
        TextKey::BreakContent => "Contenu de coupure",
        TextKey::BreakContentBody => {
            "Le contenu de coupure permet de diffuser des éléments précis (via des listes de lecture) et de compléter les blocs jusqu'à leur durée configurée."
        }
        TextKey::BreakContentItem => "Élément de contenu de coupure",
        TextKey::BreakContentMode => "Mode du contenu de coupure",
        TextKey::DeadAirFallback => "Secours d'antenne vide",
        TextKey::DeadAirFallbackBody => {
            "Lorsqu'aucun élément de diffusion n'est trouvé pour l'heure actuelle, un élément est choisi au hasard dans cette collection, lu en boucle et coupé pour occuper exactement le temps jusqu'au début de l'élément de diffusion suivant. Cela remplace l'image « Chaîne hors ligne » qui s'afficherait sinon."
        }
        TextKey::DeadAirFallbackCollectionType => "Type de collection du secours d'antenne vide",
        TextKey::DeadAirFallbackMode => "Mode du secours d'antenne vide",
        TextKey::Deco => "Déco",
        TextKey::DecoCannotBeAdded => {
            "La déco ne peut pas être ajoutée car elle chevaucherait une autre ou franchirait minuit."
        }
        TextKey::DecoCannotBeMoved => {
            "La déco ne peut pas être déplacée ici car elle chevaucherait une autre ou franchirait minuit."
        }
        TextKey::DecoCreated => "Déco créée.",
        TextKey::DecoDeleted => "Déco supprimée.",
        TextKey::DecoGroup => "Groupe de décos",
        TextKey::DecoGroupCreated => "Groupe de décos créé.",
        TextKey::DecoGroupDeleted => "Groupe de décos supprimé.",
        TextKey::DecoGroupName => "Nom du groupe de décos",
        TextKey::DecoGroups => "Groupes de décos",
        TextKey::DecoName => "Nom de la déco",
        TextKey::DecoNotFound => "Déco introuvable.",
        TextKey::DecoSaved => "Déco enregistrée.",
        TextKey::Decos => "Décos",
        TextKey::DecoTemplate => "Modèle de déco",
        TextKey::DecoTemplateCreated => "Modèle de déco créé.",
        TextKey::DecoTemplateDeleted => "Modèle de déco supprimé.",
        TextKey::DecoTemplateGroup => "Groupe de modèles de déco",
        TextKey::DecoTemplateGroupCreated => "Groupe de modèles de déco créé.",
        TextKey::DecoTemplateGroupDeleted => "Groupe de modèles de déco supprimé.",
        TextKey::DecoTemplateGroupName => "Nom du groupe de modèles de déco",
        TextKey::DecoTemplateGroups => "Groupes de modèles de déco",
        TextKey::DecoTemplateName => "Nom du modèle de déco",
        TextKey::DecoTemplateNotFound => "Modèle de déco introuvable.",
        TextKey::DecoTemplateSaved => "Modèle de déco enregistré.",
        TextKey::AddDecoToTemplate => "Ajouter une déco au modèle",
        TextKey::DecoToRemove => "Déco à retirer",
        TextKey::DecoTemplates => "Modèles de déco",
        TextKey::DefaultFiller => "Filler par défaut",
        TextKey::DefaultFillerBody => {
            "Une fois tous les blocs planifiés, une seconde passe remplit le temps restant avec des éléments aléatoires de cette collection."
        }
        TextKey::DefaultFillerCollectionType => "Type de collection du filler par défaut",
        TextKey::DefaultFillerMode => "Mode du filler par défaut",
        TextKey::DeleteDeco => "Supprimer la déco",
        TextKey::DeleteDecoGroup => "Supprimer le groupe de décos",
        TextKey::DeleteDecoTemplate => "Supprimer le modèle de déco",
        TextKey::DeleteDecoTemplateGroup => "Supprimer le groupe de modèles de déco",
        TextKey::Disable => "Désactiver",
        TextKey::EditDeco => "Modifier la déco",
        TextKey::EditDecoTemplate => "Modifier le modèle de déco",
        TextKey::GraphicsElementsMode => "Mode des éléments graphiques",
        TextKey::Inherit => "Hériter",
        TextKey::Merge => "Fusionner",
        TextKey::NoDecosFound => "Aucune déco trouvée.",
        TextKey::NoDecoTemplatesFound => "Aucun modèle de déco trouvé.",
        TextKey::Override => "Forcer",
        TextKey::RemoveBreakContent => "Retirer le contenu de coupure",
        TextKey::RemoveDecoFromTemplate => "Retirer la déco du modèle",
        TextKey::Replace => "Remplacer",
        TextKey::SavedDecos => "Décos enregistrées",
        TextKey::SavedDecoTemplates => "Modèles de déco enregistrés",
        TextKey::SearchForDecos => "Rechercher des décos",
        TextKey::SearchForDecoTemplates => "Rechercher des modèles de déco",
        TextKey::TrimToFit => "Couper pour ajuster",
        TextKey::UseGraphicsElementsDuringFiller => {
            "Utiliser les éléments graphiques pendant le filler"
        }
        TextKey::UseWatermarkDuringFiller => "Utiliser le filigrane pendant le filler",
        TextKey::WatermarkMode => "Mode du filigrane",
        TextKey::DefaultFfmpegProfile => "Profil FFmpeg par défaut",
        TextKey::DefaultFfmpegProfileHelp => {
            "Le profil FFmpeg à utiliser lors de la création de nouvelles chaînes"
        }
        TextKey::DefaultMinimumLogLevel => "Niveau de journalisation minimal par défaut",
        TextKey::DefaultMpegTsScript => "Script MPEG-TS par défaut",
        TextKey::DefaultMpegTsScriptHelp => "Le script MPEG-TS à utiliser pour la diffusion",
        TextKey::Decorations => "Décorations",
        TextKey::Debug => "Debug",
        TextKey::Delete => "Supprimer",
        TextKey::DeleteAll => "Tout supprimer",
        TextKey::DeleteBlock => "Supprimer le bloc",
        TextKey::DeleteBlockGroup => "Supprimer le groupe de blocs",
        TextKey::DeleteCustomResolution => "Supprimer la résolution personnalisée",
        TextKey::DeleteCollection => "Supprimer la collection",
        TextKey::DeleteFillerPreset => "Supprimer le préréglage de filler",
        TextKey::DeleteMultiCollection => "Supprimer la multi-collection",
        TextKey::DeletePlaylist => "Supprimer la liste de lecture",
        TextKey::DeletePlaylistGroup => "Supprimer le groupe de listes de lecture",
        TextKey::DeleteRerunCollection => "Supprimer la collection de rediffusions",
        TextKey::DeleteTemplate => "Supprimer le modèle",
        TextKey::DeleteTemplateGroup => "Supprimer le groupe de modèles",
        TextKey::DeleteAddonMediaList => "Supprimer la liste add-on",
        TextKey::DisabledCollectionsAlreadyPresent => {
            "Les collections désactivées sont déjà présentes dans cette multi-collection"
        }
        TextKey::EmptyTrash => "Vider la corbeille",
        TextKey::ExtractEmbeddedSubtitles => "Extraire les sous-titres (texte) intégrés",
        TextKey::ErasedPlayoutItemsAndHistory => "Éléments de diffusion et historique effacés.",
        TextKey::EraseFailed => "Échec de l'effacement",
        TextKey::EraseItemsAndHistory => "Effacer les éléments et l'historique",
        TextKey::ErasePlayoutItemsAndHistoryConfirm => {
            "Cela supprimera tous les éléments de diffusion et l'historique de cette diffusion séquentielle. Continuer ?"
        }
        TextKey::Episode => "Épisode",
        TextKey::FanArt => "fan art",
        TextKey::Ffmpeg => "FFmpeg",
        TextKey::FileNotFound => "Fichier introuvable",
        TextKey::Finish => "Fin",
        TextKey::Fatal => "Fatal",
        TextKey::FfmpegPath => "Chemin de FFmpeg",
        TextKey::FfmpegPathHelp => "Le chemin complet vers l'exécutable ffmpeg",
        TextKey::FfmpegProfiles => "Profils FFmpeg",
        TextKey::FfprobePath => "Chemin de FFprobe",
        TextKey::FfprobePathHelp => "Le chemin complet vers l'exécutable ffprobe",
        TextKey::FillerPreset => "Préréglage de filler",
        TextKey::FillerPresetActionFailed => "Échec de l'action sur le préréglage de filler.",
        TextKey::FillerPresetCouldNotBeSaved => {
            "Impossible d'enregistrer le préréglage de filler. Vérifiez les valeurs du formulaire et réessayez."
        }
        TextKey::FillerPresetDeleted => "Préréglage de filler supprimé.",
        TextKey::FillerPresetExpressionHelp => {
            "Pour le filler mid-roll, n'ajouter du filler que lorsque cette expression est vraie pour un point mid-roll donné."
        }
        TextKey::FillerPresetSaved => "Préréglage de filler enregistré.",
        TextKey::FillerPresets => "Préréglages de filler",
        TextKey::Flags => "Indicateurs",
        TextKey::GlobalFallbackFiller => "Filler de secours global",
        TextKey::GlobalWatermark => "Filigrane global",
        TextKey::GoHome => "Retour à l'accueil",
        TextKey::HdHomeRun => "HDHomeRun",
        TextKey::HealthChecks => "Vérifications d'état",
        TextKey::Height => "Hauteur",
        TextKey::HlsDirectOutputFormat => "Format de sortie HLS Direct",
        TextKey::HlsSegmenterIdleTimeout => "Délai d'inactivité du segmenteur HLS",
        TextKey::HlsSegmenterIdleTimeoutHelp => {
            "Le nombre de secondes pendant lesquelles continuer à transcoder une chaîne alors qu'aucune requête client n'a été reçue"
        }
        TextKey::HlsSegmenterInitialSegmentCount => "Nombre de segments initiaux du segmenteur HLS",
        TextKey::HlsSegmenterInitialSegmentCountHelp => {
            "Retarde le démarrage du flux jusqu'à ce que le nombre indiqué de segments (de 4 secondes) ait été transcodé ; une valeur plus élevée ralentit le démarrage initial mais peut réduire les mises en mémoire tampon"
        }
        TextKey::HomeRunning => "Le serveur IPTV est en cours d'exécution.",
        TextKey::HttpMinimumLogLevel => "Niveau de journalisation minimal des requêtes",
        TextKey::Id => "Id",
        TextKey::Images => "Images",
        TextKey::Item => "Élément",
        TextKey::ItemActions => "Actions sur l'élément",
        TextKey::ItemType => "Type d'élément",
        TextKey::Items => "Éléments",
        TextKey::Language => "Langue",
        TextKey::Languages => "Langues",
        TextKey::MusicLabels => "Labels de musique",
        TextKey::LeaveEmptyToAutoDetect => "Laisser vide pour la détection automatique",
        TextKey::Libraries => "Bibliothèques",
        TextKey::LibraryRefreshInterval => "Intervalle d'actualisation des bibliothèques",
        TextKey::Light => "Clair",
        TextKey::Lists => "Listes",
        TextKey::Loading => "Chargement...",
        TextKey::LoadingMembers => "Chargement des membres...",
        TextKey::Local => "Local",
        TextKey::Logging => "Journalisation",
        TextKey::M3u => "M3U",
        TextKey::Media => "Médias",
        TextKey::MediaInfo => "Infos média",
        TextKey::MediaSources => "Sources multimédias",
        TextKey::Movie => "Film",
        TextKey::MovieNotFound => "Film introuvable",
        TextKey::Movies => "Films",
        TextKey::MusicVideo => "Clip musical",
        TextKey::MultiCollections => "Multi-collections",
        TextKey::ManualCollections => "Collections manuelles",
        TextKey::MatchFailed => "Échec de la mise en correspondance",
        TextKey::MatchStatus => "État de correspondance",
        TextKey::MatchAddonMediaListItems => "Faire correspondre les éléments de la liste add-on",
        TextKey::Members => "Membres",
        TextKey::MultiCollection => "Multi-collection",
        TextKey::MultiCollectionDeleted => "Multi-collection supprimée.",
        TextKey::MultiCollectionItems => "Éléments de la multi-collection",
        TextKey::MultiCollectionNameRequired => "Le nom de la multi-collection est obligatoire !",
        TextKey::MultiCollectionNameTooLong => {
            "Le nom de la multi-collection ne peut pas dépasser 50 caractères."
        }
        TextKey::MultiCollectionSaved => "Multi-collection enregistrée.",
        TextKey::Music => "Musique",
        TextKey::Name => "Nom",
        TextKey::NameRequired => "Le nom est obligatoire",
        TextKey::Networks => "Réseaux",
        TextKey::NewCollection => "(Nouvelle collection)",
        TextKey::NewCollectionTitle => "Nouvelle collection",
        TextKey::NewFillerPreset => "Nouveau préréglage de filler",
        TextKey::NewMultiCollection => "Nouvelle multi-collection",
        TextKey::NewTemplateGroup => "Nouveau groupe de modèles",
        TextKey::NewTemplateName => "Nom du nouveau modèle",
        TextKey::Next24Hours => "Prochaines 24 h",
        TextKey::NextPage => "Page suivante",
        TextKey::NoFillerPresets => "Aucun préréglage de filler pour l'instant.",
        TextKey::NoBlockItems => "Aucun élément de bloc.",
        TextKey::NoItems => "Aucun élément.",
        TextKey::NoItemsToDisplay => "Aucun élément à afficher.",
        TextKey::NoAddonMediaListsYet => "Aucune liste add-on pour l'instant.",
        TextKey::No => "Non",
        TextKey::NoCollections => "Aucune collection pour l'instant.",
        TextKey::NoMembers => "Aucun membre pour l'instant.",
        TextKey::NoMultiCollections => "Aucune multi-collection pour l'instant.",
        TextKey::NoTemplatesFound => "Aucun modèle trouvé.",
        TextKey::NotFound => "Page introuvable.",
        TextKey::OtherVideo => "Autre vidéo",
        TextKey::OtherVideoNotFound => "Autre vidéo introuvable",
        TextKey::OtherVideos => "Autres vidéos",
        TextKey::OpenBrowser => "Ouvrir le navigateur",
        TextKey::Playout => "Diffusion",
        TextKey::PlayoutItemsAndHistory => "Éléments de diffusion et historique",
        TextKey::PlayoutNotFound => "Diffusion introuvable.",
        TextKey::Playouts => "Diffusions",
        TextKey::AddPlayout => "Ajouter une diffusion",
        TextKey::ClassicScheduleMenu => "Grille classique",
        TextKey::BlockScheduleMenu => "Grille par blocs",
        TextKey::SequentialScheduleMenu => "Grille séquentielle",
        TextKey::ScriptedScheduleMenu => "Grille scriptée",
        TextKey::JsonDizqueTvScheduleMenu => "Grille JSON (dizqueTV)",
        TextKey::ResetAllPlayouts => "Réinitialiser toutes les diffusions",
        TextKey::PlayoutBuildQueued => "Reconstruction de la diffusion mise en file d'attente",
        TextKey::PlayoutBuildsQueued => "reconstructions de diffusion mises en file d'attente",
        TextKey::SearchForPlayouts => "Rechercher des diffusions",
        TextKey::PlayoutDetail => "Détail de la diffusion",
        TextKey::ShowFiller => "Afficher le filler",
        TextKey::EditPlayout => "Modifier la diffusion",
        TextKey::RefreshPlayout => "Actualiser la diffusion",
        TextKey::ResetPlayout => "Réinitialiser la diffusion",
        TextKey::ScheduleReset => "Planifier la réinitialisation",
        TextKey::SchedulePlayoutReset => "Planifier la réinitialisation de la diffusion",
        TextKey::EditExternalJsonFile => "Modifier le fichier JSON externe",
        TextKey::DeletePlayout => "Supprimer la diffusion",
        TextKey::Troubleshoot => "Dépanner",
        TextKey::DailyResetTime => "Heure de réinitialisation quotidienne",
        TextKey::DoNotAutomaticallyReset => "Ne pas réinitialiser automatiquement",
        TextKey::SaveChanges => "Enregistrer les modifications",
        TextKey::ExternalJsonFileLabel => "Fichier JSON externe",
        TextKey::EditExternalJsonFilePrompt => "Modifier le fichier JSON externe de la diffusion",
        TextKey::UnscheduledPlayoutItem => "NON PLANIFIÉ",
        TextKey::DisabledChannelsAlreadyHavePlayout => {
            "Les chaînes désactivées ont déjà une diffusion"
        }
        TextKey::ProgramSchedule => "Grille de programmes",
        TextKey::SequentialScheduleFileHelp => {
            "Le chemin complet vers le fichier de grille séquentielle (YAML)"
        }
        TextKey::BlockTemplatesAddedLater => "Les modèles de blocs s'ajoutent plus tard",
        TextKey::UnsupportedPlayoutKind => "Type de diffusion non pris en charge.",
        TextKey::ClassicPlayout => "Diffusion classique",
        TextKey::ClassicPlayoutState => "État de la diffusion classique",
        TextKey::ScanRefreshState => "État de l’actualisation après analyse",
        TextKey::ArtworkCardinality => "Cardinalité des illustrations",
        TextKey::HlsLiveDelivery => "Diffusion HLS en direct",
        TextKey::BlockPlayout => "Diffusion par blocs",
        TextKey::ScriptedPlayout => "Grille scriptée",
        TextKey::AlternateSchedules => "Grilles alternatives",
        TextKey::EditAlternateSchedules => "Modifier les grilles alternatives",
        TextKey::PlayoutTemplates => "Modèles de diffusion",
        TextKey::EditTemplates => "Modifier les modèles",
        TextKey::DefaultDeco => "Déco par défaut",
        TextKey::EnableDefaultDeco => "Activer la déco par défaut",
        TextKey::SaveDefaultDeco => "Enregistrer la déco par défaut",
        TextKey::SaveScriptedSchedule => "Enregistrer la grille scriptée",
        TextKey::EraseItems => "Effacer les éléments",
        TextKey::ErasedPlayoutItems => "Éléments de diffusion effacés.",
        TextKey::SavedDefaultDeco => "Déco par défaut enregistrée.",
        TextKey::SavedScriptedSchedule => "Grille scriptée enregistrée.",
        TextKey::Maintenance => "Maintenance",
        TextKey::SaveAlternateSchedules => "Enregistrer les grilles alternatives",
        TextKey::AddAlternateSchedule => "Ajouter une grille alternative",
        TextKey::SaveTemplates => "Enregistrer les modèles",
        TextKey::PreviewCalendar => "Aperçu du calendrier",
        TextKey::EditContent => "Modifier le contenu",
        TextKey::PlayoutNestedCatchAllHelp => {
            "Par ordre de priorité de haut en bas. L'entrée du bas correspond toujours à tous les jours et tous les mois (attrape-tout)."
        }
        TextKey::PlayoutNestedPriorityHelp => "Par ordre de priorité de haut en bas.",
        TextKey::DaysOfWeek => "Jours de la semaine",
        TextKey::DaysOfMonth => "Jours du mois",
        TextKey::Months => "Mois",
        TextKey::LimitToDateRange => "Limiter à une plage de dates",
        TextKey::StartDay => "Jour de début",
        TextKey::EndDay => "Jour de fin",
        TextKey::Enable => "Activer",
        TextKey::Up => "Monter",
        TextKey::Down => "Descendre",
        TextKey::Weekdays => "Jours ouvrés",
        TextKey::Weekends => "Week-ends",
        TextKey::Sunday => "Dimanche",
        TextKey::Monday => "Lundi",
        TextKey::Tuesday => "Mardi",
        TextKey::Wednesday => "Mercredi",
        TextKey::Thursday => "Jeudi",
        TextKey::Friday => "Vendredi",
        TextKey::Saturday => "Samedi",
        TextKey::January => "Janvier",
        TextKey::February => "Février",
        TextKey::March => "Mars",
        TextKey::April => "Avril",
        TextKey::May => "Mai",
        TextKey::June => "Juin",
        TextKey::July => "Juillet",
        TextKey::August => "Août",
        TextKey::September => "Septembre",
        TextKey::October => "Octobre",
        TextKey::November => "Novembre",
        TextKey::December => "Décembre",
        TextKey::JanuaryShort => "Janv",
        TextKey::FebruaryShort => "Févr",
        TextKey::MarchShort => "Mars",
        TextKey::AprilShort => "Avr",
        TextKey::MayShort => "Mai",
        TextKey::JuneShort => "Juin",
        TextKey::JulyShort => "Juil",
        TextKey::AugustShort => "Août",
        TextKey::SeptemberShort => "Sept",
        TextKey::OctoberShort => "Oct",
        TextKey::NovemberShort => "Nov",
        TextKey::DecemberShort => "Déc",
        TextKey::PlayAll => "Tout lire",
        TextKey::Playlist => "Liste de lecture",
        TextKey::PlaylistAdded => "Liste de lecture ajoutée.",
        TextKey::PlaylistDeleted => "Liste de lecture supprimée.",
        TextKey::PlaylistGroup => "Groupe de listes de lecture",
        TextKey::PlaylistGroupAdded => "Groupe de listes de lecture ajouté.",
        TextKey::PlaylistGroupDeleted => "Groupe de listes de lecture supprimé.",
        TextKey::PlaylistGroupName => "Nom du groupe de listes de lecture",
        TextKey::PlaylistGroups => "Groupes de listes de lecture",
        TextKey::PlaylistItem => "Élément de liste de lecture",
        TextKey::PlaylistItems => "Éléments de la liste de lecture",
        TextKey::PlaylistName => "Nom de la liste de lecture",
        TextKey::PlaylistNotFound => "Liste de lecture introuvable.",
        TextKey::PlaylistPlayoutPreview => "Aperçu de diffusion de la liste de lecture",
        TextKey::PlaylistPreviewEmpty => {
            "L'aperçu de diffusion de la liste de lecture ne contient aucun élément"
        }
        TextKey::Playlists => "Listes de lecture",
        TextKey::ProbeForInterlacedFrames => "Détecter les images entrelacées",
        TextKey::PreferredAudioLanguage => "Langue audio préférée",
        TextKey::DefaultSubtitleLanguage => "Langue de sous-titres par défaut",
        TextKey::FallbackSubtitleLanguage => "Langue de sous-titres de secours",
        TextKey::OriginalLanguageOption => "Original (und)",
        TextKey::UndefinedLanguageOption => "Indéfini (und)",
        TextKey::Previous24Hours => "24 h précédentes",
        TextKey::PreviousPage => "Page précédente",
        TextKey::RerunCollections => "Collections de rediffusions",
        TextKey::RerunCollection => "Collection de rediffusions",
        TextKey::RerunCollectionActionFailed => {
            "Échec de l'action sur la collection de rediffusions."
        }
        TextKey::RerunCollectionDeleted => "Collection de rediffusions supprimée.",
        TextKey::RerunCollectionNameRequired => {
            "Le nom de la collection de rediffusions est obligatoire !"
        }
        TextKey::RerunCollectionSaved => "Collection de rediffusions enregistrée.",
        TextKey::RerunPlaybackOrder => "Ordre de lecture des rediffusions",
        TextKey::RemoteStreams => "Flux distants",
        TextKey::RemoteStream => "Flux distant",
        TextKey::PublishedOptions => "Options publiées",
        TextKey::PlaybackHealth => "État de lecture",
        TextKey::Configure => "Configurer",
        TextKey::Watch => "Regarder",
        TextKey::RemoteStreamPlaylists => "Listes de lecture de flux distants",
        TextKey::PlaylistUrl => "URL de la liste de lecture",
        TextKey::Extractor => "Extracteur",
        TextKey::DestinationFolder => "Dossier de destination",
        TextKey::SyncInterval => "Intervalle de synchronisation",
        TextKey::MaxItems => "Nombre maximal d'éléments",
        TextKey::RemovedEntries => "Éléments supprimés",
        TextKey::CreateOrderedPlaylist => "Créer une liste de lecture ordonnée",
        TextKey::SyncNow => "Synchroniser maintenant",
        TextKey::LastSync => "Dernière synchronisation",
        TextKey::NeverSynced => "Jamais synchronisé",
        TextKey::ManagedManifestReadOnly => {
            "Ce manifeste rédigé manuellement est en lecture seule ici."
        }
        TextKey::Truncated => "Tronqué",
        TextKey::MissingDuration => "Durée manquante",
        TextKey::Orphaned => "Orphelin",
        TextKey::Materialized => "Matérialisé",
        TextKey::State => "État",
        TextKey::Upstream => "Ordre de la source",
        TextKey::NewestFirst => "Plus récents d'abord",
        TextKey::Keep => "Conserver",
        TextKey::ResetChannels => "Réinitialiser les chaînes",
        TextKey::RowsPerPage => "Lignes par page",
        TextKey::Save => "Enregistrer",
        TextKey::SaveMultiCollection => "Enregistrer la multi-collection",
        TextKey::SavePlaylist => "Enregistrer la liste de lecture",
        TextKey::SaveTroubleshootingReportsToDisk => {
            "Enregistrer les rapports de dépannage sur le disque"
        }
        TextKey::SaveAsSmartCollection => "Enregistrer comme collection intelligente",
        TextKey::SearchQueryRequired => "La requête de recherche est obligatoire",
        TextKey::SaveSettings => "Enregistrer les paramètres",
        TextKey::Scanner => "Scanner",
        TextKey::ScanningMinimumLogLevel => "Niveau de journalisation minimal de l'analyse",
        TextKey::Scheduling => "Planification",
        TextKey::SchedulingMinimumLogLevel => {
            "Niveau de journalisation minimal de la planification"
        }
        TextKey::StartWithWindows => "Démarrer ErsatzRS à l’ouverture de session Windows",
        TextKey::StartWithWindowsHelp => {
            "Démarre automatiquement l’application de la zone de notification pour l’utilisateur Windows actuel."
        }
        TextKey::Schedules => "Grilles",
        TextKey::AddSchedule => "Ajouter une grille",
        TextKey::AddScheduleItem => "Ajouter un élément de grille",
        TextKey::AddSmartCollection => "Ajouter une collection intelligente",
        TextKey::Album => "Album",
        TextKey::Chronological => "Chronologique",
        TextKey::CollectionSize => "Taille de la collection",
        TextKey::CollectionType => "Type de collection",
        TextKey::Count => "Nombre",
        TextKey::CopySchedule => "Copier la grille",
        TextKey::CopyScheduleItem => "Copier l'élément de grille",
        TextKey::CustomTitle => "Titre personnalisé",
        TextKey::DeleteSchedule => "Supprimer la grille",
        TextKey::DeleteScheduleItem => "Supprimer l'élément de grille",
        TextKey::DeleteSmartCollection => "Supprimer la collection intelligente",
        TextKey::DetailRowsPerPage => "Lignes de détail par page",
        TextKey::Director => "Réalisateur",
        TextKey::Directors => "Réalisateurs",
        TextKey::DiscardToFillAttempts => "Tentatives de remplissage par rejet",
        TextKey::Duration => "Durée",
        TextKey::Dynamic => "Dynamique",
        TextKey::EditFillerPreset => "Modifier le préréglage de filler",
        TextKey::EditProperties => "Modifier les propriétés",
        TextKey::EditScheduleItems => "Modifier les éléments de grille",
        TextKey::EditSmartCollection => "Modifier la collection intelligente",
        TextKey::EnterNameForNewSchedule => "Saisissez un nom pour la nouvelle grille",
        TextKey::Fixed => "Fixe",
        TextKey::FixedStartTimeBehavior => "Comportement de l'heure de début fixe",
        TextKey::Flood => "Flood",
        TextKey::Flexible => "Flexible",
        TextKey::FlexibleStartSoonAsPossible => "Flexible - commencer dès que possible",
        TextKey::FlexibleStartSoonAsPossibleAfterStartTime => {
            "Flexible - commencer dès que possible après l'heure de début"
        }
        TextKey::FillWithGroupMode => "Mode remplissage par groupe (série ou artiste)",
        TextKey::GraphicsElements => "Éléments graphiques",
        TextKey::AddGraphicsElement => "Ajouter un élément graphique",
        TextKey::DrawtextRendererOption => "Drawtext — dynamique/basé sur fichier",
        TextKey::DrawtextSettings => "Paramètres Drawtext",
        TextKey::DrawvgRendererOption => "Drawvg — statique/vectoriel",
        TextKey::EditGraphicsElement => "Modifier l’élément graphique",
        TextKey::FontFile => "Fichier de police",
        TextKey::FontFileOptionalHelp => {
            "Facultatif. Laissez vide pour utiliser la police Roboto fournie."
        }
        TextKey::GraphicsElement => "Élément graphique",
        TextKey::GraphicsElementConfigInvalid => {
            "La configuration de cet élément graphique de texte est invalide et n’a pas été modifiée."
        }
        TextKey::GraphicsElementKindImage => "Image",
        TextKey::GraphicsElementKindMotion => "Animation",
        TextKey::GraphicsElementKindScript => "Script",
        TextKey::GraphicsElementKindSubtitle => "Sous-titre",
        TextKey::GraphicsElementKindText => "Texte",
        TextKey::GraphicsElementNameTooLong => "Le nom doit contenir au maximum 50 caractères.",
        TextKey::GraphicsElementNotEditable => "Non modifiable ici",
        TextKey::GraphicsElementNotFound => "Élément graphique introuvable.",
        TextKey::GraphicsElementNotText => {
            "Seuls les éléments graphiques de texte peuvent être modifiés ici."
        }
        TextKey::GraphicsElementSaved => "Élément graphique enregistré.",
        TextKey::GraphicsElementSaveFailed => "L’élément graphique n’a pas pu être enregistré.",
        TextKey::GraphicsElementType => "Type",
        TextKey::GraphicsExistingDrawvgHelp => {
            "Les éléments existants et les nouveaux éléments utilisent Drawvg par défaut."
        }
        TextKey::GraphicsStreamRestartHelp => {
            "Les modifications s’appliquent aux nouveaux flux et ne redémarrent pas les flux actifs."
        }
        TextKey::NoGraphicsElements => "Aucun élément graphique trouvé.",
        TextKey::RefreshPeriodBounds => {
            "La période d’actualisation doit être comprise entre 250 et 60000 millisecondes."
        }
        TextKey::RefreshPeriodMilliseconds => "Période d’actualisation (millisecondes)",
        TextKey::ReloadFrameInterval => "Intervalle d’images de rechargement",
        TextKey::ReloadFrameIntervalBounds => {
            "L’intervalle d’images de rechargement doit être d’au moins 1."
        }
        TextKey::Renderer => "Moteur de rendu",
        TextKey::SaveGraphicsElement => "Enregistrer l’élément graphique",
        TextKey::TextTemplate => "Modèle de texte",
        TextKey::TextTemplateRequired => "Le modèle de texte est obligatoire.",
        TextKey::TextTemplateTooLong => {
            "Le modèle doit être un texte UTF-8 valide de 64 Kio maximum."
        }
        TextKey::GraphicsIds => "ID graphiques",
        TextKey::Genres => "Genres",
        TextKey::GroupName => "Nom du groupe",
        TextKey::GroupMultiPartEpisodesHelp => {
            "Regrouper les épisodes en plusieurs parties lors du mélange"
        }
        TextKey::GuideMode => "Mode guide",
        TextKey::KeepMultiPartEpisodesTogether => {
            "Garder ensemble les épisodes en plusieurs parties"
        }
        TextKey::Marathon => "Marathon",
        TextKey::MarathonBatchSize => "Taille de lot du marathon",
        TextKey::MarathonBatchSizeHelp => {
            "Nombre d'éléments à lire de chaque groupe avant de passer au suivant ; vide ou zéro lit tous les éléments"
        }
        TextKey::MarathonGroupBy => "Regroupement du marathon",
        TextKey::MarathonShuffleGroups => "Mélanger les groupes du marathon",
        TextKey::MarathonShuffleItems => "Mélanger les éléments du marathon",
        TextKey::MidRollFiller => "Filler mid-roll",
        TextKey::MidRoll => "Mid-roll",
        TextKey::MoveDown => "Déplacer vers le bas",
        TextKey::MoveUp => "Déplacer vers le haut",
        TextKey::ReorderPlaylistItem => "Faire glisser pour réorganiser l’élément de la playlist",
        TextKey::MultiEpisodeGroupSize => "Taille de groupe multi-épisodes",
        TextKey::MultiEpisodeShuffle => "Mélange multi-épisodes",
        TextKey::Multiple => "Multiple",
        TextKey::MultipleCount => "Nombre multiple",
        TextKey::MultipleCountHelp => {
            "Peut être une expression avec « count » (taille de la collection) et « random » (0 à count-1)"
        }
        TextKey::MultipleMode => "Mode multiple",
        TextKey::NewScheduleName => "Nom de la nouvelle grille",
        TextKey::NewSmartCollection => "Nouvelle collection intelligente",
        TextKey::NoScheduleItems => "Aucun élément de grille.",
        TextKey::NoSchedules => "Aucune grille.",
        TextKey::Normal => "Normal",
        TextKey::Offline => "Hors ligne",
        TextKey::One => "Un",
        TextKey::OrderedGroups => "Groupes ordonnés",
        TextKey::Playback => "Lecture",
        TextKey::PlaybackOrder => "Ordre de lecture",
        TextKey::PlaybackOrderUpdateFailed => "Échec de la mise à jour de l'ordre de lecture",
        TextKey::PlaybackOrderUpdated => "Ordre de lecture mis à jour.",
        TextKey::PlaylistItemSize => "Taille d'élément de liste de lecture",
        TextKey::PlayoutDurationHours => "Durée de diffusion (heures)",
        TextKey::PlayoutDurationMinutes => "Durée de diffusion (minutes)",
        TextKey::PostRollFiller => "Filler post-roll",
        TextKey::PostRoll => "Post-roll",
        TextKey::PreRollFiller => "Filler pre-roll",
        TextKey::PreRoll => "Pre-roll",
        TextKey::Random => "Aléatoire",
        TextKey::RandomCount => "Nombre aléatoire",
        TextKey::RandomRotation => "Rotation aléatoire",
        TextKey::RemainingDurationHelp => "durée du contenu après ce point mid-roll, en secondes",
        TextKey::RandomStartPoint => "Point de départ aléatoire",
        TextKey::RemoveFailed => "Échec du retrait",
        TextKey::RemovedFromCollection => "Retiré de la collection.",
        TextKey::RemoveFromCollection => "Retirer de la collection",
        TextKey::Rename => "Renommer",
        TextKey::RenameFailed => "Échec du renommage",
        TextKey::ReorderFailed => "Échec du réordonnancement",
        TextKey::RerunFirstRun => "Première diffusion",
        TextKey::RerunRerun => "Rediffusion",
        TextKey::ReadOnlyTwentyFourHourPlayoutView => {
            "Vue en lecture seule sur 24 heures des éléments de diffusion planifiés."
        }
        TextKey::SaveSchedule => "Enregistrer la grille",
        TextKey::SaveScheduleItems => "Enregistrer les éléments de grille",
        TextKey::SaveSmartCollection => "Enregistrer la collection intelligente",
        TextKey::Schedule => "Grille",
        TextKey::ScheduleActionFailed => "Échec de l'action sur la grille.",
        TextKey::ScheduleAsGroup => "Planifier comme groupe",
        TextKey::ScheduleKind => "Type de grille",
        TextKey::ScheduleKindBlock => "Blocs",
        TextKey::ScheduleKindClassic => "Classique",
        TextKey::ScheduleKindExternalJson => "JSON (dizqueTV)",
        TextKey::ScheduleKindScripted => "Scriptée",
        TextKey::ScheduleKindSequential => "Séquentielle",
        TextKey::SchedulingContext => "Contexte de planification",
        TextKey::ScheduleItem => "Élément de grille",
        TextKey::ScheduleItemActionFailed => "Échec de l'action sur l'élément de grille.",
        TextKey::ScheduleItemFiller => "Filler d'élément de grille",
        TextKey::ScheduleItemFillerHelp => {
            "Les fillers pre-roll, mid-roll et post-roll s'appliquent à chaque média sélectionné par cet élément de grille. Pour lire un contenu une seule fois après un bloc Multiple, ajoutez-le comme élément de grille suivant."
        }
        TextKey::ScheduleItemOverrides => "Surcharges de l'élément de grille",
        TextKey::ScheduleItems => "Éléments de grille",
        TextKey::ScheduleNotFound => "Grille introuvable.",
        TextKey::ScheduleSaveFailed => "Échec de l'enregistrement de la grille.",
        TextKey::SmartShuffle => "Lecture aléatoire intelligente",
        TextKey::SmartShufflePreset => "Préréglage d'intervalle",
        TextKey::Balanced => "Équilibré",
        TextKey::SmartShuffleScope => "Portée de l'intervalle",
        TextKey::SmartShuffleScopePlayout => "Diffusion",
        TextKey::SmartShuffleScopeScheduleItem => "Élément de grille",
        TextKey::SmartShuffleValidation => {
            "Choisissez un seuil d'espacement et au moins une règle de similarité."
        }
        TextKey::SmartShuffleScopeContentSource => "Source de contenu",
        TextKey::SmartShuffleAfterItems => "Vidéos entre les éléments similaires",
        TextKey::SmartShuffleAfterDurationMinutes => "Durée de l'intervalle (minutes)",
        TextKey::SmartShuffleIncludeFiller => "Inclure le filler",
        TextKey::SmartShuffleInheritedBlock => {
            "Cet élément n'a pas de remplacement Smart Shuffle et conserve toute stratégie de diffusion héritée."
        }
        TextKey::SmartShuffleInactive => {
            "La lecture aléatoire intelligente est inactive car cet ordre de lecture n'est pas aléatoire."
        }
        TextKey::SmartShuffleOverrideBlock => "Remplacer pour cet élément de bloc",
        TextKey::SimilarityRules => "Règles de similarité",
        TextKey::AlbumArtist => "Artiste de l'album",
        TextKey::Apply => "Appliquer",
        TextKey::ContentParent => "Parent ou variante",
        TextKey::SmartShuffleFilterRules => "Règles de filtre",
        TextKey::SearchForSchedules => "Rechercher des grilles",
        TextKey::SearchForManualCollections => "Rechercher des collections manuelles",
        TextKey::SearchForRerunCollections => "Rechercher des collections de rediffusions",
        TextKey::SearchForSmartCollections => "Rechercher des collections intelligentes",
        TextKey::SearchForTemplates => "Rechercher des modèles",
        TextKey::SearchForBlocks => "Rechercher des blocs",
        TextKey::SearchForMultiCollections => "Rechercher des multi-collections",
        TextKey::SearchQuery => "Requête de recherche",
        TextKey::BlockSearchQueryPlaceholder => "p. ex. genre:action",
        TextKey::SearchTitle => "Titre de recherche",
        TextKey::SearchTitleDisplay => "Titre de recherche (affichage)",
        TextKey::SearchSmartCollection => "Rechercher une collection intelligente",
        TextKey::Season => "Saison",
        TextKey::SeasonEpisode => "Saison, épisode",
        TextKey::Unnumbered => "Non numéroté",
        TextKey::SeasonNotFound => "Saison introuvable",
        TextKey::Select => "Sélectionner",
        TextKey::SelectCollection => "Sélectionner une collection",
        TextKey::SelectGroupForNewTemplate => "Sélectionnez un groupe pour le nouveau modèle",
        TextKey::SelectScheduleItemPrompt => {
            "Sélectionnez un élément de grille à modifier, ou ajoutez-en un nouveau."
        }
        TextKey::SelectSchedule => "Sélectionner une grille",
        TextKey::SelectSmartCollection => "Sélectionner une collection intelligente",
        TextKey::Shuffle => "Mélange",
        TextKey::ShuffleInOrder => "Mélange dans l'ordre",
        TextKey::ShuffleScheduleItems => "Mélanger les éléments de grille",
        TextKey::ShuffleScheduleItemsHelp => {
            "Remarque : cela désactive les heures de début fixes et le mode flood"
        }
        TextKey::ShuffledGroups => "Groupes mélangés",
        TextKey::Show => "Série",
        TextKey::ShowNotFound => "Série introuvable",
        TextKey::Shows => "Séries",
        TextKey::Start => "Début",
        TextKey::StartTime => "Heure de début",
        TextKey::StartTimeOnOrAfter => "Heure de début à partir de",
        TextKey::StartType => "Type de début",
        TextKey::Strict => "Strict",
        TextKey::StrictAlwaysWaitExactStartTime => {
            "Strict - toujours attendre l'heure de début exacte"
        }
        TextKey::StrictWaitExactStartTime => "Strict - attendre l'heure de début exacte",
        TextKey::TailFiller => "Filler tail",
        TextKey::TailMode => "Mode tail",
        TextKey::Tail => "Tail",
        TextKey::TelevisionSeason => "Saison de série",
        TextKey::TelevisionShow => "Série télévisée",
        TextKey::TreatCollectionsAsShows => "Traiter les collections comme des séries",
        TextKey::TreatCollectionsAsShowsHelp => {
            "Utile pour les épisodes crossover en plusieurs parties"
        }
        TextKey::Type => "Type",
        TextKey::TypeToSearch => "Saisissez pour rechercher...",
        TextKey::Search => "Rechercher",
        TextKey::Filters => "Filtres",
        TextKey::ActiveFilters => "filtres actifs",
        TextKey::ClearFilters => "Effacer les filtres",
        TextKey::CreateSmartCollection => "Créer une collection intelligente",
        TextKey::FromYear => "À partir de l’année",
        TextKey::ToYear => "Jusqu’à l’année",
        TextKey::CollectionComposer => "Éditeur de collection intelligente",
        TextKey::CollectionDetails => "Détails de la collection",
        TextKey::Rules => "Règles",
        TextKey::Overview => "Vue d’ensemble",
        TextKey::MatchAll => "Toutes les règles",
        TextKey::MatchAllHelp => "Toutes les règles de ce groupe doivent correspondre.",
        TextKey::MatchAny => "Au moins une règle",
        TextKey::MatchAnyHelp => "Au moins une règle de ce groupe doit correspondre.",
        TextKey::ExcludeAny => "Exclure les correspondances",
        TextKey::ExcludeAnyHelp => {
            "Les éléments correspondant à une règle de ce groupe sont exclus."
        }
        TextKey::AddRule => "Ajouter une règle",
        TextKey::EditRule => "Modifier la règle",
        TextKey::RemoveRule => "Supprimer la règle",
        TextKey::RuleField => "Champ",
        TextKey::Genre => "Genre",
        TextKey::Studio => "Studio",
        TextKey::Network => "Réseau",
        TextKey::Actor => "Acteur",
        TextKey::Writer => "Scénariste",
        TextKey::SubtitleLanguage => "Langue des sous-titres",
        TextKey::ShowTitle => "Titre de la série",
        TextKey::ContentKindLabel => "Type de contenu",
        TextKey::SubKindLabel => "Sous-type",
        TextKey::HasCommentary => "Avec commentaire",
        TextKey::AllFields => "Tous les champs",
        TextKey::MatchType => "Type de correspondance",
        TextKey::MatchValue => "Valeur",
        TextKey::TextContains => "Le texte contient",
        TextKey::ExactPhrase => "Expression exacte",
        TextKey::Wildcard => "Caractère générique",
        TextKey::Fuzzy => "Approximatif",
        TextKey::Range => "Plage",
        TextKey::RangeStart => "Début de la plage",
        TextKey::RangeEnd => "Fin de la plage",
        TextKey::NestedGroup => "Groupe imbriqué",
        TextKey::NestedGroupHelp => {
            "Modifiez ce groupe en mode focus pour n’afficher qu’un niveau booléen à la fois."
        }
        TextKey::AddNestedGroup => "Ajouter un groupe imbriqué",
        TextKey::EditNestedGroup => "Modifier le groupe imbriqué",
        TextKey::BackToRules => "Retour aux règles",
        TextKey::BackToSearch => "Retour à la recherche",
        TextKey::CanonicalQuery => "Requête canonique",
        TextKey::CanonicalQueryHelp => {
            "Cette requête est utilisée par la recherche, la planification et la collection enregistrée."
        }
        TextKey::LivePreview => "Aperçu en direct",
        TextKey::MatchingItems => "éléments correspondants",
        TextKey::QueryValid => "Requête valide",
        TextKey::QueryInvalid => "La requête nécessite une vérification",
        TextKey::SearchRuleValueRequired => "Chaque règle de recherche nécessite une valeur.",
        TextKey::SearchGroupRequired => "Ajoutez au moins une règle à ce groupe.",
        TextKey::SearchWildcardRequired => "Une règle avec joker doit contenir * ou ?.",
        TextKey::SavedSearches => "Recherches enregistrées",
        TextKey::SavedSearchName => "Nom de la recherche enregistrée",
        TextKey::SavedSearchHelp => {
            "Enregistrez cette recherche complexe pour la reprendre et la modifier plus tard."
        }
        TextKey::ChooseSavedSearch => "Choisir une recherche enregistrée",
        TextKey::LoadSavedSearch => "Charger la recherche",
        TextKey::SaveSavedSearch => "Enregistrer la recherche",
        TextKey::UpdateSavedSearch => "Mettre à jour la recherche",
        TextKey::NewSavedSearch => "Nouvelle recherche enregistrée",
        TextKey::DeleteSavedSearch => "Supprimer la recherche enregistrée",
        TextKey::SavedSearchSaved => "Recherche enregistrée.",
        TextKey::SavedSearchDeleted => "Recherche enregistrée supprimée.",
        TextKey::SavedSearchLoaded => "Recherche enregistrée chargée.",
        TextKey::SavedSearchNameRequired => "Le nom de la recherche enregistrée est obligatoire.",
        TextKey::SavedSearchNameTooLong => "Le nom doit comporter au maximum 80 caractères.",
        TextKey::SavedSearchNameUnique => "Le nom de la recherche enregistrée doit être unique.",
        TextKey::NoSavedSearches => "Aucune recherche enregistrée pour le moment.",
        TextKey::RefreshPreview => "Actualiser l’aperçu",
        TextKey::NoRules => "Aucune règle dans ce groupe.",
        TextKey::SearchFiltersHelp => {
            "Filtres courants uniquement. Utilisez l’éditeur de collection pour la logique imbriquée."
        }
        TextKey::SearchMedia => "Rechercher des médias",
        TextKey::SearchMoviesOrShows => "Rechercher des films ou des séries...",
        TextKey::SearchAddonMediaList => "Rechercher une liste add-on",
        TextKey::SearchingMinimumLogLevel => "Niveau de journalisation minimal de la recherche",
        TextKey::Settings => "Paramètres",
        TextKey::SettingsSaveFailed => "Échec de l'enregistrement des paramètres",
        TextKey::ShowMediaInfo => "Afficher les infos média",
        TextKey::ScriptedScheduleTimeout => "Délai d'expiration de la grille scriptée",
        TextKey::SkipMissingItems => "Ignorer les éléments manquants",
        TextKey::SkipMissingItemsHelp => {
            "Détermine si les éléments introuvables ou indisponibles doivent être inclus dans les diffusions"
        }
        TextKey::SmartCollections => "Collections intelligentes",
        TextKey::SmartCollection => "Collection intelligente",
        TextKey::SmartCollectionDeleted => "Collection intelligente supprimée.",
        TextKey::SmartCollectionQueryPlaceholder => "p. ex. genre:action OR genre:comedy",
        TextKey::SmartCollectionSaved => "Collection intelligente enregistrée.",
        TextKey::SaveAddonMediaList => "Enregistrer la liste add-on",
        TextKey::Songs => "Chansons",
        TextKey::SplitTimeEvenly => "Répartir le temps équitablement",
        TextKey::StreamingMinimumLogLevel => "Niveau de journalisation minimal du streaming",
        TextKey::Studios => "Studios",
        TextKey::Support => "Assistance",
        TextKey::Template => "Modèle",
        TextKey::TemplateCopied => "Modèle copié.",
        TextKey::TemplateCreated => "Modèle créé.",
        TextKey::TemplateDeleted => "Modèle supprimé.",
        TextKey::TemplateGroup => "Groupe de modèles",
        TextKey::TemplateGroupCreated => "Groupe de modèles créé.",
        TextKey::TemplateGroupDeleted => "Groupe de modèles supprimé.",
        TextKey::TemplateGroupName => "Nom du groupe de modèles",
        TextKey::TemplateGroups => "Groupes de modèles",
        TextKey::TemplateName => "Nom du modèle",
        TextKey::TemplateNotFound => "Modèle introuvable.",
        TextKey::Templates => "Modèles",
        TextKey::Theme => "Thème",
        TextKey::ThisActionCannotBeUndone => "Cette action est irréversible.",
        TextKey::ThisCollectionHasNoItemsYet => {
            "Cette collection ne contient encore aucun élément."
        }
        TextKey::Thumbnail => "vignette",
        TextKey::Title => "Titre",
        TextKey::ToggleNavigation => "Basculer la navigation",
        TextKey::TotalDurationHelp => "durée totale du contenu, en secondes",
        TextKey::TotalPointsHelp => "nombre total de points mid-roll potentiels",
        TextKey::TotalProgressHelp => "position normalisée de 0 à 1",
        TextKey::AddonMediaList => "Liste add-on",
        TextKey::AddonMediaListActionFailed => "Échec de l'action sur la liste add-on.",
        TextKey::AddonMediaListAdded => "Liste add-on ajoutée.",
        TextKey::AddonMediaListDeleted => "Liste add-on supprimée.",
        TextKey::AddonMediaListMatched => "Liste add-on mise en correspondance.",
        TextKey::AddonMediaListNotFound => "Liste add-on introuvable.",
        TextKey::AddonMediaListSaveFailed => "Échec de l'enregistrement de la liste add-on.",
        TextKey::AddonMediaListSaved => "Liste add-on enregistrée.",
        TextKey::AddonMediaLists => "Listes add-on",
        TextKey::AddonToolMaintenance => "Maintenance des outils",
        TextKey::AddonToolMaintenanceNone => "Aucune tentative de maintenance.",
        TextKey::AddonToolUpdatePermission => "Autoriser les mises à jour natives des programmes configurés",
        TextKey::AddonToolMaintenanceQueued => "En attente",
        TextKey::AddonToolMaintenanceRunning => "En cours",
        TextKey::AddonToolMaintenanceWaiting => "En attente d’une nouvelle tentative",
        TextKey::AddonToolUpdated => "Mis à jour",
        TextKey::AddonToolAlreadyCurrent => "Déjà à jour",
        TextKey::AddonToolManualUpdate => "Mise à jour manuelle requise",
        TextKey::AddonToolManualHelp => "Mettez à jour manuellement l’outil configuré avec sa méthode d’installation d’origine, puis vérifiez l’extension.",
        TextKey::AddonToolFailedHelp => "La mise à jour a échoué. Vérifiez la disponibilité de l’outil et l’accès réseau ; la maintenance automatique réessaiera plus tard.",
        TextKey::AddonToolUpdateFailedStale => "Le programme n’a pas pu être mis à jour et une version plus récente est disponible. La maintenance automatique réessaiera plus tard. Pour le mettre à jour maintenant, utilisez sa méthode d’installation d’origine, puis vérifiez l’extension.",
        TextKey::AddonToolUpdateVersions => "{program} : version installée {installed}, dernière version {latest}.",
        TextKey::AddonToolVersionHelp => "La version de l’outil n’a pas pu être lue. Vérifiez le programme configuré et ses autorisations.",
        TextKey::AddonToolUnavailableHelp => "L’outil configuré est indisponible. Vérifiez le paramètre du programme et son installation.",
        TextKey::AddonToolBusyHelp => "En attente de la disponibilité de l’outil ; la lecture en cours n’est pas interrompue.",
        TextKey::AddonToolCleanupHelp => "Arrêtez le serveur, le scanner et tous les processus associés du fournisseur et de l’outil. Depuis le dossier d’installation du serveur, lancez la commande de récupération hors ligne pour cet outil avec confirmation explicite, en remplaçant <absolute-executable-path> par son chemin absolu configuré. Sous Windows, utilisez ersatzrs-web.exe. Pour un profil personnalisé, ajoutez --state-dir avec son dossier absolu. Redémarrez seulement ensuite. Cette opération lève le blocage de sécurité des processus ; elle ne répare pas le programme.",
        TextKey::AddonListDiskWarnings => "Avertissements d’utilisation du disque",
        TextKey::AddonListDiskWarningGb => "Cette liste : avertir à partir de (Go)",
        TextKey::AddonListsCombinedDiskWarningGb => "Toutes les listes d’extensions : avertir à partir de (Go)",
        TextKey::AddonListDiskWarningHelp => "Go décimaux entiers (1 Go = 1 000 000 000 octets). 0 désactive uniquement cet avertissement. Atteindre ou dépasser le seuil affiche seulement un avertissement de santé ; aucun téléchargement n’est arrêté et aucun fichier n’est supprimé.",
        TextKey::AddonListDiskWarningFailed => "Impossible de charger ou d’enregistrer l’avertissement d’utilisation du disque. Rechargez la page et réessayez.",
        TextKey::AddonListDiskUsageTitle => "Utilisation du disque des listes d’extensions",
        TextKey::AddonListDiskUsageBrief => "Avertissement d’utilisation du disque des listes d’extensions",
        TextKey::AddonListDiskUsageUnavailable => "L’utilisation du disque des listes d’extensions n’a pas pu être mesurée complètement. Vérifiez l’accès au stockage et relancez le contrôle de santé.",
        TextKey::AddonListDiskUsageUnavailableBrief => "L’utilisation du disque des listes d’extensions n’a pas pu être mesurée.",
        TextKey::AddonListDiskUsageLine => "Liste nº {id} : {used} Go utilisés ; avertissement à {limit} Go.",
        TextKey::AddonListsCombinedDiskUsageLine => "Total : {used} Go utilisés ; avertissement à {limit} Go.",
        TextKey::AddonMediaListUrl => "URL de la liste add-on",
        TextKey::AddonMediaListFolder => "Dossier de stockage",
        TextKey::AddonMediaListFolderHelp => "Facultatif ; sinon dérivé du nom de la liste",
        TextKey::DestructiveActions => "Actions destructives",
        TextKey::ReconcileEntireSource => "Rapprocher toute la source",
        TextKey::ReconcileEntireSourceHelp => {
            "Compare un instantané complet de la source sans modifier la publication actuelle ni les réglages des éléments."
        }
        TextKey::ReconcileQueued => "Rapprochement complet de la source mis en file d’attente.",
        TextKey::ReconcileFailed => "Échec du rapprochement complet de la source.",
        TextKey::ResetAndReload => "Réinitialiser et recharger",
        TextKey::ResetAddonMediaListHelp => {
            "Supprime tous les résultats générés pour cette liste, puis met en file d’attente une importation complète."
        }
        TextKey::SourceHost => "Hôte source",
        TextKey::ResetRemoves => "Cette réinitialisation supprime",
        TextKey::ResetRemovesItems => {
            "Les éléments importés, les métadonnées du fournisseur, les remplacements de l’opérateur, la numérotation des épisodes et les substitutions."
        }
        TextKey::ResetRemovesArtifacts => {
            "Les médias générés, les fichiers NFO, les illustrations, les sous-titres, les caches et les archives."
        }
        TextKey::ResetRemovesRuntime => {
            "Les tâches, les points de contrôle, les artefacts de révision, les éléments de la liste de lecture générée et les médias projetés."
        }
        TextKey::ResetRetains => "Cette réinitialisation conserve",
        TextKey::ResetRetainsSettings => {
            "Le nom de la liste, les liaisons avec l’add-on, la source et la médiathèque, l’identité de la liste de lecture générée, les politiques d’actualisation et de publication, le quota d’artefacts et les autres réglages de l’opérateur."
        }
        TextKey::ResetReloadFailureWarning => {
            "Si le rechargement échoue, les chaînes et la liste de lecture générée restent vides."
        }
        TextKey::TypeListNameToConfirm => "Saisissez le nom actuel de la liste pour confirmer",
        TextKey::ResetQueued => "Réinitialisation et rechargement mis en file d’attente.",
        TextKey::ResetAddonMediaListFailed => "Échec de la réinitialisation et du rechargement.",
        TextKey::ChapterInput => "Chapitres",
        TextKey::ChapterInputHelp => "Facultatif, un par ligne : 0:00 Introduction",
        TextKey::RestoreProviderChapters => "Restaurer les chapitres du fournisseur",
        TextKey::ManagedByAddon => "Géré par une extension",
        TextKey::Trash => "Corbeille",
        TextKey::TrashAlreadyEmpty => "La corbeille est déjà vide.",
        TextKey::TrashEmpty => "La corbeille est vide.",
        TextKey::TrashEmptyConfirm => {
            "Supprimer de la base de données tous les éléments dont le fichier est introuvable ?"
        }
        TextKey::TrashEmptyConfirmHelp => {
            "Cela supprime définitivement les éléments multimédias actuellement manquants et leurs métadonnées dépendantes."
        }
        TextKey::TrashSubtitle => {
            "Éléments dont les fichiers source sont actuellement absents du disque ou du fournisseur."
        }
        TextKey::Troubleshooting => "Dépannage",
        TextKey::TunerCount => "Nombre de tuners",
        TextKey::TvShows => "Séries TV",
        TextKey::Ui => "Interface",
        TextKey::Unavailable => "Indisponible",
        TextKey::UseActualTimes => "Utiliser les heures réelles",
        TextKey::UseCustomPlaybackOrder => "Utiliser un ordre de lecture personnalisé",
        TextKey::UseChaptersAsMediaItems => "Utiliser les chapitres comme éléments multimédias",
        TextKey::UseChaptersAsMediaItemsHelp => {
            "Planifier des chapitres individuels au lieu de fichiers entiers"
        }
        TextKey::UseEmbeddedSubtitles => "Utiliser les sous-titres intégrés",
        TextKey::Uuid => "UUID",
        TextKey::UntitledItem => "Élément sans titre",
        TextKey::WindowsStartup => "Démarrage avec Windows",
        TextKey::WindowsStartupNeedsRepair => {
            "La commande de démarrage enregistrée pointe vers un autre emplacement. Appliquez ce réglage pour la corriger."
        }
        TextKey::WindowsStartupUnavailable => {
            "Le démarrage avec Windows est indisponible car le lanceur fourni est introuvable ou l’inscription ne peut pas être lue."
        }
        TextKey::ActiveTranscodeLimit => "Nombre maximal de transcodages simultanés",
        TextKey::ActiveTranscodeLimitHelp => {
            "Plafond strict des transcodages FFmpeg de diffusion. Lorsque tous les emplacements sont occupés, un nouveau spectateur reçoit une réponse « tous les flux sont utilisés » jusqu'à ce qu'un emplacement se libère ; les flux actifs ne dépassent pas la limite."
        }
        TextKey::ActiveTranscodeSuggestionHardware => {
            "Encodeur matériel détecté ; il s'agit d'une valeur suggérée, pas d'un changement automatique."
        }
        TextKey::ActiveTranscodeSuggestionSoftware => {
            "Encodage logiciel uniquement ; il s'agit d'une valeur suggérée, pas d'un changement automatique."
        }
        TextKey::Peak => "pic",
        TextKey::StreamsInUse => "Flux en cours d'utilisation",
        TextKey::SuggestedTranscodeLimit => "Valeur suggérée",
        TextKey::Watermarks => "Filigranes",
        TextKey::Width => "Largeur",
        TextKey::WorkAheadSegmenterLimit => "Transcodages HLS en avance",
        TextKey::WorkAheadSegmenterLimitHelp => {
            "Nombre d'emplacements de transcodage HLS pouvant travailler en avance à vitesse maximale pour constituer le tampon. Le travail en avance cède toujours la priorité aux flux regardés."
        }
        TextKey::AccelerationKind => "Type d'accélération",
        TextKey::AddFfmpegProfile => "Ajouter un profil FFmpeg",
        TextKey::AddProfile => "Ajouter un profil",
        TextKey::AllowBFrames => "Autoriser les images B",
        TextKey::AllowWatermarks => "Autoriser les filigranes",
        TextKey::Audio => "Audio",
        TextKey::AudioBitrate => "Débit audio (kBit/s)",
        TextKey::AudioBufferSize => "Taille du tampon audio (kBit)",
        TextKey::AudioChannels => "Canaux",
        TextKey::AudioFormat => "Format audio",
        TextKey::BitDepth => "Profondeur de bits",
        TextKey::Copy => "Copier",
        TextKey::CopyBlock => "Copier le bloc",
        TextKey::CopyFailed => "Échec de la copie",
        TextKey::CopyNameSuffix => "Copie",
        TextKey::CopyProfile => "Copier le profil",
        TextKey::CopyTemplate => "Copier le modèle",
        TextKey::Crop => "Rogner",
        TextKey::DeinterlaceVideo => "Désentrelacer la vidéo",
        TextKey::Edit => "Modifier",
        TextKey::EditBlock => "Modifier le bloc",
        TextKey::EditCollection => "Modifier la collection",
        TextKey::EditFfmpegProfile => "Modifier le profil FFmpeg",
        TextKey::EditMultiCollection => "Modifier la multi-collection",
        TextKey::EditRerunCollection => "Modifier la collection de rediffusions",
        TextKey::EditTemplate => "Modifier le modèle",
        TextKey::Expression => "Expression",
        TextKey::Fallback => "Secours",
        TextKey::FillerKind => "Type de filler",
        TextKey::General => "Général",
        TextKey::HardwareAcceleration => "Accélération matérielle",
        TextKey::HardwareIfPossible => "Matérielle si possible",
        TextKey::NewProfileName => "Nom du nouveau profil",
        TextKey::NoneOption => "(Aucun)",
        TextKey::NumHelp => "le numéro du point mid-roll, à partir de 1",
        TextKey::OptionalDisplayTitle => "Titre d'affichage facultatif...",
        TextKey::TranscodeAudio => "Transcoder l’audio",
        TextKey::NormalizeColors => "Normaliser les couleurs",
        TextKey::NormalizeFramerate => "Normaliser la fréquence d'images",
        TextKey::NormalizeLoudness => "Normaliser le volume",
        TextKey::TranscodeVideo => "Transcoder la vidéo",
        TextKey::Off => "Désactivé",
        TextKey::Pad => "Compléter",
        TextKey::PadNearestFifteenMinutes => "15 (:00, :15, :30, :45)",
        TextKey::PadNearestFiveMinutes => "5 (:00, :05, :10, :15, :20, etc.)",
        TextKey::PadNearestTenMinutes => "10 (:00, :10, :20, :30, :40, :50)",
        TextKey::PadNearestThirtyMinutes => "30 (:00, :30)",
        TextKey::PadMode => "Mode de complément",
        TextKey::PadToNearestMinute => "Compléter à la minute la plus proche",
        TextKey::PointHelp => "la position du point mid-roll, en secondes",
        TextKey::ProfileCopied => "Profil copié.",
        TextKey::ProfileNotFound => "Profil introuvable.",
        TextKey::QsvExtraHardwareFrames => "Images matérielles supplémentaires QSV",
        TextKey::Resolution => "Résolution",
        TextKey::SampleRateKhz => "Fréquence d'échantillonnage (kHz)",
        TextKey::SaveDecoTemplate => "Enregistrer le modèle de déco",
        TextKey::SaveProfile => "Enregistrer le profil",
        TextKey::SaveFillerPreset => "Enregistrer le préréglage de filler",
        TextKey::SaveRerunCollection => "Enregistrer la collection de rediffusions",
        TextKey::SaveSequentialSchedule => "Enregistrer la grille séquentielle",
        TextKey::SaveTemplate => "Enregistrer le modèle",
        TextKey::SaveBlock => "Enregistrer le bloc",
        TextKey::ScaleAndPad => "Mettre à l'échelle et compléter",
        TextKey::ScalingBehavior => "Comportement de mise à l'échelle",
        TextKey::Software => "Logicielle",
        TextKey::Stretch => "Étirer",
        TextKey::TargetLoudnessLufs => "Volume cible (LUFS)",
        TextKey::ThreadCountAuto => "Nombre de threads (0 = auto)",
        TextKey::OutputDynamicRange => "Plage dynamique de sortie",
        TextKey::OutputDynamicRangeCopyHint => {
            "La vidéo est copiée, la sortie conserve donc la plage dynamique de la source."
        }
        TextKey::HdrStaticMetadataHint => {
            "Laissez vide pour utiliser les valeurs HDR10 documentées par défaut. Elles ne s'appliquent que si la source ne porte aucune métadonnée HDR ; l'écran de mastering et les niveaux de la source priment toujours."
        }
        TextKey::HdrMasteringDisplay => "Écran de mastering",
        TextKey::HdrMaxCll => "MaxCLL (cd/m²)",
        TextKey::HdrMaxFall => "MaxFALL (cd/m²)",
        TextKey::TonemapAlgorithm => "Algorithme de tonemapping",
        TextKey::VaapiDevicePath => "Chemin du périphérique VAAPI",
        TextKey::VaapiDisplay => "Affichage VAAPI",
        TextKey::VaapiDriver => "Pilote VAAPI",
        TextKey::Video => "Vidéo",
        TextKey::VideoBitrate => "Débit vidéo (kBit/s)",
        TextKey::VideoBufferSize => "Taille du tampon vidéo (kBit)",
        TextKey::VideoFormat => "Format vidéo",
        TextKey::VideoPreset => "Préréglage vidéo",
        TextKey::VideoTune => "Réglage vidéo",
        TextKey::VideoProfile => "Profil vidéo",
        TextKey::ViewCollection => "Voir la collection",
        TextKey::ViewPlaylist => "Voir la liste de lecture",
        TextKey::AddAll => "Tout ajouter",
        TextKey::AddAllToCollection => "Tout ajouter à une collection",
        TextKey::AddAllToPlaylist => "Tout ajouter à une liste de lecture",
        TextKey::AddedItemToPlaylist => "Élément ajouté à la liste de lecture.",
        TextKey::AddedArtistToSchedule => "Artiste ajouté à la grille",
        TextKey::AddedMovieToSchedule => "Film ajouté à la grille",
        TextKey::AddedSeasonToSchedule => "Saison ajoutée à la grille",
        TextKey::AddedShowToSchedule => "Série ajoutée à la grille",
        TextKey::AddingItem => "Ajout de l'élément...",
        TextKey::AddingItemToPlaylist => "Ajout de l'élément à la liste de lecture...",
        TextKey::Artists => "Artistes",
        TextKey::ClearSelection => "Effacer la sélection",
        TextKey::Collection => "Collection",
        TextKey::CollectionNameRequired => "Le nom de la collection est obligatoire",
        TextKey::CollectionNotFound => "Collection introuvable",
        TextKey::CollectionRenamed => "Collection renommée.",
        TextKey::EditAddonMediaListProperties => "Modifier les propriétés de la liste add-on",
        TextKey::EnterAddonMediaListUrl => "Saisissez l'URL de la liste add-on",
        TextKey::EnterSearchQuery => "Saisissez une requête de recherche.",
        TextKey::Episodes => "Épisodes",
        TextKey::LoadingCollections => "Chargement des collections...",
        TextKey::LoadingPlaylistGroups => "Chargement des groupes de listes de lecture...",
        TextKey::LoadingPlaylists => "Chargement des listes de lecture...",
        TextKey::GeneratePlaylist => "Créer la liste dans l’ordre d’origine",
        TextKey::GeneratePlaylistFromSortedAddonMediaListItems => {
            "Conserver l’ordre d’origine du fournisseur après chaque publication"
        }
        TextKey::MusicVideos => "Clips musicaux",
        TextKey::NewCollectionName => "Nom de la nouvelle collection",
        TextKey::NoMediaMatched => "Aucun média correspondant.",
        TextKey::NoItemsToAdd => "Aucun élément à ajouter.",
        TextKey::NoItemsToPreview => "Aucun élément à prévisualiser.",
        TextKey::NoPlaylistsInGroup => "Aucune liste de lecture dans ce groupe.",
        TextKey::PleaseChoosePlaylist => "Veuillez choisir une liste de lecture.",
        TextKey::RequestFailed => "Échec de la requête",
        TextKey::Released => "Sortie",
        TextKey::SaveAs => "Enregistrer sous",
        TextKey::Seasons => "Saisons",
        TextKey::SeeAll => "Tout voir >>",
        TextKey::SelectAll => "Tout sélectionner",
        TextKey::SelectGroup => "(Sélectionner un groupe)",
        TextKey::SelectPlaylist => "(Sélectionner une liste de lecture)",
        TextKey::SelectSearchCollectionHelp => {
            "Sélectionnez la collection où ajouter les résultats de recherche."
        }
        TextKey::SelectSearchPlaylistHelp => {
            "Sélectionnez la liste de lecture où ajouter les résultats de recherche."
        }
        TextKey::Selected => "Sélectionné",
        TextKey::SelectCollectionToAddThisMovie => {
            "Sélectionnez une collection pour y ajouter ce film."
        }
        TextKey::SelectPlaylistToAddThisMovie => {
            "Sélectionnez une liste de lecture pour y ajouter ce film."
        }
        TextKey::Add => "Ajouter",
        TextKey::AddLibrary => "Ajouter une bibliothèque",
        TextKey::Address => "Adresse",
        TextKey::AddressRequired => "L'adresse est obligatoire.",
        TextKey::ApiKey => "Clé API",
        TextKey::ApiKeyRequired => "La clé API est obligatoire.",
        TextKey::AuthorizedRedirecting => "Autorisé - redirection...",
        TextKey::CheckingEveryTwoSeconds => "Vérification toutes les 2 secondes...",
        TextKey::ClickAuthorizePlex => "Cliquez ici pour autoriser sur Plex.tv",
        TextKey::ClickPlayoutItemHelp => {
            "Cliquez sur un élément de diffusion pour inspecter son titre, ses horaires et son contexte de planification."
        }
        TextKey::ConfigureConnection => "Configurer la connexion",
        TextKey::Connection => "Connexion",
        TextKey::DeepScanCollections => "Analyse approfondie des collections",
        TextKey::DeepScanLibrary => "Analyse approfondie de la bibliothèque",
        TextKey::DeepScan => "Analyse approfondie",
        TextKey::EditLibrary => "Modifier la bibliothèque",
        TextKey::EditPlaylist => "Modifier la liste de lecture",
        TextKey::EnterOnePathPerLine => "Saisissez un chemin par ligne.",
        TextKey::Error => "Erreur",
        TextKey::ExternalCollections => "Collections externes",
        TextKey::Information => "Information",
        TextKey::Kind => "Type",
        TextKey::Library => "Bibliothèque",
        TextKey::LibraryKind => "Type de bibliothèque",
        TextKey::LibraryPaths => "Chemins de la bibliothèque",
        TextKey::LocalLibraries => "Bibliothèques locales",
        TextKey::LocalPath => "Chemin local",
        TextKey::MediaKind => "Type de média",
        TextKey::NoLibrariesConfigured => {
            "Aucune bibliothèque configurée. Ajoutez d'abord une source multimédia."
        }
        TextKey::NoSchedulingContext => "Aucun contexte de planification",
        TextKey::NoPlayoutItemsInWindow => {
            "Aucun élément de diffusion dans cette fenêtre de 24 heures"
        }
        TextKey::NoPlayoutsFound => "Aucune diffusion trouvée.",
        TextKey::NoPathReplacements => "Aucun remplacement de chemin.",
        TextKey::Paths => "Chemins",
        TextKey::PathReplacements => "Remplacements de chemins",
        TextKey::PlexSignInHelp => "Connectez-vous à Plex pour ajouter une source multimédia.",
        TextKey::Remote => "Distant",
        TextKey::RemotePath => "Chemin distant",
        TextKey::Remove => "Retirer",
        TextKey::RemoveBlockFromTemplate => "Retirer le bloc du modèle",
        TextKey::RemoveContent => "Retirer le contenu",
        TextKey::Retry => "Réessayer",
        TextKey::Saved => "Enregistré.",
        TextKey::Saving => "Enregistrement...",
        TextKey::QuickScan => "Analyse rapide",
        TextKey::ScanCollections => "Analyser les collections",
        TextKey::ScanFailed => "Échec de l'analyse",
        TextKey::ScanLibrary => "Analyser la bibliothèque",
        TextKey::ScanQueued => "Analyse mise en file d'attente",
        TextKey::Scanning => "Analyse...",
        TextKey::Server => "Serveur",
        TextKey::ServerAddress => "Adresse du serveur",
        TextKey::SignInToPlex => "Se connecter à Plex",
        TextKey::SignOutOfPlex => "Se déconnecter de Plex",
        TextKey::Sync => "Synchroniser",
        TextKey::WaitingForPlexAuthorization => "En attente de l'autorisation Plex.",
        TextKey::AddChannel => "Ajouter une chaîne",
        TextKey::Any => "Toutes",
        TextKey::BlockPlayoutTroubleshooting => "Dépannage de la diffusion par blocs",
        TextKey::BlockPlayoutHistory => "Historique de la diffusion par blocs",
        TextKey::Categories => "Catégories",
        TextKey::CategoriesHelp => "Liste de catégories séparées par des virgules",
        TextKey::Channel => "Chaîne",
        TextKey::ChannelCouldNotBeSaved => {
            "Impossible d'enregistrer la chaîne. Vérifiez les valeurs du formulaire et réessayez."
        }
        TextKey::ChannelDisabled => "Chaîne désactivée",
        TextKey::ChannelHiddenFromEpg => "Chaîne masquée de l'EPG",
        TextKey::ChannelLogo => "Logo de la chaîne",
        TextKey::ChannelLogoPreview => "Aperçu du logo de la chaîne",
        TextKey::ChannelLogoUploadFailed => {
            "Échec de l'envoi du logo de la chaîne. Vérifiez le fichier et réessayez."
        }
        TextKey::Check => "Vérification",
        TextKey::Continuous => "Continu",
        TextKey::DateAndTime => "Date et heure",
        TextKey::DefaultOption => "Par défaut",
        TextKey::DefaultSchedule => "Grille par défaut",
        TextKey::DeleteChannel => "Supprimer la chaîne",
        TextKey::DownloadMediaSample => "Télécharger un échantillon média",
        TextKey::DownloadResults => "Télécharger les résultats",
        TextKey::DisableWatermarks => "Désactiver les filigranes",
        TextKey::EditChannel => "Modifier la chaîne",
        TextKey::EditChannelNumbers => "Modifier les numéros de chaîne",
        TextKey::EditNumbers => "Modifier les numéros",
        TextKey::ExternalLogoUrl => "URL du logo externe",
        TextKey::Fail => "Échec",
        TextKey::FallbackFiller => "Filler de secours",
        TextKey::FfmpegProfile => "Profil FFmpeg",
        TextKey::FirstPage => "Première page",
        TextKey::FirstRunPlaybackOrder => "Ordre de lecture des premières diffusions",
        TextKey::Forced => "Forcés",
        TextKey::GenerateSubtitles => "Générer les sous-titres",
        TextKey::Generated => "Générés",
        TextKey::Details => "Détails",
        TextKey::Full => "Complet",
        TextKey::Group => "Groupe",
        TextKey::FfmpegRuntime => "Runtime FFmpeg",
        TextKey::FfmpegRuntimeDownloaded => "Runtime FFmpeg téléchargé",
        TextKey::FfmpegRuntimeLatestRelease => "Dernière version",
        TextKey::FfmpegRuntimeRelease => "Version installée",
        TextKey::FfmpegRuntimeSource => "Source",
        TextKey::FfmpegRuntimeSourceDownloaded => "Téléchargé",
        TextKey::FfmpegRuntimeSourceManaged => "Géré",
        TextKey::FfmpegRuntimeSourceOverride => "Remplacement",
        TextKey::FfmpegRuntimeSourcePath => "PATH",
        TextKey::FfmpegRuntimeSourceUnknown => "Inconnue",
        TextKey::FfmpegRuntimeUpdateAvailable => "Mise à jour disponible",
        TextKey::FfmpegRuntimeUpdateCheckFailed => "Échec de la vérification de mise à jour",
        TextKey::FfmpegRuntimeUpdateFailed => "Échec de la mise à jour du runtime FFmpeg",
        TextKey::FfmpegRuntimeUpToDate => "À jour",
        TextKey::FfmpegRuntimeUpdated => "Runtime FFmpeg mis à jour",
        TextKey::FfmpegRuntimeVersion => "Version",
        TextKey::HealthNoPersistedRun => {
            "Aucune vérification d'état n'est applicable pour le moment."
        }
        TextKey::HealthPersistedRunHelp => {
            "Résultats actuels des vérifications d'état de parité. Cette page s'actualise après que les analyses de médias ont mis à jour l'état des éléments."
        }
        TextKey::HistoryDetails => "Détails de l'historique",
        TextKey::HlsDirect => "HLS Direct",
        TextKey::HlsSegmenter => "Segmenteur HLS",
        TextKey::Hours => "heures",
        TextKey::IdleBehavior => "Comportement en inactivité",
        TextKey::IdleBehaviorHelp => {
            "Détermine le comportement du processus de transcodage après la déconnexion de tous les clients"
        }
        TextKey::Info => "Info",
        TextKey::Import => "Importer",
        TextKey::IsEnabled => "Activée",
        TextKey::Json => "JSON",
        TextKey::KeepRunning => "Laisser tourner",
        TextKey::Key => "Clé",
        TextKey::LastMidFillerHelp => "secondes depuis le dernier filler mid-roll",
        TextKey::LastPage => "Dernière page",
        TextKey::Level => "Niveau",
        TextKey::LoadingMediaInfo => "Chargement des infos média...",
        TextKey::Logs => "Journaux",
        TextKey::Logo => "Logo",
        TextKey::MediaItem => "Élément multimédia",
        TextKey::Message => "Message",
        TextKey::MissingMediaItemQueryParameter => "Paramètre de requête mediaItem manquant",
        TextKey::Mirror => "Miroir",
        TextKey::MirrorOptionsHelp => {
            "Disponible uniquement lorsque la source de diffusion est définie sur Miroir."
        }
        TextKey::MirrorSourceChannel => "Chaîne source du miroir",
        TextKey::MusicVideoCreditsMode => "Mode des crédits de clip musical",
        TextKey::MusicVideoCreditsTemplate => "Modèle des crédits de clip musical",
        TextKey::NoMatchingRecordsFound => "Aucun enregistrement correspondant trouvé",
        TextKey::NoFfmpegProfiles => "Aucun profil FFmpeg trouvé. Créez-en d'abord un.",
        TextKey::NoBlockPlayoutSelected => {
            "Sélectionnez une diffusion par blocs pour inspecter ses blocs."
        }
        TextKey::NoBlocks => "Aucun bloc trouvé.",
        TextKey::NoHistory => "Aucun historique trouvé.",
        TextKey::NoMessages => "Aucun message.",
        TextKey::NvidiaCapabilitiesNotCollected => {
            "Les capacités NVIDIA n'ont pas encore été collectées."
        }
        TextKey::Number => "Numéro",
        TextKey::OnDemand => "À la demande",
        TextKey::MatchedPointsHelp => "nombre de points mid-roll ayant déjà satisfait l'expression",
        TextKey::Pass => "Réussite",
        TextKey::ParityDebt => "ParityDebt",
        TextKey::PlaybackTroubleshooting => "Dépannage de lecture",
        TextKey::Play => "Lire",
        TextKey::PlayoutTimeline => "Chronologie de diffusion",
        TextKey::PlayoutMode => "Mode de diffusion",
        TextKey::PlayoutModeHelp => "Détermine la progression de la diffusion de la chaîne",
        TextKey::PlayoutOffset => "Décalage de diffusion",
        TextKey::PlayoutSource => "Source de diffusion",
        TextKey::PlayoutSourceHelp => {
            "Détermine la source du contenu de la chaîne. Cela ne peut pas être modifié lorsque la chaîne a déjà une diffusion."
        }
        TextKey::Preview => "Aperçu",
        TextKey::PreviewBlockPlayout => "Aperçu de la diffusion par blocs",
        TextKey::PreviewChannel => "Aperçu de la chaîne",
        TextKey::ChannelPreview => "Aperçu de la chaîne",
        TextKey::ReloadStream => "Recharger le flux",
        TextKey::StopPreview => "Arrêter l’aperçu",
        TextKey::OpenFullPlayer => "Ouvrir le lecteur complet",
        TextKey::BackToChannels => "Retour aux chaînes",
        TextKey::NowPlaying => "En cours",
        TextKey::UpNext => "À suivre",
        TextKey::ProgrammeGuide => "Guide des programmes",
        TextKey::EpgPreview => "Aperçu EPG",
        TextKey::EpgPreviewDescription => {
            "Parcourez le guide publié avec deux heures d'historique et 24 heures à venir."
        }
        TextKey::EpgZoom => "Zoom",
        TextKey::EpgNow => "Maintenant",
        TextKey::NoEpgChannels => "Aucune chaîne activée n'est visible dans l'EPG.",
        TextKey::ScheduleGap => "Intervalle de programmation",
        TextKey::Filler => "Remplissage",
        TextKey::NoProgramming => "Aucun programme n’est prévu dans cette période.",
        TextKey::ChannelDisabledPreview => "Cette chaîne est désactivée.",
        TextKey::ChannelNotAiring => "Cette chaîne n’est pas diffusée actuellement.",
        TextKey::ChannelNoPlayout => "Cette chaîne n’a aucune programmation.",
        TextKey::PreviewIdle => "L’aperçu démarre automatiquement en mode silencieux.",
        TextKey::PreviewConnecting => "Connexion au flux en direct…",
        TextKey::PreviewLive => "La lecture en direct est en cours.",
        TextKey::PreviewPaused => "Aperçu en pause.",
        TextKey::PreviewRecovering => "Récupération de la lecture…",
        TextKey::PreviewStopped => "Aperçu arrêté.",
        TextKey::PreviewUnsupported => "Ce navigateur ne peut pas lire la vidéo HLS.",
        TextKey::PreviewAuthorizationFailed => "L’autorisation de l’aperçu a échoué.",
        TextKey::PreviewUnavailable => "Le flux en direct est indisponible.",
        TextKey::PreviewCapacityReached => "Tous les emplacements de transcodage sont utilisés.",
        TextKey::PreviewNetworkError => "L’aperçu a perdu sa connexion réseau.",
        TextKey::PreviewMediaError => {
            "Le navigateur ne peut pas décoder la sortie configurée de cette chaîne."
        }
        TextKey::PreviewDerivativeNote => {
            "L'aperçu d'administration utilise une version H.264/AAC compatible avec les navigateurs ; la sortie configurée reste inchangée."
        }
        TextKey::JavascriptRequiredPlayback => "JavaScript est requis pour la lecture interactive.",
        TextKey::ConfirmStopTranscodeSession => {
            "Arrêter la session de transcodage partagée de cette chaîne ?"
        }
        TextKey::PreviewPlaylistPlayout => "Aperçu de la diffusion de la liste de lecture",
        TextKey::PreferredAudioTitle => "Titre audio préféré",
        TextKey::PreferredSubtitleLanguage => "Langue de sous-titres préférée",
        TextKey::QsvCapabilitiesNotCollected => {
            "Les capacités QSV n'ont pas encore été collectées."
        }
        TextKey::RemoveStoredLogo => "Supprimer le logo stocké",
        TextKey::SaveChannel => "Enregistrer la chaîne",
        TextKey::SaveChannels => "Enregistrer les chaînes",
        TextKey::Second => "seconde",
        TextKey::Seconds => "secondes",
        TextKey::SelectProfile => "- Sélectionner un profil -",
        TextKey::SequentialSchedule => "Grille séquentielle",
        TextKey::SequentialScheduleValidation => "Validation de la grille séquentielle",
        TextKey::SeekSeconds => "Position de départ (secondes)",
        TextKey::Schema => "Schéma",
        TextKey::ShowDisabled => "Afficher les désactivées",
        TextKey::ShowInEpg => "Afficher dans l'EPG",
        TextKey::SlugSeconds => "Slug (secondes)",
        TextKey::SlugSecondsHelp => {
            "Secondes de vidéo noire et d'audio silencieux insérées entre chaque élément de diffusion"
        }
        TextKey::Song => "Chanson",
        TextKey::SongVideoMode => "Mode vidéo des chansons",
        TextKey::SortById => "Trier par identifiant",
        TextKey::SortByLevel => "Trier par niveau",
        TextKey::SortByName => "Trier par nom",
        TextKey::SortByTimestamp => "Trier par horodatage",
        TextKey::StopOnDisconnect => "Arrêter à la déconnexion",
        TextKey::StopScheduling => "Arrêter la planification",
        TextKey::StopSchedulingBlockItems => "Arrêter de planifier les éléments de bloc",
        TextKey::StopTranscodeSession => "Arrêter la session de transcodage",
        TextKey::StartFromBeginning => "Commencer depuis le début",
        TextKey::StreamSelector => "Sélecteur de flux",
        TextKey::StreamSelectorMode => "Mode du sélecteur de flux",
        TextKey::StreamingMode => "Mode de streaming",
        TextKey::Styles => "Styles",
        TextKey::Subtitle => "Sous-titre",
        TextKey::SubtitleMode => "Mode des sous-titres",
        TextKey::SubtitleModeHelp => {
            "« Forcés » n'affiche que les pistes narratives forcées. Pour toujours afficher les \
             sous-titres dans votre langue préférée, choisissez « Toutes »."
        }
        TextKey::Timestamp => "Horodatage",
        TextKey::Timeline => "Chronologie",
        TextKey::Target => "Cible",
        TextKey::Tags => "Étiquettes",
        TextKey::Tools => "Outils",
        TextKey::TranscodeMode => "Mode de transcodage",
        TextKey::TranscodeModeHelp => "Détermine quand le processus de transcodage est actif",
        TextKey::TroubleshootPlayback => "Dépanner la lecture",
        TextKey::TroubleshootChannel => "Dépanner la chaîne",
        TextKey::TroubleshootingReports => "Rapports de dépannage",
        TextKey::MpegTsTroubleshooting => "Dépannage MPEG-TS",
        TextKey::InternalMpegTs => "MPEG-TS interne",
        TextKey::PublicMpegTs => "MPEG-TS public",
        TextKey::RetainRawSample => "Conserver l'échantillon brut",
        TextKey::TsdDuckVersion => "Version de TSDuck",
        TextKey::Report => "Rapport",
        TextKey::RunId => "ID d'exécution",
        TextKey::Status => "Statut",
        TextKey::Summary => "Résumé",
        TextKey::SampleBytes => "Octets de l'échantillon",
        TextKey::RawSampleArchived => "Échantillon brut archivé",
        TextKey::ArchivePath => "Chemin de l'archive",
        TextKey::SampleUrl => "URL de l'échantillon",
        TextKey::StabilityMeter => "Indicateur de stabilité",
        TextKey::Stable => "Stable",
        TextKey::Packets => "paquets",
        TextKey::Services => "Services",
        TextKey::Pids => "PID",
        TextKey::ContinuityFindings => "Anomalies de continuité",
        TextKey::PcrFindings => "Anomalies PCR",
        TextKey::Bitrate => "Débit",
        TextKey::AnalysisJson => "JSON d'analyse",
        TextKey::StandardOutput => "Sortie standard",
        TextKey::StandardError => "Erreur standard",
        TextKey::NotAvailable => "Non disponible",
        TextKey::NotApplicable => "Sans objet",
        TextKey::Completed => "Terminé",
        TextKey::Failed => "Échoué",
        TextKey::TroubleshootingPlaybackStartDebt => {
            "L'aperçu de lecture des éléments multimédias et des chaînes écrit désormais les rapports FFmpeg, les journaux, media_info.json, troubleshooting_info.json, le YAML du sélecteur de flux retenu et les fichiers de capacités, exécute les règles du sélecteur de flux et redirige vers /iptv/session/.troubleshooting/live.m3u8."
        }
        TextKey::UnableToLoadMediaInfo => "Impossible de charger les infos média",
        TextKey::UnableToLoadPlaybackTroubleshooting => {
            "Impossible de charger le contexte de dépannage de lecture"
        }
        TextKey::UploadLogo => "Envoyer un logo",
        TextKey::UploadLogoHelp => {
            "Choisissez une image locale pour remplacer le logo de chaîne stocké lors de l'enregistrement"
        }
        TextKey::Warning => "Avertissement",
        TextKey::Watermark => "Filigrane",
        TextKey::WatermarkIds => "ID de filigrane",
        TextKey::Window => "Fenêtre",
        TextKey::WithProgress => "Avec progression",
        TextKey::Writers => "Scénaristes",
        TextKey::Validate => "Valider",
        TextKey::Yaml => "YAML",
        TextKey::YamlFile => "Fichier YAML",
        TextKey::Messages => "Messages",
        TextKey::CollectionName => "Nom de la collection",
        TextKey::MediaItemType => "Type d'élément multimédia",
        TextKey::MediaItemTitle => "Titre de l'élément multimédia",
        TextKey::ActualSize => "Taille réelle",
        TextKey::AddWatermark => "Ajouter un filigrane",
        TextKey::AspectRatioGuides => "Repères de format d’image",
        TextKey::BottomLeft => "En bas à gauche",
        TextKey::BottomMiddle => "En bas au centre",
        TextKey::BottomRight => "En bas à droite",
        TextKey::CopyWatermark => "Copier le filigrane",
        TextKey::CopyOf => "Copie de",
        TextKey::CurrentImage => "Image actuelle",
        TextKey::Custom => "Personnalisé",
        TextKey::ChannelLogoPreviewUnavailable => {
            "L’aperçu du logo de chaîne nécessite une chaîne. Choisissez ici une image personnalisée."
        }
        TextKey::DeleteWatermark => "Supprimer le filigrane",
        TextKey::DurationSeconds => "Durée (secondes)",
        TextKey::EditWatermark => "Modifier le filigrane",
        TextKey::EnableWatermarkToPreview => "Activez le filigrane pour afficher son emplacement.",
        TextKey::Frequency => "Fréquence",
        TextKey::GeneratingPreview => "Génération de l'aperçu...",
        TextKey::HorizontalMarginPercent => "Marge horizontale (%)",
        TextKey::GuideFourThree => "Cadre sûr 4:3",
        TextKey::Identity => "Identité",
        TextKey::Image => "Image",
        TextKey::ImageSource => "Source de l'image",
        TextKey::Intermittent => "Intermittent",
        TextKey::IntermittentTiming => "Rythme intermittent",
        TextKey::LeftMiddle => "À gauche au centre",
        TextKey::Location => "Emplacement",
        TextKey::MiddleCenter => "Au centre",
        TextKey::Mode => "Mode",
        TextKey::Minutes => "minutes",
        TextKey::Moods => "Ambiances",
        TextKey::NewName => "Nouveau nom",
        TextKey::NativeAssetDimensions => "Taille native et taille affichée",
        TextKey::NextWatermark => "Filigrane suivant",
        TextKey::Opacity => "Opacité",
        TextKey::OpacityExpression => "Expression d'opacité",
        TextKey::OpacityOneToHundred => "Opacité (1-100)",
        TextKey::Permanent => "Permanent",
        TextKey::PerPass => "par passage",
        TextKey::PlaceWithinSourceContent => "Placer dans le contenu source",
        TextKey::Placement => "Placement",
        TextKey::PlacementPreview => "Aperçu du placement",
        TextKey::PreviewFailed => "Échec de l’aperçu",
        TextKey::PreviewReferenceFullHd => "Cadre de référence Full HD · 1920×1080",
        TextKey::PreviousWatermark => "Filigrane précédent",
        TextKey::RightMiddle => "À droite au centre",
        TextKey::SaveWatermark => "Enregistrer le filigrane",
        TextKey::Scaled => "Mis à l'échelle",
        TextKey::SelectCustomImageToPreview => {
            "Sélectionnez ou importez une image personnalisée à prévisualiser."
        }
        TextKey::Size => "Taille",
        TextKey::TopLeft => "En haut à gauche",
        TextKey::TopMiddle => "En haut au centre",
        TextKey::TopRight => "En haut à droite",
        TextKey::UploadFailed => "Échec de l'envoi.",
        TextKey::UploadImage => "Envoyer une image",
        TextKey::UnsavedChanges => "Modifications non enregistrées",
        TextKey::UnsavedWatermarkChangesMessage => {
            "Abandonner les modifications du filigrane et ouvrir le filigrane sélectionné ?"
        }
        TextKey::VerticalMarginPercent => "Marge verticale (%)",
        TextKey::WatermarkCopied => "Filigrane copié.",
        TextKey::WatermarkNotFound => "Filigrane introuvable.",
        TextKey::WatermarkPlacementPreview => "aperçu du placement du filigrane",
        TextKey::WatermarkPreview => "aperçu du filigrane",
        TextKey::WatermarkNavigation => "Navigation entre les filigranes",
        TextKey::WidthPercent => "Largeur (%)",
        TextKey::ZIndex => "Z-Index",
        TextKey::DiscardChanges => "Abandonner les modifications",
        TextKey::XmltvBlockBehavior => "Comportement des blocs XMLTV",
        TextKey::XmltvTimeZone => "Fuseau horaire XMLTV",
        TextKey::Xmltv => "XMLTV",
        TextKey::Badge => "Badge",
        TextKey::Buttons => "Boutons",
        TextKey::CheckValue => "Case à cocher",
        TextKey::ChannelName => "Nom de la chaîne",
        TextKey::ChannelNumber => "Numéro de chaîne",
        TextKey::ComponentGallery => "Galerie de composants ErsatzRS",
        TextKey::DangerButton => "Danger",
        TextKey::DeleteDialog => "Boîte de dialogue de suppression",
        TextKey::Decrement => "Décrémenter",
        TextKey::Dialogs => "Boîtes de dialogue",
        TextKey::DisabledButton => "Désactivé",
        TextKey::EnterName => "Saisissez un nom",
        TextKey::ErrorOccurred => "Une erreur s'est produite.",
        TextKey::ErrorToast => "Toast d'erreur",
        TextKey::ExampleModal => "Exemple de fenêtre modale",
        TextKey::ExampleModalBody => "Ceci est un exemple de corps de fenêtre modale.",
        TextKey::FormInputs => "Champs de formulaire",
        TextKey::Icons => "Icônes",
        TextKey::IncrementBadge => "Incrémenter le badge",
        TextKey::InfoToast => "Toast d'information",
        TextKey::JustFyi => "Pour information.",
        TextKey::LinkButton => "Bouton lien",
        TextKey::ModalCancelled => "Annulé.",
        TextKey::ModalConfirmed => "Confirmé !",
        TextKey::NumValue => "Nombre",
        TextKey::OpenModal => "Ouvrir la fenêtre modale",
        TextKey::OperationSucceeded => "Opération réussie.",
        TextKey::PagedTableDemo => "PagedTable (5 lignes par page, 7 au total)",
        TextKey::PrimaryButton => "Principal",
        TextKey::SecondaryButton => "Secondaire",
        TextKey::SelectValue => "Sélection",
        TextKey::SomethingMightBeWrong => "Quelque chose ne va peut-être pas.",
        TextKey::SuccessToast => "Toast de succès",
        TextKey::TextButton => "Texte",
        TextKey::TextValue => "Texte",
        TextKey::Toasts => "Toasts",
        TextKey::WarningToast => "Toast d'avertissement",
        TextKey::YamlScript => "Script YAML",
        TextKey::Yes => "Oui",
    }
}

fn schedule_error_message(error: &str) -> String {
    match error.trim() {
        "Schedule name is required!" => "Le nom de la grille est obligatoire !".to_owned(),
        "New Schedule Name is required." => {
            "Le nom de la nouvelle grille est obligatoire.".to_owned()
        }
        "Schedule name must be 50 characters or less." => {
            "Le nom de la grille ne peut pas dépasser 50 caractères.".to_owned()
        }
        "Schedule name must be unique" => "Le nom de la grille doit être unique".to_owned(),
        "Schedule does not exist" => "La grille n'existe pas".to_owned(),
        "[ProgramScheduleId] does not exist." => "[ProgramScheduleId] n'existe pas.".to_owned(),
        "A collection must use the same playback order in every schedule item." => {
            "Une collection doit utiliser le même ordre de lecture dans chaque élément de grille."
                .to_owned()
        }
        "Shuffle In Order cannot be used with Fill With Group Mode." => {
            "Le mélange dans l'ordre ne peut pas être utilisé avec le mode remplissage par groupe."
                .to_owned()
        }
        "Fixed schedule items require a start time." => {
            "Les éléments de grille fixes nécessitent une heure de début.".to_owned()
        }
        "Multi Collection items can only use Shuffle or Shuffle In Order." => {
            "Les éléments de multi-collection ne peuvent utiliser que Mélange ou Mélange dans l'ordre."
                .to_owned()
        }
        "Playlist Item Size can only be used with Playlist schedule items." => {
            "La taille d'élément de liste de lecture ne peut être utilisée qu'avec des éléments de grille de type liste de lecture."
                .to_owned()
        }
        "Multiple playout mode with Count requires a count expression." => {
            "Le mode de diffusion multiple avec Nombre nécessite une expression de nombre.".to_owned()
        }
        "Duration playout mode requires a duration between 1 minute and 24 hours." => {
            "Le mode de diffusion par durée nécessite une durée entre 1 minute et 24 heures."
                .to_owned()
        }
        "Duration playout mode requires discard-to-fill attempts." => {
            "Le mode de diffusion par durée nécessite des tentatives de remplissage par rejet."
                .to_owned()
        }
        "Tail Mode Filler requires a tail filler preset." => {
            "Le mode tail Filler nécessite un préréglage de filler tail.".to_owned()
        }
        "Tail filler can only be set when Tail Mode is Filler." => {
            "Le filler tail ne peut être défini que lorsque le mode tail est Filler.".to_owned()
        }
        "Collection is required." => "La collection est obligatoire.".to_owned(),
        "Media item is required." => "L'élément multimédia est obligatoire.".to_owned(),
        "Multi Collection is required." => "La multi-collection est obligatoire.".to_owned(),
        "Smart Collection is required." => "La collection intelligente est obligatoire.".to_owned(),
        "Playlist is required." => "La liste de lecture est obligatoire.".to_owned(),
        "Rerun Collection is required." => {
            "La collection de rediffusions est obligatoire.".to_owned()
        }
        "Search Query is required." => "La requête de recherche est obligatoire.".to_owned(),
        "Only one pre-, mid-, or post-roll filler can pad to the nearest minute." => {
            "Un seul filler pre-roll, mid-roll ou post-roll peut compléter à la minute la plus proche."
                .to_owned()
        }
        "Padding filler requires a fallback filler." => {
            "Le filler de complément nécessite un filler de secours.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn scheduling_content_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("Block group name \"") && trimmed.ends_with("\" is invalid") {
        return trimmed.to_owned();
    }
    if trimmed.starts_with("Block name \"") && trimmed.ends_with("\" is invalid") {
        return trimmed.to_owned();
    }
    if trimmed.starts_with("Template name \"") && trimmed.ends_with("\" is invalid") {
        return trimmed.to_owned();
    }
    if trimmed.starts_with("A template named \"")
        && trimmed.ends_with("\" already exists in that template group")
    {
        return trimmed.to_owned();
    }
    if trimmed.starts_with("Block from ") && trimmed.contains(" intersects block from ") {
        return trimmed.to_owned();
    }
    if trimmed.starts_with("TemplateGroup ") && trimmed.ends_with(" does not exist.") {
        return "Le groupe de modèles n'existe pas.".to_owned();
    }
    if trimmed.starts_with("Template ") && trimmed.ends_with(" does not exist.") {
        return "Le modèle n'existe pas.".to_owned();
    }
    if trimmed.starts_with("DecoGroup ") && trimmed.ends_with(" does not exist.") {
        return "Le groupe de décos n'existe pas.".to_owned();
    }
    if trimmed.starts_with("Deco ") && trimmed.ends_with(" does not exist.") {
        return "La déco n'existe pas.".to_owned();
    }
    if trimmed.starts_with("DecoTemplateGroup ") && trimmed.ends_with(" does not exist.") {
        return "Le groupe de modèles de déco n'existe pas.".to_owned();
    }
    if trimmed.starts_with("DecoTemplate ") && trimmed.ends_with(" does not exist.") {
        return "Le modèle de déco n'existe pas.".to_owned();
    }
    if trimmed.starts_with("Deco name \"") && trimmed.ends_with("\" is invalid") {
        return "Le nom de la déco ne peut pas dépasser 50 caractères.".to_owned();
    }
    if trimmed.starts_with("A deco named \"")
        && trimmed.ends_with("\" already exists in that deco group")
    {
        return "Une déco portant ce nom existe déjà dans ce groupe de décos.".to_owned();
    }
    if trimmed.starts_with("Deco template name \"") && trimmed.ends_with("\" is invalid") {
        return "Le nom du modèle de déco ne peut pas dépasser 50 caractères.".to_owned();
    }
    if trimmed.starts_with("A deco template named \"")
        && trimmed.ends_with("\" already exists in that deco template group")
    {
        return "Un modèle de déco portant ce nom existe déjà dans ce groupe de modèles de déco."
            .to_owned();
    }
    if trimmed.starts_with("Watermark ") && trimmed.ends_with(" does not exist.") {
        return "Le filigrane n'existe pas.".to_owned();
    }
    if trimmed.starts_with("Graphics element ") && trimmed.ends_with(" does not exist.") {
        return "L'élément graphique n'existe pas.".to_owned();
    }
    if trimmed.starts_with("Deco from ") && trimmed.contains(" intersects deco from ") {
        return "La déco chevauche une autre déco.".to_owned();
    }

    match trimmed {
        "Block does not exist." => "Le bloc n'existe pas.".to_owned(),
        "Block group name is required" => "Le nom du groupe de blocs est obligatoire".to_owned(),
        "Block group name must be unique" => {
            "Le nom du groupe de blocs doit être unique".to_owned()
        }
        "Block name is required" => "Le nom du bloc est obligatoire".to_owned(),
        "A block named with that value already exists in that block group" => {
            "Un bloc portant ce nom existe déjà dans ce groupe de blocs".to_owned()
        }
        "Block duration must be between 5 minutes and 24 hours" => {
            "La durée du bloc doit être comprise entre 5 minutes et 24 heures".to_owned()
        }
        "[Collection] is required for collection type 'Collection'" => {
            "La collection est obligatoire pour le type de collection Collection.".to_owned()
        }
        "[MediaItem] is required for collection type 'TelevisionShow'" => {
            "L'élément multimédia est obligatoire pour le type de collection Série télévisée."
                .to_owned()
        }
        "[MediaItem] is required for collection type 'TelevisionSeason'" => {
            "L'élément multimédia est obligatoire pour le type de collection Saison de série."
                .to_owned()
        }
        "[MultiCollection] is required for collection type 'MultiCollection'" => {
            "La multi-collection est obligatoire pour le type de collection Multi-collection."
                .to_owned()
        }
        "[SmartCollection] is required for collection type 'SmartCollection'" => {
            "La collection intelligente est obligatoire pour le type de collection Collection intelligente."
                .to_owned()
        }
        "[SearchQuery] is required for collection type 'SearchQuery'" => {
            "La requête de recherche est obligatoire pour le type de collection Requête de recherche."
                .to_owned()
        }
        "[CollectionType] is invalid" => "Le type de collection est invalide.".to_owned(),
        "[BlockId] does not exist." => "Le bloc n'existe pas.".to_owned(),
        "Template does not exist." => "Le modèle n'existe pas.".to_owned(),
        "Template group name is required" => {
            "Le nom du groupe de modèles est obligatoire".to_owned()
        }
        "Template group name must be 50 characters or fewer" => {
            "Le nom du groupe de modèles ne peut pas dépasser 50 caractères".to_owned()
        }
        "Template group name must be unique" => {
            "Le nom du groupe de modèles doit être unique".to_owned()
        }
        "Template group does not exist." => "Le groupe de modèles n'existe pas.".to_owned(),
        "[TemplateId] does not exist." => "Le modèle n'existe pas.".to_owned(),
        "Template name is required" => "Le nom du modèle est obligatoire".to_owned(),
        "Template items cannot cross midnight." => {
            "Les éléments de modèle ne peuvent pas franchir minuit.".to_owned()
        }
        "Deco group name is required" => "Le nom du groupe de décos est obligatoire".to_owned(),
        "Deco group name must be 50 characters or fewer" => {
            "Le nom du groupe de décos ne peut pas dépasser 50 caractères".to_owned()
        }
        "Deco group name must be unique" => "Le nom du groupe de décos doit être unique".to_owned(),
        "Deco group does not exist." => "Le groupe de décos n'existe pas.".to_owned(),
        "Deco name is required" => "Le nom de la déco est obligatoire".to_owned(),
        "Deco does not exist" => "La déco n'existe pas.".to_owned(),
        "Default filler has unsupported collection type." => {
            "Le filler par défaut a un type de collection non pris en charge.".to_owned()
        }
        "Dead air fallback has unsupported collection type." => {
            "Le secours d'antenne vide a un type de collection non pris en charge.".to_owned()
        }
        "Break content has unsupported collection type." => {
            "Le contenu de coupure a un type de collection non pris en charge.".to_owned()
        }
        "Default filler must have valid item" => {
            "Le filler par défaut doit avoir un élément valide.".to_owned()
        }
        "Dead air fallback must have valid item" => {
            "Le secours d'antenne vide doit avoir un élément valide.".to_owned()
        }
        "Break content collection must have valid item" => {
            "La collection du contenu de coupure doit avoir un élément valide.".to_owned()
        }
        "Break content multi collection must have valid item" => {
            "La multi-collection du contenu de coupure doit avoir un élément valide.".to_owned()
        }
        "Break content smart collection must have valid item" => {
            "La collection intelligente du contenu de coupure doit avoir un élément valide."
                .to_owned()
        }
        "Break content media item must have valid item" => {
            "L'élément multimédia du contenu de coupure doit avoir un élément valide.".to_owned()
        }
        "Break content playlist must have valid item" => {
            "La liste de lecture du contenu de coupure doit avoir un élément valide.".to_owned()
        }
        "Default filler item does not exist." => {
            "L'élément du filler par défaut n'existe pas.".to_owned()
        }
        "Dead air fallback item does not exist." => {
            "L'élément du secours d'antenne vide n'existe pas.".to_owned()
        }
        "Break content collection item does not exist." => {
            "L'élément de collection du contenu de coupure n'existe pas.".to_owned()
        }
        "Break content multi collection item does not exist." => {
            "L'élément de multi-collection du contenu de coupure n'existe pas.".to_owned()
        }
        "Break content smart collection item does not exist." => {
            "L'élément de collection intelligente du contenu de coupure n'existe pas.".to_owned()
        }
        "Break content media item item does not exist." => {
            "L'élément multimédia du contenu de coupure n'existe pas.".to_owned()
        }
        "Break content playlist item does not exist." => {
            "La liste de lecture du contenu de coupure n'existe pas.".to_owned()
        }
        "Deco may only contain one [Block Start] break content" => {
            "Une déco ne peut contenir qu'un seul contenu de coupure Début de bloc.".to_owned()
        }
        "Deco may only contain one [Between Block Items] break content" => {
            "Une déco ne peut contenir qu'un seul contenu de coupure Entre les éléments de bloc."
                .to_owned()
        }
        "Deco may only contain one [At Chapter Markers] break content" => {
            "Une déco ne peut contenir qu'un seul contenu de coupure Aux marqueurs de chapitre."
                .to_owned()
        }
        "Deco may only contain one [Block Finish] break content" => {
            "Une déco ne peut contenir qu'un seul contenu de coupure Fin de bloc.".to_owned()
        }
        "Deco template group name is required" => {
            "Le nom du groupe de modèles de déco est obligatoire".to_owned()
        }
        "Deco template group name must be 50 characters or fewer" => {
            "Le nom du groupe de modèles de déco ne peut pas dépasser 50 caractères".to_owned()
        }
        "Deco template group name must be unique" => {
            "Le nom du groupe de modèles de déco doit être unique".to_owned()
        }
        "Deco template group does not exist." => {
            "Le groupe de modèles de déco n'existe pas.".to_owned()
        }
        "[DecoTemplateId] does not exist." => "Le modèle de déco n'existe pas.".to_owned(),
        "Deco template name is required" => "Le nom du modèle de déco est obligatoire".to_owned(),
        "[DecoId] does not exist." => "La déco n'existe pas.".to_owned(),
        "Deco template items cannot be empty." => {
            "Les éléments du modèle de déco ne peuvent pas être vides.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn watermark_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("Watermark ") && trimmed.ends_with(" does not exist.") {
        return "Filigrane introuvable.".to_owned();
    }

    match trimmed {
        "Name is required" => "Le nom est obligatoire".to_owned(),
        "Name must be 50 characters or fewer" => {
            "Le nom ne peut pas dépasser 50 caractères".to_owned()
        }
        "Watermark image is required" | "Watermark image is required!" => {
            "L'image du filigrane est obligatoire".to_owned()
        }
        "Width must be between 0 and 100%" => {
            "La largeur doit être comprise entre 0 et 100 %".to_owned()
        }
        "Horizontal margin must be between 0 and 50%" => {
            "La marge horizontale doit être comprise entre 0 et 50 %".to_owned()
        }
        "Vertical margin must be between 0 and 50%" => {
            "La marge verticale doit être comprise entre 0 et 50 %".to_owned()
        }
        "Duration must be positive and less than frequency window" => {
            "La durée doit être positive et inférieure à la fenêtre de fréquence".to_owned()
        }
        "Opacity must be between 1 and 100" => {
            "L'opacité doit être comprise entre 1 et 100".to_owned()
        }
        "Watermark name must be unique" | "ChannelWatermark name must be unique" => {
            "Le nom du filigrane doit être unique".to_owned()
        }
        "Watermark not found" | "Source watermark not found" | "Watermark does not exist." => {
            "Filigrane introuvable.".to_owned()
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
        _ => return format!("Inconnu ({value})"),
    }
    .to_owned()
}

fn ffmpeg_profile_video_format_label(format: i32) -> String {
    match format {
        0 => "Aucun",
        1 => "H264",
        2 => "H265 (HEVC)",
        3 => "MPEG-2",
        4 => "AV1",
        99 => text(TextKey::Copy),
        _ => return format!("Inconnu ({format})"),
    }
    .to_owned()
}

fn ffmpeg_profile_audio_format_label(format: i32) -> String {
    match format {
        0 => "Aucun",
        1 => "AAC",
        2 => "AC3",
        3 => "AAC (LATM)",
        99 => text(TextKey::Copy),
        _ => return format!("Inconnu ({format})"),
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
            "Le nombre de jours XMLTV à générer ({current}) ne peut pas dépasser le nombre de jours de diffusion à générer ({limit})"
        );
    }
    if let Some(name) = trimmed.strip_suffix(" path does not exist") {
        return format!("Le chemin {name} n'existe pas");
    }
    if let Some(name) = trimmed
        .strip_prefix("Unable to verify ")
        .and_then(|value| value.strip_suffix(" version"))
    {
        return format!("Impossible de vérifier la version de {name}");
    }
    if let Some(value) = trimmed.strip_prefix("Invalid option: ") {
        return format!("Option invalide : {value}");
    }
    if trimmed.starts_with("Resolution ") && trimmed.ends_with(" does not exist.") {
        return "La résolution n'existe pas.".to_owned();
    }

    match trimmed {
        "Windows startup registration is not supported on this platform" => {
            "L’inscription au démarrage de Windows n’est pas prise en charge sur cette plateforme."
                .to_owned()
        }
        "ErsatzRS.exe was not found next to ersatzrs-web.exe" => {
            "ErsatzRS.exe est introuvable à côté de ersatzrs-web.exe.".to_owned()
        }
        "Unable to update Windows startup registration" => {
            "Impossible de mettre à jour l’inscription au démarrage de Windows.".to_owned()
        }
        "Tuner count must be greater than zero" => {
            "Le nombre de tuners doit être supérieur à zéro".to_owned()
        }
        "Library refresh interval must be 0 (do not refresh) or greater than zero" => {
            "L'intervalle d'actualisation des bibliothèques doit être 0 (ne pas actualiser) ou supérieur à zéro".to_owned()
        }
        "Library refresh interval must be less than 1,000,000. Use 0 to disable automatic refresh" => {
            "L'intervalle d'actualisation des bibliothèques doit être inférieur à 1 000 000. Utilisez 0 pour désactiver l'actualisation automatique".to_owned()
        }
        "Playout days to build must be greater than zero" => {
            "Le nombre de jours de diffusion à générer doit être supérieur à zéro".to_owned()
        }
        "Scripted schedule timeout seconds must be at least 10" => {
            "Le délai d'expiration de la grille scriptée doit être d'au moins 10 secondes".to_owned()
        }
        "XMLTV days to build must be greater than zero" => {
            "Le nombre de jours XMLTV à générer doit être supérieur à zéro".to_owned()
        }
        "Resolution width or height is invalid" => {
            "La largeur ou la hauteur de la résolution est invalide".to_owned()
        }
        "Resolution width and height must be unique" => {
            "La largeur et la hauteur de la résolution doivent être uniques".to_owned()
        }
        "HLS Segmenter idle timeout must be greater than or equal to 30" => {
            "Le délai d'inactivité du segmenteur HLS doit être supérieur ou égal à 30".to_owned()
        }
        "Work-Ahead HLS Segmenter limit must be greater than or equal to 0" => {
            "La limite de transcodages HLS en avance doit être supérieure ou égale à 0".to_owned()
        }
        "Active HLS transcode limit must be greater than or equal to 1" => {
            "La limite de transcodages HLS actifs doit être supérieure ou égale à 1".to_owned()
        }
        "HLS Segmenter initial segment count must be greater than or equal to 1" => {
            "Le nombre de segments initiaux du segmenteur HLS doit être supérieur ou égal à 1".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn media_action_error_message(error: &str) -> String {
    match error.trim() {
        "Collection name is required" => "Le nom de la collection est obligatoire".to_owned(),
        "Collection name must be 50 characters or less" => {
            "Le nom de la collection ne peut pas dépasser 50 caractères".to_owned()
        }
        "Collection name must be unique" => "Le nom de la collection doit être unique".to_owned(),
        "Media item does not exist" => "L'élément multimédia n'existe pas".to_owned(),
        "Collection does not exist" | "Collection does not exist." => {
            "La collection n'existe pas.".to_owned()
        }
        "Playlist does not exist" | "Playlist does not exist." => {
            "La liste de lecture n'existe pas.".to_owned()
        }
        "Unable to locate media item" => "Impossible de trouver l'élément multimédia.".to_owned(),
        "Media action failed" => "Échec de l'action sur le média".to_owned(),
        _ => error.to_owned(),
    }
}

fn filler_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("FillerPreset ") && trimmed.ends_with(" does not exist.") {
        return "Le préréglage de filler n'existe pas.".to_owned();
    }

    match trimmed {
        "Filler preset does not exist" => "Le préréglage de filler n'existe pas.".to_owned(),
        "Filler preset name must be unique" => {
            "Le nom du préréglage de filler doit être unique".to_owned()
        }
        "Name must not be empty" => "Le nom ne peut pas être vide".to_owned(),
        "Name must be 50 characters or fewer" => {
            "Le nom ne peut pas dépasser 50 caractères".to_owned()
        }
        "Filler kind must not be None" => "Le type de filler ne peut pas être Aucun".to_owned(),
        "Filler mode must not be None" => "Le mode de filler ne peut pas être Aucun".to_owned(),
        "Count must be greater than or equal to 0" => {
            "Le nombre doit être supérieur ou égal à 0".to_owned()
        }
        "Duration must not be empty" => "La durée ne peut pas être vide".to_owned(),
        "Pad To Nearest Minute must not be empty" => {
            "Compléter à la minute la plus proche ne peut pas être vide".to_owned()
        }
        "Use Chapters As Media Items must not be enabled for fallback filler" => {
            "Utiliser les chapitres comme éléments multimédias ne peut pas être activé pour le filler de secours".to_owned()
        }
        "Collection must not be empty" => "La collection ne peut pas être vide".to_owned(),
        "Multi Collection must not be empty" => {
            "La multi-collection ne peut pas être vide".to_owned()
        }
        "Smart Collection must not be empty" => {
            "La collection intelligente ne peut pas être vide".to_owned()
        }
        "Media item must not be empty" => "L'élément multimédia ne peut pas être vide".to_owned(),
        _ => error.to_owned(),
    }
}

fn media_browser_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed == "Pinned stream selection is invalid or no longer available." {
        return "La sélection de flux épinglée est invalide ou n’est plus disponible.".to_owned();
    }
    if trimmed == "Movie was not found." {
        return "Le film est introuvable.".to_owned();
    }
    if trimmed.starts_with("Collection ") && trimmed.ends_with(" is already present.") {
        return "La collection est déjà présente.".to_owned();
    }
    if trimmed.starts_with("Collection ") && trimmed.ends_with(" does not exist.") {
        return "La collection n'existe pas.".to_owned();
    }
    if trimmed.starts_with("Smart collection ") && trimmed.ends_with(" is already present.") {
        return "La collection intelligente est déjà présente.".to_owned();
    }
    if trimmed.starts_with("Smart collection ") && trimmed.ends_with(" does not exist.") {
        return "La collection intelligente n'existe pas.".to_owned();
    }
    if trimmed.starts_with("MultiCollection ") && trimmed.ends_with(" does not exist.") {
        return "La multi-collection n'existe pas.".to_owned();
    }
    if trimmed.starts_with("Rerun collection ") && trimmed.ends_with(" does not exist.") {
        return "La collection de rediffusions n'existe pas.".to_owned();
    }
    if trimmed.starts_with("Referenced item ") && trimmed.ends_with(" does not exist.") {
        return "L'élément référencé n'existe pas.".to_owned();
    }
    if trimmed.starts_with("PlaylistGroup ") && trimmed.ends_with(" does not exist.") {
        return "Le groupe de listes de lecture n'existe pas.".to_owned();
    }
    if trimmed.starts_with("Playlist ") && trimmed.ends_with(" does not exist.") {
        return "La liste de lecture n'existe pas.".to_owned();
    }
    if trimmed.starts_with("Playlist name \"") && trimmed.ends_with("\" is invalid") {
        return "Le nom de la liste de lecture est invalide.".to_owned();
    }
    if let Some(error) = trimmed.strip_prefix("Playlist preview failed: ") {
        return format!("Échec de l'aperçu de la liste de lecture : {error}");
    }
    if trimmed.starts_with("Media item ") && trimmed.ends_with(" does not exist.") {
        return "L'élément multimédia n'existe pas.".to_owned();
    }

    match trimmed {
        "Collection name is required" => "Le nom de la collection est obligatoire".to_owned(),
        "Collection name must be 50 characters or less" => {
            "Le nom de la collection ne peut pas dépasser 50 caractères".to_owned()
        }
        "Collection name must be unique" => "Le nom de la collection doit être unique".to_owned(),
        "Collection does not exist." => "La collection n'existe pas.".to_owned(),
        "Smart collection name is required" => {
            "Le nom de la collection intelligente est obligatoire".to_owned()
        }
        "Smart collection query is required" => {
            "La requête de la collection intelligente est obligatoire".to_owned()
        }
        "SmartCollection name must be unique" => {
            "Le nom de la collection intelligente doit être unique".to_owned()
        }
        "MultiCollection does not exist." => "La multi-collection n'existe pas.".to_owned(),
        "Multi-collection name is required!" => {
            "Le nom de la multi-collection est obligatoire.".to_owned()
        }
        "Multi-collection name cannot be longer than 50 characters." => {
            "Le nom de la multi-collection ne peut pas dépasser 50 caractères.".to_owned()
        }
        "MultiCollection name must be unique" => {
            "Le nom de la multi-collection doit être unique".to_owned()
        }
        "Each multi-collection item must reference exactly one collection." => {
            "Chaque élément de multi-collection doit référencer exactement une collection."
                .to_owned()
        }
        "Rerun collection does not exist." => {
            "La collection de rediffusions n'existe pas.".to_owned()
        }
        "[Name] is an empty string" | "[Name] must not be empty." => {
            "Le nom ne peut pas être vide.".to_owned()
        }
        "[Name] must not be longer than 50" | "[Name] must not be longer than 50." => {
            "Le nom ne peut pas dépasser 50 caractères.".to_owned()
        }
        "Rerun collection name must be unique" => {
            "Le nom de la collection de rediffusions doit être unique".to_owned()
        }
        "Unsupported collection type." => "Type de collection non pris en charge.".to_owned(),
        "[ProgramScheduleId] does not exist." => "La grille n'existe pas.".to_owned(),
        "[MediaItem] does not exist." => "L'élément multimédia n'existe pas.".to_owned(),
        "[PlaylistGroupId] does not exist." => {
            "Le groupe de listes de lecture n'existe pas.".to_owned()
        }
        "Cannot delete system playlist group" => {
            "Impossible de supprimer le groupe de listes de lecture système".to_owned()
        }
        "Cannot delete system (generated) playlist" => {
            "Impossible de supprimer la liste de lecture système (générée)".to_owned()
        }
        "[PlaylistId] does not exist." => "La liste de lecture n'existe pas.".to_owned(),
        "Cannot save system (generated) playlist" => {
            "Impossible d'enregistrer la liste de lecture système (générée)".to_owned()
        }
        "Playlist playout preview has no items" => {
            "L'aperçu de diffusion de la liste de lecture ne contient aucun élément".to_owned()
        }
        "Playlist name is required" => "Le nom de la liste de lecture est obligatoire".to_owned(),
        "[Collection] is required for collection type 'Collection'" => {
            "La collection est obligatoire.".to_owned()
        }
        "[MultiCollection] is required for collection type 'MultiCollection'" => {
            "La multi-collection est obligatoire.".to_owned()
        }
        "[SmartCollection] is required for collection type 'SmartCollection'" => {
            "La collection intelligente est obligatoire.".to_owned()
        }
        "[CollectionType] is invalid" => "Le type de collection est invalide.".to_owned(),
        _ => error.to_owned(),
    }
}

fn media_source_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if let Some(error) = trimmed.strip_prefix("Could not reach Jellyfin: ") {
        return format!("Impossible de joindre Jellyfin : {error}");
    }

    match trimmed {
        "Address is required." => "L'adresse est obligatoire.".to_owned(),
        "API key is required." => "La clé API est obligatoire.".to_owned(),
        "'Address' must be a valid URL" => "L'adresse doit être une URL valide.".to_owned(),
        _ => error.to_owned(),
    }
}

fn addon_operation_error_message(code: &str, detail: &str) -> String {
    let summary = match code {
        "missing-setting" => "Configurez le paramètre d’add-on requis.",
        "missing-secret" => {
            "Configurez la référence secrète requise dans les paramètres de l’add-on."
        }
        "provider-unreachable" => "L’add-on n’a pas pu joindre son fournisseur.",
        "provider-rejected" => "Le fournisseur a rejeté la demande de l’add-on.",
        "provider-rate-limited" => "Le fournisseur a temporairement limité l’add-on.",
        "not-found" => "L’élément d’add-on demandé est introuvable.",
        "unsupported-url" => "L’add-on ne prend pas en charge cette URL.",
        _ => "L’add-on a signalé un échec.",
    };
    if detail.trim().is_empty() {
        summary.to_owned()
    } else {
        format!("{summary} {detail}")
    }
}

fn addon_media_list_error_message(error: &str) -> String {
    let trimmed = error.trim();
    let normalized = trimmed.strip_prefix("conflict: ").unwrap_or(trimmed);
    if let Some(error) = normalized.strip_prefix("failed to decode media-list add-on output: ") {
        return format!("L’add-on a renvoyé des données de liste non valides : {error}");
    }
    if normalized.starts_with("add-on media list with id ") && normalized.ends_with(" not found") {
        return "La liste add-on n'existe pas.".to_owned();
    }

    if let Some(folder) =
        normalized.strip_prefix("another list of this add-on already uses the folder ")
    {
        return format!("Une autre liste de cette extension utilise déjà le dossier « {folder} ».");
    }
    if let Some(folder) = normalized.strip_suffix(" is not usable as a folder name") {
        return format!("« {folder} » n'est pas utilisable comme nom de dossier.");
    }

    match normalized {
        "Invalid add-on list url" => "URL de liste add-on invalide.".to_owned(),
        "add-on API client is not configured for this operation" => {
            "Le client API add-on n'est pas configuré pour cette opération.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn upload_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if let Some(error) = trimmed.strip_prefix("Cannot create upload directory: ") {
        return format!("Impossible de créer le répertoire d'envoi : {error}");
    }
    if let Some(error) = trimmed.strip_prefix("Write failed: ") {
        return format!("Échec de l'écriture : {error}");
    }
    if let Some(error) = trimmed.strip_prefix("Read field: ") {
        return format!("Échec de la lecture du champ : {error}");
    }

    match trimmed {
        "Upload exceeds maximum allowed size" => {
            "L'envoi dépasse la taille maximale autorisée.".to_owned()
        }
        "No file field" => "Aucun champ de fichier trouvé.".to_owned(),
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
        return trimmed.to_owned();
    }

    match trimmed {
        "Profile name is required." => "Le nom du profil est obligatoire.".to_owned(),
        "Profile name must be 50 characters or fewer." => {
            "Le nom du profil ne peut pas dépasser 50 caractères.".to_owned()
        }
        "Thread count must be 0 or greater." => {
            "Le nombre de threads doit être supérieur ou égal à 0.".to_owned()
        }
        "QSV supports formats (h264, hevc, mpeg2video)" => {
            "QSV prend en charge les formats (h264, hevc, mpeg2video).".to_owned()
        }
        "NVENC supports formats (h264, hevc, av1)" => {
            "NVENC prend en charge les formats (h264, hevc, av1).".to_owned()
        }
        "VAAPI supports formats (h264, hevc, av1, mpeg2video)" => {
            "VAAPI prend en charge les formats (h264, hevc, av1, mpeg2video).".to_owned()
        }
        "VideoToolbox supports formats (h264, hevc)" => {
            "VideoToolbox prend en charge les formats (h264, hevc).".to_owned()
        }
        "Amf supports formats (h264, hevc)" => {
            "AMF prend en charge les formats (h264, hevc).".to_owned()
        }
        "V4L2 M2M supports formats (h264, hevc)" => {
            "V4L2 M2M prend en charge les formats (h264, hevc).".to_owned()
        }
        "Rkmpp supports formats (h264, hevc)" => {
            "RKMPP prend en charge les formats (h264, hevc).".to_owned()
        }
        "HDR10 output requires hevc or av1; h264 and mpeg2video cannot carry HDR10 static metadata" => {
            "La sortie HDR10 nécessite hevc ou av1 ; h264 et mpeg2video ne peuvent pas porter de métadonnées statiques HDR10.".to_owned()
        }
        "HDR10 output requires 10-bit; 8-bit cannot represent a PQ signal without banding" => {
            "La sortie HDR10 nécessite 10 bits ; 8 bits ne peut pas représenter un signal PQ sans bandes.".to_owned()
        }
        "Mastering display must look like G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1), with chromaticities up to 50000 and max luminance above min" => {
            "L'écran de mastering doit ressembler à G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1), avec des chromaticités jusqu'à 50000 et une luminance max supérieure à la min.".to_owned()
        }
        "MaxCLL and MaxFALL must both be set (or both left empty), at most 65535, with MaxFALL no greater than MaxCLL" => {
            "MaxCLL et MaxFALL doivent être renseignés tous les deux (ou laissés vides tous les deux), au maximum 65535, avec MaxFALL au plus égal à MaxCLL.".to_owned()
        }
        "Mpeg2Video does not support 10-bit content" => {
            "Mpeg2Video ne prend pas en charge le contenu 10 bits.".to_owned()
        }
        "VideoProfile must be high10 with 10-bit h264" => {
            "Le profil vidéo doit être high10 avec h264 10 bits.".to_owned()
        }
        "VideoProfile must be high444p with NVIDIA 10-bit h264" => {
            "Le profil vidéo doit être high444p avec h264 10 bits NVIDIA.".to_owned()
        }
        "VideoProfile cannot be high10 with 8-bit h264" => {
            "Le profil vidéo ne peut pas être high10 avec h264 8 bits.".to_owned()
        }
        "Source profile not found." => "Profil source introuvable.".to_owned(),
        "No FFmpeg resolutions are available; database bootstrap is incomplete" => {
            "Aucune résolution FFmpeg n'est disponible ; l'initialisation de la base de données est incomplète".to_owned()
        }
        _ => error.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::i18n::{
        addon_media_list_error_text, episode_display_subtitle_text, ffmpeg_loudness_mode_text,
        ffmpeg_profile_audio_format_text, ffmpeg_profile_error_text,
        ffmpeg_profile_video_format_text, ffmpeg_tonemap_algorithm_text, filler_error_text,
        hardware_acceleration_option_text, library_kind_text, log_level_text,
        media_action_error_text, media_browser_error_text, media_kind_text,
        media_source_error_text, playout_schedule_kind_text, schedule_error_text,
        scheduling_content_error_text, settings_error_text, upload_error_text,
        watermark_error_text, UiLanguage,
    };

    #[test]
    fn season_display_names() {
        assert_eq!(STRINGS.season_display_name(0), "Épisodes spéciaux");
        assert_eq!(STRINGS.season_display_name(2), "Saison 2");
    }

    #[test]
    fn episode_display_subtitles() {
        assert_eq!(
            episode_display_subtitle_text(UiLanguage::French, Some("Série"), Some(2), Some(5))
                .as_deref(),
            Some("Série - S02E05")
        );
        assert_eq!(
            episode_display_subtitle_text(UiLanguage::French, None, Some(2), Some(5)).as_deref(),
            Some("S02E05")
        );
        assert_eq!(
            episode_display_subtitle_text(UiLanguage::French, Some("Série"), None, None).as_deref(),
            Some("Série - Non numéroté")
        );
    }

    #[test]
    fn fallback_item_labels() {
        assert_eq!(
            STRINGS.media_item_fallback_label(42),
            "Élément multimédia #42"
        );
        assert_eq!(
            STRINGS.schedule_item_fallback_label(7),
            "Élément de grille #7"
        );
    }

    #[test]
    fn log_level_labels() {
        assert_eq!(
            log_level_text(UiLanguage::French, "Warning"),
            "Avertissement"
        );
        assert_eq!(log_level_text(UiLanguage::French, "TRACE"), "TRACE");
    }

    #[test]
    fn channel_labels() {
        assert_eq!(STRINGS.channel_language_label("fra"), "Français");
        assert_eq!(STRINGS.channel_streaming_mode_label(99), "Inconnu (99)");
        assert_eq!(STRINGS.channel_language_label("tlh"), "tlh");
    }

    #[test]
    fn media_and_library_kind_labels() {
        assert_eq!(media_kind_text(UiLanguage::French, 1), "Films");
        assert_eq!(media_kind_text(UiLanguage::French, 99), "Inconnu");
        assert_eq!(library_kind_text(UiLanguage::French, "Local"), "Local");
    }

    #[test]
    fn playout_schedule_kind_labels() {
        assert_eq!(
            playout_schedule_kind_text(UiLanguage::French, 1),
            "Classique"
        );
        assert_eq!(
            playout_schedule_kind_text(UiLanguage::French, 20),
            "JSON (dizqueTV)"
        );
        assert_eq!(playout_schedule_kind_text(UiLanguage::French, 99), "");
    }

    #[test]
    fn hardware_acceleration_labels() {
        assert_eq!(
            hardware_acceleration_option_text(UiLanguage::French, "NVENC", false),
            "NVENC (non disponible sur ce système)"
        );
        assert_eq!(
            hardware_acceleration_option_text(UiLanguage::French, "VAAPI", true),
            "VAAPI"
        );
    }

    #[test]
    fn ffmpeg_option_labels() {
        assert_eq!(
            ffmpeg_tonemap_algorithm_text(UiLanguage::French, 99),
            "Inconnu (99)"
        );
        assert_eq!(
            ffmpeg_loudness_mode_text(UiLanguage::French, 0),
            "Désactivé"
        );
        assert_eq!(
            ffmpeg_profile_video_format_text(UiLanguage::French, 42, 0),
            "Inconnu (42)"
        );
        assert_eq!(
            ffmpeg_profile_video_format_text(UiLanguage::French, 1, 2),
            "H264 / NVENC"
        );
        assert_eq!(
            ffmpeg_profile_audio_format_text(UiLanguage::French, 99),
            "Copier"
        );
    }

    #[test]
    fn deco_template_action_labels() {
        assert_eq!(
            STRINGS.text(TextKey::AddDecoToTemplate),
            "Ajouter une déco au modèle"
        );
    }

    #[test]
    fn smart_shuffle_policy_labels() {
        assert_eq!(
            STRINGS.text(TextKey::SmartShuffle),
            "Lecture aléatoire intelligente"
        );
        assert_eq!(
            STRINGS.text(TextKey::SmartShuffleAfterItems),
            "Vidéos entre les éléments similaires"
        );
    }

    #[test]
    fn server_errors_follow_language_without_changing_unknown_values() {
        assert_eq!(
            ffmpeg_profile_error_text(UiLanguage::French, "Source profile not found."),
            "Profil source introuvable."
        );
        assert_eq!(
            ffmpeg_profile_error_text(UiLanguage::French, "custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            schedule_error_text(
                UiLanguage::French,
                "Fixed schedule items require a start time."
            ),
            "Les éléments de grille fixes nécessitent une heure de début."
        );
        assert_eq!(
            schedule_error_text(UiLanguage::French, "custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            scheduling_content_error_text(
                UiLanguage::French,
                "[Collection] is required for collection type 'Collection'"
            ),
            "La collection est obligatoire pour le type de collection Collection."
        );
        assert_eq!(
            scheduling_content_error_text(UiLanguage::French, "Template 12 does not exist."),
            "Le modèle n'existe pas."
        );
        assert_eq!(
            scheduling_content_error_text(
                UiLanguage::French,
                "Break content playlist item does not exist."
            ),
            "La liste de lecture du contenu de coupure n'existe pas."
        );
        assert_eq!(
            scheduling_content_error_text(UiLanguage::French, "custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            watermark_error_text(UiLanguage::French, "Watermark 12 does not exist."),
            "Filigrane introuvable."
        );
        assert_eq!(
            watermark_error_text(UiLanguage::French, "custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            settings_error_text(UiLanguage::French, "ffmpeg path does not exist"),
            "Le chemin ffmpeg n'existe pas"
        );
        assert_eq!(
            settings_error_text(UiLanguage::French, "custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            media_action_error_text(UiLanguage::French, "Unable to locate media item"),
            "Impossible de trouver l'élément multimédia."
        );
        assert_eq!(
            media_action_error_text(UiLanguage::French, "custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            filler_error_text(UiLanguage::French, "FillerPreset 42 does not exist."),
            "Le préréglage de filler n'existe pas."
        );
        assert_eq!(
            filler_error_text(UiLanguage::French, "custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            media_browser_error_text(
                UiLanguage::French,
                "[MultiCollection] is required for collection type 'MultiCollection'"
            ),
            "La multi-collection est obligatoire."
        );
        assert_eq!(
            media_browser_error_text(UiLanguage::French, "[ProgramScheduleId] does not exist."),
            "La grille n'existe pas."
        );
        assert_eq!(
            media_browser_error_text(UiLanguage::French, "custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            media_source_error_text(UiLanguage::French, "'Address' must be a valid URL"),
            "L'adresse doit être une URL valide."
        );
        assert_eq!(
            media_source_error_text(UiLanguage::French, "custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            addon_media_list_error_text(
                UiLanguage::French,
                "add-on media list with id 42 not found"
            ),
            "La liste add-on n'existe pas."
        );
        assert_eq!(
            addon_media_list_error_text(UiLanguage::French, "custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            upload_error_text(UiLanguage::French, "Read field: connection reset"),
            "Échec de la lecture du champ : connection reset"
        );
        assert_eq!(
            upload_error_text(UiLanguage::French, "custom upload failure"),
            "custom upload failure"
        );
    }
}
