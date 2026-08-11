use crate::admin::i18n::{LanguageStrings, TextKey};

pub const SORT_ORDER: u16 = 60;
pub const CODE: &str = "de-de";
pub const LABEL: &str = "Deutsch (Deutschland)";
pub const ALIASES: &[&str] = &["de", "de-de", "de_de"];
pub const TRANSLATION_COMPLETE: bool = false;

pub struct GermanStrings;

pub static STRINGS: GermanStrings = GermanStrings;

fn episode_display_subtitle(
    show_title: Option<&str>,
    season_number: Option<i64>,
    episode_number: Option<i64>,
) -> Option<String> {
    let show = show_title.unwrap_or("").trim();
    match (season_number, episode_number) {
        (Some(season), Some(episode)) if !show.is_empty() => {
            Some(format!("{show} - Staffel {season}, Folge {episode}"))
        }
        (Some(season), Some(episode)) => Some(format!("Staffel {season}, Folge {episode}")),
        _ => (!show.is_empty()).then(|| show.to_owned()),
    }
}

impl LanguageStrings for GermanStrings {
    fn text(&self, key: TextKey) -> &'static str {
        text(key)
    }

    fn selected_count(&self, count: usize) -> String {
        format!("{count} ausgewählt")
    }

    fn item_count(&self, count: u64) -> String {
        if count == 1 {
            "1 Element".to_owned()
        } else {
            format!("{count} Elemente")
        }
    }

    fn range_of(&self, start: u32, end: u32, total: u64) -> String {
        format!("{start}-{end} von {total}")
    }

    fn match_status(&self, matched: u64, total: u64) -> String {
        format!("{matched} von {total}")
    }

    fn rows_per_page(&self, page_size: u32) -> String {
        format!("Zeilen pro Seite: {page_size}")
    }

    fn page_of(&self, page: u32, total: u32) -> String {
        format!("Seite {page} von {total}")
    }

    fn save_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Speichern fehlgeschlagen: {error}")
    }

    fn delete_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Löschen fehlgeschlagen: {error}")
    }

    fn empty_trash_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Papierkorb konnte nicht geleert werden: {error}")
    }

    fn search_count(&self, total: u32, label: &'static str) -> String {
        format!("{total} {label}")
    }

    fn selected_items_count(&self, count: usize) -> String {
        if count == 1 {
            "1 Element ausgewählt".to_owned()
        } else {
            format!("{count} Elemente ausgewählt")
        }
    }

    fn delete_selected_count(&self, count: usize) -> String {
        format!("Ausgewählte löschen ({count})")
    }

    fn deleted_items_from_database(&self, count: u64) -> String {
        if count == 1 {
            "1 Element aus der Datenbank gelöscht.".to_owned()
        } else {
            format!("{count} Elemente aus der Datenbank gelöscht.")
        }
    }

    fn season_display_name(&self, number: i64) -> String {
        if number == 0 {
            "Spezialfolgen".to_owned()
        } else {
            format!("Staffel {number}")
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
        format!("Medienelement #{id}")
    }

    fn schedule_item_fallback_label(&self, id: i64) -> String {
        format!("Sendeplanelement #{id}")
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
            1 => "MPEG-TS (Legacy)".to_owned(),
            2 => "HLS direkt".to_owned(),
            4 => "HLS-Segmenter".to_owned(),
            5 => "MPEG-TS".to_owned(),
            _ => format!("Unbekannt ({mode})"),
        }
    }

    fn channel_language_label(&self, code: &str) -> String {
        match code.trim().to_ascii_lowercase().as_str() {
            "" => String::new(),
            "eng" | "en" => "Englisch".to_owned(),
            "nld" | "dut" | "nl" => "Niederländisch".to_owned(),
            "deu" | "ger" | "de" => "Deutsch".to_owned(),
            "fra" | "fre" | "fr" => "Französisch".to_owned(),
            "spa" | "es" => "Spanisch".to_owned(),
            "ita" | "it" => "Italienisch".to_owned(),
            "jpn" | "ja" => "Japanisch".to_owned(),
            "kor" | "ko" => "Koreanisch".to_owned(),
            "zho" | "chi" | "zh" => "Chinesisch".to_owned(),
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
            _ => "Unbekannt",
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
            format!("{label} (auf diesem System nicht verfügbar)")
        }
    }

    fn ffmpeg_tonemap_algorithm_label(&self, algorithm: i32) -> String {
        match algorithm {
            0 => "Linear",
            1 => "Clip",
            2 => "Gamma",
            3 => "Reinhard",
            4 => "Mobius",
            5 => "Hable",
            _ => return format!("Unbekannt ({algorithm})"),
        }
        .to_owned()
    }

    fn ffmpeg_output_dynamic_range_label(&self, range: i32) -> String {
        match range {
            0 => "SDR (BT.709)",
            1 => "HDR10 (BT.2020 PQ)",
            _ => return format!("Unbekannt ({range})"),
        }
        .to_owned()
    }

    fn ffmpeg_loudness_mode_label(&self, mode: i32) -> String {
        match mode {
            0 => text(TextKey::Off),
            1 => "loudnorm",
            _ => return format!("Unbekannt ({mode})"),
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
        format!("Papierkorb geleert: {count} Elemente gelöscht.")
    }

    fn select_collection_to_add_selected_items(&self, count: usize) -> String {
        format!(
            "Sammlung auswählen, der die {count} ausgewählten Elemente hinzugefügt werden sollen"
        )
    }

    fn select_playlist_to_add_selected_items(&self, count: usize) -> String {
        format!("Wiedergabeliste auswählen, der die {count} ausgewählten Elemente hinzugefügt werden sollen")
    }

    fn added_items_to_collection(&self, count: usize) -> String {
        format!("{count} Elemente zur Sammlung hinzugefügt")
    }

    fn added_items_to_playlist(&self, count: usize) -> String {
        format!("{count} Elemente zur Wiedergabeliste hinzugefügt")
    }

    fn add_to_collection_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Hinzufügen zur Sammlung fehlgeschlagen: {error}")
    }

    fn add_to_playlist_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Hinzufügen zur Wiedergabeliste fehlgeschlagen: {error}")
    }

    fn no_media_source_configured(&self, provider: &'static str) -> String {
        format!("Keine {provider}-Quelle konfiguriert. Klicken Sie auf Verbindung konfigurieren, um eine hinzuzufügen.")
    }

    fn provider_connection_title(&self, provider: &'static str) -> String {
        format!("{provider}-Verbindung")
    }

    fn provider_libraries_title(&self, provider: &'static str) -> String {
        format!("{provider}-Bibliotheken")
    }

    fn provider_path_replacements_title(&self, provider: &'static str) -> String {
        format!("{provider}-Pfadersetzungen")
    }

    fn provider_path_label(&self, provider: &'static str) -> String {
        format!("{provider}-Pfad")
    }

    fn health_summary(&self, errors: u32, warnings: u32) -> String {
        format!(
            "{} Fehler, {} Warnung{}",
            errors,
            warnings,
            if warnings == 1 { "" } else { "en" },
        )
    }

    fn classic_playout_state_message(&self, playouts: &str) -> String {
        format!(
            "Der Enumeratorzustand der klassischen Ausspielung ist unvollständig für: {playouts}. Der Planer muss diese Ausspielungen aktualisieren, bevor sie sicher fortgesetzt werden können."
        )
    }

    fn unable_to_load_playback_troubleshooting(&self, error: &dyn std::fmt::Display) -> String {
        format!("Kontext für die Wiedergabefehlerbehebung konnte nicht geladen werden: {error}")
    }

    fn playout_reset_prompt(&self, channel: &str, schedule: &str) -> String {
        format!(
            "Geben Sie die Uhrzeit ein, zu der die Ausspielung auf Kanal {channel} mit Sendeplan {schedule} täglich zurückgesetzt werden soll"
        )
    }
}

