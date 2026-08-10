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
            Some(format!("{show} - saison {season}, épisode {episode}"))
        }
        (Some(season), Some(episode)) => Some(format!("Saison {season}, épisode {episode}")),
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

    fn trakt_error_message(&self, error: &str) -> String {
        trakt_error_message(error)
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
            "L’état de l’énumérateur de diffusion classique est incomplet pour : {playouts}. Le planificateur doit actualiser ces diffusions avant qu’elles puissent continuer en toute sécurité."
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
        TextKey::AddTraktList => "Ajouter une liste Trakt",
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
        },
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
        TextKey::BlockPreviewFailed => "Impossible de générer l'aperçu du bloc. Vérifiez les éléments du bloc et réessayez.",
        TextKey::Blocks => "Blocs",
        TextKey::BlockToRemove => "Bloc à retirer",
        TextKey::BlockWillNotFit => {
            "Le bloc ne tient pas à partir de l'heure de début sélectionnée."
        },
        TextKey::Cancel => "Annuler",
        TextKey::Clear => "Effacer",
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
        },
        TextKey::BreakContentItem => "Élément de contenu de coupure",
        TextKey::BreakContentMode => "Mode du contenu de coupure",
        TextKey::DeadAirFallback => "Secours d'antenne vide",
        TextKey::DeadAirFallbackBody => {
            "Lorsqu'aucun élément de diffusion n'est trouvé pour l'heure actuelle, un élément est choisi au hasard dans cette collection, lu en boucle et coupé pour occuper exactement le temps jusqu'au début de l'élément de diffusion suivant. Cela remplace l'image « Chaîne hors ligne » qui s'afficherait sinon."
        },
        TextKey::DeadAirFallbackCollectionType => "Type de collection du secours d'antenne vide",
        TextKey::DeadAirFallbackMode => "Mode du secours d'antenne vide",
        TextKey::Deco => "Déco",
        TextKey::DecoCannotBeAdded => {
            "La déco ne peut pas être ajoutée car elle chevaucherait une autre ou franchirait minuit."
        },
        TextKey::DecoCannotBeMoved => {
            "La déco ne peut pas être déplacée ici car elle chevaucherait une autre ou franchirait minuit."
        },
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
        },
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
        },
        TextKey::UseWatermarkDuringFiller => "Utiliser le filigrane pendant le filler",
        TextKey::WatermarkMode => "Mode du filigrane",
        TextKey::DefaultFfmpegProfile => "Profil FFmpeg par défaut",
        TextKey::DefaultFfmpegProfileHelp => {
            "Le profil FFmpeg à utiliser lors de la création de nouvelles chaînes"
        },
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
        TextKey::DeleteTraktList => "Supprimer la liste Trakt",
        TextKey::DisabledCollectionsAlreadyPresent => {
            "Les collections désactivées sont déjà présentes dans cette multi-collection"
        },
        TextKey::EmptyTrash => "Vider la corbeille",
        TextKey::ExtractEmbeddedSubtitles => "Extraire les sous-titres (texte) intégrés",
        TextKey::ErasedPlayoutItemsAndHistory => "Éléments de diffusion et historique effacés.",
        TextKey::EraseFailed => "Échec de l'effacement",
        TextKey::EraseItemsAndHistory => "Effacer les éléments et l'historique",
        TextKey::ErasePlayoutItemsAndHistoryConfirm => {
            "Cela supprimera tous les éléments de diffusion et l'historique de cette diffusion séquentielle. Continuer ?"
        },
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
        },
        TextKey::FillerPresetDeleted => "Préréglage de filler supprimé.",
        TextKey::FillerPresetExpressionHelp => {
            "Pour le filler mid-roll, n'ajouter du filler que lorsque cette expression est vraie pour un point mid-roll donné."
        },
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
        },
        TextKey::HlsSegmenterInitialSegmentCount => "Nombre de segments initiaux du segmenteur HLS",
        TextKey::HlsSegmenterInitialSegmentCountHelp => {
            "Retarde le démarrage du flux jusqu'à ce que le nombre indiqué de segments (de 4 secondes) ait été transcodé ; une valeur plus élevée ralentit le démarrage initial mais peut réduire les mises en mémoire tampon"
        },
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
        TextKey::MatchTraktListItems => "Faire correspondre les éléments de la liste Trakt",
        TextKey::Members => "Membres",
        TextKey::MultiCollection => "Multi-collection",
        TextKey::MultiCollectionDeleted => "Multi-collection supprimée.",
        TextKey::MultiCollectionItems => "Éléments de la multi-collection",
        TextKey::MultiCollectionNameRequired => "Le nom de la multi-collection est obligatoire !",
        TextKey::MultiCollectionNameTooLong => {
            "Le nom de la multi-collection ne peut pas dépasser 50 caractères."
        },
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
        TextKey::NoTraktListsYet => "Aucune liste Trakt pour l'instant.",
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
        },
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
        },
        TextKey::RerunCollectionDeleted => "Collection de rediffusions supprimée.",
        TextKey::RerunCollectionNameRequired => {
            "Le nom de la collection de rediffusions est obligatoire !"
        },
        TextKey::RerunCollectionSaved => "Collection de rediffusions enregistrée.",
        TextKey::RerunPlaybackOrder => "Ordre de lecture des rediffusions",
        TextKey::RemoteStreams => "Flux distants",
        TextKey::RemoteStream => "Flux distant",
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
        TextKey::ManagedManifestReadOnly => "Ce manifeste rédigé manuellement est en lecture seule ici.",
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
        },
        TextKey::SaveAsSmartCollection => "Enregistrer comme collection intelligente",
        TextKey::SearchQueryRequired => "La requête de recherche est obligatoire",
        TextKey::SaveSettings => "Enregistrer les paramètres",
        TextKey::Scanner => "Scanner",
        TextKey::ScanningMinimumLogLevel => "Niveau de journalisation minimal de l'analyse",
        TextKey::Scheduling => "Planification",
        TextKey::SchedulingMinimumLogLevel => "Niveau de journalisation minimal de la planification",
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
        },
        TextKey::FillWithGroupMode => "Mode remplissage par groupe (série ou artiste)",
        TextKey::GraphicsElements => "Éléments graphiques",
        TextKey::GraphicsIds => "ID graphiques",
        TextKey::Genres => "Genres",
        TextKey::GroupName => "Nom du groupe",
        TextKey::GroupMultiPartEpisodesHelp => {
            "Regrouper les épisodes en plusieurs parties lors du mélange"
        },
        TextKey::GuideMode => "Mode guide",
        TextKey::KeepMultiPartEpisodesTogether => {
            "Garder ensemble les épisodes en plusieurs parties"
        },
        TextKey::Marathon => "Marathon",
        TextKey::MarathonBatchSize => "Taille de lot du marathon",
        TextKey::MarathonBatchSizeHelp => {
            "Nombre d'éléments à lire de chaque groupe avant de passer au suivant ; vide ou zéro lit tous les éléments"
        },
        TextKey::MarathonGroupBy => "Regroupement du marathon",
        TextKey::MarathonShuffleGroups => "Mélanger les groupes du marathon",
        TextKey::MarathonShuffleItems => "Mélanger les éléments du marathon",
        TextKey::MidRollFiller => "Filler mid-roll",
        TextKey::MidRoll => "Mid-roll",
        TextKey::MoveDown => "Déplacer vers le bas",
        TextKey::MoveUp => "Déplacer vers le haut",
        TextKey::MultiEpisodeGroupSize => "Taille de groupe multi-épisodes",
        TextKey::MultiEpisodeShuffle => "Mélange multi-épisodes",
        TextKey::Multiple => "Multiple",
        TextKey::MultipleCount => "Nombre multiple",
        TextKey::MultipleCountHelp => {
            "Peut être une expression avec « count » (taille de la collection) et « random » (0 à count-1)"
        },
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
        TextKey::RemainingDurationHelp => {
            "durée du contenu après ce point mid-roll, en secondes"
        },
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
        },
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
        },
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
        TextKey::SmartShuffleValidation => "Choisissez un seuil d'espacement et au moins une règle de similarité.",
        TextKey::SmartShuffleScopeContentSource => "Source de contenu",
        TextKey::SmartShuffleAfterItems => "Vidéos entre les éléments similaires",
        TextKey::SmartShuffleAfterDurationMinutes => "Durée de l'intervalle (minutes)",
        TextKey::SmartShuffleIncludeFiller => "Inclure le filler",
        TextKey::SmartShuffleInheritedBlock => {
            "Cet élément n'a pas de remplacement Smart Shuffle et conserve toute stratégie de diffusion héritée."
        }
        TextKey::SmartShuffleInactive => "La lecture aléatoire intelligente est inactive car cet ordre de lecture n'est pas aléatoire.",
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
        TextKey::SeasonNotFound => "Saison introuvable",
        TextKey::Select => "Sélectionner",
        TextKey::SelectCollection => "Sélectionner une collection",
        TextKey::SelectGroupForNewTemplate => "Sélectionnez un groupe pour le nouveau modèle",
        TextKey::SelectScheduleItemPrompt => {
            "Sélectionnez un élément de grille à modifier, ou ajoutez-en un nouveau."
        },
        TextKey::SelectSchedule => "Sélectionner une grille",
        TextKey::SelectSmartCollection => "Sélectionner une collection intelligente",
        TextKey::Shuffle => "Mélange",
        TextKey::ShuffleInOrder => "Mélange dans l'ordre",
        TextKey::ShuffleScheduleItems => "Mélanger les éléments de grille",
        TextKey::ShuffleScheduleItemsHelp => {
            "Remarque : cela désactive les heures de début fixes et le mode flood"
        },
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
        },
        TextKey::StrictWaitExactStartTime => "Strict - attendre l'heure de début exacte",
        TextKey::TailFiller => "Filler tail",
        TextKey::TailMode => "Mode tail",
        TextKey::Tail => "Tail",
        TextKey::TelevisionSeason => "Saison de série",
        TextKey::TelevisionShow => "Série télévisée",
        TextKey::TreatCollectionsAsShows => "Traiter les collections comme des séries",
        TextKey::TreatCollectionsAsShowsHelp => {
            "Utile pour les épisodes crossover en plusieurs parties"
        },
        TextKey::Type => "Type",
        TextKey::TypeToSearch => "Saisissez pour rechercher...",
        TextKey::Search => "Rechercher",
        TextKey::SearchMedia => "Rechercher des médias",
        TextKey::SearchMoviesOrShows => "Rechercher des films ou des séries...",
        TextKey::SearchTraktList => "Rechercher une liste Trakt",
        TextKey::SearchingMinimumLogLevel => "Niveau de journalisation minimal de la recherche",
        TextKey::Settings => "Paramètres",
        TextKey::SettingsSaveFailed => "Échec de l'enregistrement des paramètres",
        TextKey::ShowMediaInfo => "Afficher les infos média",
        TextKey::ScriptedScheduleTimeout => "Délai d'expiration de la grille scriptée",
        TextKey::SkipMissingItems => "Ignorer les éléments manquants",
        TextKey::SkipMissingItemsHelp => {
            "Détermine si les éléments introuvables ou indisponibles doivent être inclus dans les diffusions"
        },
        TextKey::SmartCollections => "Collections intelligentes",
        TextKey::SmartCollection => "Collection intelligente",
        TextKey::SmartCollectionDeleted => "Collection intelligente supprimée.",
        TextKey::SmartCollectionQueryPlaceholder => "p. ex. genre:action OR genre:comedy",
        TextKey::SmartCollectionSaved => "Collection intelligente enregistrée.",
        TextKey::SaveTraktList => "Enregistrer la liste Trakt",
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
        TextKey::ThisCollectionHasNoItemsYet => "Cette collection ne contient encore aucun élément.",
        TextKey::Thumbnail => "vignette",
        TextKey::Title => "Titre",
        TextKey::ToggleNavigation => "Basculer la navigation",
        TextKey::TotalDurationHelp => "durée totale du contenu, en secondes",
        TextKey::TotalPointsHelp => "nombre total de points mid-roll potentiels",
        TextKey::TotalProgressHelp => "position normalisée de 0 à 1",
        TextKey::TraktList => "Liste Trakt",
        TextKey::TraktListActionFailed => "Échec de l'action sur la liste Trakt.",
        TextKey::TraktListAdded => "Liste Trakt ajoutée.",
        TextKey::TraktListDeleted => "Liste Trakt supprimée.",
        TextKey::TraktListMatched => "Liste Trakt mise en correspondance.",
        TextKey::TraktListNotFound => "Liste Trakt introuvable.",
        TextKey::TraktListSaveFailed => "Échec de l'enregistrement de la liste Trakt.",
        TextKey::TraktListSaved => "Liste Trakt enregistrée.",
        TextKey::TraktLists => "Listes Trakt",
        TextKey::TraktListUrl => "URL de la liste Trakt",
        TextKey::Trash => "Corbeille",
        TextKey::TrashAlreadyEmpty => "La corbeille est déjà vide.",
        TextKey::TrashEmpty => "La corbeille est vide.",
        TextKey::TrashEmptyConfirm => {
            "Supprimer de la base de données tous les éléments dont le fichier est introuvable ?"
        },
        TextKey::TrashEmptyConfirmHelp => {
            "Cela supprime définitivement les éléments multimédias actuellement manquants et leurs métadonnées dépendantes."
        },
        TextKey::TrashSubtitle => {
            "Éléments dont les fichiers source sont actuellement absents du disque ou du fournisseur."
        },
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
        },
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
        },
        TextKey::ActiveTranscodeSuggestionHardware => {
            "Encodeur matériel détecté ; il s'agit d'une valeur suggérée, pas d'un changement automatique."
        },
        TextKey::ActiveTranscodeSuggestionSoftware => {
            "Encodage logiciel uniquement ; il s'agit d'une valeur suggérée, pas d'un changement automatique."
        },
        TextKey::Peak => "pic",
        TextKey::StreamsInUse => "Flux en cours d'utilisation",
        TextKey::SuggestedTranscodeLimit => "Valeur suggérée",
        TextKey::Watermarks => "Filigranes",
        TextKey::Width => "Largeur",
        TextKey::WorkAheadSegmenterLimit => "Transcodages HLS en avance",
        TextKey::WorkAheadSegmenterLimitHelp => {
            "Nombre d'emplacements de transcodage HLS pouvant travailler en avance à vitesse maximale pour constituer le tampon. Le travail en avance cède toujours la priorité aux flux regardés."
        },
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
        TextKey::OutputDynamicRangeCopyHint => "La vidéo est copiée, la sortie conserve donc la plage dynamique de la source.",
        TextKey::HdrStaticMetadataHint => "Laissez vide pour utiliser les valeurs HDR10 documentées par défaut. Elles ne s'appliquent que si la source ne porte aucune métadonnée HDR ; l'écran de mastering et les niveaux de la source priment toujours.",
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
        TextKey::EditTraktListProperties => "Modifier les propriétés de la liste Trakt",
        TextKey::EnterTraktListUrl => "Saisissez l'URL de la liste Trakt",
        TextKey::EnterSearchQuery => "Saisissez une requête de recherche.",
        TextKey::Episodes => "Épisodes",
        TextKey::LoadingCollections => "Chargement des collections...",
        TextKey::LoadingPlaylistGroups => "Chargement des groupes de listes de lecture...",
        TextKey::LoadingPlaylists => "Chargement des listes de lecture...",
        TextKey::GeneratePlaylist => "Générer une liste de lecture",
        TextKey::GeneratePlaylistFromSortedTraktItems => {
            "Générer une liste de lecture à partir des éléments triés de la liste Trakt"
        },
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
        },
        TextKey::SelectSearchPlaylistHelp => {
            "Sélectionnez la liste de lecture où ajouter les résultats de recherche."
        },
        TextKey::Selected => "Sélectionné",
        TextKey::SelectCollectionToAddThisMovie => {
            "Sélectionnez une collection pour y ajouter ce film."
        },
        TextKey::SelectPlaylistToAddThisMovie => {
            "Sélectionnez une liste de lecture pour y ajouter ce film."
        },
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
        },
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
        },
        TextKey::NoSchedulingContext => "Aucun contexte de planification",
        TextKey::NoPlayoutItemsInWindow => {
            "Aucun élément de diffusion dans cette fenêtre de 24 heures"
        },
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
        },
        TextKey::ChannelDisabled => "Chaîne désactivée",
        TextKey::ChannelHiddenFromEpg => "Chaîne masquée de l'EPG",
        TextKey::ChannelLogo => "Logo de la chaîne",
        TextKey::ChannelLogoPreview => "Aperçu du logo de la chaîne",
        TextKey::ChannelLogoUploadFailed => {
            "Échec de l'envoi du logo de la chaîne. Vérifiez le fichier et réessayez."
        },
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
        },
        TextKey::HealthPersistedRunHelp => {
            "Résultats actuels des vérifications d'état de parité. Cette page s'actualise après que les analyses de médias ont mis à jour l'état des éléments."
        },
        TextKey::HistoryDetails => "Détails de l'historique",
        TextKey::HlsDirect => "HLS Direct",
        TextKey::HlsSegmenter => "Segmenteur HLS",
        TextKey::Hours => "heures",
        TextKey::IdleBehavior => "Comportement en inactivité",
        TextKey::IdleBehaviorHelp => {
            "Détermine le comportement du processus de transcodage après la déconnexion de tous les clients"
        },
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
        },
        TextKey::NoBlocks => "Aucun bloc trouvé.",
        TextKey::NoHistory => "Aucun historique trouvé.",
        TextKey::NoMessages => "Aucun message.",
        TextKey::NvidiaCapabilitiesNotCollected => {
            "Les capacités NVIDIA n'ont pas encore été collectées."
        },
        TextKey::Number => "Numéro",
        TextKey::OnDemand => "À la demande",
        TextKey::MatchedPointsHelp => {
            "nombre de points mid-roll ayant déjà satisfait l'expression"
        },
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
        },
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
        TextKey::PreviewMediaError => "Le navigateur ne peut pas décoder la sortie configurée de cette chaîne.",
        TextKey::PreviewDerivativeNote => {
            "L'aperçu d'administration utilise une version H.264/AAC compatible avec les navigateurs ; la sortie configurée reste inchangée."
        }
        TextKey::JavascriptRequiredPlayback => "JavaScript est requis pour la lecture interactive.",
        TextKey::ConfirmStopTranscodeSession => "Arrêter la session de transcodage partagée de cette chaîne ?",
        TextKey::PreviewPlaylistPlayout => "Aperçu de la diffusion de la liste de lecture",
        TextKey::PreferredAudioTitle => "Titre audio préféré",
        TextKey::PreferredSubtitleLanguage => "Langue de sous-titres préférée",
        TextKey::QsvCapabilitiesNotCollected => {
            "Les capacités QSV n'ont pas encore été collectées."
        },
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
        },
        TextKey::Song => "Chanson",
        TextKey::SongVideoMode => "Mode vidéo des chansons",
        TextKey::SortByLevel => "Trier par niveau",
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
        },
        TextKey::UnableToLoadMediaInfo => "Impossible de charger les infos média",
        TextKey::UnableToLoadPlaybackTroubleshooting => {
            "Impossible de charger le contexte de dépannage de lecture"
        },
        TextKey::UpdateListFromTraktDaily => {
            "Mettre à jour la liste depuis trakt.tv une fois par jour"
        },
        TextKey::UploadLogo => "Envoyer un logo",
        TextKey::UploadLogoHelp => {
            "Choisissez une image locale pour remplacer le logo de chaîne stocké lors de l'enregistrement"
        },
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
        TextKey::EnableWatermarkToPreview => {
            "Activez le filigrane pour afficher son emplacement."
        }
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