pub fn text(key: TextKey) -> &'static str {
    match key {
        TextKey::Addon => "Add-on",
        TextKey::Addons => "Add-ons",
        TextKey::AddonRepositories => "Add-on-Repositorys",
        TextKey::AvailableAddons => "Verfügbare Add-ons",
        TextKey::InstalledAddons => "Installierte Add-ons",
        TextKey::AddRepository => "Repository hinzufügen",
        TextKey::RepositoryUrl => "Repository-URL",
        TextKey::PublicSigningKey => "Öffentlicher Signaturschlüssel",
        TextKey::DeploymentManagedRepositories => {
            "Repositorys werden durch die Bereitstellungskonfiguration verwaltet."
        }
        TextKey::Refresh => "Aktualisieren",
        TextKey::Install => "Installieren",
        TextKey::UpdateAddon => "Aktualisieren",
        TextKey::Uninstall => "Deinstallieren",
        TextKey::Configuration => "Konfiguration",
        TextKey::SecretReference => "Secret-Referenz",
        TextKey::EnvironmentVariable => "Umgebungsvariable",
        TextKey::SecretFile => "Secret-Datei",
        TextKey::Incompatible => "Inkompatibel",
        TextKey::PackageUnavailable => "Paket nicht verfügbar",
        TextKey::Official => "Offiziell",
        TextKey::ThirdParty => "Drittanbieter-Repository",
        TextKey::Permissions => "Berechtigungen",
        TextKey::Capabilities => "Funktionen",
        TextKey::Dependencies => "Abhängigkeiten",
        TextKey::Optional => "Optional",
        TextKey::MediaReadAccess => "Lesezugriff auf Medien",
        TextKey::ExternalCommands => "Externe Befehle",
        TextKey::ThirdPartyRepositoryWarning => {
            "Fügen Sie nur vertrauenswürdige Repositorys hinzu; signierte Add-ons werden unter dem ErsatzRS-Dienstkonto ausgeführt."
        }
        TextKey::AddCustomResolution => "Benutzerdefinierte Auflösung hinzufügen",
        TextKey::AddBlock => "Block hinzufügen",
        TextKey::AddBlockGroup => "Block-Gruppe hinzufügen",
        TextKey::AddBlockItem => "Block-Element hinzufügen",
        TextKey::AddBlockToTemplate => "Block zur Vorlage hinzufügen",
        TextKey::AddCollection => "Sammlung hinzufügen",
        TextKey::AddContent => "Inhalt hinzufügen",
        TextKey::AddFailed => "Hinzufügen fehlgeschlagen",
        TextKey::AddFillerPreset => "Füllervorlage hinzufügen",
        TextKey::AddMultiCollection => "Mehrsammlung hinzufügen",
        TextKey::AddPlaylist => "Wiedergabeliste hinzufügen",
        TextKey::AddPlaylistGroup => "Wiedergabelisten-Gruppe hinzufügen",
        TextKey::AddPlaylistItem => "Wiedergabelistenelement hinzufügen",
        TextKey::AddRerunCollection => "Wiederholungssammlung hinzufügen",
        TextKey::AddTemplate => "Vorlage hinzufügen",
        TextKey::AddTemplateGroup => "Vorlagen-Gruppe hinzufügen",
        TextKey::AddTraktList => "Trakt-Liste hinzufügen",
        TextKey::AddToCollection => "Zur Sammlung hinzufügen",
        TextKey::AddToEllipsis => "Zu ... hinzufügen",
        TextKey::AddToPlaylist => "Zur Wiedergabeliste hinzufügen",
        TextKey::AddToSchedule => "Zum Zeitplan hinzufügen",
        TextKey::Actions => "Aktionen",
        TextKey::All => "Alle",
        TextKey::AreYouSureDelete => "Möchten Sie wirklich löschen?",
        TextKey::Actors => "Schauspieler",
        TextKey::Artist => "Künstler",
        TextKey::ArtistNotFound => "Künstler nicht gefunden",
        TextKey::AfterDurationEnd => "Nach Ende der Dauer",
        TextKey::AutomaticRefresh => "Automatischer Aktualisierungszyklus",
        TextKey::Back => "Zurück",
        TextKey::BackToSettings => "Zurück zu den Einstellungen",
        TextKey::BackToArtists => "Zurück zu den Künstlern",
        TextKey::BackToCollections => "Zurück zu den Sammlungen",
        TextKey::BackToMovies => "Zurück zu den Filmen",
        TextKey::BackToOtherVideos => "Zurück zu anderen Videos",
        TextKey::BackToTvShows => "Zurück zu TV-Sendungen",
        TextKey::BeforeDurationEnd => "Vor Ende der Dauer",
        TextKey::BackToTemplates => "Zurück zu den Vorlagen",
        TextKey::Block => "Block",
        TextKey::BlockGroup => "Block-Gruppe",
        TextKey::BlockCannotBeMoved => {
            "Der Block kann nicht dorthin verschoben werden, da dies zu einer Überlappung führen oder die Mitternacht überschreiten würde."
        },
        TextKey::BlockCopied => "Block kopiert.",
        TextKey::BlockCreated => "Block erstellt.",
        TextKey::BlockDeleted => "Block gelöscht.",
        TextKey::BlockGroupCreated => "Block-Gruppe erstellt.",
        TextKey::BlockGroupDeleted => "Block-Gruppe gelöscht.",
        TextKey::BlockGroupName => "Block-Gruppenname",
        TextKey::BlockGroups => "Block-Gruppen",
        TextKey::BlockItem => "Block-Element",
        TextKey::BlockItems => "Block-Elemente",
        TextKey::BlockName => "Block-Name",
        TextKey::BlockNotFound => "Block nicht gefunden.",
        TextKey::BlockPreviewFailed => "Die Block-Vorschau konnte nicht generiert werden. Überprüfen Sie die Block-Elemente und versuchen Sie es erneut.",
        TextKey::Blocks => "Blöcke",
        TextKey::BlockToRemove => "Block zum Entfernen",
        TextKey::BlockWillNotFit => "Der Block passt nicht zum oder nach dem ausgewählten Startzeitpunkt.",
        TextKey::Cancel => "Abbrechen",
        TextKey::Clear => "Löschen",
        TextKey::Channels => "Kanäle",
        TextKey::Close => "Schließen",
        TextKey::Collections => "Sammlungen",
        TextKey::CollectionDeleted => "Sammlung gelöscht.",
        TextKey::CollectionSaved => "Sammlung gespeichert.",
        TextKey::SaveCollection => "Sammlung speichern",
        TextKey::ComputingPreview => "Vorschau wird berechnet...",
        TextKey::Confirm => "Bestätigen",
        TextKey::ConfirmDelete => "Löschen bestätigen",
        TextKey::CommentaryTracks => "Kommentarspuren",
        TextKey::Content => "Inhalt",
        TextKey::ContentVariantAcoustic => "Akustisch",
        TextKey::ContentVariantAlternateVersion => "Version",
        TextKey::ContentVariantBehindTheScenes => "Hinter den Kulissen",
        TextKey::ContentVariantDeletedScene => "Gelöschte Szene",
        TextKey::ContentVariantDirectorsCut => "Regie-Schnitt",
        TextKey::ContentVariantExtended => "Extended",
        TextKey::ContentVariantExtra => "Extra",
        TextKey::ContentVariantFeaturette => "Featurette",
        TextKey::ContentVariantInterview => "Interview",
        TextKey::ContentVariantLive => "Live",
        TextKey::ContentVariantOfficialVideo => "Offizielles Video",
        TextKey::ContentVariantOther => "Sonstiges",
        TextKey::ContentVariantRemaster => "Remaster",
        TextKey::ContentVariantRemix => "Remix",
        TextKey::ContentVariantScene => "Szene",
        TextKey::ContentVariantSearchHelp => {
            "Varianten-Token: content_kind:extra, content_kind:alternate_version, sub_kind:featurette, sub_kind:unplugged, parent_title:\"Movie Title\", audio_title:commentary, subtitle_title:commentary, has_commentary:true"
        }
        TextKey::ContentVariantShort => "Kurz",
        TextKey::ContentVariantSpecialFeatures => "Extras / Besondere Features",
        TextKey::ContentVariantTrailer => "Trailer",
        TextKey::ContentVariantUnplugged => "Unplugged",
        TextKey::ContentVariantVersions => "Versionen",
        TextKey::ContentRatings => "Altersfreigaben",
        TextKey::CustomResolutionHeight => "Benutzerdefinierte Auflösungshöhe",
        TextKey::CustomResolutionWidth => "Benutzerdefinierte Auflösungsbreite",
        TextKey::CustomResolutions => "Benutzerdefinierte Auflösungen",
        TextKey::Dark => "Dunkel",
        TextKey::DaysToBuild => "Tage bis Fertigstellung",
        TextKey::AddBreakContent => "Pauseninhalte hinzufügen",
        TextKey::AddDeco => "Deko hinzufügen",
        TextKey::AddDecoGroup => "Deko-Gruppe hinzufügen",
        TextKey::AddDecoTemplate => "Deko-Vorlage hinzufügen",
        TextKey::AddDecoTemplateGroup => "Deko-Vorlagen-Gruppe hinzufügen",
        TextKey::AtChapterMarkers => "Bei Kapitelmarkern",
        TextKey::BackToDecos => "Zurück zu Dekos",
        TextKey::BackToDecoTemplates => "Zurück zu Deko-Vorlagen",
        TextKey::BetweenBlockItems => "Zwischen Block-Elementen",
        TextKey::BlockFinish => "Blockende",
        TextKey::BlockStart => "Blockstart",
        TextKey::BreakContent => "Pauseninhalte",
        TextKey::BreakContentBody => {
            "Pauseninhalte können verwendet werden, um bestimmte Elemente abzuspielen (unter Verwendung von Wiedergabelisten) und Blöcke auf ihre vollständig konfigurierte Dauer aufzufüllen."
        },
        TextKey::BreakContentItem => "Pauseninhaltelement",
        TextKey::BreakContentMode => "Pauseninhalte-Modus",
        TextKey::DeadAirFallback => "Fallback bei Totzeit",
        TextKey::DeadAirFallbackBody => {
            "Wenn kein Abspielinhalt für die aktuelle Zeit gefunden wird, wird zufällig ein Element aus dieser Sammlung ausgewählt, geloopt und genau auf die Dauer bis zum Start des nächsten Abspielinhalts zugeschnitten. Dies ersetzt das Bild „Kanal ist offline\", das sonst angezeigt würde."
        },
        TextKey::DeadAirFallbackCollectionType => "Typ der Fallback-Sammlung bei Totzeit",
        TextKey::DeadAirFallbackMode => "Fallback-Modus bei Totzeit",
        TextKey::Deco => "Deko",
        TextKey::DecoCannotBeAdded => {
            "Die Deko kann nicht hinzugefügt werden, da dies zu einer Überlappung führen oder die Mitternacht überschreiten würde."
        },
        TextKey::DecoCannotBeMoved => {
            "Die Deko kann nicht dorthin verschoben werden, da dies zu einer Überlappung führen oder die Mitternacht überschreiten würde."
        },
        TextKey::DecoCreated => "Deko erstellt.",
        TextKey::DecoDeleted => "Deko gelöscht.",
        TextKey::DecoGroup => "Deko-Gruppe",
        TextKey::DecoGroupCreated => "Deko-Gruppe erstellt.",
        TextKey::DecoGroupDeleted => "Deko-Gruppe gelöscht.",
        TextKey::DecoGroupName => "Deko-Gruppenname",
        TextKey::DecoGroups => "Deko-Gruppen",
        TextKey::DecoName => "Deko-Name",
        TextKey::DecoNotFound => "Deco not found.",
        TextKey::DecoSaved => "Deco saved.",
        TextKey::Decos => "Dekorationen",
        TextKey::DecoTemplate => "Dekoration-Vorlage",
        TextKey::DecoTemplateCreated => "Dekoration-Vorlage erstellt.",
        TextKey::DecoTemplateDeleted => "Dekoration-Vorlage gelöscht.",
        TextKey::DecoTemplateGroup => "Dekoration-Vorlagengruppe",
        TextKey::DecoTemplateGroupCreated => "Dekoration-Vorlagengruppe erstellt.",
        TextKey::DecoTemplateGroupDeleted => "Dekoration-Vorlagengruppe gelöscht.",
        TextKey::DecoTemplateGroupName => "Name der Dekoration-Vorlagengruppe",
        TextKey::DecoTemplateGroups => "Dekoration-Vorlagengruppen",
        TextKey::DecoTemplateName => "Name der Dekoration-Vorlage",
        TextKey::DecoTemplateNotFound => "Dekoration-Vorlage nicht gefunden.",
        TextKey::DecoTemplateSaved => "Dekoration-Vorlage gespeichert.",
        TextKey::AddDecoToTemplate => "Dekoration zur Vorlage hinzufügen",
        TextKey::DecoToRemove => "Dekoration zum Entfernen",
        TextKey::DefaultFiller => "Standardfüller",
        TextKey::DefaultFillerBody => {
            "Nach dem Planen aller Blöcke wird ein zweiter Durchlauf durchgeführt, um unplanbare Zeiten mit zufälligen Elementen aus dieser Sammlung zu füllen."
        },
        TextKey::DefaultFillerCollectionType => "Typ der Standardfüllersammlung",
        TextKey::DefaultFillerMode => "Modus des Standardfüllers",
        TextKey::DeleteDeco => "Dekoration löschen",
        TextKey::DeleteDecoGroup => "Deko-Gruppe löschen",
        TextKey::DeleteDecoTemplate => "Dekoration-Vorlage löschen",
        TextKey::DeleteDecoTemplateGroup => "Dekoration-Vorlagengruppe löschen",
        TextKey::Disable => "Deaktivieren",
        TextKey::EditDeco => "Dekoration bearbeiten",
        TextKey::EditDecoTemplate => "Dekoration-Vorlage bearbeiten",
        TextKey::GraphicsElementsMode => "Modus für grafische Elemente",
        TextKey::Inherit => "Vererben",
        TextKey::Merge => "Zusammenführen",
        TextKey::NoDecosFound => "Keine Dekorationen gefunden.",
        TextKey::NoDecoTemplatesFound => "Keine Dekoration-Vorlagen gefunden.",
        TextKey::Override => "Überschreiben",
        TextKey::RemoveBreakContent => "Inhalt der Pause entfernen",
        TextKey::RemoveDecoFromTemplate => "Dekoration aus Vorlage entfernen",
        TextKey::Replace => "Ersetzen",
        TextKey::SavedDecos => "Gespeicherte Dekorationen",
        TextKey::SavedDecoTemplates => "Gespeicherte Dekoration-Vorlagen",
        TextKey::SearchForDecos => "Dekorationen suchen",
        TextKey::SearchForDecoTemplates => "Dekoration-Vorlagen suchen",
        TextKey::TrimToFit => "Zuschneiden zum Anpassen",
        TextKey::UseGraphicsElementsDuringFiller => "Grafische Elemente während des Füllens verwenden",
        TextKey::UseWatermarkDuringFiller => "Wasserzeichen während des Füllens verwenden",
        TextKey::WatermarkMode => "Modus des Wasserzeichens",
        TextKey::DecoTemplates => "Dekoration-Vorlagen",
        TextKey::DefaultFfmpegProfile => "Standardprofil FFmpeg",
        TextKey::DefaultFfmpegProfileHelp => {
            "Das Profil FFmpeg, das beim Erstellen neuer Kanäle verwendet wird"
        },
        TextKey::DefaultMinimumLogLevel => "Standardmäßige minimale Protokollstufe",
        TextKey::DefaultMpegTsScript => "Standard-Skript MPEG-TS",
        TextKey::DefaultMpegTsScriptHelp => "Das Skript MPEG-TS, das beim Streaming verwendet wird",
        TextKey::Decorations => "Dekorationen",
        TextKey::Debug => "Debug",
        TextKey::Delete => "Löschen",
        TextKey::DeleteAll => "Alles löschen",
        TextKey::DeleteBlock => "Block löschen",
        TextKey::DeleteBlockGroup => "Blockgruppe löschen",
        TextKey::DeleteCustomResolution => "Benutzerdefinierte Auflösung löschen",
        TextKey::DeleteCollection => "Sammlung löschen",
        TextKey::DeleteFillerPreset => "Füllervorlage löschen",
        TextKey::DeleteMultiCollection => "Mehrfachsammlung löschen",
        TextKey::DeletePlaylist => "Wiedergabeliste löschen",
        TextKey::DeletePlaylistGroup => "Wiedergabelistengruppe löschen",
        TextKey::DeleteRerunCollection => "Wiedergabesammlung löschen",
        TextKey::DeleteTemplate => "Vorlage löschen",
        TextKey::DeleteTemplateGroup => "Vorlagengruppe löschen",
        TextKey::DeleteTraktList => "Trakt-Liste löschen",
        TextKey::DisabledCollectionsAlreadyPresent => {
            "Deaktivierte Sammlungen sind bereits in dieser Mehrfachsammlung vorhanden"
        },
        TextKey::EmptyTrash => "Papierkorb leeren",
        TextKey::ExtractEmbeddedSubtitles => "Eingebettete (Text-)Untertitel extrahieren",
        TextKey::ErasedPlayoutItemsAndHistory => "Wiedergabeartikel und Verlauf gelöscht.",
        TextKey::EraseFailed => "Löschen fehlgeschlagen",
        TextKey::EraseItemsAndHistory => "Artikel und Verlauf löschen",
        TextKey::ErasePlayoutItemsAndHistoryConfirm => {
            "Dies löscht alle Wiedergabeartikel und den Verlauf für diese sequenzielle Wiedergabe. Fortfahren?"
        },
        TextKey::Episode => "Folge",
        TextKey::FanArt => "Fan-Art",
        TextKey::Ffmpeg => "FFmpeg",
        TextKey::FileNotFound => "Datei nicht gefunden",
        TextKey::Finish => "Beenden",
        TextKey::Fatal => "Fatal",
        TextKey::FfmpegPath => "FFmpeg-Pfad",
        TextKey::FfmpegPathHelp => "Der vollständige Pfad zur ffmpeg-Executable-Datei",
        TextKey::FfmpegProfiles => "FFmpeg-Profile",
        TextKey::FfprobePath => "FFprobe-Pfad",
        TextKey::FfprobePathHelp => "Der vollständige Pfad zur ffprobe-Executable-Datei",
        TextKey::FillerPreset => "Füllervorlage",
        TextKey::FillerPresetActionFailed => "Die Aktion der Füllervorlage ist fehlgeschlagen.",
        TextKey::FillerPresetCouldNotBeSaved => {
            "Die Füllervorlage konnte nicht gespeichert werden. Überprüfen Sie die Formularwerte und versuchen Sie es erneut."
        },
        TextKey::FillerPresetDeleted => "Füllervorlage gelöscht.",
        TextKey::FillerPresetExpressionHelp => {
            "Für Füller in der Mitte der Rolle wird nur dann ein Füller hinzugefügt, wenn dieser Ausdruck für einen gegebenen Punkt in der Mitte der Rolle den Wert true ergibt."
        },
        TextKey::FillerPresetSaved => "Füllervorlage gespeichert.",
        TextKey::FillerPresets => "Füllervorlagen",
        TextKey::Flags => "Flaggen",
        TextKey::GlobalFallbackFiller => "Globaler Fallback-Füller",
        TextKey::GlobalWatermark => "Globales Wasserzeichen",
        TextKey::GoHome => "Nach Hause gehen",
        TextKey::HdHomeRun => "HDHomeRun",
        TextKey::HealthChecks => "Gesundheitsüberprüfungen",
        TextKey::Height => "Höhe",
        TextKey::HlsDirectOutputFormat => "Direktes Ausgabeformat HLS",
        TextKey::HlsSegmenterIdleTimeout => "Segmenter-Leerlauf-Timeout HLS",
        TextKey::HlsSegmenterIdleTimeoutHelp => {
            "Die Anzahl der Sekunden, während der die Transkodierung eines Kanals fortgesetzt wird, während keine Anfragen von einem Client empfangen wurden"
        },
        TextKey::HlsSegmenterInitialSegmentCount => "Anzahl der initialen Segmente des Segmenters HLS",
        TextKey::HlsSegmenterInitialSegmentCountHelp => {
            "Verschiebt den Start des Streams, bis die angegebene Anzahl von (4-Sekunden-)Segmenten transkodiert wurden; eine größere Anzahl bedeutet eine langsamere initiale Wiedergabe, kann aber potenziell weniger Pufferung bedeuten"
        },
        TextKey::HomeRunning => "Der IPTV-Server läuft.",
        TextKey::HttpMinimumLogLevel => "Minimale Protokollierungsstufe für Anforderungsprotokollierung",
        TextKey::Id => "ID",
        TextKey::Images => "Bilder",
        TextKey::Item => "Artikel",
        TextKey::ItemActions => "Artikelaktionen",
        TextKey::ItemType => "Artikeltyp",
        TextKey::Items => "Artikel",
        TextKey::Language => "Sprache",
        TextKey::Languages => "Sprachen",
        TextKey::MusicLabels => "Musiklabels",
        TextKey::LeaveEmptyToAutoDetect => "Leer lassen, um automatische Erkennung durchzuführen",
        TextKey::Libraries => "Bibliotheken",
        TextKey::LibraryRefreshInterval => "Aktualisierungsintervall der Bibliothek",
        TextKey::Light => "Licht",
        TextKey::Lists => "Listen",
        TextKey::Loading => "Wird geladen...",
        TextKey::LoadingMembers => "Mitglieder werden gel",
        TextKey::Local => "Local",
        TextKey::Logging => "Protokollierung",
        TextKey::M3u => "M3U",
        TextKey::Media => "Media",
        TextKey::MediaInfo => "Media Info",
        TextKey::MediaSources => "Media Sources",
        TextKey::Movie => "Movie",
        TextKey::MovieNotFound => "Movie not found",
        TextKey::Movies => "Filme",
        TextKey::MusicVideo => "Music Video",
        TextKey::MultiCollections => "Multi Collections",
        TextKey::ManualCollections => "Manual Collections",
        TextKey::MatchFailed => "Match failed",
        TextKey::MatchStatus => "Match Status",
        TextKey::MatchTraktListItems => "Match Trakt List Items",
        TextKey::Members => "Members",
        TextKey::MultiCollection => "Mehrsammlung",
        TextKey::MultiCollectionDeleted => "Mehrsammlung gelöscht.",
        TextKey::MultiCollectionItems => "Mehrsammlungselemente",
        TextKey::MultiCollectionNameRequired => "Mehrsammlungsnamen sind erforderlich!",
        TextKey::MultiCollectionNameTooLong => {
            "Mehrsammlungsnamen dürfen nicht länger als 50 Zeichen sein."
        },
        TextKey::MultiCollectionSaved => "Mehrsammlung gespeichert.",
        TextKey::Music => "Musik",
        TextKey::Name => "Name",
        TextKey::NameRequired => "Name ist erforderlich",
        TextKey::Networks => "Netzwerke",
        TextKey::NewCollection => "(Neue Sammlung)",
        TextKey::NewCollectionTitle => "Neue Sammlung",
        TextKey::NewFillerPreset => "Neuer Filler-Vorlage",
        TextKey::NewMultiCollection => "Neue Mehrsammlung",
        TextKey::NewTemplateGroup => "Neue Vorlagengruppe",
        TextKey::NewTemplateName => "Neuer Vorlagenname",
        TextKey::Next24Hours => "Nächste 24 h",
        TextKey::NextPage => "Nächste Seite",
        TextKey::NoFillerPresets => "Noch keine Filler-Vorlagen vorhanden.",
        TextKey::NoBlockItems => "Keine Blockelemente.",
        TextKey::NoItems => "Keine Elemente.",
        TextKey::NoItemsToDisplay => "Keine Elemente zur Anzeige.",
        TextKey::NoTraktListsYet => "Noch keine Trakt-Listen vorhanden.",
        TextKey::No => "Keine",
        TextKey::NoCollections => "Noch keine Sammlungen vorhanden.",
        TextKey::NoMembers => "Noch keine Mitglieder vorhanden.",
        TextKey::NoMultiCollections => "Noch keine Mehrsammlungen vorhanden.",
        TextKey::NoTemplatesFound => "Keine Vorlagen gefunden.",
        TextKey::NotFound => "Seite nicht gefunden.",
        TextKey::OtherVideo => "Andere Videos",
        TextKey::OtherVideoNotFound => "Andere Videos nicht gefunden",
        TextKey::OtherVideos => "Andere Videos",
        TextKey::OpenBrowser => "Browser öffnen",
        TextKey::Playout => "Ausspielung",
        TextKey::PlayoutItemsAndHistory => "Playout-Elemente und Verlauf",
        TextKey::PlayoutNotFound => "Playout nicht gefunden.",
        TextKey::Playouts => "Playouts",
        TextKey::AddPlayout => "Playout hinzufügen",
        TextKey::ClassicScheduleMenu => "Klassischer Zeitplan",
        TextKey::BlockScheduleMenu => "Block-Zeitplan",
        TextKey::SequentialScheduleMenu => "Sequenzieller Zeitplan",
        TextKey::ScriptedScheduleMenu => "Skriptierter Zeitplan",
        TextKey::JsonDizqueTvScheduleMenu => "JSON (dizqueTV) Zeitplan",
        TextKey::ResetAllPlayouts => "Alle Playouts zurücksetzen",
        TextKey::PlayoutBuildQueued => "Playout-Wiederherstellung in der Warteschlange",
        TextKey::PlayoutBuildsQueued => "Playout-Wiederherstellungen in der Warteschlange",
        TextKey::SearchForPlayouts => "Playouts suchen",
        TextKey::PlayoutDetail => "Playout-Details",
        TextKey::ShowFiller => "Filler anzeigen",
        TextKey::EditPlayout => "Playout bearbeiten",
        TextKey::RefreshPlayout => "Playout aktualisieren",
        TextKey::ResetPlayout => "Playout zurücksetzen",
        TextKey::ScheduleReset => "Zeitplan zurücksetzen",
        TextKey::SchedulePlayoutReset => "Zeitplan-Playout zurücksetzen",
        TextKey::EditExternalJsonFile => "Externe JSON-Datei bearbeiten",
        TextKey::DeletePlayout => "Playout löschen",
        TextKey::Troubleshoot => "Fehlerbehebung",
        TextKey::DailyResetTime => "Tägliche Reset-Zeit",
        TextKey::DoNotAutomaticallyReset => "Nicht automatisch zurücksetzen",
        TextKey::SaveChanges => "Änderungen speichern",
        TextKey::ExternalJsonFileLabel => "Externe JSON-Datei",
        TextKey::EditExternalJsonFilePrompt => "Externe JSON-Datei des Playouts bearbeiten",
        TextKey::UnscheduledPlayoutItem => "NICHT GEPLANT",
        TextKey::DisabledChannelsAlreadyHavePlayout => {
            "Deaktivierte Kanäle verfügen bereits über einen Playout"
        }
        TextKey::ProgramSchedule => "Programm-Zeitplan",
        TextKey::SequentialScheduleFileHelp => {
            "Der vollständige Pfad zur YAML-Datei des sequenziellen Zeitplans"
        }
        TextKey::BlockTemplatesAddedLater => "Blockvorlagen werden später hinzugefügt",
        TextKey::UnsupportedPlayoutKind => "Nicht unterstützter Playout-Typ.",
        TextKey::ClassicPlayout => "Klassischer Playout",
        TextKey::ClassicPlayoutState => "Klassischer Playout-Zustand",
        TextKey::BlockPlayout => "Block-Playout",
        TextKey::ScriptedPlayout => "Skriptierter Zeitplan",
        TextKey::AlternateSchedules => "Alternative Zeitpläne",
        TextKey::EditAlternateSchedules => "Alternative Zeitpläne bearbeiten",
        TextKey::PlayoutTemplates => "Playout-Vorlagen",
        TextKey::EditTemplates => "Vorlagen bearbeiten",
        TextKey::DefaultDeco => "Standard-Deko",
        TextKey::EnableDefaultDeco => "Standard-Deko aktivieren",
        TextKey::SaveDefaultDeco => "Standard-Deko speichern",
        TextKey::SaveScriptedSchedule => "Skriptierten Zeitplan speichern",
        TextKey::EraseItems => "Elemente löschen",
        TextKey::ErasedPlayoutItems => "Playout-Elemente gelöscht.",
        TextKey::SavedDefaultDeco => "Standard-Deko gespeichert.",
        TextKey::SavedScriptedSchedule => "Skriptierter Zeitplan gespeichert.",
        TextKey::Maintenance => "Wartung",
        TextKey::SaveAlternateSchedules => "Alternative Zeitpläne speichern",
        TextKey::AddAlternateSchedule => "Alternative Zeitpläne hinzufügen",
        TextKey::SaveTemplates => "Vorlagen speichern",
        TextKey::PreviewCalendar => "Kalender-Vorschau",
        TextKey::EditContent => "Inhalt bearbeiten",
        TextKey::PlayoutNestedCatchAllHelp => {
            "In Prioritätsreihenfolge von oben nach unten. Der unterste Eintrag stimmt immer für alle Tage und Monate als Auffangnetz überein."
        }
        TextKey::PlayoutNestedPriorityHelp => "In Prioritätsreihenfolge von oben nach unten.",
        TextKey::DaysOfWeek => "Wochentage",
        TextKey::DaysOfMonth => "Tage des Monats",
        TextKey::Months => "Monate",
        TextKey::LimitToDateRange => "Auf Datumsbereich beschränken",
        TextKey::StartDay => "Starttag",
        TextKey::EndDay => "Endtag",
        TextKey::Enable => "Aktivieren",
        TextKey::Up => "Hoch",
        TextKey::Down => "Runter",
        TextKey::Weekdays => "Werktage",
        TextKey::Weekends => "Wochenenden",
        TextKey::Sunday => "Sonntag",
        TextKey::Monday => "Montag",
        TextKey::Tuesday => "Dienstag",
        TextKey::Wednesday => "Mittwoch",
        TextKey::Thursday => "Donnerstag",
        TextKey::Friday => "Freitag",
        TextKey::Saturday => "Samstag",
        TextKey::January => "Januar",
        TextKey::February => "Februar",
        TextKey::March => "März",
        TextKey::April => "April",
        TextKey::May => "Mai",
        TextKey::June => "Juni",
        TextKey::July => "Juli",
        TextKey::August => "August",
        TextKey::September => "September",
        TextKey::October => "Oktober",
        TextKey::November => "November",
        TextKey::December => "Dezember",
        TextKey::JanuaryShort => "Jan",
        TextKey::FebruaryShort => "Feb",
        TextKey::MarchShort => "Mär",
        TextKey::AprilShort => "Apr",
        TextKey::MayShort => "Mai",
        TextKey::JuneShort => "Jun",
        TextKey::JulyShort => "Jul",
        TextKey::AugustShort => "Aug",
        TextKey::SeptemberShort => "Sep",
        TextKey::OctoberShort => "Okt",
        TextKey::NovemberShort => "Nov",
        TextKey::DecemberShort => "Dez",
        TextKey::PlayAll => "Alle abspielen",
        TextKey::Playlist => "Wiedergabeliste",
        TextKey::PlaylistAdded => "Wiedergabeliste hinzugefügt.",
        TextKey::PlaylistDeleted => "Wiedergabeliste gelöscht.",
        TextKey::PlaylistGroup => "Wiedergabelisten-Gruppe",
        TextKey::PlaylistGroupAdded => "Wiedergabelisten-Gruppe hinzugefügt.",
        TextKey::PlaylistGroupDeleted => "Wiedergabelisten-Gruppe gelöscht.",
        TextKey::PlaylistGroupName => "Wiedergabelisten-Gruppenname",
        TextKey::PlaylistGroups => "Wiedergabelisten-Gruppen",
        TextKey::PlaylistItem => "Wiedergabelisten-Element",
        TextKey::PlaylistItems => "Wiedergabelisten-Elemente",
        TextKey::PlaylistName => "Wiedergabelistenname",
        TextKey::PlaylistNotFound => "Wiedergabeliste nicht gefunden.",
        TextKey::PlaylistPlayoutPreview => "Wiedergabelisten-Vorschau",
        TextKey::PlaylistPreviewEmpty => "Die Wiedergabelisten-Vorschau enthält keine Elemente",
        TextKey::Playlists => "Wiedergabelisten",
        TextKey::ProbeForInterlacedFrames => "Nach Interlaced-Frames suchen",
        TextKey::PreferredAudioLanguage => "Bevorzugte Audiosprache",
        TextKey::DefaultSubtitleLanguage => "Standardsprache für Untertitel",
        TextKey::FallbackSubtitleLanguage => "Ersatzsprache für Untertitel",
        TextKey::OriginalLanguageOption => "Original (und)",
        TextKey::UndefinedLanguageOption => "Unbekannt (und)",
        TextKey::Previous24Hours => "Vorherige 24 Stunden",
        TextKey::PreviousPage => "Vorherige Seite",
        TextKey::RerunCollections => "Wiederholungs-Sammlungen",
        TextKey::RerunCollection => "Wiederholungs-Sammlung",
        TextKey::RerunCollectionActionFailed => "Die Aktion für die Wiederholungs-Sammlung ist fehlgeschlagen.",
        TextKey::RerunCollectionDeleted => "Wiederholungs-Sammlung gelöscht.",
        TextKey::RerunCollectionNameRequired => "Der Name der Wiederholungs-Sammlung ist erforderlich!",
        TextKey::RerunCollectionSaved => "Wiederholungs-Sammlung gespeichert.",
        TextKey::RerunPlaybackOrder => "Reihenfolge der Wiedergabe bei Wiederholungen",
        TextKey::RemoteStreams => "Ferngesteuerte Streams",
        TextKey::RemoteStream => "Ferngesteuerter Stream",
        TextKey::RemoteStreamPlaylists => "Wiedergabelisten für Ferngesteuerte Streams",
        TextKey::PlaylistUrl => "Wiedergabeliste URL",
        TextKey::Extractor => "Extraktor",
        TextKey::DestinationFolder => "Zielordner",
        TextKey::SyncInterval => "Synchronisationsintervall",
        TextKey::MaxItems => "Maximale Anzahl der Elemente",
        TextKey::RemovedEntries => "Entfernte Einträge",
        TextKey::CreateOrderedPlaylist => "Eine geordnete Wiedergabeliste erstellen",
        TextKey::SyncNow => "Jetzt synchronisieren",
        TextKey::LastSync => "Letzte Synchronisation",
        TextKey::NeverSynced => "Nie synchronisiert",
        TextKey::ManagedManifestReadOnly => "Diese manuell erstellte Manifestdatei ist hier schreibgeschützt.",
        TextKey::Truncated => "Abgeschnitten",
        TextKey::MissingDuration => "Fehlende Dauer",
        TextKey::Orphaned => "Waisen",
        TextKey::Materialized => "Materialisiert",
        TextKey::State => "Status",
        TextKey::Upstream => "Quellseite",
        TextKey::NewestFirst => "Neueste zuerst",
        TextKey::Keep => "Beibehalten",
        TextKey::ResetChannels => "Kanäle zurücksetzen",
        TextKey::RowsPerPage => "Zeilen pro Seite",
        TextKey::Save => "Speichern",
        TextKey::SaveMultiCollection => "Mehrfachsammlung speichern",
        TextKey::SavePlaylist => "Wiedergabeliste speichern",
        TextKey::SaveTroubleshootingReportsToDisk => "Fehlerberichte auf Festplatte speichern",
        TextKey::SaveAsSmartCollection => "Als intelligente Sammlung speichern",
        TextKey::SearchQueryRequired => "Eine Suchanfrage ist erforderlich",
        TextKey::SaveSettings => "Einstellungen speichern",
        TextKey::Scanner => "Scanner",
        TextKey::ScanningMinimumLogLevel => "Mindest-Protokollebene beim Scannen",
        TextKey::Scheduling => "Planung",
        TextKey::SchedulingMinimumLogLevel => "Mindest-Protokollebene bei der Planung",
        TextKey::StartWithWindows => "ErsatzRS starten, wenn ich mich bei Windows anmelde",
        TextKey::StartWithWindowsHelp => {
            "Den Tray-Startverwalter automatisch für den aktuellen Windows-Benutzer starten."
        }
        TextKey::Schedules => "Pläne",
        TextKey::AddSchedule => "Plan hinzufügen",
        TextKey::AddScheduleItem => "Plan-Element hinzufügen",
        TextKey::AddSmartCollection => "Intelligente Sammlung hinzufügen",
        TextKey::Album => "Album",
        TextKey::Chronological => "Chronologisch",
        TextKey::CollectionSize => "Sammlungsgröße",
        TextKey::CollectionType => "Sammlungstyp",
        TextKey::Count => "Anzahl",
        TextKey::CopySchedule => "Plan kopieren",
        TextKey::CopyScheduleItem => "Plan-Element kopieren",
        TextKey::CustomTitle => "Benutzerdefinierter Titel",
        TextKey::DeleteSchedule => "Plan löschen",
        TextKey::DeleteScheduleItem => "Plan-Element löschen",
        TextKey::DeleteSmartCollection => "Intelligente Sammlung löschen",
        TextKey::DetailRowsPerPage => "Detailzeilen pro Seite",
        TextKey::Director => "Regisseur",
        TextKey::Directors => "Regisseure",
        TextKey::DiscardToFillAttempts => "Versuche zum Verwerfen zum Auffüllen",
        TextKey::Duration => "Dauer",
        TextKey::Dynamic => "Dynamisch",
        TextKey::EditFillerPreset => "Füllervorlage bearbeiten",
        TextKey::EditProperties => "Eigenschaften bearbeiten",
        TextKey::EditScheduleItems => "Plan-Elemente bearbeiten",
        TextKey::EditSmartCollection => "Intelligente Sammlung bearbeiten",
        TextKey::EnterNameForNewSchedule => "Geben Sie einen Namen für den neuen Plan ein",
        TextKey::Fixed => "Fest",
        TextKey::FixedStartTimeBehavior => "Verhalten bei festgelegter Startzeit",
        TextKey::Flood => "Überflutung",
        TextKey::Flexible => "Fixiert",
        TextKey::FlexibleStartSoonAsPossible => "Flexibel - So bald wie möglich starten",
        TextKey::FlexibleStartSoonAsPossibleAfterStartTime => {
            "Flexibel - So bald wie möglich nach Startzeit starten"
        },
        TextKey::FillWithGroupMode => "Füllmodus für Gruppen (Sendung oder Künstler)",
        TextKey::GraphicsElements => "Grafikelemente",
        TextKey::GraphicsIds => "Grafiken-IDs",
        TextKey::Genres => "Genres",
        TextKey::GroupName => "Gruppenname",
        TextKey::GroupMultiPartEpisodesHelp => "Mehrteilige Episoden beim Mischen zusammenhalten",
        TextKey::GuideMode => "Leitmodus",
        TextKey::KeepMultiPartEpisodesTogether => "Mehrteilige Episoden zusammenhalten",
        TextKey::Marathon => "Marathon",
        TextKey::MarathonBatchSize => "Marathon-Batch-Größe",
        TextKey::MarathonBatchSizeHelp => {
            "Anzahl der Elemente, die von jeder Gruppe abgespielt werden, bevor fortgefahren wird; leer oder null spielt alle Elemente ab"
        },
        TextKey::MarathonGroupBy => "Marathon-Gruppierung nach",
        TextKey::MarathonShuffleGroups => "Marathon-Gruppen mischen",
        TextKey::MarathonShuffleItems => "Marathon-Elemente mischen",
        TextKey::MidRollFiller => "Füllsendung in der Mitte",
        TextKey::MidRoll => "Füllsendung in der Mitte",
        TextKey::MoveDown => "Nach unten verschieben",
        TextKey::MoveUp => "Nach oben verschieben",
        TextKey::MultiEpisodeGroupSize => "Größe der Mehr-Episoden-Gruppe",
        TextKey::MultiEpisodeShuffle => "Mehr-Episoden-Mischen",
        TextKey::Multiple => "Mehrfach",
        TextKey::MultipleCount => "Mehrfach-Anzahl",
        TextKey::MultipleCountHelp => {
            "Kann ein Ausdruck mit 'count' (Sammlungsgröße) und 'random' (0 bis count-1) sein"
        },
        TextKey::MultipleMode => "Mehrfach-Modus",
        TextKey::NewScheduleName => "Neuer Planname",
        TextKey::NewSmartCollection => "Neue intelligente Sammlung",
        TextKey::NoScheduleItems => "Keine Plan-Elemente.",
        TextKey::NoSchedules => "Keine Pläne.",
        TextKey::Normal => "Normal",
        TextKey::Offline => "Offline\nZX",
        TextKey::One => "One",
        TextKey::OrderedGroups => "Ordered Groups",
        TextKey::Playback => "Playback",
        TextKey::PlaybackOrder => "Playback Order",
        TextKey::PlaybackOrderUpdateFailed => "Playback order update failed",
        TextKey::PlaybackOrderUpdated => "Playback order updated.",
        TextKey::PlaylistItemSize => "Playlist Item Size",
        TextKey::PlayoutDurationHours => "Playout Duration Hours",
        TextKey::PlayoutDurationMinutes => "Playout Duration Minutes",
        TextKey::PostRollFiller => "Post-Roll Filler",
        TextKey::PostRoll => "Post-Roll",
        TextKey::PreRollFiller => "Pre-Roll Filler",
        TextKey::PreRoll => "Pre-Roll",
        TextKey::Random => "Random",
        TextKey::RandomCount => "Random Count",
        TextKey::RandomRotation => "Random Rotation",
        TextKey::RemainingDurationHelp => {
            "duration of the content after this mid-roll point, in seconds"
        },
        TextKey::RandomStartPoint => "Random Start Point",
        TextKey::RemoveFailed => "Remove failed",
        TextKey::RemovedFromCollection => "Removed from collection.",
        TextKey::RemoveFromCollection => "Remove From Collection",
        TextKey::Rename => "Rename",
        TextKey::RenameFailed => "Rename failed",
        TextKey::ReorderFailed => "Reorder failed",
        TextKey::RerunFirstRun => "Rerun First Run",
        TextKey::RerunRerun => "Rerun Rerun",
        TextKey::ReadOnlyTwentyFourHourPlayoutView => {
            "Read-only 24-hour view of scheduled playout items."
        },
        TextKey::SaveSchedule => "Save Schedule",
        TextKey::SaveScheduleItems => "Save Schedule Items",
        TextKey::SaveSmartCollection => "Save Smart Collection",
        TextKey::Schedule => "Schedule",
        TextKey::ScheduleActionFailed => "Schedule action failed.",
        TextKey::ScheduleAsGroup => "Schedule As Group",
        TextKey::ScheduleKind => "Schedule Kind",
        TextKey::ScheduleKindBlock => "Block",
        TextKey::ScheduleKindClassic => "Classic",
        TextKey::ScheduleKindExternalJson => "JSON (dizqueTV)",
        TextKey::ScheduleKindScripted => "Scripted",
        TextKey::ScheduleKindSequential => "Sequential",
        TextKey::SchedulingContext => "Scheduling Context",
        TextKey::ScheduleItem => "Schedule Item",
        TextKey::ScheduleItemActionFailed => "Schedule item action failed.",
        TextKey::ScheduleItemFiller => "Schedule Item Filler",
        TextKey::ScheduleItemFillerHelp => {
            "Pre-, mid-, and post-roll filler applies to every media item selected by this schedule item. To play content once after a Multiple block, add it as the next schedule item."
        },
        TextKey::ScheduleItemOverrides => "Schedule Item Overrides",
        TextKey::ScheduleItems => "Schedule Items",
        TextKey::ScheduleNotFound => "Schedule not found.",
        TextKey::ScheduleSaveFailed => "Schedule save failed.",
        TextKey::SmartShuffle => "Smart Shuffle",
        TextKey::SmartShufflePreset => "Spacing preset",
        TextKey::Balanced => "Balanced",
        TextKey::SmartShuffleScope => "Share spacing across",
        TextKey::SmartShuffleScopePlayout => "Playout",
        TextKey::SmartShuffleScopeScheduleItem => "Schedule Item",
        TextKey::SmartShuffleValidation => "Choose a spacing threshold and at least one similarity rule.",
        TextKey::SmartShuffleScopeContentSource => "Content Source",
        TextKey::SmartShuffleAfterItems => "Videos between similar items",
        TextKey::SmartShuffleAfterDurationMinutes => "Minutes between similar items",
        TextKey::SmartShuffleIncludeFiller => "Include filler",
        TextKey::SmartShuffleInheritedBlock => {
            "This item has no Smart Shuffle override and keeps any legacy playout policy."
        }
        TextKey::SmartShuffleInactive => "Smart Shuffle is inactive because this playback order is not randomized.",
        TextKey::SmartShuffleOverrideBlock => "Override for this Block item",
        TextKey::SimilarityRules => "Similarity Rules",
        TextKey::AlbumArtist => "Album Artist",
        TextKey::Apply => "Apply",
        TextKey::ContentParent => "Parent or Variant",
        TextKey::SmartShuffleFilterRules => "Filter Rules",
        TextKey::SearchForSchedules => "Search for schedules",
        TextKey::SearchForManualCollections => "Search for manual collections",
        TextKey::SearchForRerunCollections => "Search for rerun collections",
        TextKey::SearchForSmartCollections => "Search for smart collections",
        TextKey::SearchForTemplates => "Search for templates",
        TextKey::SearchForBlocks => "Search for blocks",
        TextKey::SearchForMultiCollections => "Search for multi collections",
        TextKey::SearchQuery => "Search Query",
        TextKey::BlockSearchQueryPlaceholder => "e.g. genre:action",
        TextKey::SearchTitle => "Search Title",
        TextKey::SearchTitleDisplay => "Search Title (display)",
        TextKey::SearchSmartCollection => "Search Smart Collection",
        TextKey::Season => "Season",
        TextKey::SeasonEpisode => "Season, Episode",
        TextKey::SeasonNotFound => "Season not found",
        TextKey::Select => "Select",
        TextKey::SelectCollection => "Select collection",
        TextKey::SelectGroupForNewTemplate => "Select a group for the new template",
        TextKey::SelectScheduleItemPrompt => "Select a schedule item to edit, or add a new item.",
        TextKey::SelectSchedule => "Select a schedule",
        TextKey::SelectSmartCollection => "Select smart collection",
        TextKey::Shuffle => "Shuffle",
        TextKey::ShuffleInOrder => "Shuffle In Order",
        TextKey::ShuffleScheduleItems => "Shuffle Schedule Items",
        TextKey::ShuffleScheduleItemsHelp => "Note: this disables fixed start times and flood mode",
        TextKey::ShuffledGroups => "Shuffled Groups",
        TextKey::Show => "Show",
        TextKey::ShowNotFound => "Show not found",
        TextKey::Shows => "Shows",
        TextKey::Start => "Start",
        TextKey::StartTime => "Start Time",
        TextKey::StartTimeOnOrAfter => "Start Time On Or After",
        TextKey::StartType => "Start Type",
        TextKey::Strict => "Strict",
        TextKey::StrictAlwaysWaitExactStartTime => "Strict - Always Wait For Exact Start Time",
        TextKey::StrictWaitExactStartTime => "Strict - Wait For Exact Start Time",
        TextKey::TailFiller => "Tail Filler",
        TextKey::TailMode => "Tail Mode",
        TextKey::Tail => "Tail",
        TextKey::TelevisionSeason => "Television Season",
        TextKey::TelevisionShow => "Television Show",
        TextKey::TreatCollectionsAsShows => "Treat Collections As Shows",
        TextKey::TreatCollectionsAsShowsHelp => "This is useful for multi-part crossover episodes",
        TextKey::Type => "Type",
        TextKey::TypeToSearch => "Type to search...",
        TextKey::Search => "Search",
        TextKey::SearchMedia => "Search media",
        TextKey::SearchMoviesOrShows => "Search movies or shows...",
        TextKey::SearchTraktList => "Search Trakt List",
        TextKey::SearchingMinimumLogLevel => "Searching Minimum Log Level",
        TextKey::Settings => "Einstellungen",
        TextKey::SettingsSaveFailed => "Settings Save Failed",
        TextKey::ShowMediaInfo => "Show Media Info",
        TextKey::ScriptedScheduleTimeout => "Scripted Schedule Timeout",
        TextKey::SkipMissingItems => "Fehlende Elemente überspringen",
        TextKey::SkipMissingItemsHelp => {
            "Controls whether file-not-found or unavailable items should be included in playouts"
        },
        TextKey::SmartCollections => "Smart Collections",
        TextKey::SmartCollection => "Smart Collection",
        TextKey::SmartCollectionDeleted => "Smart collection deleted.",
        TextKey::SmartCollectionQueryPlaceholder => "e.g. genre:action OR genre:comedy",
        TextKey::SmartCollectionSaved => "Smart collection saved.",
        TextKey::SaveTraktList => "Save Trakt List",
        TextKey::Songs => "Songs",
        TextKey::SplitTimeEvenly => "Split Time Evenly",
        TextKey::StreamingMinimumLogLevel => "Streaming Minimum Log Level",
        TextKey::Studios => "Studios",
        TextKey::Support => "Support",
        TextKey::Template => "Template",
        TextKey::TemplateCopied => "Template copied.",
        TextKey::TemplateCreated => "Template created.",
        TextKey::TemplateDeleted => "Template deleted.",
        TextKey::TemplateGroup => "Template Group",
        TextKey::TemplateGroupCreated => "Template group created.",
        TextKey::TemplateGroupDeleted => "Template group deleted.",
        TextKey::TemplateGroupName => "Template Group Name",
        TextKey::TemplateGroups => "Template Groups",
        TextKey::TemplateName => "Template Name",
        TextKey::TemplateNotFound => "Template not found.",
        TextKey::Templates => "Templates",
        TextKey::Theme => "Design",
        TextKey::ThisActionCannotBeUndone => "This action cannot be undone.",
        TextKey::ThisCollectionHasNoItemsYet => "This collection has no items yet.",
        TextKey::Thumbnail => "thumbnail",
        TextKey::Title => "Title",
        TextKey::ToggleNavigation => "Toggle navigation",
        TextKey::TotalDurationHelp => "total duration of the content, in seconds",
        TextKey::TotalPointsHelp => "total number of potential mid-roll points",
        TextKey::TotalProgressHelp => "normalized position from 0 to 1",
        TextKey::TraktList => "Trakt List",
        TextKey::TraktListActionFailed => "Trakt list action failed.",
        TextKey::TraktListAdded => "Trakt list added.",
        TextKey::TraktListDeleted => "Trakt list deleted.",
        TextKey::TraktListMatched => "Trakt list matched.",
        TextKey::TraktListNotFound => "Trakt list not found.",
        TextKey::TraktListSaveFailed => "Trakt list save failed.",
        TextKey::TraktListSaved => "Trakt list saved.",
        TextKey::TraktLists => "Trakt Lists",
        TextKey::TraktListUrl => "Trakt List URL",
        TextKey::Trash => "Trash",
        TextKey::TrashAlreadyEmpty => "Trash is already empty.",
        TextKey::TrashEmpty => "Trash is empty.",
        TextKey::TrashEmptyConfirm => "Delete every file-not-found item from the database?",
        TextKey::TrashEmptyConfirmHelp => {
            "This permanently removes the currently missing media items and their dependent metadata rows."
        },
        TextKey::TrashSubtitle => {
            "Items whose source files are currently missing from disk or the provider."
        },
        TextKey::Troubleshooting => "Troubleshooting",
        TextKey::TunerCount => "Anzahl der Tuner",
        TextKey::TvShows => "TV Shows",
        TextKey::Ui => "Benutzeroberfläche",
        TextKey::Unavailable => "Unavailable",
        TextKey::UseActualTimes => "Use Actual Times",
        TextKey::UseCustomPlaybackOrder => "Use Custom Playback Order",
        TextKey::UseChaptersAsMediaItems => "Use Chapters As Media Items",
        TextKey::UseChaptersAsMediaItemsHelp => {
            "Schedule individual chapters instead of entire files"
        },
        TextKey::UseEmbeddedSubtitles => "Use Embedded Subtitles",
        TextKey::Uuid => "UUID",
        TextKey::UntitledItem => "Untitled item",
        TextKey::WindowsStartup => "Windows startup",
        TextKey::WindowsStartupNeedsRepair => {
            "The saved startup command points to another location. Apply this setting to repair it."
        }
        TextKey::WindowsStartupUnavailable => {
            "Windows startup is unavailable because the packaged launcher could not be found or the registration could not be read."
        }
        TextKey::ActiveTranscodeLimit => "Maximum Simultaneous Streaming Transcodes",
        TextKey::ActiveTranscodeLimitHelp => {
            "Hard ceiling for streaming FFmpeg transcodes. When all slots are busy, a new viewer gets an all-streams-in-use response until a slot frees; active streams are not bypassed over the limit."
        },
        TextKey::ActiveTranscodeSuggestionHardware => {
            "Hardware encoder detected; this is a suggested value, not an automatic change."
        },
        TextKey::ActiveTranscodeSuggestionSoftware => {
            "Software encoding only; this is a suggested value, not an automatic change."
        },
        TextKey::Peak => "peak",
        TextKey::StreamsInUse => "Streams in use",
        TextKey::SuggestedTranscodeLimit => "Suggested value",
        TextKey::Watermarks => "Watermarks",
        TextKey::Width => "Width",
        TextKey::WorkAheadSegmenterLimit => "Work-Ahead HLS Transcodes",
        TextKey::WorkAheadSegmenterLimitHelp => {
            "How many HLS transcode slots may run ahead at maximum speed to build buffer. Work-ahead always yields to watched streams."
        },
        TextKey::AccelerationKind => "Acceleration Kind",
        TextKey::AddFfmpegProfile => "Add FFmpeg Profile",
        TextKey::AddProfile => "Add Profile",
        TextKey::AllowBFrames => "Allow B-Frames",
        TextKey::AllowWatermarks => "Allow Watermarks",
        TextKey::Audio => "Audio",
        TextKey::AudioBitrate => "Audio Bitrate (kBit/s)",
        TextKey::AudioBufferSize => "Audio Buffer Size (kBit)",
        TextKey::AudioChannels => "Channels",
        TextKey::AudioFormat => "Audio Format",
        TextKey::BitDepth => "Bit Depth",
        TextKey::Copy => "Copy",
        TextKey::CopyBlock => "Copy Block",
        TextKey::CopyFailed => "Copy failed",
        TextKey::CopyNameSuffix => "Copy",
        TextKey::CopyProfile => "Copy Profile",
        TextKey::CopyTemplate => "Copy Template",
        TextKey::Crop => "Crop",
        TextKey::DeinterlaceVideo => "Deinterlace Video",
        TextKey::Edit => "Edit",
        TextKey::EditBlock => "Edit Block",
        TextKey::EditCollection => "Edit Collection",
        TextKey::EditFfmpegProfile => "Edit FFmpeg Profile",
        TextKey::EditMultiCollection => "Edit Multi Collection",
        TextKey::EditRerunCollection => "Edit Rerun Collection",
        TextKey::EditTemplate => "Edit Template",
        TextKey::Expression => "Expression",
        TextKey::Fallback => "Fallback",
        TextKey::FillerKind => "Filler Kind",
        TextKey::General => "General",
        TextKey::HardwareAcceleration => "Hardware Acceleration",
        TextKey::HardwareIfPossible => "Hardware If Possible",
        TextKey::NewProfileName => "New Profile Name",
        TextKey::NoneOption => "(None)",
        TextKey::NumHelp => "the mid-roll point number, starting with 1",
        TextKey::OptionalDisplayTitle => "Optional display title...",
        TextKey::TranscodeAudio => "Transcode Audio",
        TextKey::NormalizeColors => "Normalize Colors",
        TextKey::NormalizeFramerate => "Normalize Framerate",
        TextKey::NormalizeLoudness => "Normalize Loudness",
        TextKey::TranscodeVideo => "Transcode Video",
        TextKey::Off => "Off",
        TextKey::Pad => "Pad",
        TextKey::PadNearestFifteenMinutes => "15 (:00, :15, :30, :45)",
        TextKey::PadNearestFiveMinutes => "5 (:00, :05, :10, :15, :20, etc)",
        TextKey::PadNearestTenMinutes => "10 (:00, :10, :20, :30, :40, :50)",
        TextKey::PadNearestThirtyMinutes => "30 (:00, :30)",
        TextKey::PadMode => "Pad Mode",
        TextKey::PadToNearestMinute => "Pad To Nearest Minute",
        TextKey::PointHelp => "the position of the mid-roll point, in seconds",
        TextKey::ProfileCopied => "Profile copied.",
        TextKey::ProfileNotFound => "Profile not found.",
        TextKey::QsvExtraHardwareFrames => "QSV Extra Hardware Frames",
        TextKey::Resolution => "Resolution",
        TextKey::SampleRateKhz => "Sample Rate (kHz)",
        TextKey::SaveDecoTemplate => "Save Deco Template",
        TextKey::SaveProfile => "Save Profile",
        TextKey::SaveFillerPreset => "Save Filler Preset",
        TextKey::SaveRerunCollection => "Save Rerun Collection",
        TextKey::SaveSequentialSchedule => "Save Sequential Schedule",
        TextKey::SaveTemplate => "Save Template",
        TextKey::SaveBlock => "Save Block",
        TextKey::ScaleAndPad => "Scale and Pad",
        TextKey::ScalingBehavior => "Scaling Behavior",
        TextKey::Software => "Software",
        TextKey::Stretch => "Stretch",
        TextKey::TargetLoudnessLufs => "Target Loudness (LUFS)",
        TextKey::ThreadCountAuto => "Thread Count (0 = auto)",
        TextKey::OutputDynamicRange => "Output Dynamic Range",
        TextKey::OutputDynamicRangeCopyHint => "Video is copied, so the output keeps the source dynamic range.",
        TextKey::HdrStaticMetadataHint => "Leave blank to use the documented HDR10 defaults. These apply only when the source carries no HDR metadata of its own; a source's own mastering display and light levels always win.",
        TextKey::HdrMasteringDisplay => "Mastering Display",
        TextKey::HdrMaxCll => "MaxCLL (cd/m²)",
        TextKey::HdrMaxFall => "MaxFALL (cd/m²)",
        TextKey::TonemapAlgorithm => "Tonemap Algorithm",
        TextKey::VaapiDevicePath => "VAAPI Device Path",
        TextKey::VaapiDisplay => "VAAPI Display",
        TextKey::VaapiDriver => "VAAPI Driver",
        TextKey::Video => "Video",
        TextKey::VideoBitrate => "Video Bitrate (kBit/s)",
        TextKey::VideoBufferSize => "Video Buffer Size (kBit)",
        TextKey::VideoFormat => "Video Format",
        TextKey::VideoPreset => "Video Preset",
        TextKey::VideoTune => "Video Tune",
        TextKey::VideoProfile => "Video Profile",
        TextKey::ViewCollection => "View Collection",
        TextKey::ViewPlaylist => "View Playlist",
        TextKey::AddAll => "Add All",
        TextKey::AddedItemToPlaylist => "Added item to playlist.",
        TextKey::AddedArtistToSchedule => "Added artist to schedule",
        TextKey::AddedMovieToSchedule => "Added movie to schedule",
        TextKey::AddedSeasonToSchedule => "Added season to schedule",
        TextKey::AddedShowToSchedule => "Added show to schedule",
        TextKey::AddingItem => "Adding item...",
        TextKey::AddingItemToPlaylist => "Adding item to playlist...",
        TextKey::Artists => "Artists",
        TextKey::ClearSelection => "Clear Selection",
        TextKey::Collection => "Collection",
        TextKey::CollectionNameRequired => "Collection name is required",
        TextKey::CollectionNotFound => "Collection not found",
        TextKey::CollectionRenamed => "Collection renamed.",
        TextKey::EditTraktListProperties => "Edit Trakt List Properties",
        TextKey::EnterTraktListUrl => "Enter Trakt List URL",
        TextKey::EnterSearchQuery => "Enter a search query.",
        TextKey::Episodes => "Episodes",
        TextKey::LoadingCollections => "Loading collections...",
        TextKey::LoadingPlaylistGroups => "Loading playlist groups...",
        TextKey::LoadingPlaylists => "Loading playlists...",
        TextKey::GeneratePlaylist => "Generate Playlist",
        TextKey::GeneratePlaylistFromSortedTraktItems => {
            "Generate a playlist from the sorted trakt list items"
        },
        TextKey::MusicVideos => "Music Videos",
        TextKey::NewCollectionName => "New Collection Name",
        TextKey::NoMediaMatched => "No media matched.",
        TextKey::NoItemsToAdd => "No items to add.",
        TextKey::NoItemsToPreview => "No items to preview.",
        TextKey::NoPlaylistsInGroup => "No playlists in this group.",
        TextKey::PleaseChoosePlaylist => "Please choose a playlist.",
        TextKey::RequestFailed => "Request failed",
        TextKey::Released => "Released",
        TextKey::SaveAs => "Save As",
        TextKey::Seasons => "Seasons",
        TextKey::SeeAll => "See All >>",
        TextKey::SelectAll => "Select All",
        TextKey::SelectGroup => "(Select a group)",
        TextKey::SelectPlaylist => "(Select a playlist)",
        TextKey::SelectSearchCollectionHelp => "Select the collection to add the search results to.",
        TextKey::SelectSearchPlaylistHelp => "Select the playlist to add the search results to.",
        TextKey::Selected => "Selected",
        TextKey::SelectCollectionToAddThisMovie => "Select a collection to add this movie.",
        TextKey::SelectPlaylistToAddThisMovie => "Select a playlist to add this movie.",
        TextKey::Add => "Add",
        TextKey::AddLibrary => "Add Library",
        TextKey::Address => "Address",
        TextKey::AddressRequired => "Address is required.",
        TextKey::ApiKey => "API Key",
        TextKey::ApiKeyRequired => "API key is required.",
        TextKey::AuthorizedRedirecting => "Authorized - redirecting...",
        TextKey::CheckingEveryTwoSeconds => "Checking every 2 seconds...",
        TextKey::ClickAuthorizePlex => "Click here to authorize on Plex.tv",
        TextKey::ClickPlayoutItemHelp => {
            "Click a playout item to inspect its title, times, and scheduling context."
        },
        TextKey::ConfigureConnection => "Configure Connection",
        TextKey::Connection => "Connection",
        TextKey::DeepScanCollections => "Deep Scan Collections",
        TextKey::DeepScanLibrary => "Deep Scan Library",
        TextKey::DeepScan => "Deep Scan",
        TextKey::EditLibrary => "Edit Library",
        TextKey::EditPlaylist => "Edit Playlist",
        TextKey::EnterOnePathPerLine => "Enter one path per line.",
        TextKey::Error => "Error",
        TextKey::ExternalCollections => "External Collections",
        TextKey::Information => "Information",
        TextKey::Kind => "Kind",
        TextKey::Library => "Library",
        TextKey::LibraryKind => "Library Kind",
        TextKey::LibraryPaths => "Library Paths",
        TextKey::LocalLibraries => "Local Libraries",
        TextKey::LocalPath => "Local Path",
        TextKey::MediaKind => "Media Kind",
        TextKey::NoLibrariesConfigured => "No libraries configured. Add a media source first.",
        TextKey::NoSchedulingContext => "No scheduling context",
        TextKey::NoPlayoutItemsInWindow => "No playout items in this 24-hour window",
        TextKey::NoPlayoutsFound => "No playouts found.",
        TextKey::NoPathReplacements => "No path replacements.",
        TextKey::Paths => "Paths",
        TextKey::PathReplacements => "Path Replacements",
        TextKey::PlexSignInHelp => "Sign in to Plex to add a media source.",
        TextKey::Remote => "Remote",
        TextKey::RemotePath => "Remote Path",
        TextKey::Remove => "Remove",
        TextKey::RemoveBlockFromTemplate => "Remove Block From Template",
        TextKey::RemoveContent => "Remove Content",
        TextKey::Retry => "Retry",
        TextKey::Saved => "Saved.",
        TextKey::Saving => "Saving...",
        TextKey::QuickScan => "Quick Scan",
        TextKey::ScanCollections => "Scan Collections",
        TextKey::ScanFailed => "Scan failed",
        TextKey::ScanLibrary => "Scan Library",
        TextKey::ScanQueued => "Scan queued",
        TextKey::Scanning => "Scanning...",
        TextKey::Server => "Server",
        TextKey::ServerAddress => "Server Address",
        TextKey::SignInToPlex => "Sign in to Plex",
        TextKey::SignOutOfPlex => "Sign out of Plex",
        TextKey::Sync => "Sync",
        TextKey::WaitingForPlexAuthorization => "Waiting for Plex authorization.",
        TextKey::AddChannel => "Add Channel",
        TextKey::Any => "Any",
        TextKey::BlockPlayoutTroubleshooting => "Block Playout Troubleshooting",
        TextKey::BlockPlayoutHistory => "Block Playout History",
        TextKey::Categories => "Categories",
        TextKey::CategoriesHelp => "Comma-separated list of categories",
        TextKey::Channel => "Channel",
        TextKey::ChannelCouldNotBeSaved => "Channel could not be saved. Check the form values and try again.",
        TextKey::ChannelDisabled => "Channel disabled",
        TextKey::ChannelHiddenFromEpg => "Channel hidden from EPG",
        TextKey::ChannelLogo => "Channel Logo",
        TextKey::ChannelLogoPreview => "Channel logo preview",
        TextKey::ChannelLogoUploadFailed => "Channel logo upload failed. Check the file and try again.",
        TextKey::Check => "Check",
        TextKey::Continuous => "Continuous",
        TextKey::DateAndTime => "Date and Time",
        TextKey::DefaultOption => "Default",
        TextKey::DefaultSchedule => "Default Schedule",
        TextKey::DeleteChannel => "Delete channel",
        TextKey::DownloadMediaSample => "Download Media Sample",
        TextKey::DownloadResults => "Download Results",
        TextKey::DisableWatermarks => "Disable Watermarks",
        TextKey::EditChannel => "Edit Channel",
        TextKey::EditChannelNumbers => "Edit Channel Numbers",
        TextKey::EditNumbers => "Edit Numbers",
        TextKey::ExternalLogoUrl => "External Logo URL",
        TextKey::Fail => "Fail",
        TextKey::FallbackFiller => "Fallback Filler",
        TextKey::FfmpegProfile => "FFmpeg Profile",
        TextKey::FirstPage => "First page",
        TextKey::FirstRunPlaybackOrder => "First Run Playback Order",
        TextKey::Forced => "Forced",
        TextKey::GenerateSubtitles => "Generate Subtitles",
        TextKey::Generated => "Generated",
        TextKey::Details => "Details",
        TextKey::Full => "Full",
        TextKey::Group => "Group",
        TextKey::FfmpegRuntime => "FFmpeg Runtime",
        TextKey::FfmpegRuntimeDownloaded => "FFmpeg runtime downloaded",
        TextKey::FfmpegRuntimeLatestRelease => "Latest Release",
        TextKey::FfmpegRuntimeRelease => "Installed Release",
        TextKey::FfmpegRuntimeSource => "Source",
        TextKey::FfmpegRuntimeSourceDownloaded => "Downloaded",
        TextKey::FfmpegRuntimeSourceManaged => "Managed",
        TextKey::FfmpegRuntimeSourceOverride => "Override",
        TextKey::FfmpegRuntimeSourcePath => "PATH",
        TextKey::FfmpegRuntimeSourceUnknown => "Unknown",
        TextKey::FfmpegRuntimeUpdateAvailable => "Update available",
        TextKey::FfmpegRuntimeUpdateCheckFailed => "Update check failed",
        TextKey::FfmpegRuntimeUpdateFailed => "FFmpeg runtime update failed",
        TextKey::FfmpegRuntimeUpToDate => "Up to date",
        TextKey::FfmpegRuntimeUpdated => "FFmpeg runtime updated",
        TextKey::FfmpegRuntimeVersion => "Version",
        TextKey::HealthNoPersistedRun => "No health checks are applicable right now.",
        TextKey::HealthPersistedRunHelp => "Current parity health check results. This page refreshes after media scans update item state.",
        TextKey::HistoryDetails => "History Details",
        TextKey::HlsDirect => "HLS Direct",
        TextKey::HlsSegmenter => "HLS Segmenter",
        TextKey::Hours => "Stunden",
        TextKey::IdleBehavior => "Idle Behavior",
        TextKey::IdleBehaviorHelp => "Controls how the transcoding process behaves after all clients have disconnected",
        TextKey::Info => "Info",
        TextKey::Import => "Import",
        TextKey::IsEnabled => "Is Enabled",
        TextKey::Json => "JSON",
        TextKey::KeepRunning => "Keep Running",
        TextKey::Key => "Key",
        TextKey::LastMidFillerHelp => "seconds since last mid-roll filler",
        TextKey::LastPage => "Last page",
        TextKey::Level => "Level",
        TextKey::LoadingMediaInfo => "Loading media info...",
        TextKey::Logs => "Logs",
        TextKey::Logo => "Logo",
        TextKey::MediaItem => "Media item",
        TextKey::Message => "Message",
        TextKey::MissingMediaItemQueryParameter => "Missing mediaItem query parameter",
        TextKey::Mirror => "Mirror",
        TextKey::MirrorOptionsHelp => {
            "Available only when Playout Source is set to Mirror."
        }
        TextKey::MirrorSourceChannel => "Mirror Source Channel",
        TextKey::MusicVideoCreditsMode => "Music Video Credits Mode",
        TextKey::MusicVideoCreditsTemplate => "Music Video Credits Template",
        TextKey::NoMatchingRecordsFound => "No matching records found",
        TextKey::NoFfmpegProfiles => "No FFmpeg profiles found. Create one first.",
        TextKey::NoBlockPlayoutSelected => "Select a block playout to inspect its blocks.",
        TextKey::NoBlocks => "No blocks found.",
        TextKey::NoHistory => "No history found.",
        TextKey::NoMessages => "No messages.",
        TextKey::NvidiaCapabilitiesNotCollected => "NVIDIA capabilities have not been collected yet.",
        TextKey::Number => "Number",
        TextKey::OnDemand => "On Demand",
        TextKey::MatchedPointsHelp => {
            "number of mid-roll points that have already matched the expression"
        },
        TextKey::Pass => "Pass",
        TextKey::ParityDebt => "ParityDebt",
        TextKey::PlaybackTroubleshooting => "Playback Troubleshooting",
        TextKey::Play => "Play",
        TextKey::PlayoutTimeline => "Playout Timeline",
        TextKey::PlayoutMode => "Playout Mode",
        TextKey::PlayoutModeHelp => "Controls the progression of the channel's playout",
        TextKey::PlayoutOffset => "Playout Offset",
        TextKey::PlayoutSource => "Playout Source",
        TextKey::PlayoutSourceHelp => "Controls the source of the channel's content. This cannot be changed when the channel already has a playout.",
        TextKey::Preview => "Preview",
        TextKey::PreviewBlockPlayout => "Preview Block Playout",
        TextKey::PreviewChannel => "Preview channel",
        TextKey::ChannelPreview => "Channel Preview",
        TextKey::ReloadStream => "Reload Stream",
        TextKey::StopPreview => "Stop Preview",
        TextKey::OpenFullPlayer => "Open Full Player",
        TextKey::BackToChannels => "Back to channels",
        TextKey::NowPlaying => "Now Playing",
        TextKey::UpNext => "Up Next",
        TextKey::ProgrammeGuide => "Programme Guide",
        TextKey::EpgPreview => "EPG Preview",
        TextKey::EpgPreviewDescription => {
            "Browse the published guide with two hours of history and 24 hours ahead."
        }
        TextKey::EpgZoom => "Zoom",
        TextKey::EpgNow => "Now",
        TextKey::NoEpgChannels => "No enabled channels are visible in the EPG.",
        TextKey::ScheduleGap => "Schedule gap",
        TextKey::Filler => "Filler",
        TextKey::NoProgramming => "No programming is scheduled in this window.",
        TextKey::ChannelDisabledPreview => "This channel is disabled.",
        TextKey::ChannelNotAiring => "This channel is not currently airing.",
        TextKey::ChannelNoPlayout => "This channel has no playout.",
        TextKey::PreviewIdle => "The preview starts muted automatically.",
        TextKey::PreviewConnecting => "Connecting to the live stream…",
        TextKey::PreviewLive => "Live playback is running.",
        TextKey::PreviewPaused => "Preview paused.",
        TextKey::PreviewRecovering => "Recovering playback…",
        TextKey::PreviewStopped => "Preview stopped.",
        TextKey::PreviewUnsupported => "This browser cannot play HLS video.",
        TextKey::PreviewAuthorizationFailed => "Preview authorization failed.",
        TextKey::PreviewUnavailable => "The live stream is unavailable.",
        TextKey::PreviewCapacityReached => "All transcode slots are currently in use.",
        TextKey::PreviewNetworkError => "The preview lost its network connection.",
        TextKey::PreviewMediaError => "The browser could not decode this channel's configured output.",
        TextKey::PreviewDerivativeNote => {
            "Admin preview uses a browser-compatible H.264/AAC derivative; the configured channel output is unchanged."
        }
        TextKey::JavascriptRequiredPlayback => "JavaScript is required for interactive playback.",
        TextKey::ConfirmStopTranscodeSession => "Stop the shared transcode session for this channel?",
        TextKey::PreviewPlaylistPlayout => "Preview Playlist Playout",
        TextKey::PreferredAudioTitle => "Preferred Audio Title",
        TextKey::PreferredSubtitleLanguage => "Preferred Subtitle Language",
        TextKey::QsvCapabilitiesNotCollected => "QSV capabilities have not been collected yet.",
        TextKey::RemoveStoredLogo => "Remove Stored Logo",
        TextKey::SaveChannel => "Save Channel",
        TextKey::SaveChannels => "Save Channels",
        TextKey::Second => "second",
        TextKey::Seconds => "seconds",
        TextKey::SelectProfile => "- Select profile -",
        TextKey::SequentialSchedule => "Sequential Schedule",
        TextKey::SequentialScheduleValidation => "Sequential Schedule Validation",
        TextKey::SeekSeconds => "Seek Seconds",
        TextKey::Schema => "Schema",
        TextKey::ShowDisabled => "Show Disabled",
        TextKey::ShowInEpg => "Show In EPG",
        TextKey::SlugSeconds => "Slug Seconds",
        TextKey::SlugSecondsHelp => "Seconds of black video and silent audio to insert between every playout item",
        TextKey::Song => "Song",
        TextKey::SongVideoMode => "Song Video Mode",
        TextKey::SortByLevel => "Sort by Level",
        TextKey::SortByTimestamp => "Sort by Timestamp",
        TextKey::StopOnDisconnect => "Stop On Disconnect",
        TextKey::StopScheduling => "Stop Scheduling",
        TextKey::StopSchedulingBlockItems => "Stop scheduling block items",
        TextKey::StopTranscodeSession => "Stop Transcode Session",
        TextKey::StartFromBeginning => "Start From Beginning",
        TextKey::StreamSelector => "Stream Selector",
        TextKey::StreamSelectorMode => "Stream Selector Mode",
        TextKey::StreamingMode => "Streaming Mode",
        TextKey::Styles => "Styles",
        TextKey::Subtitle => "Subtitle",
        TextKey::SubtitleMode => "Subtitle Mode",
        TextKey::SubtitleModeHelp => {
            "Forced shows only forced-narrative tracks. To always show subtitles in your \
             preferred language, choose Any."
        }
        TextKey::Timestamp => "Timestamp",
        TextKey::Timeline => "Timeline",
        TextKey::Target => "Target",
        TextKey::Tags => "Tags",
        TextKey::Tools => "Tools",
        TextKey::TranscodeMode => "Transcode Mode",
        TextKey::TranscodeModeHelp => "Controls when the transcoding process is active",
        TextKey::TroubleshootPlayback => "Troubleshoot Playback",
        TextKey::TroubleshootChannel => "Troubleshoot channel",
        TextKey::TroubleshootingReports => "Troubleshooting reports",
        TextKey::MpegTsTroubleshooting => "MPEG-TS Troubleshooting",
        TextKey::InternalMpegTs => "Internal MPEG-TS",
        TextKey::PublicMpegTs => "Public MPEG-TS",
        TextKey::RetainRawSample => "Retain raw sample",
        TextKey::TsdDuckVersion => "TSDuck version",
        TextKey::Report => "Report",
        TextKey::RunId => "Run ID",
        TextKey::Status => "Status",
        TextKey::Summary => "Summary",
        TextKey::SampleBytes => "Sample bytes",
        TextKey::RawSampleArchived => "Raw sample archived",
        TextKey::ArchivePath => "Archive path",
        TextKey::SampleUrl => "Sample URL",
        TextKey::StabilityMeter => "Stability meter",
        TextKey::Stable => "Stable",
        TextKey::Packets => "packets",
        TextKey::Services => "Services",
        TextKey::Pids => "PIDs",
        TextKey::ContinuityFindings => "Continuity findings",
        TextKey::PcrFindings => "PCR findings",
        TextKey::Bitrate => "Bitrate",
        TextKey::AnalysisJson => "Analysis JSON",
        TextKey::StandardOutput => "Standard output",
        TextKey::StandardError => "Standard error",
        TextKey::NotAvailable => "Not available",
        TextKey::NotApplicable => "Not applicable",
        TextKey::Completed => "Completed",
        TextKey::Failed => "Failed",
        TextKey::TroubleshootingPlaybackStartDebt => {
            "Media-item and channel playback preview now write FFmpeg reports, logs, media_info.json, troubleshooting_info.json, selected stream-selector YAML, capability files, execute stream-selector rules, and redirect to /iptv/session/.troubleshooting/live.m3u8."
        },
        TextKey::UnableToLoadMediaInfo => "Unable to load media info",
        TextKey::UnableToLoadPlaybackTroubleshooting => {
            "Unable to load playback troubleshooting context"
        },
        TextKey::UpdateListFromTraktDaily => "Update list from trakt.tv once each day",
        TextKey::UploadLogo => "Upload Logo",
        TextKey::UploadLogoHelp => "Choose a local image to replace the stored channel logo on save",
        TextKey::Warning => "Warning",
        TextKey::Watermark => "Watermark",
        TextKey::WatermarkIds => "Watermark IDs",
        TextKey::Window => "Window",
        TextKey::WithProgress => "With Progress",
        TextKey::Writers => "Writers",
        TextKey::Validate => "Validate",
        TextKey::Yaml => "YAML",
        TextKey::YamlFile => "YAML File",
        TextKey::Messages => "Messages",
        TextKey::CollectionName => "Collection Name",
        TextKey::MediaItemType => "Media Item Type",
        TextKey::MediaItemTitle => "Media Item Title",
        TextKey::ActualSize => "Actual Size",
        TextKey::AddWatermark => "Add Watermark",
        TextKey::AspectRatioGuides => "Aspect ratio guides",
        TextKey::BottomLeft => "Bottom Left",
        TextKey::BottomMiddle => "Bottom Middle",
        TextKey::BottomRight => "Bottom Right",
        TextKey::CopyWatermark => "Copy Watermark",
        TextKey::CopyOf => "Copy of",
        TextKey::CurrentImage => "Current image",
        TextKey::Custom => "Custom",
        TextKey::ChannelLogoPreviewUnavailable => {
            "Channel logo previews require a channel context. Choose a custom image to preview here."
        }
        TextKey::DeleteWatermark => "Delete watermark",
        TextKey::DurationSeconds => "Duration (seconds)",
        TextKey::EditWatermark => "Edit Watermark",
        TextKey::EnableWatermarkToPreview => "Enable the watermark to see its placement.",
        TextKey::Frequency => "Frequency",
        TextKey::GeneratingPreview => "Generating preview...",
        TextKey::HorizontalMarginPercent => "Horizontal Margin (%)",
        TextKey::GuideFourThree => "4:3 safe frame",
        TextKey::Identity => "Identity",
        TextKey::Image => "Image",
        TextKey::ImageSource => "Image Source",
        TextKey::Intermittent => "Intermittent",
        TextKey::IntermittentTiming => "Intermittent Timing",
        TextKey::LeftMiddle => "Left Middle",
        TextKey::Location => "Location",
        TextKey::MiddleCenter => "Middle Center",
        TextKey::Mode => "Mode",
        TextKey::Minutes => "minutes",
        TextKey::Moods => "Moods",
        TextKey::NewName => "New Name",
        TextKey::NativeAssetDimensions => "Native asset and rendered size",
        TextKey::NextWatermark => "Next watermark",
        TextKey::Opacity => "Opacity",
        TextKey::OpacityExpression => "Opacity Expression",
        TextKey::OpacityOneToHundred => "Opacity (1-100)",
        TextKey::Permanent => "Permanent",
        TextKey::PerPass => "per pass",
        TextKey::PlaceWithinSourceContent => "Place Within Source Content",
        TextKey::Placement => "Placement",
        TextKey::PlacementPreview => "Placement Preview",
        TextKey::PreviewFailed => "Preview failed",
        TextKey::PreviewReferenceFullHd => "Full HD reference frame · 1920×1080",
        TextKey::PreviousWatermark => "Previous watermark",
        TextKey::RightMiddle => "Right Middle",
        TextKey::SaveWatermark => "Save Watermark",
        TextKey::Scaled => "Scaled",
        TextKey::SelectCustomImageToPreview => "Select or upload a custom image to preview.",
        TextKey::Size => "Size",
        TextKey::TopLeft => "Top Left",
        TextKey::TopMiddle => "Top Middle",
        TextKey::TopRight => "Top Right",
        TextKey::UploadFailed => "Upload failed.",
        TextKey::UploadImage => "Upload image",
        TextKey::UnsavedChanges => "Unsaved changes",
        TextKey::UnsavedWatermarkChangesMessage => {
            "Discard your watermark edits and open the selected watermark?"
        }
        TextKey::VerticalMarginPercent => "Vertical Margin (%)",
        TextKey::WatermarkCopied => "Watermark copied.",
        TextKey::WatermarkNotFound => "Watermark not found.",
        TextKey::WatermarkPlacementPreview => "watermark placement preview",
        TextKey::WatermarkPreview => "watermark preview",
        TextKey::WatermarkNavigation => "Watermark navigation",
        TextKey::WidthPercent => "Width (%)",
        TextKey::ZIndex => "Z-Index",
        TextKey::DiscardChanges => "Discard changes",
        TextKey::XmltvBlockBehavior => "XMLTV Block Behavior",
        TextKey::XmltvTimeZone => "XMLTV-Zeitzone",
        TextKey::Xmltv => "XMLTV",
        TextKey::Badge => "Badge",
        TextKey::Buttons => "Buttons",
        TextKey::CheckValue => "Check",
        TextKey::ChannelName => "Channel Name",
        TextKey::ChannelNumber => "Channel Number",
        TextKey::ComponentGallery => "ErsatzRS Component Gallery",
        TextKey::DangerButton => "Danger",
        TextKey::DeleteDialog => "Delete Dialog",
        TextKey::Decrement => "Decrement",
        TextKey::Dialogs => "Dialogs",
        TextKey::DisabledButton => "Disabled",
        TextKey::EnterName => "Enter name",
        TextKey::ErrorOccurred => "An error occurred.",
        TextKey::ErrorToast => "Error toast",
        TextKey::ExampleModal => "Example Modal",
        TextKey::ExampleModalBody => "This is an example modal dialog body.",
        TextKey::FormInputs => "Form Inputs",
        TextKey::Icons => "Icons",
        TextKey::IncrementBadge => "Increment badge",
        TextKey::InfoToast => "Info toast",
        TextKey::JustFyi => "Just FYI.",
        TextKey::LinkButton => "Link Button",
        TextKey::ModalCancelled => "Cancelled.",
        TextKey::ModalConfirmed => "Confirmed!",
        TextKey::NumValue => "Num",
        TextKey::OpenModal => "Open Modal",
        TextKey::OperationSucceeded => "Operation succeeded.",
        TextKey::PagedTableDemo => "PagedTable (5 rows per page, 7 total)",
        TextKey::PrimaryButton => "Primary",
        TextKey::SecondaryButton => "Secondary",
        TextKey::SelectValue => "Select",
        TextKey::SomethingMightBeWrong => "Something might be wrong.",
        TextKey::SuccessToast => "Success toast",
        TextKey::TextButton => "Text",
        TextKey::TextValue => "Text",
        TextKey::Toasts => "Toasts",
        TextKey::WarningToast => "Warning toast",
        TextKey::YamlScript => "YAML Script",
        TextKey::Yes => "Yes",
    }
}

fn schedule_error_message(error: &str) -> String {
    match error.trim() {
        "Schedule name is required!" => "Schedule name is required!".to_owned(),
        "New Schedule Name is required." => "New Schedule Name is required.".to_owned(),
        "Schedule name must be 50 characters or less." => {
            "Schedule name must be 50 characters or less.".to_owned()
        }
        "Schedule name must be unique" => "Schedule name must be unique".to_owned(),
        "Schedule does not exist" => "Schedule does not exist".to_owned(),
        "[ProgramScheduleId] does not exist." => "[ProgramScheduleId] does not exist.".to_owned(),
        "A collection must use the same playback order in every schedule item." => {
            "A collection must use the same playback order in every schedule item.".to_owned()
        }
        "Shuffle In Order cannot be used with Fill With Group Mode." => {
            "Shuffle In Order cannot be used with Fill With Group Mode.".to_owned()
        }
        "Fixed schedule items require a start time." => {
            "Fixed schedule items require a start time.".to_owned()
        }
        "Multi Collection items can only use Shuffle or Shuffle In Order." => {
            "Multi Collection items can only use Shuffle or Shuffle In Order.".to_owned()
        }
        "Playlist Item Size can only be used with Playlist schedule items." => {
            "Playlist Item Size can only be used with Playlist schedule items.".to_owned()
        }
        "Multiple playout mode with Count requires a count expression." => {
            "Multiple playout mode with Count requires a count expression.".to_owned()
        }
        "Duration playout mode requires a duration between 1 minute and 24 hours." => {
            "Duration playout mode requires a duration between 1 minute and 24 hours.".to_owned()
        }
        "Duration playout mode requires discard-to-fill attempts." => {
            "Duration playout mode requires discard-to-fill attempts.".to_owned()
        }
        "Tail Mode Filler requires a tail filler preset." => {
            "Tail Mode Filler requires a tail filler preset.".to_owned()
        }
        "Tail filler can only be set when Tail Mode is Filler." => {
            "Tail filler can only be set when Tail Mode is Filler.".to_owned()
        }
        "Collection is required." => "Collection is required.".to_owned(),
        "Media item is required." => "Media item is required.".to_owned(),
        "Multi Collection is required." => "Multi Collection is required.".to_owned(),
        "Smart Collection is required." => "Smart Collection is required.".to_owned(),
        "Playlist is required." => "Playlist is required.".to_owned(),
        "Rerun Collection is required." => "Rerun Collection is required.".to_owned(),
        "Search Query is required." => "Search Query is required.".to_owned(),
        "Only one pre-, mid-, or post-roll filler can pad to the nearest minute." => {
            "Only one pre-, mid-, or post-roll filler can pad to the nearest minute.".to_owned()
        }
        "Padding filler requires a fallback filler." => {
            "Padding filler requires a fallback filler.".to_owned()
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
        return trimmed.to_owned();
    }
    if trimmed.starts_with("Template ") && trimmed.ends_with(" does not exist.") {
        return trimmed.to_owned();
    }
    if trimmed.starts_with("DecoGroup ") && trimmed.ends_with(" does not exist.") {
        return "Deco group does not exist.".to_owned();
    }
    if trimmed.starts_with("Deco ") && trimmed.ends_with(" does not exist.") {
        return "Deco does not exist.".to_owned();
    }
    if trimmed.starts_with("DecoTemplateGroup ") && trimmed.ends_with(" does not exist.") {
        return "Deco template group does not exist.".to_owned();
    }
    if trimmed.starts_with("DecoTemplate ") && trimmed.ends_with(" does not exist.") {
        return "Deco template does not exist.".to_owned();
    }
    if trimmed.starts_with("Deco name \"") && trimmed.ends_with("\" is invalid") {
        return "Deco name must be 50 characters or fewer.".to_owned();
    }
    if trimmed.starts_with("A deco named \"")
        && trimmed.ends_with("\" already exists in that deco group")
    {
        return "A deco with that name already exists in that deco group.".to_owned();
    }
    if trimmed.starts_with("Deco template name \"") && trimmed.ends_with("\" is invalid") {
        return "Deco template name must be 50 characters or fewer.".to_owned();
    }
    if trimmed.starts_with("A deco template named \"")
        && trimmed.ends_with("\" already exists in that deco template group")
    {
        return "A deco template with that name already exists in that deco template group."
            .to_owned();
    }
    if trimmed.starts_with("Watermark ") && trimmed.ends_with(" does not exist.") {
        return "Watermark does not exist.".to_owned();
    }
    if trimmed.starts_with("Graphics element ") && trimmed.ends_with(" does not exist.") {
        return "Graphics element does not exist.".to_owned();
    }
    if trimmed.starts_with("Deco from ") && trimmed.contains(" intersects deco from ") {
        return "Deco overlaps another deco.".to_owned();
    }

    match trimmed {
        "Block does not exist." => "Block does not exist.".to_owned(),
        "Block group name is required" => "Block group name is required".to_owned(),
        "Block group name must be unique" => "Block group name must be unique".to_owned(),
        "Block name is required" => "Block name is required".to_owned(),
        "A block named with that value already exists in that block group" => {
            "A block named with that value already exists in that block group".to_owned()
        }
        "Block duration must be between 5 minutes and 24 hours" => {
            "Block duration must be between 5 minutes and 24 hours".to_owned()
        }
        "[Collection] is required for collection type 'Collection'" => {
            "[Collection] is required for collection type 'Collection'".to_owned()
        }
        "[MediaItem] is required for collection type 'TelevisionShow'" => {
            "[MediaItem] is required for collection type 'TelevisionShow'".to_owned()
        }
        "[MediaItem] is required for collection type 'TelevisionSeason'" => {
            "[MediaItem] is required for collection type 'TelevisionSeason'".to_owned()
        }
        "[MultiCollection] is required for collection type 'MultiCollection'" => {
            "[MultiCollection] is required for collection type 'MultiCollection'".to_owned()
        }
        "[SmartCollection] is required for collection type 'SmartCollection'" => {
            "[SmartCollection] is required for collection type 'SmartCollection'".to_owned()
        }
        "[SearchQuery] is required for collection type 'SearchQuery'" => {
            "[SearchQuery] is required for collection type 'SearchQuery'".to_owned()
        }
        "[CollectionType] is invalid" => "[CollectionType] is invalid".to_owned(),
        "[BlockId] does not exist." => "[BlockId] does not exist.".to_owned(),
        "Template does not exist." => "Template does not exist.".to_owned(),
        "Template group name is required" => "Template group name is required".to_owned(),
        "Template group name must be 50 characters or fewer" => {
            "Template group name must be 50 characters or fewer".to_owned()
        }
        "Template group name must be unique" => "Template group name must be unique".to_owned(),
        "Template group does not exist." => "Template group does not exist.".to_owned(),
        "[TemplateId] does not exist." => "[TemplateId] does not exist.".to_owned(),
        "Template name is required" => "Template name is required".to_owned(),
        "Template items cannot cross midnight." => {
            "Template items cannot cross midnight.".to_owned()
        }
        "Deco group name is required" => "Deco group name is required".to_owned(),
        "Deco group name must be 50 characters or fewer" => {
            "Deco group name must be 50 characters or fewer".to_owned()
        }
        "Deco group name must be unique" => "Deco group name must be unique".to_owned(),
        "Deco group does not exist." => "Deco group does not exist.".to_owned(),
        "Deco name is required" => "Deco name is required".to_owned(),
        "Deco does not exist" => "Deco does not exist.".to_owned(),
        "Default filler has unsupported collection type." => {
            "Default filler has unsupported collection type.".to_owned()
        }
        "Dead air fallback has unsupported collection type." => {
            "Dead air fallback has unsupported collection type.".to_owned()
        }
        "Break content has unsupported collection type." => {
            "Break content has unsupported collection type.".to_owned()
        }
        "Default filler must have valid item" => "Default filler must have a valid item".to_owned(),
        "Dead air fallback must have valid item" => {
            "Dead air fallback must have a valid item".to_owned()
        }
        "Break content collection must have valid item" => {
            "Break content collection must have a valid item".to_owned()
        }
        "Break content multi collection must have valid item" => {
            "Break content multi-collection must have a valid item".to_owned()
        }
        "Break content smart collection must have valid item" => {
            "Break content smart collection must have a valid item".to_owned()
        }
        "Break content media item must have valid item" => {
            "Break content media item must have a valid item".to_owned()
        }
        "Break content playlist must have valid item" => {
            "Break content playlist must have a valid item".to_owned()
        }
        "Default filler item does not exist." => "Default filler item does not exist.".to_owned(),
        "Dead air fallback item does not exist." => {
            "Dead air fallback item does not exist.".to_owned()
        }
        "Break content collection item does not exist." => {
            "Break content collection item does not exist.".to_owned()
        }
        "Break content multi collection item does not exist." => {
            "Break content multi-collection item does not exist.".to_owned()
        }
        "Break content smart collection item does not exist." => {
            "Break content smart collection item does not exist.".to_owned()
        }
        "Break content media item item does not exist." => {
            "Break content media item does not exist.".to_owned()
        }
        "Break content playlist item does not exist." => {
            "Break content playlist does not exist.".to_owned()
        }
        "Deco may only contain one [Block Start] break content" => {
            "Deco may only contain one Block Start break content.".to_owned()
        }
        "Deco may only contain one [Between Block Items] break content" => {
            "Deco may only contain one Between Block Items break content.".to_owned()
        }
        "Deco may only contain one [At Chapter Markers] break content" => {
            "Deco may only contain one At Chapter Markers break content.".to_owned()
        }
        "Deco may only contain one [Block Finish] break content" => {
            "Deco may only contain one Block Finish break content.".to_owned()
        }
        "Deco template group name is required" => "Deco template group name is required".to_owned(),
        "Deco template group name must be 50 characters or fewer" => {
            "Deco template group name must be 50 characters or fewer".to_owned()
        }
        "Deco template group name must be unique" => {
            "Deco template group name must be unique".to_owned()
        }
        "Deco template group does not exist." => "Deco template group does not exist.".to_owned(),
        "[DecoTemplateId] does not exist." => "Deco template does not exist.".to_owned(),
        "Deco template name is required" => "Deco template name is required".to_owned(),
        "[DecoId] does not exist." => "Deco does not exist.".to_owned(),
        "Deco template items cannot be empty." => "Deco template items cannot be empty.".to_owned(),
        _ => error.to_owned(),
    }
}

fn watermark_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("Watermark ") && trimmed.ends_with(" does not exist.") {
        return "Watermark not found.".to_owned();
    }

    match trimmed {
        "Name is required" => "Name is required".to_owned(),
        "Name must be 50 characters or fewer" => "Name must be 50 characters or fewer".to_owned(),
        "Watermark image is required" | "Watermark image is required!" => {
            "Watermark image is required".to_owned()
        }
        "Width must be between 0 and 100%" => "Width must be between 0 and 100%".to_owned(),
        "Horizontal margin must be between 0 and 50%" => {
            "Horizontal margin must be between 0 and 50%".to_owned()
        }
        "Vertical margin must be between 0 and 50%" => {
            "Vertical margin must be between 0 and 50%".to_owned()
        }
        "Duration must be positive and less than frequency window" => {
            "Duration must be positive and less than frequency window".to_owned()
        }
        "Opacity must be between 1 and 100" => "Opacity must be between 1 and 100".to_owned(),
        "Watermark name must be unique" | "ChannelWatermark name must be unique" => {
            "Watermark name must be unique".to_owned()
        }
        "Watermark not found" | "Source watermark not found" | "Watermark does not exist." => {
            "Watermark not found.".to_owned()
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
        _ => return format!("Unknown ({value})"),
    }
    .to_owned()
}

fn ffmpeg_profile_video_format_label(format: i32) -> String {
    match format {
        0 => "None",
        1 => "H264",
        2 => "H265 (HEVC)",
        3 => "MPEG-2",
        4 => "AV1",
        99 => text(TextKey::Copy),
        _ => return format!("Unknown ({format})"),
    }
    .to_owned()
}

fn ffmpeg_profile_audio_format_label(format: i32) -> String {
    match format {
        0 => "None",
        1 => "AAC",
        2 => "AC3",
        3 => "AAC (LATM)",
        99 => text(TextKey::Copy),
        _ => return format!("Unknown ({format})"),
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
            "XMLTV days to build ({current}) cannot be greater than Playout days to build ({limit})"
        );
    }
    if let Some(name) = trimmed.strip_suffix(" path does not exist") {
        return format!("{name} path does not exist");
    }
    if let Some(name) = trimmed
        .strip_prefix("Unable to verify ")
        .and_then(|value| value.strip_suffix(" version"))
    {
        return format!("Unable to verify {name} version");
    }
    if let Some(value) = trimmed.strip_prefix("Invalid option: ") {
        return format!("Invalid option: {value}");
    }
    if trimmed.starts_with("Resolution ") && trimmed.ends_with(" does not exist.") {
        return "Resolution does not exist.".to_owned();
    }

    match trimmed {
        "Windows startup registration is not supported on this platform" => {
            "Windows startup registration is not supported on this platform".to_owned()
        }
        "ErsatzRS.exe was not found next to ersatzrs-web.exe" => {
            "ErsatzRS.exe was not found next to ersatzrs-web.exe".to_owned()
        }
        "Unable to update Windows startup registration" => {
            "Unable to update Windows startup registration".to_owned()
        }
        "Tuner count must be greater than zero" => {
            "Tuner count must be greater than zero".to_owned()
        }
        "Library refresh interval must be 0 (do not refresh) or greater than zero" => {
            "Library refresh interval must be 0 (do not refresh) or greater than zero".to_owned()
        }
        "Library refresh interval must be less than 1,000,000. Use 0 to disable automatic refresh" => {
            "Library refresh interval must be less than 1,000,000. Use 0 to disable automatic refresh".to_owned()
        }
        "Playout days to build must be greater than zero" => {
            "Playout days to build must be greater than zero".to_owned()
        }
        "Scripted schedule timeout seconds must be at least 10" => {
            "Scripted schedule timeout seconds must be at least 10".to_owned()
        }
        "XMLTV days to build must be greater than zero" => {
            "XMLTV days to build must be greater than zero".to_owned()
        }
        "Resolution width or height is invalid" => {
            "Resolution width or height is invalid".to_owned()
        }
        "Resolution width and height must be unique" => {
            "Resolution width and height must be unique".to_owned()
        }
        "HLS Segmenter idle timeout must be greater than or equal to 30" => {
            "HLS Segmenter idle timeout must be greater than or equal to 30".to_owned()
        }
        "Work-Ahead HLS Segmenter limit must be greater than or equal to 0" => {
            "Work-Ahead HLS Segmenter limit must be greater than or equal to 0".to_owned()
        }
        "Active HLS transcode limit must be greater than or equal to 1" => {
            "Active HLS transcode limit must be greater than or equal to 1".to_owned()
        }
        "HLS Segmenter initial segment count must be greater than or equal to 1" => {
            "HLS Segmenter initial segment count must be greater than or equal to 1".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn media_action_error_message(error: &str) -> String {
    match error.trim() {
        "Collection name is required" => "Collection name is required".to_owned(),
        "Collection name must be 50 characters or less" => {
            "Collection name must be 50 characters or less".to_owned()
        }
        "Collection name must be unique" => "Collection name must be unique".to_owned(),
        "Media item does not exist" => "Media item does not exist".to_owned(),
        "Collection does not exist" | "Collection does not exist." => {
            "Collection does not exist.".to_owned()
        }
        "Playlist does not exist" | "Playlist does not exist." => {
            "Playlist does not exist.".to_owned()
        }
        "Unable to locate media item" => "Unable to locate media item".to_owned(),
        "Media action failed" => "Media action failed".to_owned(),
        _ => error.to_owned(),
    }
}

fn filler_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("FillerPreset ") && trimmed.ends_with(" does not exist.") {
        return "Filler preset does not exist.".to_owned();
    }

    match trimmed {
        "Filler preset does not exist" => "Filler preset does not exist.".to_owned(),
        "Filler preset name must be unique" => "Filler preset name must be unique".to_owned(),
        "Name must not be empty" => "Name must not be empty".to_owned(),
        "Name must be 50 characters or fewer" => "Name must be 50 characters or fewer".to_owned(),
        "Filler kind must not be None" => "Filler kind must not be None".to_owned(),
        "Filler mode must not be None" => "Filler mode must not be None".to_owned(),
        "Count must be greater than or equal to 0" => {
            "Count must be greater than or equal to 0".to_owned()
        }
        "Duration must not be empty" => "Duration must not be empty".to_owned(),
        "Pad To Nearest Minute must not be empty" => {
            "Pad To Nearest Minute must not be empty".to_owned()
        }
        "Use Chapters As Media Items must not be enabled for fallback filler" => {
            "Use Chapters As Media Items must not be enabled for fallback filler".to_owned()
        }
        "Collection must not be empty" => "Collection must not be empty".to_owned(),
        "Multi Collection must not be empty" => "Multi Collection must not be empty".to_owned(),
        "Smart Collection must not be empty" => "Smart Collection must not be empty".to_owned(),
        "Media item must not be empty" => "Media item must not be empty".to_owned(),
        _ => error.to_owned(),
    }
}

fn media_browser_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed == "Pinned stream selection is invalid or no longer available." {
        return trimmed.to_owned();
    }
    if trimmed == "Movie was not found." {
        return trimmed.to_owned();
    }
    if trimmed.starts_with("Collection ") && trimmed.ends_with(" is already present.") {
        return "Collection is already present.".to_owned();
    }
    if trimmed.starts_with("Collection ") && trimmed.ends_with(" does not exist.") {
        return "Collection does not exist.".to_owned();
    }
    if trimmed.starts_with("Smart collection ") && trimmed.ends_with(" is already present.") {
        return "Smart collection is already present.".to_owned();
    }
    if trimmed.starts_with("Smart collection ") && trimmed.ends_with(" does not exist.") {
        return "Smart collection does not exist.".to_owned();
    }
    if trimmed.starts_with("MultiCollection ") && trimmed.ends_with(" does not exist.") {
        return "Multi-collection does not exist.".to_owned();
    }
    if trimmed.starts_with("Rerun collection ") && trimmed.ends_with(" does not exist.") {
        return "Rerun collection does not exist.".to_owned();
    }
    if trimmed.starts_with("Referenced item ") && trimmed.ends_with(" does not exist.") {
        return "Referenced item does not exist.".to_owned();
    }
    if trimmed.starts_with("PlaylistGroup ") && trimmed.ends_with(" does not exist.") {
        return "Playlist group does not exist.".to_owned();
    }
    if trimmed.starts_with("Playlist ") && trimmed.ends_with(" does not exist.") {
        return "Playlist does not exist.".to_owned();
    }
    if trimmed.starts_with("Playlist name \"") && trimmed.ends_with("\" is invalid") {
        return "Playlist name is invalid.".to_owned();
    }
    if let Some(error) = trimmed.strip_prefix("Playlist preview failed: ") {
        return format!("Playlist preview failed: {error}");
    }
    if trimmed.starts_with("Media item ") && trimmed.ends_with(" does not exist.") {
        return "Media item does not exist.".to_owned();
    }

    match trimmed {
        "Collection name is required" => "Collection name is required".to_owned(),
        "Collection name must be 50 characters or less" => {
            "Collection name must be 50 characters or less".to_owned()
        }
        "Collection name must be unique" => "Collection name must be unique".to_owned(),
        "Collection does not exist." => "Collection does not exist.".to_owned(),
        "Smart collection name is required" => "Smart collection name is required".to_owned(),
        "Smart collection query is required" => "Smart collection query is required".to_owned(),
        "SmartCollection name must be unique" => "Smart collection name must be unique".to_owned(),
        "MultiCollection does not exist." => "Multi-collection does not exist.".to_owned(),
        "Multi-collection name is required!" => "Multi-collection name is required.".to_owned(),
        "Multi-collection name cannot be longer than 50 characters." => {
            "Multi-collection name cannot be longer than 50 characters.".to_owned()
        }
        "MultiCollection name must be unique" => "Multi-collection name must be unique".to_owned(),
        "Each multi-collection item must reference exactly one collection." => {
            "Each multi-collection item must reference exactly one collection.".to_owned()
        }
        "Rerun collection does not exist." => "Rerun collection does not exist.".to_owned(),
        "[Name] is an empty string" | "[Name] must not be empty." => {
            "Name must not be empty.".to_owned()
        }
        "[Name] must not be longer than 50" | "[Name] must not be longer than 50." => {
            "Name must not be longer than 50 characters.".to_owned()
        }
        "Rerun collection name must be unique" => "Rerun collection name must be unique".to_owned(),
        "Unsupported collection type." => "Unsupported collection type.".to_owned(),
        "[ProgramScheduleId] does not exist." => "Schedule does not exist.".to_owned(),
        "[MediaItem] does not exist." => "Media item does not exist.".to_owned(),
        "[PlaylistGroupId] does not exist." => "Playlist group does not exist.".to_owned(),
        "Cannot delete system playlist group" => "Cannot delete system playlist group".to_owned(),
        "Cannot delete system (generated) playlist" => {
            "Cannot delete system (generated) playlist".to_owned()
        }
        "[PlaylistId] does not exist." => "Playlist does not exist.".to_owned(),
        "Cannot save system (generated) playlist" => {
            "Cannot save system (generated) playlist".to_owned()
        }
        "Playlist playout preview has no items" => {
            "Playlist playout preview has no items".to_owned()
        }
        "Playlist name is required" => "Playlist name is required".to_owned(),
        "[Collection] is required for collection type 'Collection'" => {
            "Collection is required.".to_owned()
        }
        "[MultiCollection] is required for collection type 'MultiCollection'" => {
            "Multi-collection is required.".to_owned()
        }
        "[SmartCollection] is required for collection type 'SmartCollection'" => {
            "Smart collection is required.".to_owned()
        }
        "[CollectionType] is invalid" => "Collection type is invalid.".to_owned(),
        _ => error.to_owned(),
    }
}

fn media_source_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if let Some(error) = trimmed.strip_prefix("Could not reach Jellyfin: ") {
        return format!("Could not reach Jellyfin: {error}");
    }

    match trimmed {
        "Address is required." => "Address is required.".to_owned(),
        "API key is required." => "API key is required.".to_owned(),
        "'Address' must be a valid URL" => "'Address' must be a valid URL".to_owned(),
        _ => error.to_owned(),
    }
}