fn trakt_error_message(error: &str) -> String {
    let trimmed = error.trim();
    let normalized = trimmed.strip_prefix("conflict: ").unwrap_or(trimmed);
    if let Some(error) = normalized.strip_prefix("failed to decode trakt http: ") {
        return format!("Impossible de joindre Trakt : {error}");
    }
    if normalized.starts_with("trakt list with id ") && normalized.ends_with(" not found") {
        return "La liste Trakt n'existe pas.".to_owned();
    }

    match normalized {
        "Invalid Trakt list url" => "URL de liste Trakt invalide.".to_owned(),
        "Trakt ClientId is not configured; set TRAKT__CLIENTID" => {
            "Le ClientId Trakt n'est pas configuré ; définissez TRAKT__CLIENTID.".to_owned()
        }
        "Trakt API client is not configured for this operation" => {
            "Le client API Trakt n'est pas configuré pour cette opération.".to_owned()
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
        episode_display_subtitle_text, ffmpeg_loudness_mode_text, ffmpeg_profile_audio_format_text,
        ffmpeg_profile_error_text, ffmpeg_profile_video_format_text, ffmpeg_tonemap_algorithm_text,
        filler_error_text, hardware_acceleration_option_text, library_kind_text, log_level_text,
        media_action_error_text, media_browser_error_text, media_kind_text,
        media_source_error_text, playout_schedule_kind_text, schedule_error_text,
        scheduling_content_error_text, settings_error_text, trakt_error_text, upload_error_text,
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
            Some("Série - saison 2, épisode 5")
        );
        assert_eq!(
            episode_display_subtitle_text(UiLanguage::French, None, Some(2), Some(5)).as_deref(),
            Some("Saison 2, épisode 5")
        );
        assert_eq!(
            episode_display_subtitle_text(UiLanguage::French, Some("Série"), None, None).as_deref(),
            Some("Série")
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
            trakt_error_text(UiLanguage::French, "trakt list with id 42 not found"),
            "La liste Trakt n'existe pas."
        );
        assert_eq!(
            trakt_error_text(UiLanguage::French, "custom database failure"),
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