fn trakt_error_message(error: &str) -> String {
    let trimmed = error.trim();
    let normalized = trimmed.strip_prefix("conflict: ").unwrap_or(trimmed);
    if let Some(error) = normalized.strip_prefix("failed to decode trakt http: ") {
        return format!("Could not reach Trakt: {error}");
    }
    if normalized.starts_with("trakt list with id ") && normalized.ends_with(" not found") {
        return "Trakt list does not exist.".to_owned();
    }

    match normalized {
        "Invalid Trakt list url" => "Invalid Trakt list URL.".to_owned(),
        "Trakt ClientId is not configured; set TRAKT__CLIENTID" => {
            "Trakt ClientId is not configured; set TRAKT__CLIENTID.".to_owned()
        }
        "Trakt API client is not configured for this operation" => {
            "Trakt API client is not configured for this operation.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn upload_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if let Some(error) = trimmed.strip_prefix("Cannot create upload directory: ") {
        return format!("Upload-Verzeichnis kann nicht erstellt werden: {error}");
    }
    if let Some(error) = trimmed.strip_prefix("Write failed: ") {
        return format!("Schreiben fehlgeschlagen: {error}");
    }
    if let Some(error) = trimmed.strip_prefix("Read field: ") {
        return format!("Feld konnte nicht gelesen werden: {error}");
    }

    match trimmed {
        "Upload exceeds maximum allowed size" => {
            "Der Upload überschreitet die maximal zulässige Größe.".to_owned()
        }
        "No file field" => "Kein Dateifeld gefunden.".to_owned(),
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
    if let Some(name) = trimmed
        .strip_prefix("An FFmpeg profile named \"")
        .and_then(|value| value.strip_suffix("\" already exists."))
    {
        return format!("Ein FFmpeg-Profil mit dem Namen \"{name}\" ist bereits vorhanden.");
    }

    match trimmed {
        "Profile name is required." => "Der Profilname ist erforderlich.".to_owned(),
        "Profile name must be 50 characters or fewer." => {
            "Der Profilname darf höchstens 50 Zeichen lang sein.".to_owned()
        }
        "Thread count must be 0 or greater." => {
            "Die Threadanzahl muss mindestens 0 betragen.".to_owned()
        }
        "QSV supports formats (h264, hevc, mpeg2video)" => {
            "QSV unterstützt die Formate (h264, hevc, mpeg2video).".to_owned()
        }
        "NVENC supports formats (h264, hevc, av1)" => {
            "NVENC unterstützt die Formate (h264, hevc, av1).".to_owned()
        }
        "VAAPI supports formats (h264, hevc, av1, mpeg2video)" => {
            "VAAPI unterstützt die Formate (h264, hevc, av1, mpeg2video).".to_owned()
        }
        "VideoToolbox supports formats (h264, hevc)" => {
            "VideoToolbox unterstützt die Formate (h264, hevc).".to_owned()
        }
        "Amf supports formats (h264, hevc)" => {
            "AMF unterstützt die Formate (h264, hevc).".to_owned()
        }
        "V4L2 M2M supports formats (h264, hevc)" => {
            "V4L2 M2M unterstützt die Formate (h264, hevc).".to_owned()
        }
        "Rkmpp supports formats (h264, hevc)" => {
            "RKMPP unterstützt die Formate (h264, hevc).".to_owned()
        }
        "HDR10 output requires hevc or av1; h264 and mpeg2video cannot carry HDR10 static metadata" => {
            "Für die HDR10-Ausgabe ist hevc oder av1 erforderlich; h264 und mpeg2video können keine statischen HDR10-Metadaten übertragen.".to_owned()
        }
        "HDR10 output requires 10-bit; 8-bit cannot represent a PQ signal without banding" => {
            "Für die HDR10-Ausgabe sind 10 Bit erforderlich; 8 Bit können ein PQ-Signal nicht ohne Banding darstellen.".to_owned()
        }
        "Mastering display must look like G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1), with chromaticities up to 50000 and max luminance above min" => {
            "Das Mastering-Display muss dem Format G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1) entsprechen, mit Farbwerten bis 50000 und einer maximalen Leuchtdichte über der minimalen.".to_owned()
        }
        "MaxCLL and MaxFALL must both be set (or both left empty), at most 65535, with MaxFALL no greater than MaxCLL" => {
            "MaxCLL und MaxFALL müssen beide festgelegt (oder beide leer gelassen) werden und dürfen höchstens 65535 betragen; MaxFALL darf nicht größer als MaxCLL sein.".to_owned()
        }
        "Mpeg2Video does not support 10-bit content" => {
            "Mpeg2Video unterstützt keine 10-Bit-Inhalte.".to_owned()
        }
        "VideoProfile must be high10 with 10-bit h264" => {
            "VideoProfile muss bei 10-Bit-h264 auf high10 gesetzt sein.".to_owned()
        }
        "VideoProfile must be high444p with NVIDIA 10-bit h264" => {
            "VideoProfile muss bei NVIDIA 10-Bit-h264 auf high444p gesetzt sein.".to_owned()
        }
        "VideoProfile cannot be high10 with 8-bit h264" => {
            "VideoProfile darf bei 8-Bit-h264 nicht auf high10 gesetzt sein.".to_owned()
        }
        "Source profile not found." => "Quellprofil nicht gefunden.".to_owned(),
        "No FFmpeg resolutions are available; database bootstrap is incomplete" => {
            "Es sind keine FFmpeg-Auflösungen verfügbar; die Initialisierung der Datenbank ist unvollständig".to_owned()
        }
        _ => error.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_matches_german_locale_contract() {
        assert_eq!(SORT_ORDER, 60);
        assert_eq!(CODE, "de-de");
        assert_eq!(LABEL, "Deutsch (Deutschland)");
        assert_eq!(ALIASES, &["de", "de-de", "de_de"]);
    }

    #[test]
    fn dynamic_labels_are_localized_and_preserve_identifiers() {
        assert_eq!(STRINGS.season_display_name(0), "Spezialfolgen");
        assert_eq!(STRINGS.season_display_name(2), "Staffel 2");
        assert_eq!(
            STRINGS.episode_display_subtitle(Some("Serie"), Some(2), Some(5)),
            Some("Serie - Staffel 2, Folge 5".to_owned())
        );
        assert_eq!(STRINGS.channel_language_label("deu"), "Deutsch");
        assert_eq!(
            STRINGS.hardware_acceleration_option_label("NVENC", false),
            "NVENC (auf diesem System nicht verfügbar)"
        );
    }

    #[test]
    fn representative_product_text_is_german() {
        assert_eq!(STRINGS.text(TextKey::Save), "Speichern");
        assert_eq!(STRINGS.text(TextKey::Cancel), "Abbrechen");
        assert_eq!(STRINGS.text(TextKey::Movies), "Filme");
        assert_eq!(STRINGS.text(TextKey::Settings), "Einstellungen");
    }

    #[test]
    fn unknown_diagnostic_values_pass_through() {
        assert_eq!(
            STRINGS.media_browser_error_message("custom database failure"),
            "custom database failure"
        );
    }
}
