use crate::admin::i18n::{LanguageStrings, TextKey};

pub const SORT_ORDER: u16 = 60;
pub const CODE: &str = "de-de";
pub const LABEL: &str = "Deutsch (Deutschland)";
pub const ALIASES: &[&str] = &["de", "de-de", "de_de"];
pub const TRANSLATION_COMPLETE: bool = true;

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
            Some(format!("{show} - S{season:02}E{episode:02}"))
        }
        (Some(season), Some(episode)) => Some(format!("S{season:02}E{episode:02}")),
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
        format!("Papierkorb geleert: {count} Elemente gelöscht.")
    }

    fn select_collection_to_add_selected_items(&self, count: usize) -> String {
        format!(
            "Sammlung auswählen, der die {count} ausgewählten Elemente hinzugefügt werden sollen"
        )
    }

    fn select_playlist_to_add_selected_items(&self, count: usize) -> String {
        format!(
            "Wiedergabeliste auswählen, der die {count} ausgewählten Elemente hinzugefügt werden sollen"
        )
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
        format!(
            "Keine {provider}-Quelle konfiguriert. Klicken Sie auf Verbindung konfigurieren, um eine hinzuzufügen."
        )
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
            "Der klassische Playout-Zustand erfordert Aufmerksamkeit für: {playouts}. Prüfen Sie die Programmzeiten und den gespeicherten Fortsetzungszustand. Aktualisieren erfordert eine verifizierte Programmgrenze und einen vollständigen Prüfpunkt; unsichere Anfragen lassen veröffentlichte Programme unverändert."
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
            "Der Status der Scan-Aktualisierung erfordert Aufmerksamkeit: {unresolved_resolutions} nicht abgeschlossene Auswirkungsauflösung(en), {stale_resolutions} festgefahrene Auswirkungsauflösung(en), {failed_intents} fehlgeschlagene Playout-Aktualisierungsanforderung(en) und {stale_intents} festgefahrene Playout-Aktualisierungsanforderung(en). Prüfen Sie die Scanner- und Planungsprotokolle und wiederholen Sie anschließend den betroffenen Scan oder die Playout-Aktualisierung."
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
            "{recent} Kanal/Kanäle lieferten in den letzten {minutes} Minuten Live-HLS-Medien langsamer als die Echtzeit; {active} war(en) bei der letzten Beobachtung noch nicht wiederhergestellt; {fallbacks} Kanal/Kanäle verwenden bis zu {fallback_hours} Stunden lang einen verifizierten Rückfall auf Softwarekodierung und/oder niedrigere Qualität. Prüfen Sie die Streaming-Protokolle und führen Sie vor Änderungen an den Anbietereinstellungen den TSDuck-Wiedergabetest mit Lieferzeitprüfung aus."
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
            "Die Kardinalität des Bildmaterials erfordert Aufmerksamkeit: {show_duplicate_rows} doppelte Bildzeile(n) für Serien, {season_duplicate_rows} doppelte Bildzeile(n) für Staffeln und {episode_duplicate_rows} doppelte Bildzeile(n) für Episoden; die Zählwerte der Fernsehprojektion sind {published_items} veröffentlichte Episodenelement(e) aus Add-on-Listen, {projection_rows} Projektionszeile(n), {distinct_episodes} eindeutige projizierte Episode(n), {complete_hierarchy_rows} vollständige Hierarchieprojektion(en) und {hierarchy_episodes} Hierarchieepisode(n). Aktualisieren oder veröffentlichen Sie die betroffenen Add-on-Fernsehlisten erneut."
        )
    }

    fn unable_to_load_playback_troubleshooting(&self, error: &dyn std::fmt::Display) -> String {
        format!("Kontext für die Wiedergabefehlerbehebung konnte nicht geladen werden: {error}")
    }

    fn playout_reset_prompt(&self, channel: &str, schedule: &str) -> String {
        format!(
            "Geben Sie die Uhrzeit ein, zu der das Playout auf Kanal {channel} mit Sendeplan {schedule} täglich zurückgesetzt werden soll"
        )
    }
}

pub fn text(key: TextKey) -> &'static str {
    match key {
        TextKey::Center => "Mitte",
        TextKey::FilePath => "Dateipfad",
        TextKey::Format => "Format",
        TextKey::GraphicsAuthoringInvalid => "Art, Name, Geometrie, Zeitangaben und YAML-Werte prüfen.",
        TextKey::GraphicsAuthoringPreviewFailed => "Die native Grafikdarstellung ist fehlgeschlagen oder hat das Zeitlimit überschritten. Renderer und Eingabedateien prüfen.",
        TextKey::GraphicsBaseStyle => "Basisstil",
        TextKey::GraphicsDisappear => "Ausblenden",
        TextKey::GraphicsEndBehavior => "Endverhalten",
        TextKey::GraphicsEpgEntries => "Programmeinträge",
        TextKey::GraphicsExport => "YAML exportieren",
        TextKey::GraphicsFixedSize => "Feste Größe",
        TextKey::GraphicsFontDirectory => "Schriftverzeichnis",
        TextKey::GraphicsFontFamily => "Schriftfamilie",
        TextKey::GraphicsFontSize => "Schriftgröße (Pixel)",
        TextKey::GraphicsGeometry => "Platzierung und Ebenen",
        TextKey::GraphicsHold => "Halten",
        TextKey::GraphicsHoldSeconds => "Halten (Sekunden)",
        TextKey::GraphicsLineHeight => "Zeilenhöhe (Pixel)",
        TextKey::GraphicsLoop => "Wiederholen",
        TextKey::GraphicsLuaCommand => "Lua-Quelltext oder Skriptdatei",
        TextKey::GraphicsMissingFont => "Die gewählte Schriftdatei fehlt oder ist leer.",
        TextKey::GraphicsMissingMedia => "Die gewählte Bild-, Video- oder Untertiteldatei fehlt oder ist leer.",
        TextKey::GraphicsMissingVariable => "Eine Vorlagenvariable fehlt. Kanal/Zeit auswählen oder explizite Vorschauwerte angeben.",
        TextKey::GraphicsPixelX => "X (Pixel)",
        TextKey::GraphicsPixelY => "Y (Pixel)",
        TextKey::GraphicsPreviewVariables => "Vorschauvariablen (JSON-Objekt)",
        TextKey::GraphicsRealPreview => "Echte Grafikvorschau",
        TextKey::GraphicsScaleWidth => "Skalierungsbreite (%)",
        TextKey::GraphicsScriptArgs => "Skriptargumente (eines pro Zeile)",
        TextKey::GraphicsSourceContent => "Innerhalb des Quellinhalts platzieren",
        TextKey::GraphicsStartSeconds => "Start (Sekunden)",
        TextKey::GraphicsTextAlign => "Textausrichtung",
        TextKey::GraphicsTextColor => "Textfarbe",
        TextKey::GraphicsTextFit => "Textanpassung",
        TextKey::GraphicsTextWidth => "Textbreite (%)",
        TextKey::GraphicsWrap => "Umbruch",
        TextKey::GraphicsYaml => "Grafik-YAML",
        TextKey::Left => "Links",
        TextKey::None => "Keine",
        TextKey::PixelFormat => "Pixelformat",
        TextKey::ProgrammeSeekSeconds => "Position ab Programmstart (Sekunden)",
        TextKey::Right => "Rechts",
        TextKey::GraphicsDrawvgTypography => "Drawvg nutzt eingebaute Bitmap-Zeichen. Schriftdateien, Familien und Verzeichnisse werden nicht verwendet. Breite, Umbruch und Skalierung nutzen diese Bitmap-Metriken.",
        TextKey::GraphicsDrawtextTypography => "Drawtext nutzt die gewählte echte Schrift in fester Größe. Breite, Umbruch und Skalierung begrenzen den Text nicht; Zeilenumbrüche explizit einfügen. Lange Titel und Sonderzeichen in der gerenderten Vorschau prüfen. Eine Schriftdatei hat Vorrang vor der Familie. Die Familiensuche kann eine Systemschrift ersetzen; verwenden Sie eine Datei für die genaue Schrift. Ohne Auswahl wird das mitgelieferte Roboto verwendet.",
        TextKey::GraphicsJpegXlHelp => "Kompaktes animiertes JPEG XL wird automatisch erkannt. Image wiederholt ab der Programmphase; Motion verwendet Start und Endverhalten. Native Dekodierung erhält Transparenz und sucht vorwärts durch dekodierte Frames.",
        TextKey::GraphicsGeometryHelp => "Image, Text und Motion verwenden exakte Ausgabepixel, wenn X und Y gesetzt sind; diese ersetzen Anker, Ränder und Auffüllung um den Quellinhalt. Skalierung und Deckkraft folgen den Fähigkeiten des nativen Renderers. Die Position von Subtitle und Script stammt aus ASS oder Lua; gemeinsame Positionswerte bleiben erhalten, werden aber nicht angewendet.",
        TextKey::GraphicsLuaHelp => "Lua läuft in der nativen Grafik-Engine und liefert ein VGS-Zeichenskript. Es startet keinen externen Befehl. Start und Dauer steuern die Sichtbarkeit. Format und Pixelformat bleiben für YAML-Kompatibilität erhalten, ändern aber nicht die VGS-Ausgabe.",
        TextKey::GraphicsRealPreviewHelp => "Ungespeicherte Definition mit dem nativen Modul auf transparenter Fläche rendern. Kanal und Zeitpunkt wählen den echten Programmkontext; die Position verschiebt diesen Zeitpunkt. Vorschau speichert nichts und ändert die Wiedergabe nicht.",
        TextKey::GraphicsYamlHelp => "Import prüft und speichert die gewählte Art und ersetzt beim Bearbeiten diese Definition. Export enthält ungespeicherte Felder. Unbekannte YAML-Felder werden abgelehnt.",

        TextKey::ExternalJsonPlayoutState => "Status des External-JSON-Sendeplans",
        TextKey::ExternalJsonDirectHelp => "Die Quelldatei wird direkt gelesen. Für kontrollierte Aktualisierungen wählen Sie {refresh} oder {reset} und deaktivieren Sie einmalig den Programmerhalt. Dies kann die Wiedergabe unterbrechen.",
        TextKey::ExternalJsonAcceptedHelp => "{continue} hängt nur Programme an. {refresh} und {reset} übernehmen zukünftige Programme, ohne externe Zeiten zu ändern. {reset} kann die Reihenfolge des externen Erstellers nicht zurücksetzen.",
        TextKey::ExternalJsonSourceHelp => "Bei kontrollierten Aktualisierungen bleibt der akzeptierte Plan nach einer Pfadänderung erhalten. Übernehmen Sie die neue Datei mit {refresh} oder {reset}.",
        TextKey::ExternalJsonInvalidHelp => "Der akzeptierte Plan ist ungültig oder wird nicht unterstützt. Prüfen Sie die Quelldatei und ersetzen Sie den Plan mit {refresh} oder {reset} bei deaktiviertem Programmerhalt.",
        TextKey::ExternalJsonSourceChangedHelp => "Der Quellpfad wurde geändert. Der akzeptierte Plan bleibt erhalten; übernehmen Sie die neue Datei mit {refresh} oder {reset}.",
        TextKey::ExternalJsonNextHelp => "External-JSON-Aktualisierungen benötigen die native Wiedergabe auf diesem Kanal und seinen Spiegelkanälen. Next liest keine externen Pläne.",
        TextKey::ExternalJsonExpiredHelp => "Der akzeptierte Plan enthält keine zukünftigen Programme. Prüfen und erweitern Sie die Quelldatei und aktualisieren Sie danach den Sendeplan.",
        TextKey::ExternalJsonBuildFailedHelp => "Der externe Plan konnte nicht übernommen werden. Prüfen Sie Quelldatei, Programmgrenzen und gewählte Aktion. Der zuletzt akzeptierte Plan bleibt in Verwendung.",
        TextKey::ExternalJsonDirect => "Direkte Dateiaktualisierungen",
        TextKey::ExternalJsonControlled => "Kontrollierte Aktualisierungen",
        TextKey::AcceptedRevision => "Akzeptierte Revision",
        TextKey::PlayoutContinuityUnavailable => "Der Fortsetzungspunkt des Programms fehlt oder ist inkompatibel. Mit Reset einen gültigen Zustand erstellen; Refresh behält bis dahin den bestehenden Sendeplan bei.",
        TextKey::BlockPlayoutState => "Zustand der Block-Ausstrahlung",
        TextKey::YamlPlayoutState => "Zustand der YAML-Ausstrahlung",
        TextKey::ScriptedPlayoutState => "Zustand der skriptgesteuerten Ausstrahlung",
        TextKey::ScriptedTeletextOverrideUnavailable => "Kanal {channel}: Die aktive Teletext-Auswahl ist nicht verfügbar. Der veröffentlichte Sendeplan bleibt erhalten.",
        TextKey::ScriptedTeletextGenerationUnavailable => "Kanal {channel}: Die Seiten des aktiven Teletextprofils sind nicht bereit. Der veröffentlichte Sendeplan bleibt erhalten.",
        TextKey::ScriptedTimelineRejected => "Die Zeitleiste wurde abgelehnt. Veröffentlichte Programme bleiben unverändert. Korrigieren Sie das Skript und versuchen Sie es erneut.",
        TextKey::ItemDetailsNotRequested => "Details nicht abgerufen",
        TextKey::ItemDetailsFailed => "Detailabruf fehlgeschlagen",
        TextKey::ItemDetailsRetry => "Details warten auf erneuten Versuch",
        TextKey::ItemProviderPlayback => "Anbieterwiedergabe",
        TextKey::ItemPosition => "Position",
        TextKey::ItemPrevious => "Zurück",
        TextKey::ItemNext => "Weiter",
        TextKey::ItemBack => "Zurück zu den Einträgen",
        TextKey::ItemTools => "Eintragswerkzeuge",
        TextKey::ItemEpisodePosition => "Episodenposition",
        TextKey::ItemNumbered => "Staffel und Episode",
        TextKey::ItemUnnumberedHelp => "Dieser Eintrag hat nach dem Speichern keine Staffel- oder Episodennummer.",
        TextKey::ItemNumberingHelp => "Ein nummerierter Eintrag benötigt beide Werte. Staffel 0 ist für Specials.",
        TextKey::ItemMetadata => "Zusammenfassung, Genres und Tags",
        TextKey::ItemMetadataHelp => "Weitere Titel, Sprachen und Mitwirkende",
        TextKey::ItemSaveHelp => "Speichern Sie Änderungen, um sie automatisch anzuwenden.",
        TextKey::ItemEditNumbering => "Nummerierung in der Liste bearbeiten",
        TextKey::ItemReplacementHelp => "Das Auswählen oder Entfernen eines Ersatzes ändert diesen Entwurf. Mit Eintrag speichern übernehmen.",
        TextKey::ItemReplacementPending => "Die Ersatzänderung ist nicht gespeichert.",
        TextKey::ItemSearchReplacement => "Lokale Medien suchen",
        TextKey::ItemRemoveReplacement => "Ersatz entfernen",
        TextKey::ItemInformation => "Medieninformationen und Herkunft",
        TextKey::ItemInformationHelp => "Audio, Untertitel, Streamdetails und Anbieteroptionen",
        TextKey::ItemSavedToolsHelp => "Diese Aktionen verwenden den gespeicherten Eintrag. Ungespeicherte Änderungen werden nicht berücksichtigt.",
        TextKey::ItemViewMedia => "Mediendetails anzeigen",
        TextKey::ItemSavedValue => "Gespeichert",
        TextKey::AddonWorkspaceSelectFirst => "Wählen Sie zuerst Einträge zur Bearbeitung aus.",
        TextKey::AddonsNoneInstalled => "Es sind noch keine Add-ons installiert.",
        TextKey::AddonsNoneAvailable => "Ihre Quellen bieten noch keine Add-ons an.",
        TextKey::AddonsSetupHelp => "Öffnen Sie Quellen, um ein Repository hinzuzufügen oder aus einer Datei zu installieren.",
        TextKey::AddonPackageHelp => "Verwenden Sie diese Zeile für eine ZIP-Datei mit einem einzelnen Add-on.",
        TextKey::AddonRepositoryPackageHelp => "Verwenden Sie diese Zeile für eine Repository-ZIP wie ErsatzRS-addons.zip mit einem Add-on-Katalog.",
        TextKey::AddonPackageUploadHelp => "Dieses Add-on-Paket konnte nicht installiert werden. Für eine Repository-ZIP verwenden Sie die Zeile Repository-Paket unten.",
        TextKey::AddonRepositoryUploadHelp => "Dieses Repository-Paket konnte nicht hinzugefügt werden. Für eine ZIP mit einem Add-on verwenden Sie die Zeile Add-on-Paket oben.",
        TextKey::AddonConfigurationSavedRecheck => "Gespeichert. Das Add-on ist deaktiviert, bis Sie es erneut prüfen und aktivieren.",
        TextKey::AddonCheckRequired => "Konfiguration geändert. Prüfen Sie das Add-on vor dem Aktivieren erneut.",
        TextKey::AddonCheckPending => "Add-on wird geprüft…",
        TextKey::AddonWorkspaceIncludeSelected => "Ausgewählte Einträge einschließen",
        TextKey::AddonWorkspaceExcludeSelected => "Ausgewählte Einträge ausschließen",
        TextKey::AddonWorkspaceInclusionHelp => "Speichern Sie Auswahländerungen, um sie automatisch anzuwenden.",
        TextKey::AddonWorkspacePublicationFailed => "Veröffentlichung fehlgeschlagen",
        TextKey::AddonWorkspacePublicationRestoredHelp => "Die vorherige Version wurde wiederhergestellt. Gespeicherte Änderungen bleiben erhalten und werden automatisch erneut versucht. Technische Fehler stehen unter Aktivität.",

        TextKey::AddonWorkspaceParentMetadataPreserved => "Die Verarbeitung wurde zum Schutz geänderter oder ungültiger übergeordneter NFO-Metadaten angehalten. Korrigieren Sie die Datei vor einem erneuten Versuch.",

        TextKey::AddonWorkspaceImporting => "Liste wird importiert…",
        TextKey::AddonWorkspaceImportingHelp => "Die Liste öffnet sich automatisch nach Abschluss der Ermittlung. Ohne JavaScript verwenden Sie Aktualisieren.",
        TextKey::AddonWorkspaceImportSteps => "Importieren Sie die Liste. Metadaten und benötigte Medien werden automatisch verarbeitet und veröffentlicht.",
        TextKey::AddonWorkspaceChooseProvider => "Wählen Sie unten ein Add-on, wenn für diese URL keines erkannt wird.",
        TextKey::AddonWorkspaceExcluded => "Von der Veröffentlichung ausgeschlossen",
        TextKey::AddonWorkspaceSort => "Sortierreihenfolge",
        TextKey::AddonWorkspaceAscending => "Aufsteigend",
        TextKey::AddonWorkspaceDescending => "Absteigend",
        TextKey::AddonWorkspaceItemDate => "Datum",
        TextKey::AddonWorkspaceSelectVisible => "Sichtbare Einträge auswählen",
        TextKey::AddonWorkspaceDeselectVisible => "Sichtbare Einträge abwählen",
        TextKey::ItemFromList => "Aus der Liste",
        TextKey::ItemUseListSetting => "Listeneinstellung verwenden",
        TextKey::ItemOverride => "Eintragsüberschreibung",
        TextKey::ItemPackageDefault => "Paketstandard; noch nicht aktiviert",
        TextKey::ItemPreviewOptions => "Optionsänderungen prüfen",
        TextKey::ItemAccessActive => "Eine Zugriffsprüfung läuft bereits.",
        TextKey::OpenItemInNewTab => "In neuem Tab öffnen",
        TextKey::BulkItemsToChange => "Zu ändernde Elemente",
        TextKey::BulkScopeHelp => "Nummerierung, Staffelerkennung und Änderungen der Medienart verwenden diese Elemente. Der Längenfilter gilt für Nummerierung und Staffelerkennung.",
        TextKey::BulkDetectSeasons => "Staffeln erkennen",
        TextKey::BulkFirstSeason => "Erste Staffel",
        TextKey::BulkReviewChanges => "Änderungen prüfen",
        TextKey::BulkPreviewSaves => "Anwenden speichert diese Werte; gespeicherte Änderungen werden automatisch veröffentlicht.",
        TextKey::BulkPreviewNothing => "Dieser Vorschlag ändert nichts.",
        TextKey::BulkApplyNumbering => "Nummerierung anwenden",
        TextKey::BulkApplyChanges => "Änderungen anwenden",
        TextKey::BulkCancelPreview => "Vorschau abbrechen",
        TextKey::ActiveFilter => "Aktiver Filter",
        TextKey::AddonWorkspaceLengthFilter => "Nach Medienlänge filtern",
        TextKey::AddonWorkspaceMinimumLength => "Mindestlänge",
        TextKey::AddonWorkspaceMaximumLength => "Höchstlänge",
        TextKey::AddonWorkspaceUnknownLength => "Unbekannte Längen einschließen",
        TextKey::AddonWorkspaceLengthHelp => "Minuten:Sekunden. Ohne Begrenzung leer lassen.",
        TextKey::AddonWorkspaceReviewLatest => "Neueste gespeicherte Version prüfen",
        TextKey::AddonWorkspaceReviewRetained => "Ihre Eingabe bleibt erhalten. Prüfen Sie sie vor dem Speichern anhand der neuesten Werte. Andere offene Aufgaben müssen separat geprüft werden.",
        TextKey::AddonWorkspaceStreamHelp => "Direkt von der Quelle abspielen.",
        TextKey::AddonWorkspaceDownloadHelp => "Jedes Video vor der Wiedergabe vorbereiten.",
        TextKey::AddonWorkspaceNoOptions => "Dieses Add-on bietet keine Zugriffs- oder Segmenteinstellungen.",
        TextKey::AddonWorkspaceNewHelp => "Neu importierte Videos herunterladen; vorhandene streamen.",
        TextKey::AddonWorkspaceSizeLimit => "Dieses Formular ist zu groß. Gehen Sie zu Ihrer Eingabe zurück und speichern Sie eine kleinere Aufgabe.",
        TextKey::AddonWorkspaceItems => "Einträge",
        TextKey::AddonWorkspaceSettings => "Listeneinstellungen",
        TextKey::AddonWorkspacePlayback => "Wiedergabe und Zugriff",
        TextKey::AddonWorkspaceActivity => "Aktivität und Wiederherstellung",
        TextKey::AddonWorkspaceSections => "Listenbereiche",
        TextKey::AddonWorkspaceSaveSettings => "Einstellungen speichern",
        TextKey::AddonWorkspaceSaveItem => "Eintrag speichern",
        TextKey::AddonWorkspaceSaveNumbering => "Nummerierung speichern",
        TextKey::AddonWorkspacePublish => "Gespeicherte Änderungen veröffentlichen",
        TextKey::AddonWorkspacePublished => "Veröffentlichte Liste ist aktuell",
        TextKey::AddonWorkspacePending => "Gespeicherte Änderungen werden automatisch angewendet",
        TextKey::AddonWorkspacePreparationRequired => "Benötigte Medien werden automatisch vorbereitet",
        TextKey::AddonWorkspacePreparing => "Medien werden für die Veröffentlichung vorbereitet",
        TextKey::AddonWorkspaceAddIncomplete => "Die Liste wurde hinzugefügt, aber einige Einstellungen wurden nicht gespeichert. Listeneinstellungen öffnen, prüfen und erneut speichern.",
        TextKey::AddonWorkspacePreparationFailed => "Medienvorbereitung fehlgeschlagen",
        TextKey::RetryPreparation => "Vorbereitung erneut versuchen",
        TextKey::PreparationFailedItems => "Vorbereitungsaufgaben",
        TextKey::PreparationFailureFailed => "Das Add-on konnte die Medien nicht vorbereiten. Vorbereitung erneut versuchen und das Add-on prüfen, wenn es weiterhin fehlschlägt.",
        TextKey::PreparationFailureInputChanged => "Die Vorbereitungsanfrage passte nicht mehr zur gespeicherten Liste oder zum installierten Add-on. Vorbereitung erneut versuchen.",
        TextKey::PreparationFailureInvalidRequest => "Die gespeicherten Optionen passen nicht zum installierten Add-on. Wiedergabe und Zugriff prüfen, speichern und die Vorbereitung erneut versuchen.",
        TextKey::PreparationFailureCancelled => "Die Vorbereitung wurde abgebrochen. Vorbereitung erneut versuchen, um neu zu beginnen.",
        TextKey::PreparationFailureAttemptsExhausted => "Die Vorbereitung wurde nach wiederholten Fehlversuchen beendet. Vorbereitung erneut versuchen und das Add-on prüfen, wenn es weiterhin fehlschlägt.",
        TextKey::PreparationFailureStorageFull => "Für die Vorbereitung der Medien war nicht genug Speicherplatz vorhanden. Speicherplatz freigeben und die Vorbereitung erneut versuchen.",
        TextKey::PreparationFailureUnsupported => "Das Add-on kann diese Medien nicht vorbereiten.",
        TextKey::PreviousSavedRevision => "Vorherige gespeicherte Revision",
        TextKey::AddonWorkspacePreparationActivity => "Medienvorbereitung",
        TextKey::AddonWorkspaceBulk => "Mehrere Einträge bearbeiten",
        TextKey::AddonWorkspaceSelection => "Zur Bearbeitung ausgewählt",
        TextKey::AddonWorkspaceOffPage => "Auf anderen Seiten",
        TextKey::AddonWorkspaceDiscard => "Bearbeitungen verwerfen",
        TextKey::AddonWorkspaceSelected => "Ausgewählte Einträge",
        TextKey::AddonWorkspaceMatching => "Alle passenden Einträge",
        TextKey::AddonWorkspacePage => "Aktuelle Seite",
        TextKey::AddonWorkspaceAccess => "Zugriff und Segmente",
        TextKey::AddonWorkspaceSaveConflict => "Die gespeicherten Einstellungen wurden geändert. Prüfen Sie vor dem Speichern die aktuelle Version.",
        TextKey::AddonWorkspaceSaveInvalid => "Diese Einstellungen konnten nicht validiert werden. Prüfen Sie Ihre Eingaben und versuchen Sie es erneut.",
        TextKey::AddonWorkspaceSaveStorage => "Die Einstellungen konnten nicht gespeichert werden. Ihre Eingaben bleiben erhalten; versuchen Sie es erneut.",
        TextKey::AddonWorkspaceLoadIncomplete => "Einige Editorinformationen konnten nicht konsistent geladen werden. Ihre Eingaben bleiben erhalten. Prüfen Sie die aktuelle Version, um es erneut zu versuchen.",
        TextKey::TechnicalDetails => "Technische Details",
        TextKey::AddonPermissionNetworkHelp => "Verbindungen zu den aufgeführten Netzwerkhosts herstellen",
        TextKey::AddonPermissionMediaHelp => "Für ErsatzRS verfügbare Medien lesen",
        TextKey::AddonPermissionFfmpegHelp => "Die verwaltete FFmpeg-Laufzeit ausführen",
        TextKey::AddonPermissionCommandHelp => "Die aufgeführten externen Befehle ausführen",
        TextKey::PreparationTasks => "Vorbereitungsaufgaben",
        TextKey::PreparationAffectedItems => "Betroffene Einträge",
        TextKey::PreparationIntervals => "Intervall- / SponsorBlock-Aufgaben",
        TextKey::PreparationDownloads => "Downloadaufgaben",
        TextKey::PreparationSubtitles => "Untertitelaufgaben",
        TextKey::PreparationQueued => "In Warteschlange",
        TextKey::PreparationRunning => "Wird ausgeführt",
        TextKey::PreparationWaiting => "Wartet",
        TextKey::PreparationNotQueued => "Nicht eingereiht",
        TextKey::LastActivity => "Letzte Aktivität",
        TextKey::AddonAccessUnknownFailure => "Die Ursache dieses Zugriffsfehlers ist unbekannt. Prüfen Sie die gespeicherten Zugriffseinstellungen, testen Sie die Quelle im Browser und wiederholen Sie die Zugriffsprüfung.",
        TextKey::AddonWorkspaceRetained => "Änderungen konnten nicht gespeichert werden. Ihre Eingaben bleiben erhalten; prüfen Sie die Fehler oder laden Sie die aktuelle Revision neu.",
        TextKey::AddonWorkspaceNavigation => "Die Navigation behält ungespeicherte Änderungen. Gespeicherte Änderungen werden automatisch angewendet.",
        TextKey::AddonWorkspaceSeasonOnly => "Nur Staffel ändern",
        TextKey::AddonWorkspaceSeasonPerYear => "Neue Staffel pro Kalenderjahr",
        TextKey::AddonWorkspaceYearDatesIncomplete => "Uploaddaten fehlen noch. Warten Sie auf die Details oder ergänzen Sie die fehlenden Daten, bevor Sie Staffeln nach Jahr gruppieren.",
        TextKey::AddonWorkspacePageSize => "Einträge pro Seite",

        TextKey::AddonAccessTest => "Zugriff testen",
        TextKey::AddonAccessHelp => {
            "Prüfen Sie dieses Element mit den gespeicherten Entwurfsoptionen. Ungespeicherte Änderungen werden nicht berücksichtigt; die veröffentlichte Wiedergabe bleibt unverändert."
        }
        TextKey::AddonAccessSaveFirst => {
            "Speichern Sie zuerst eine Option, um die Entwurfseinstellungen vor dem Zugriffstest zu aktivieren."
        }
        TextKey::AddonAccessError => {
            "Die Zugriffsprüfung konnte nicht geladen oder geändert werden. Aktualisieren Sie das Element und versuchen Sie es erneut."
        }
        TextKey::AddonAccessNotChecked => "Nicht geprüft",
        TextKey::AddonAccessQueued => "Zugriffsprüfung eingereiht",
        TextKey::AddonAccessRunning => "Zugriff wird geprüft",
        TextKey::AddonAccessWaiting => "Warten auf erneute Zugriffsprüfung",
        TextKey::AddonAccessCancelled => "Zugriffsprüfung abgebrochen",
        TextKey::AddonAccessStale => "Zugriffsnachweis abgelaufen oder Einstellungen geändert",
        TextKey::AddonAccessAccessible => "Zugänglich",
        TextKey::AddonAccessProfileUnavailable => "Das ausgewählte Browserprofil ist für den Server nicht verfügbar. Prüfen Sie das gespeicherte Browserprofil und starten Sie die Zugriffsprüfung erneut.",
        TextKey::AddonAccessProfileLocked => "Das Browserprofil ist gesperrt. Schließen Sie den Browser, der dieses Profil verwendet, und wiederholen Sie die Zugriffsprüfung.",
        TextKey::AddonAccessDecryptionFailed => "Der Server konnte die Browser-Anmeldedaten nicht entschlüsseln. Prüfen Sie das Browserprofil und das Konto, unter dem der Server läuft, und versuchen Sie es erneut.",
        TextKey::AddonDerivedMetadataLimited => "Die Serienmetadaten haben die Größenbegrenzung erreicht. Die Episodenmetadaten bleiben vollständig.",
        TextKey::AddonLocalPlaybackAvailable => "Lokale Kopie abspielbar",
        TextKey::AddonAccessRestricted => "Der Anbieter hat den Zugriff eingeschränkt. Öffnen Sie die Quelle im Browser, prüfen Sie Konto- oder Regionsbeschränkungen und wiederholen Sie die Zugriffsprüfung.",
        TextKey::AddonAccessNextAttempt => "Nächster Versuch:",
        TextKey::AddonAccessExpires => "Nachweis läuft ab:",
        TextKey::AddonAccessRefresh => "Status aktualisieren",
        TextKey::AddonOptionsCurrentFilter => "Aktueller Filter",
        TextKey::AddonOptions => "Anbieteroptionen",
        TextKey::AddonOptionsDraftHelp => "Gespeicherte Optionen werden automatisch angewendet. Die aktuelle Wiedergabe bleibt während der Verarbeitung verfügbar.",
        TextKey::AddonOptionsProvisional => {
            "Diese Paketstandardwerte sind vorläufig. Das Speichern einer Option aktiviert sie für diesen Entwurf."
        }
        TextKey::AddonOptionsListDefault => "Listenstandard",
        TextKey::AddonOptionsInherited => "Geerbt",
        TextKey::AddonOptionsOverride => "Überschrieben",
        TextKey::AddonOptionsFiniteOnly => {
            "Für Live-Inhalte oder Inhalte unbekannter Dauer nicht verfügbar: Diese Option benötigt Medien mit endlicher Dauer. Direkte Wiedergabe bleibt verfügbar."
        }
        TextKey::AddonOptionsResetPackage => "Paketstandard wiederherstellen",
        TextKey::AddonOptionsResetList => "Listenstandard wiederherstellen",
        TextKey::AddonOptionsMixed => "Unterschiedliche Werte — Wert auswählen",
        TextKey::AddonOptionsPreviewHelp => {
            "Prüfen Sie diese konkreten Einträge vor dem Anwenden. Eine zwischenzeitliche Entwurfs- oder Schemaänderung erfordert eine neue Vorschau."
        }
        TextKey::AddonOptionsBefore => "Vorher",
        TextKey::AddonOptionsAfter => "Nachher",
        TextKey::AddonOptionsConflict => {
            "Optionen konnten nicht gespeichert oder geladen werden. Entwurf, Auswahl oder Paketschema wurden möglicherweise geändert. Laden Sie den Editor neu und versuchen Sie es erneut."
        }
        TextKey::AddonOptionsNone => "Dieses Paket definiert keine Optionen.",
        TextKey::Addon => "Add-on",
        TextKey::PrimaryFeature => "Hauptfilm",
        TextKey::MovieExtra => "Filmextra",
        TextKey::NeedReview => "Hinweise",

        TextKey::ReviewRecommended => "Überprüfung empfohlen",
        TextKey::PublicationReadiness => "Veröffentlichungsbereitschaft",
        TextKey::PublicationBlockers => "Blockierende Probleme",
        TextKey::PublicationActivity => "Aktivität",
        TextKey::PublicationJobs => "Veröffentlichungsaufträge",
        TextKey::PublicationEvents => "Ereignisse",
        TextKey::NoPublicationActivity => {
            "Es wurde noch keine Veröffentlichungsaktivität aufgezeichnet."
        }
        TextKey::CopyRedactedDiagnostics => "Bereinigte Diagnose kopieren",
        TextKey::DownloadRedactedDiagnostics => "Bereinigte Diagnose herunterladen",
        TextKey::RedactedDiagnosticsCopied => "Bereinigte Diagnose kopiert.",
        TextKey::PublicationGeneration => "Generation",
        TextKey::CorrelationId => "Korrelations-ID",
        TextKey::TargetPublishedRevision => "Zielrevision der Veröffentlichung",
        TextKey::RelatedOperation => "Zugehöriger Vorgang",
        TextKey::FilesystemTransition => "Dateisystemübergang",
        TextKey::RetryClass => "Wiederholungsklasse",
        TextKey::RecoveryAction => "Wiederherstellungsaktion",
        TextKey::OperatingSystemErrorCode => "Betriebssystem-Fehlercode",
        TextKey::PublicationOutcome => "Ergebnis",
        TextKey::PublicationExpired => "Abgelaufen",
        TextKey::EventExistingJobAdopted => "Bestehende Veröffentlichung übernommen",
        TextKey::EventEnqueued => "Veröffentlichung eingereiht",
        TextKey::EventLeaseWaitStarted => "Warten auf Bibliothekszugriff",
        TextKey::EventLeaseAcquired => "Bibliothekszugriff erhalten",
        TextKey::EventLeaseReclaimed => {
            "Veröffentlichung nach Unterbrechung oder abgelaufener Sperre fortgesetzt"
        }
        TextKey::EventPhaseAdvanced => "Veröffentlichungsphase fortgeschritten",
        TextKey::EventChildScanStarted => "Prüfscan gestartet",
        TextKey::EventChildScanCompleted => "Prüfscan abgeschlossen",
        TextKey::EventChildScanFailed => "Prüfscan fehlgeschlagen",
        TextKey::EventOriginalFailure => "Ursprünglicher Veröffentlichungsfehler",
        TextKey::EventRecoveryFailure => "Wiederherstellungsfehler",
        TextKey::EventRetryScheduled => "Wiederholungsversuch geplant",
        TextKey::EventRecoveryRequested => "Wiederherstellung angefordert",
        TextKey::EventCompleted => "Veröffentlichung abgeschlossen",
        TextKey::EventHistoryTruncated => "Ältere Aktivität wurde gekürzt",
        TextKey::TriggerAutomaticRefresh => "Geplante Aktualisierung",
        TextKey::TriggerOperator => "Von einem Bediener angefordert",
        TextKey::TriggerRestoreRevision => "Durch Wiederherstellen einer Revision angefordert",
        TextKey::TestSource => "Quelle testen",
        TextKey::TestSourceHelp => {
            "Gespeicherte Quelle prüfen, ohne die Liste zu importieren oder zu ändern."
        }
        TextKey::SourceTestQueued => "Quellentest eingereiht",
        TextKey::SourceTestRunning => "Quellentest läuft…",
        TextKey::SourceTestStale => {
            "Der Quellentest ist veraltet, weil seine gespeicherten Eingaben nicht mehr zur aktuellen Liste, zum Add-on-Paket oder zu den Standardwerten passen."
        }
        TextKey::SourceAvailable => "Quelle ist verfügbar",
        TextKey::SourceUnsupported => "Quelle wird nicht unterstützt",
        TextKey::SourceTestFailed => "Quellentest fehlgeschlagen",
        TextKey::SourceRevision => "Quellrevision",
        TextKey::LastSuccessfulPlayoutBuild => "Letzter erfolgreicher Aufbau",
        TextKey::PlayoutRefreshPending => "Aktualisierung ausstehend",
        TextKey::PlayoutRefreshFailed => "Aktualisierung fehlgeschlagen; der veröffentlichte Sendeplan bleibt erhalten.",
        TextKey::PlayoutSourceCheckPending => "Quelländerungen werden geprüft",
        TextKey::PlayoutSourceCheckFailed => "Quellvergleich fehlgeschlagen; ein neuer Versuch steht aus.",
        TextKey::FirstPageComplete => "Erste Seite vollständig",
        TextKey::ProviderIdentity => "Anbieteridentität",
        TextKey::ReturnedFirstPageItems => "Elemente auf der ersten Seite",
        TextKey::ProviderTotalItemCount => "Gesamtzahl der Elemente laut Anbieter",
        TextKey::DraftChanges => "Gespeicherte Entwurfsänderungen",
        TextKey::ExactDraftDiffHelp => {
            "Exakte Unterschiede des gespeicherten Entwurfs zur aktuellen veröffentlichten Revision."
        }
        TextKey::DraftDiffUnavailable => {
            "Ein exakter Entwurfsvergleich ist erst nach einer vollständigen Veröffentlichung verfügbar."
        }
        TextKey::MetadataChanges => "Metadatenänderungen",
        TextKey::ExcludedItems => "Ausgeschlossene Elemente",
        TextKey::Conflicts => "Konflikte",
        TextKey::PendingRemovals => "Ausstehende Entfernungen",
        TextKey::PendingRemovalHelp => {
            "In einem vollständigen Abgleich einmal nicht gefunden. Jetzt entfernen oder eine zweite vollständige Beobachtung nach mindestens 24 Stunden abwarten."
        }
        TextKey::Reappearances => "Wieder erschienene Elemente",
        TextKey::RemovalCandidates => "Entfernungskandidaten",
        TextKey::ConfirmRemoval => "Entfernung bestätigen",
        TextKey::RemovalConfirmed => "Entfernung bestätigt.",
        TextKey::RemovalConfirmationFailed => "Bestätigung der Entfernung fehlgeschlagen.",
        TextKey::RevisionHistory => "Revisionsverlauf",
        TextKey::RevisionHistoryHelp => {
            "Die Wiederherstellung ersetzt den aktuellen Entwurf und veröffentlicht den gewählten Stand als neue Revision."
        }
        TextKey::NoRevisionHistory => "Noch keine veröffentlichten Revisionen gespeichert.",
        TextKey::Revision => "Revision",
        TextKey::CompareRevisions => "Revisionen vergleichen",
        TextKey::RestoreRevision => "Revision wiederherstellen",
        TextKey::RestoreRevisionWarning => {
            "Dies ersetzt den aktuellen Entwurf und veröffentlicht Revision"
        }
        TextKey::RestoreRevisionQueued => "Wiederherstellung eingereiht",
        TextKey::RestoreRevisionFailed => "Revision konnte nicht wiederhergestellt werden.",
        TextKey::PublishedItems => "Veröffentlichte Elemente",
        TextKey::GeneratedPlaylists => "Erzeugte Wiedergabelisten",
        TextKey::ActiveOrFuturePlayouts => "Aktive oder zukünftige Ausspielungen",
        TextKey::EffectiveOptions => "Wirksame Optionen",
        TextKey::Files => "Dateien",
        TextKey::Artifacts => "Artefakte",
        TextKey::Added => "Hinzugefügt",
        TextKey::Removed => "Entfernt",
        TextKey::Changed => "Geändert",
        TextKey::FilesystemPreviousBackedUp => "Vorherige Revision gesichert",
        TextKey::FilesystemCandidateCommitted => "Kandidatenrevision übernommen",
        TextKey::FilesystemPreviousRestored => "Vorherige Revision wiederhergestellt",
        TextKey::FilesystemProjectionRestored => "Datenbankprojektion wiederhergestellt",
        TextKey::FilesystemRevisionVerified => "Revision geprüft",
        TextKey::FilesystemCleanupCompleted => "Veröffentlichungsbereinigung abgeschlossen",
        TextKey::RetryTransientRecovery => "Vorübergehende Wiederherstellung",
        TextKey::ErrorFilesystemIo => "Dateisystem-E/A",
        TextKey::ErrorPermissionDenied => "Zugriff verweigert",
        TextKey::ErrorSharingViolation => "Dateifreigabeverletzung",
        TextKey::ErrorUnsafeFilesystemState => "Unsicherer Dateisystemzustand",
        TextKey::ErrorInvalidPublicationMarker => "Ungültige Veröffentlichungsmarkierung",
        TextKey::ErrorPublicationIntegrity => "Integritätsprüfung der Veröffentlichung",
        TextKey::OutcomePublished => "Veröffentlicht",
        TextKey::OutcomeRestoredAfterFailure => "Vorherige Revision nach Fehler wiederhergestellt",
        TextKey::ReviewRecommendations => "Überprüfungsempfehlungen",
        TextKey::OptionalDetails => "Optionale Details",
        TextKey::DraftSaved => "Entwurf gespeichert",
        TextKey::SavedRevision => "Gespeicherte Revision",
        TextKey::PublishList => "Liste veröffentlichen",
        TextKey::PreparePublication => "Veröffentlichung vorbereiten",
        TextKey::ProcessedMediaRequired => "Vorbereitungsaufgaben",
        TextKey::ProcessedMediaQueued => "Medienvorbereitung eingereiht",
        TextKey::PreparingProcessedMedia => "Mediendateien werden vorbereitet",
        TextKey::PreparingPublication => "Veröffentlichung wird vorbereitet",
        TextKey::Publishing => "Wird veröffentlicht…",
        TextKey::PublicationQueued => "Veröffentlichung in Warteschlange",
        TextKey::RestoringPublication => "Vorherige Revision wird wiederhergestellt…",
        TextKey::Published => "Veröffentlicht",
        TextKey::PublishSavedRevisionHelp => {
            "Veröffentlicht wird genau die gespeicherte Revision. Listenänderungen zuerst speichern."
        }
        TextKey::ChangesSavedImmediately => {
            "Element- und Massenänderungen werden sofort gespeichert und können rückgängig gemacht werden."
        }
        TextKey::ReviewItem => "Element überprüfen",
        TextKey::ShowFullDescription => "Vollständige Beschreibung anzeigen",
        TextKey::ShowLess => "Weniger anzeigen",
        TextKey::ImportAList => "Liste importieren",
        TextKey::ImportOverview => "Übersicht importieren",
        TextKey::AddonContextSelected => "Ausgewähltes Add-on",
        TextKey::CompletedIn => "Abgeschlossen in",
        TextKey::NoPublicationBlockers => "Keine blockierenden Probleme.",
        TextKey::BlockerOverviewIncomplete => "Die vollständige Übersicht muss verifiziert sein.",
        TextKey::BlockerPublicationInProgress => "Die Veröffentlichung läuft bereits.",
        TextKey::BlockerRecoveryRequired => {
            "Eine vorherige Veröffentlichung muss noch wiederhergestellt werden."
        }
        TextKey::BlockerCollectionTitleMissing => "Die Sammlung benötigt einen Titel.",
        TextKey::BlockerInvalidArtwork => {
            "Bilder müssen eine gültige begrenzte HTTPS-URL verwenden."
        }
        TextKey::BlockerNoIncludedItems => "Mindestens ein Element einschließen.",
        TextKey::BlockerMoviePrimaryCount => {
            "Die Filmveröffentlichung benötigt genau einen Hauptfilm."
        }
        TextKey::BlockerPlaybackDecisionRequired => {
            "Endliches oder Live-Wiedergabeverhalten wählen."
        }
        TextKey::BlockerPlaybackDetailsUnavailable => {
            "Erforderliche Wiedergabedetails sind über dieses Add-on nicht verfügbar."
        }
        TextKey::BlockerPlaybackDetailsFailed => {
            "Die Anbieterdetails enthalten keine positive Dauer."
        }
        TextKey::BlockerUnavailableWithoutReplacement => {
            "Das nicht verfügbare Element benötigt einen gültigen lokalen Ersatz."
        }
        TextKey::BlockerItemTitleMissing => "Das Element benötigt einen Titel.",
        TextKey::BlockerSeasonEpisodeMissing => {
            "Eine gültige Staffel- und Episodennummer eingeben."
        }
        TextKey::BlockerArtistMissing => "Mindestens einen Künstler eingeben.",
        TextKey::PublishUnsavedChanges => {
            "Änderungen vor der Veröffentlichung dieser Revision speichern."
        }
        TextKey::PublicationBlocked => "Die Veröffentlichung ist blockiert.",
        TextKey::PublishBlocked => "Veröffentlichen blockiert",
        TextKey::RetryRecovery => "Wiederherstellung wiederholen",
        TextKey::RestorePreviousRevision => "Vorherige Revision wiederherstellen",
        TextKey::PublicationPhase => "Veröffentlichungsphase",
        TextKey::PublicationAttempts => "Versuche",
        TextKey::LastProgress => "Letzter Fortschritt",
        TextKey::FailureReason => "Ursprünglicher Grund",
        TextKey::RestrictedItemsExcluded => {
            "Eingeschränkte Elemente aus dieser Revision ausgeschlossen"
        }
        TextKey::RecoveryQueued => "Wiederherstellung eingereiht",
        TextKey::RecoveryFailed => "Wiederherstellung konnte nicht eingereiht werden.",
        TextKey::PhaseAwaitingLease => "Warten auf Bibliothekszugriff",
        TextKey::PhaseStaging => "Revision wird vorbereitet",
        TextKey::PhaseScanning => "Vorbereitete Revision wird gescannt",
        TextKey::PhaseCommitting => "Revision wird übernommen",
        TextKey::PhaseRestoringFiles => "Dateien werden wiederhergestellt",
        TextKey::PhaseRestoringProjection => "Datenbankprojektion wird wiederhergestellt",
        TextKey::PhaseReconciling => "Wiederhergestellte Revision wird abgeglichen",
        TextKey::PhaseVerifying => "Revision wird überprüft",
        TextKey::PhaseCleaningUp => "Veröffentlichungsdateien werden bereinigt",
        TextKey::PhaseComplete => "Abgeschlossen",
        TextKey::InstallLeavesDisabled => {
            "Nach der Installation bleibt dieses Add-on deaktiviert, bis es aktiviert wird."
        }
        TextKey::EnableActivatesPermissions => {
            "Durch Aktivieren werden die deklarierten Berechtigungen des Add-ons wirksam."
        }
        TextKey::FirstAired => "Erstausstrahlung",
        TextKey::LoadItemDetailsAfterPublishing => "Elementdetails automatisch laden",
        TextKey::LoadItemDetailsHelp => {
            "Zuerst die Übersicht importieren. Für die Wiedergabe erforderliche technische Details werden vor der ersten Veröffentlichung geladen; übrige erweiterte Metadaten folgen kontrolliert."
        }
        TextKey::OverviewOnlyManualDetailsHelp => {
            "Nur die Übersicht importieren und beschreibende Details manuell anfordern. Für die Wiedergabe erforderliche technische Details werden vor der Veröffentlichung automatisch vorbereitet."
        }
        TextKey::PlaybackDetailsQueued => "Wiedergabedetails vorgemerkt",
        TextKey::PublishAgainAfterDetails => "Vollständige Details werden automatisch übernommen.",
TextKey::FetchDetails => "Details abrufen",
        TextKey::DetailsQueued => "Details vorgemerkt",
        TextKey::DetailsLoading => "Details werden geladen",
        TextKey::RetryDetails => "Details erneut versuchen",
        TextKey::DetailsComplete => "Details vollständig",
        TextKey::DetailsSkipped => "Details übersprungen",
        TextKey::FindingProgrammes => "Programme werden gesucht",
        TextKey::CheckingOverview => "Übersicht wird geprüft",
        TextKey::ReadyToReview => "Wird automatisch verarbeitet",

        TextKey::LoadingItemDetails => "Elementdetails werden geladen",
        TextKey::WaitingForProvider => "Warten auf Anbieter",
        TextKey::OverviewOnly => "Nur Übersicht",
        TextKey::PauseImport => "Import anhalten",
        TextKey::ResumeImport => "Import fortsetzen",
        TextKey::RetryImportNow => "Import jetzt wiederholen",
        TextKey::EmptyImportNeedsRetry => "Leerer Import muss wiederholt werden",
        TextKey::EmptyImportRetryHelp => {
            "Dieser abgeschlossene Import enthält keine Elemente. Aktualisieren Sie das Add-on und wiederholen Sie den Import; Ihre Listeneinstellungen und Bearbeitungen bleiben erhalten."
        }
        TextKey::CancelImport => "Import abbrechen",
        TextKey::OtherPageItemsAffected => {
            "Weitere betroffene Elemente befinden sich auf anderen Seiten"
        }
        TextKey::ElapsedTime => "Verstrichene Zeit",
        TextKey::LastSuccessfulProgress => "Letzter erfolgreicher Fortschritt",
        TextKey::EstimatedRemainingTime => "Geschätzte Restzeit",
        TextKey::PlaybackBehavior => "Wiedergabeverhalten",
        TextKey::UseProviderValue => "Anbieterwert verwenden",
        TextKey::FiniteMedia => "Endliches Medium",
        TextKey::LiveSource => "Livequelle",
        TextKey::ProviderValueChanged => "Anbieterwert geändert",
        TextKey::ApplyToSelected => "Auf Auswahl anwenden",
        TextKey::ApplyToAllRemoteStreams => "Auf alle Remote-Streams anwenden",
        TextKey::SelectItem => "Element auswählen",
        TextKey::NumberEpisodes => "Folgen nummerieren",
        TextKey::NumberEpisodesHelp => {
            "Ordnen Sie eine Staffel und fortlaufende Folgennummern in der ursprünglichen Reihenfolge des Add-ons zu. Ausgeschlossene Einträge und Kapitelfragmente werden übersprungen."
        }
        TextKey::FirstEpisode => "Erste Folge",
        TextKey::NumberToAssign => "Anzahl",
        TextKey::AllRemaining => "Alle verbleibenden",
        TextKey::ReplaceExistingEpisodeNumbers => "Vorhandene Folgennummern ersetzen",
        TextKey::AutoDetectSeasons => "Staffeln anhand der Erstausstrahlung automatisch erkennen",
        TextKey::AutoDetectSeasonsHelp => {
            "Eine neue Staffel beginnt, wenn der Abstand zwischen zwei bekannten Erstausstrahlungen mindestens dem eingestellten Wert entspricht. Einträge ohne Datum bleiben in der aktuellen Staffel."
        }
        TextKey::MinimumSeasonGapDays => "Mindestabstand zwischen Staffeln (Tage)",
        TextKey::NumberingStartsAt => "Nummerierung beginnt bei",
        TextKey::FirstIncludedItem => "Erster aufgenommener Eintrag",
        TextKey::StartWithFirstItem => "Mit erstem Eintrag beginnen",
        TextKey::StartHere => "Hier beginnen",
        TextKey::SaveSeasonEpisode => "Staffel und Episode speichern",
        TextKey::BulkEdit => "Massenbearbeitung",
        TextKey::Scope => "Bereich",
        TextKey::AllItems => "Alle Einträge",
        TextKey::CurrentPage => "Aktuelle Seite",
        TextKey::ItemsNeedingReview => "Einträge mit Hinweisen",

        TextKey::KeepCurrentValue => "Aktuellen Wert beibehalten",
        TextKey::UndoLastChange => "Letzte Änderung rückgängig machen",
        TextKey::Undo => "Rückgängig",
        TextKey::ChangeUndone => "Änderung rückgängig gemacht",
        TextKey::MetadataConflicts => "Metadatenkonflikte",
        TextKey::MetadataConflictsHelp => "Der Anbieter hat angepasste Felder geändert. Ihre gespeicherten Werte bleiben gültig.",
TextKey::PreviousProviderValue => "Vorheriger Anbieterwert",
        TextKey::NewProviderValue => "Neuer Anbieterwert",
        TextKey::OperatorValue => "Betreiberwert",
        TextKey::Discovered => "Erkannt",
        TextKey::Examined => "Geprüft",
        TextKey::ArchivedSkipped => "Wegen Archiv übersprungen",
        TextKey::Emitted => "Ausgegeben",
        TextKey::EditMetadata => "Metadaten bearbeiten",
        TextKey::OneTagPerLine => "Ein Tag pro Zeile",
        TextKey::OriginalTitle => "Originaltitel",
        TextKey::ReleaseDate => "Veröffentlichungsdatum",
        TextKey::Year => "Jahr",
        TextKey::SortTitle => "Sortiertitel",
        TextKey::MovieFilesAndExtras => "Filmdateien und Extras",
        TextKey::MovieFilesAndExtrasHelp => {
            "Genau einen Hauptfilm zuweisen. Die Quellreihenfolge bestimmt die Rolle nicht; ein Trailer darf zuerst stehen."
        }
        TextKey::EpisodesMetadataHelp => {
            "Episodenidentität und Nummerierung prüfen; die Quellreihenfolge bleibt unverändert."
        }
        TextKey::MusicVideosMetadataHelp => {
            "Künstler, Album und Titel gehören zu jedem Video; Listen mit mehreren Künstlern bleiben gültig."
        }
        TextKey::OtherVideosMetadataHelp => {
            "Für eigenständige Videos statt eines Films mit Extras verwenden."
        }
        TextKey::SongsMetadataHelp => {
            "Titel, Disc, Künstler und Album werden pro Song bearbeitet; die Listenreihenfolge ersetzt nicht die Titelnummer."
        }
        TextKey::ImagesMetadataHelp => {
            "Beschriftung und Urheber sind bearbeitbar; Format, Abmessungen und Quellidentität bleiben Quelldaten."
        }
        TextKey::RemoteStreamsMetadataHelp => {
            "Präsentationsmetadaten hier bearbeiten; Verbindung und Protokoll bleiben Fakten der Anbieterquelle."
        }
        TextKey::CastAndCredits => "Besetzung und Mitwirkende",
        TextKey::OneCreditPerLine => "Eine Person pro Zeile: Name | Rolle",
        TextKey::Poster => "Poster",
        TextKey::Artwork => "Bildmaterial",
        TextKey::PublishMetadata => "Veröffentlichen",
        TextKey::IncludeItem => "Aufnehmen",
        TextKey::IncludeInPublication => "In Veröffentlichung aufnehmen",
        TextKey::MetadataDraftNotice => "Gespeicherte Metadaten und Auswahländerungen werden automatisch übernommen.",
TextKey::ApplyToAllItemsPrompt => {
            "Jeder Eintrag erhält die ausgewählte Medienart und überschreibt damit Werte, die das Add-on ermittelt oder Sie selbst gesetzt haben."
        }
        TextKey::TelevisionEpisode => "Fernsehepisode",
        TextKey::CollectionTypeAuto => "Anhand der Einträge bestimmen",
        TextKey::CurrentLibraryType => "Aktueller Bibliothekstyp",
        TextKey::CollectionTypeChangeHelp => "Speichern Sie, um vorhandene Einträge automatisch neu zu ordnen. Reihenfolge, Auswahl und Metadatenänderungen bleiben erhalten.",
TextKey::NonPlayableItems => "Nicht abspielbare Einträge",
        TextKey::UnknownItems => "Unbekannte Einträge",
        TextKey::PlayableItems => "Abspielbare Einträge",
        TextKey::ApplyToAllItems => "Auf alle Einträge anwenden",
        TextKey::ChangeAddon => "Add-on ändern",
        TextKey::ChangeAddonHelp => {
            "Wählen Sie ein anderes Add-on, um die Liste darüber erneut zu importieren."
        }
        TextKey::ChangeAddonPrompt => {
            "Die Liste wird über das ausgewählte Add-on erneut importiert. Elementmetadaten, Ersetzungen und Kapitelkorrekturen werden erneut vom neuen Anbieter abgeleitet."
        }
        TextKey::AddonAutoDetect => "Anhand der URL bestimmen",
        TextKey::AddonAutoDetected => "Automatisch anhand der URL ausgewählt.",
        TextKey::AddonMediaListNoAddonForUrl => {
            "Kein installiertes Add-on verarbeitet diese URL. Wählen Sie eines aus oder legen Sie einen Standard-Verarbeiter für Remote-Streams fest."
        }
        TextKey::DefaultRemoteStreamAddon => "Standard-Verarbeiter für Remote-Streams",
        TextKey::DefaultRemoteStreamAddonHelp => {
            "Wird für HTTP- und HTTPS-Quellen verwendet, die kein installiertes Add-on beansprucht. Das Add-on muss den Host weiterhin kontaktieren dürfen."
        }
        TextKey::AddonDetailWait => "Taktung der Elementdetails",
        TextKey::MinimumDetailWait => "Minimale Wartezeit für Details",
        TextKey::MaximumDetailWait => "Maximale Wartezeit für Details",
        TextKey::AddonDetailWaitHelp => {
            "Nach jedem Versuch, Elementdetails zu laden, wartet ErsatzRS eine neu zufällig gewählte Anzahl von Sekunden aus diesem globalen Bereich, bevor eine Liste denselben Anbieterhost erneut verwenden darf."
        }
        TextKey::AddonDetailWaitInvalid => {
            "Geben Sie positive ganze Sekunden ein und stellen Sie sicher, dass das Minimum das Maximum nicht überschreitet."
        }
        TextKey::Addons => "Add-ons",
        TextKey::AddonOverview => "Übersicht",
        TextKey::AddonUpdates => "Updates",
        TextKey::AddonSources => "Quellen",
        TextKey::AddonNotFound => "Add-on nicht gefunden",
        TextKey::BackToAddons => "Zurück zu Add-ons",
        TextKey::AvailableVersion => "Verfügbare Version",
        TextKey::BrowseAddons => "Add-ons durchsuchen",
        TextKey::CheckForUpdates => "Nach Updates suchen",
        TextKey::InstallFromFile => "Aus Datei installieren",
        TextKey::InstallRepositoryFromFile => "Repository aus Datei installieren",
        TextKey::License => "Lizenz",
        TextKey::UnknownSources => "Unbekannte Quellen",
        TextKey::UnknownSourcesHelp => {
            "Erlaubt Add-ons aus lokalen Git-Checkouts und ZIP-Dateien. Add-ons laufen mit den Rechten des ErsatzRS-Dienstbenutzers."
        }
        TextKey::Verified => "Verifiziert",
        TextKey::Unverified => "Nicht verifiziert",
        TextKey::Sideloaded => "Lokal installiert",
        TextKey::GitCheckout => "Git-Checkout",
        TextKey::OfflineBundle => "Offline-Paket",
        TextKey::LegacyUnmanaged => "Alter Katalog · nicht verwaltet",
        TextKey::LegacyAdoptionHelp => {
            "Dies ersetzt die nicht verwaltete Kataloginstallation durch die ausgewählte lokale Quelle; Einstellungen und Daten bleiben erhalten."
        }
        TextKey::Enabled => "Aktiviert",
        TextKey::Disabled => "Deaktiviert",
        TextKey::Source => "Quelle",
        TextKey::ViewSourceCode => "Quellcode ansehen",
        TextKey::Version => "Version",
        TextKey::NextVersion => "Version der Next-Engine",
        TextKey::NoAddonsMatch => "Keine Add-ons entsprechen diesen Filtern.",
        TextKey::AddonsUpToDate => "Alles ist auf dem neuesten Stand.",
        TextKey::AddonRepositories => "Add-on-Repositorys",
        TextKey::AvailableAddons => "Verfügbare Add-ons",
        TextKey::InstalledAddons => "Installierte Add-ons",
        TextKey::AddRepository => "Repository hinzufügen",
        TextKey::LastRefresh => "Zuletzt geprüft",
        TextKey::NeverRefreshed => "Nie",
        TextKey::ChooseFile => "Datei auswählen…",
        TextKey::NoFileSelected => "Keine Datei ausgewählt",
        TextKey::EditRepository => "Repository bearbeiten",
        TextKey::OfficialRepositoryProtected => {
            "Das offizielle Repository kann nicht entfernt werden."
        }
        TextKey::UnknownSourcesRequired => "Aktivieren Sie zuerst unbekannte Quellen.",
        TextKey::NoRepositories => {
            "Keine lokalen Repositorys gefunden. Klonen Sie ein Repository in den obigen Checkout-Ordner und aktualisieren Sie dann."
        }
        TextKey::AddonPackage => "Add-on-Paket",
        TextKey::RepositoryPackage => "Repository-Paket",
        TextKey::NoSigningKeyPinned => "Es ist kein öffentlicher Signaturschlüssel hinterlegt.",
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
            "Laden Sie nur vertrauenswürdige lokale Repositorys und ZIP-Dateien; Add-ons werden unter dem ErsatzRS-Dienstkonto ausgeführt."
        }
        TextKey::AddCustomResolution => "Benutzerdefinierte Auflösung hinzufügen",
        TextKey::AddBlock => "Block hinzufügen",
        TextKey::AddBlockGroup => "Block-Gruppe hinzufügen",
        TextKey::AddBlockItem => "Block-Element hinzufügen",
        TextKey::AddBlockToTemplate => "Block zur Vorlage hinzufügen",
        TextKey::AddCollection => "Sammlung hinzufügen",
        TextKey::AddContent => "Inhalt hinzufügen",
        TextKey::AddFailed => "Hinzufügen fehlgeschlagen",
        TextKey::AddFillerPreset => "Filler-Vorlage hinzufügen",
        TextKey::AddMultiCollection => "Mehrsammlung hinzufügen",
        TextKey::AddPlaylist => "Wiedergabeliste hinzufügen",
        TextKey::AddPlaylistGroup => "Wiedergabelisten-Gruppe hinzufügen",
        TextKey::AddPlaylistItem => "Wiedergabelistenelement hinzufügen",
        TextKey::AddRerunCollection => "Wiederholungssammlung hinzufügen",
        TextKey::AddTemplate => "Vorlage hinzufügen",
        TextKey::AddTemplateGroup => "Vorlagen-Gruppe hinzufügen",
        TextKey::AddAddonMediaList => "add-on-Liste hinzufügen",
        TextKey::AddToCollection => "Zur Sammlung hinzufügen",
        TextKey::AddToEllipsis => "Zu ... hinzufügen",
        TextKey::AddToPlaylist => "Zur Wiedergabeliste hinzufügen",
        TextKey::AddToSchedule => "Zum Sendeplan hinzufügen",
        TextKey::Actions => "Aktionen",
        TextKey::All => "Alle",
        TextKey::AreYouSureDelete => "Möchten Sie wirklich löschen?",
        TextKey::Actors => "Schauspieler",
        TextKey::Artist => "Künstler",
        TextKey::ArtistNotFound => "Künstler nicht gefunden",
        TextKey::AfterDurationEnd => "Nach Ende der Dauer",
        TextKey::AutomaticRefresh => "Automatischer Aktualisierungszyklus",
        TextKey::AddonDeliveryMode => "Bereitstellung",
        TextKey::AddonDeliveryStream => "Streamen",
        TextKey::AddonDeliveryDownloadAhead => "Vorab herunterladen",
        TextKey::AddonDeliveryNewVideos => "Vorab herunterladen — nur neue Videos",
        TextKey::AddonDeliveryHelp => "Änderungen gelten nach der nächsten Veröffentlichung. Nur neue Videos lädt Videos herunter, die erstmals nach dem gespeicherten Stichtag importiert wurden; bereits bekannte Videos werden gestreamt. Erneutes Aktivieren legt einen neuen Stichtag fest. Nur Medien mit endlicher Dauer können vorab heruntergeladen werden. Live-Inhalte und Inhalte mit unbekannter Dauer werden weiterhin gestreamt.",
        TextKey::AddonDeliveryUnavailable => "Downloads erfordern ein kompatibles, aktiviertes Add-on und Medien mit endlicher Dauer.",
        TextKey::AddonSelectionInitialReview => "Einträge vor der ersten Veröffentlichung auswählen",
        TextKey::AddonSelectionInitialReviewHelp => "Importieren Sie zuerst nur die Übersicht. Prüfen und speichern Sie die Auswahl, bevor Medien geladen oder die Liste veröffentlicht werden.",
        TextKey::AddonSelectionReviewRequired => "Prüfen und speichern Sie die erste Auswahl. Automatische Veröffentlichung und Medienabruf warten.",
        TextKey::AddonSelectionEvaluatedAt => "Letzte Auswahlbewertung",
        TextKey::AddonSelection => "Fortlaufende Auswahl",
        TextKey::AddonSelectionHelp => "Leere Grenzen erlauben jeden Wert. Begriffe werden ohne Beachtung der Großschreibung im Titel gesucht. Angeheftete Einträge umgehen alle Filter und zählen nicht zu den neuesten N. Archivierte Mediendateien bleiben erhalten.",
        TextKey::AddonSelectionEnabled => "Auswahlfilter aktivieren",
        TextKey::AddonSelectionPause => "Automatische Abfragen pausieren",
        TextKey::AddonSelectionAge => "Maximales Alter (Tage)",
        TextKey::AddonSelectionNewest => "Neueste Einträge (ohne angeheftete)",
        TextKey::AddonSelectionMinimum => "Mindestdauer (Sekunden)",
        TextKey::AddonSelectionMaximum => "Höchstdauer (Sekunden)",
        TextKey::AddonSelectionInclude => "Titelbegriffe einschließen (einer pro Zeile)",
        TextKey::AddonSelectionExclude => "Titelbegriffe ausschließen (einer pro Zeile)",
        TextKey::AddonSelectionActive => "Aktive Auswahl",
        TextKey::AddonSelectionArchived => "Archivierte Auswahl",
        TextKey::AddonSelectionPreviewHelp => "Die Vorschau veröffentlicht oder lädt nichts herunter. Die Tabelle zeigt höchstens 100 Einträge; alle Entscheidungen zählen. Speichern wendet die geprüften Filter über den vorhandenen Hintergrunddienst an.",
        TextKey::AddonSelectionManual => "Manuelle Auswahl",
        TextKey::AddonSelectionPinned => "Angeheftet",
        TextKey::AddonSelectionExcluded => "Manuell ausgeschlossen",
        TextKey::AddonSelectionAutomatic => "Auswahlfiltern folgen",
        TextKey::AddonSelectionCutoff => "Wirksame Altersgrenze",
        TextKey::AddonSelectionMissingDate => "Veröffentlichungsdatum fehlt",
        TextKey::AddonSelectionFutureDate => "Veröffentlichungsdatum liegt in der Zukunft",
        TextKey::AddonSelectionTooOld => "Außerhalb des Altersfensters",
        TextKey::AddonSelectionMissingDuration => "Dauer fehlt",
        TextKey::AddonSelectionTooShort => "Unter der Mindestdauer",
        TextKey::AddonSelectionTooLong => "Über der Höchstdauer",
        TextKey::AddonSelectionTitleNotIncluded => "Kein Einschlussbegriff gefunden",
        TextKey::AddonSelectionTitleExcluded => "Ein Ausschlussbegriff gefunden",
        TextKey::AddonSelectionNewestLimit => "Außerhalb der neuesten N",
        TextKey::AddonSelectionInvalid => "Prüfen Sie die Auswahlgrenzen und Begriffe oder laden Sie nach einer gleichzeitigen Änderung neu.",
        TextKey::AddonDeliveryCutoff => "Stichtag für neue Videos",
        TextKey::AddonDeliveryFreshCutoff => "Beim Speichern wird ein neuer Stichtag festgelegt.",
        TextKey::AdaptiveRefreshUnavailable => {
            "Die adaptive Aktualisierung ist nicht verfügbar, da dieses Add-on keine Quellprüfungen mehr unterstützt. Wählen Sie einen anderen Zyklus, um automatische Aktualisierungen fortzusetzen."
        }
        TextKey::RefreshPublicationPolicy => "Veröffentlichung nach Aktualisierung",
        TextKey::RefreshPublicationReview => "Jede Aktualisierung prüfen",
        TextKey::RefreshPublicationAutomatic => "Gespeicherte Änderungen automatisch anwenden",
        TextKey::RefreshPublicationPolicyHelp => "Importe, Aktualisierungen, Metadaten und gespeicherte Änderungen werden automatisch angewendet.",
        TextKey::AutoPublicationHeld => "Automatische Verarbeitung erfordert Aufmerksamkeit",
        TextKey::PreparationWorkerUnavailable => {
            "Der Hintergrundprozess zur Medienvorbereitung ist nicht verfügbar. Starten Sie den Server neu, bevor Sie die Vorbereitung erneut versuchen."
        }
        TextKey::PreparationProcessRecoveryRequired => {
            "Die Medienvorbereitung erfordert eine Prozesswiederherstellung. Stellen Sie sicher, dass frühere Anbieterprozesse beendet sind, bevor Sie reservierten Speicher wiederherstellen und die Vorbereitung erneut versuchen."
        }
        TextKey::AutoPublicationBaselineUnavailable => "Eine veröffentlichte Grundlage fehlt.",
        TextKey::AutoPublicationExistingChanges => "Vorhandene Einträge wurden geändert.",
        TextKey::AutoPublicationConflicts => {
            "Metadaten- oder Ersatzkonflikte müssen geprüft werden."
        }
        TextKey::AutoPublicationRemovals => "Einträge wurden entfernt.",
        TextKey::AutoPublicationRestricted => "Einige Ergänzungen haben Zugriffsbeschränkungen.",
        TextKey::AutoPublicationUnavailable => "Einige Ergänzungen sind nicht verfügbar.",
        TextKey::AutoPublicationOptionsChanged => "Veröffentlichungsoptionen wurden geändert.",
        TextKey::AutoPublicationPreparationFailed => {
            "Die Vorbereitung der Veröffentlichung ist fehlgeschlagen."
        }
        TextKey::AutoPublicationConcurrentChange => {
            "Die Liste wurde während der Vorbereitung geändert."
        }
        TextKey::RefreshCadenceAdaptive => "Adaptiv",
        TextKey::RefreshCadenceDaily => "Täglich",
        TextKey::RefreshCadenceWeekly => "Wöchentlich",
        TextKey::RefreshCadenceMonthly => "Monatlich",
        TextKey::RefreshCadenceYearly => "Jährlich",
        TextKey::RefreshCadenceNever => "Nie",
        TextKey::LastSuccessfulRefresh => "Letzte erfolgreiche Aktualisierung",
        TextKey::NextScheduledRefresh => "Nächste geplante Aktualisierung",
        TextKey::RefreshRetryAt => "Erneuter Versuch ab",
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
        }
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
        TextKey::BlockPreviewFailed => {
            "Die Block-Vorschau konnte nicht generiert werden. Überprüfen Sie die Block-Elemente und versuchen Sie es erneut."
        }
        TextKey::Blocks => "Blöcke",
        TextKey::BlockToRemove => "Block zum Entfernen",
        TextKey::BlockWillNotFit => {
            "Der Block passt nicht zum oder nach dem ausgewählten Startzeitpunkt."
        }
        TextKey::Cancel => "Abbrechen",
        TextKey::Clear => "Löschen",
        TextKey::Available => "Verfügbar",
        TextKey::UnknownAvailability => "Unbekannt",
        TextKey::LocalReplacement => "Lokaler Ersatz",
        TextKey::NoReplacement => "Kein Ersatz",
        TextKey::InvalidReplacement => "Ungültiger Ersatz",
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
        }
        TextKey::BreakContentItem => "Pauseninhaltelement",
        TextKey::BreakContentMode => "Pauseninhalte-Modus",
        TextKey::DeadAirFallback => "Fallback bei Totzeit",
        TextKey::DeadAirFallbackBody => {
            "Wenn für die aktuelle Zeit kein Playout-Element gefunden wird, wird zufällig ein Element aus dieser Sammlung ausgewählt, geloopt und genau auf die Dauer bis zum Start des nächsten Playout-Elements zugeschnitten. Dies ersetzt das Bild „Kanal ist offline“, das sonst angezeigt würde."
        }
        TextKey::DeadAirFallbackCollectionType => "Typ der Fallback-Sammlung bei Totzeit",
        TextKey::DeadAirFallbackMode => "Fallback-Modus bei Totzeit",
        TextKey::Deco => "Deko",
        TextKey::DecoCannotBeAdded => {
            "Die Deko kann nicht hinzugefügt werden, da dies zu einer Überlappung führen oder die Mitternacht überschreiten würde."
        }
        TextKey::DecoCannotBeMoved => {
            "Die Deko kann nicht dorthin verschoben werden, da dies zu einer Überlappung führen oder die Mitternacht überschreiten würde."
        }
        TextKey::DecoCreated => "Deko erstellt.",
        TextKey::DecoDeleted => "Deko gelöscht.",
        TextKey::DecoGroup => "Deko-Gruppe",
        TextKey::DecoGroupCreated => "Deko-Gruppe erstellt.",
        TextKey::DecoGroupDeleted => "Deko-Gruppe gelöscht.",
        TextKey::DecoGroupName => "Deko-Gruppenname",
        TextKey::DecoGroups => "Deko-Gruppen",
        TextKey::DecoName => "Deko-Name",
        TextKey::DecoNotFound => "Deko nicht gefunden.",
        TextKey::DecoSaved => "Deko gespeichert.",
        TextKey::Decos => "Dekos",
        TextKey::DecoTemplate => "Deko-Vorlage",
        TextKey::DecoTemplateCreated => "Deko-Vorlage erstellt.",
        TextKey::DecoTemplateDeleted => "Deko-Vorlage gelöscht.",
        TextKey::DecoTemplateGroup => "Deko-Vorlagengruppe",
        TextKey::DecoTemplateGroupCreated => "Deko-Vorlagengruppe erstellt.",
        TextKey::DecoTemplateGroupDeleted => "Deko-Vorlagengruppe gelöscht.",
        TextKey::DecoTemplateGroupName => "Name der Deko-Vorlagengruppe",
        TextKey::DecoTemplateGroups => "Deko-Vorlagengruppen",
        TextKey::DecoTemplateName => "Name der Deko-Vorlage",
        TextKey::DecoTemplateNotFound => "Deko-Vorlage nicht gefunden.",
        TextKey::DecoTemplateSaved => "Deko-Vorlage gespeichert.",
        TextKey::AddDecoToTemplate => "Deko zur Vorlage hinzufügen",
        TextKey::DecoToRemove => "Deko zum Entfernen",
        TextKey::DefaultFiller => "Standard-Filler",
        TextKey::DefaultFillerBody => {
            "Nach dem Planen aller Blöcke wird ein zweiter Durchlauf durchgeführt, um unplanbare Zeiten mit zufälligen Elementen aus dieser Sammlung zu füllen."
        }
        TextKey::DefaultFillerCollectionType => "Sammlungstyp des Standard-Fillers",
        TextKey::DefaultFillerMode => "Modus des Standard-Fillers",
        TextKey::DeleteDeco => "Deko löschen",
        TextKey::DeleteDecoGroup => "Deko-Gruppe löschen",
        TextKey::DeleteDecoTemplate => "Deko-Vorlage löschen",
        TextKey::DeleteDecoTemplateGroup => "Deko-Vorlagengruppe löschen",
        TextKey::Disable => "Deaktivieren",
        TextKey::EditDeco => "Deko bearbeiten",
        TextKey::EditDecoTemplate => "Deko-Vorlage bearbeiten",
        TextKey::GraphicsElementsMode => "Modus für grafische Elemente",
        TextKey::Inherit => "Vererben",
        TextKey::Merge => "Zusammenführen",
        TextKey::NoDecosFound => "Keine Dekos gefunden.",
        TextKey::NoDecoTemplatesFound => "Keine Deko-Vorlagen gefunden.",
        TextKey::Override => "Überschreiben",
        TextKey::RemoveBreakContent => "Inhalt der Pause entfernen",
        TextKey::RemoveDecoFromTemplate => "Deko aus Vorlage entfernen",
        TextKey::Replace => "Ersetzen",
        TextKey::SavedDecos => "Gespeicherte Dekos",
        TextKey::SavedDecoTemplates => "Gespeicherte Deko-Vorlagen",
        TextKey::SearchForDecos => "Nach Dekos suchen",
        TextKey::SearchForDecoTemplates => "Nach Deko-Vorlagen suchen",
        TextKey::TrimToFit => "Zuschneiden zum Anpassen",
        TextKey::UseGraphicsElementsDuringFiller => "Grafikelemente während des Fillers verwenden",
        TextKey::UseWatermarkDuringFiller => "Wasserzeichen während des Fillers verwenden",
        TextKey::WatermarkMode => "Modus des Wasserzeichens",
        TextKey::DecoTemplates => "Deko-Vorlagen",
        TextKey::DefaultFfmpegProfile => "Standardprofil FFmpeg",
        TextKey::DefaultFfmpegProfileHelp => {
            "Das Profil FFmpeg, das beim Erstellen neuer Kanäle verwendet wird"
        }
        TextKey::DefaultMinimumLogLevel => "Standard-Mindest-Protokollebene",
        TextKey::DefaultMpegTsScript => "Standard-Skript MPEG-TS",
        TextKey::DefaultMpegTsScriptHelp => "Wählen Sie die integrierte Ausgabe für selbst erstellten Teletext. Skripte steuern ihre eigene Ausgabe.",
        TextKey::BuiltInMpegTsWrapper => "Integrierte MPEG-TS-Ausgabe",
        TextKey::Decorations => "Dekorationen",
        TextKey::Debug => "Debug",
        TextKey::Delete => "Löschen",
        TextKey::DeleteAll => "Alles löschen",
        TextKey::DeleteBlock => "Block löschen",
        TextKey::DeleteBlockGroup => "Blockgruppe löschen",
        TextKey::DeleteCustomResolution => "Benutzerdefinierte Auflösung löschen",
        TextKey::DeleteCollection => "Sammlung löschen",
        TextKey::DeleteFillerPreset => "Filler-Vorlage löschen",
        TextKey::DeleteMultiCollection => "Mehrsammlung löschen",
        TextKey::DeletePlaylist => "Wiedergabeliste löschen",
        TextKey::DeletePlaylistGroup => "Wiedergabelistengruppe löschen",
        TextKey::DeleteRerunCollection => "Wiedergabesammlung löschen",
        TextKey::DeleteTemplate => "Vorlage löschen",
        TextKey::DeleteTemplateGroup => "Vorlagengruppe löschen",
        TextKey::DeleteAddonMediaList => "add-on-Liste löschen",
        TextKey::DisabledCollectionsAlreadyPresent => {
            "Deaktivierte Sammlungen sind bereits in dieser Mehrsammlung vorhanden"
        }
        TextKey::EmptyTrash => "Papierkorb leeren",
        TextKey::ExtractEmbeddedSubtitles => "Eingebettete (Text-)Untertitel extrahieren",
        TextKey::ErasedPlayoutItemsAndHistory => "Playout-Elemente und Verlauf gelöscht.",
        TextKey::EraseFailed => "Löschen fehlgeschlagen",
        TextKey::EraseItemsAndHistory => "Elemente und Verlauf löschen",
        TextKey::ErasePlayoutItemsAndHistoryConfirm => {
            "Dies löscht alle Playout-Elemente und den Verlauf für dieses sequenzielle Playout. Fortfahren?"
        }
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
        TextKey::FillerPreset => "Filler-Vorlage",
        TextKey::FillerPresetActionFailed => "Die Aktion der Filler-Vorlage ist fehlgeschlagen.",
        TextKey::FillerPresetCouldNotBeSaved => {
            "Die Filler-Vorlage konnte nicht gespeichert werden. Überprüfen Sie die Formularwerte und versuchen Sie es erneut."
        }
        TextKey::FillerPresetDeleted => "Filler-Vorlage gelöscht.",
        TextKey::FillerPresetExpressionHelp => {
            "Für Mid-Roll-Filler wird nur dann ein Filler hinzugefügt, wenn dieser Ausdruck für einen gegebenen Mid-Roll-Punkt den Wert true ergibt."
        }
        TextKey::FillerPresetSaved => "Filler-Vorlage gespeichert.",
        TextKey::FillerPresets => "Filler-Vorlagen",
        TextKey::Flags => "Flaggen",
        TextKey::GlobalFallbackFiller => "Globaler Fallback-Filler",
        TextKey::GlobalWatermark => "Globales Wasserzeichen",
        TextKey::GoHome => "Nach Hause gehen",
        TextKey::HdHomeRun => "HDHomeRun",
        TextKey::HealthChecks => "Gesundheitsüberprüfungen",
        TextKey::Height => "Höhe",
        TextKey::HlsDirectOutputFormat => "Direktes Ausgabeformat HLS",
        TextKey::HlsSegmenterIdleTimeout => "Segmenter-Leerlauf-Timeout HLS",
        TextKey::HlsSegmenterIdleTimeoutHelp => {
            "Die Anzahl der Sekunden, während der die Transkodierung eines Kanals fortgesetzt wird, während keine Anfragen von einem Client empfangen wurden"
        }
        TextKey::HlsSegmenterInitialSegmentCount => {
            "Anzahl der initialen Segmente des Segmenters HLS"
        }
        TextKey::HlsSegmenterInitialSegmentCountHelp => {
            "Verschiebt den Start des Streams, bis die angegebene Anzahl von (4-Sekunden-)Segmenten transkodiert wurden; eine größere Anzahl bedeutet eine langsamere initiale Wiedergabe, kann aber potenziell weniger Pufferung bedeuten"
        }
        TextKey::HomeRunning => "Der IPTV-Server läuft.",
        TextKey::ReleaseNotes => "Versionshinweise",
        TextKey::FullChangelog => "Vollständiges Änderungsprotokoll",
        TextKey::HttpMinimumLogLevel => {
            "Mindest-Protokollebene für die Anforderungsprotokollierung"
        }
        TextKey::Id => "ID",
        TextKey::Images => "Bilder",
        TextKey::Item => "Element",
        TextKey::ItemActions => "Elementaktionen",
        TextKey::ItemType => "Elementtyp",
        TextKey::Items => "Elemente",
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
        TextKey::Local => "Lokal",
        TextKey::Logging => "Protokollierung",
        TextKey::M3u => "M3U",
        TextKey::Media => "Medien",
        TextKey::MediaInfo => "Medieninfo",
        TextKey::MediaSources => "Medienquellen",
        TextKey::Movie => "Film",
        TextKey::MovieNotFound => "Film nicht gefunden",
        TextKey::Movies => "Filme",
        TextKey::MusicVideo => "Musikvideo",
        TextKey::MultiCollections => "Mehrsammlungen",
        TextKey::ManualCollections => "Manuelle Sammlungen",
        TextKey::MatchFailed => "Abgleich fehlgeschlagen",
        TextKey::MatchStatus => "Abgleichstatus",
        TextKey::MatchAddonMediaListItems => "add-on-Listenelemente abgleichen",
        TextKey::Members => "Mitglieder",
        TextKey::MultiCollection => "Mehrsammlung",
        TextKey::MultiCollectionDeleted => "Mehrsammlung gelöscht.",
        TextKey::MultiCollectionItems => "Mehrsammlungselemente",
        TextKey::MultiCollectionNameRequired => "Mehrsammlungsnamen sind erforderlich!",
        TextKey::MultiCollectionNameTooLong => {
            "Mehrsammlungsnamen dürfen nicht länger als 50 Zeichen sein."
        }
        TextKey::MultiCollectionSaved => "Mehrsammlung gespeichert.",
        TextKey::Music => "Musik",
        TextKey::Name => "Name",
        TextKey::NameRequired => "Name ist erforderlich",
        TextKey::Networks => "Netzwerke",
        TextKey::NewCollection => "(Neue Sammlung)",
        TextKey::NewCollectionTitle => "Neue Sammlung",
        TextKey::NewFillerPreset => "Neue Filler-Vorlage",
        TextKey::NewMultiCollection => "Neue Mehrsammlung",
        TextKey::NewTemplateGroup => "Neue Vorlagengruppe",
        TextKey::NewTemplateName => "Neuer Vorlagenname",
        TextKey::Next24Hours => "Nächste 24 h",
        TextKey::NextPage => "Nächste Seite",
        TextKey::NoFillerPresets => "Noch keine Filler-Vorlagen vorhanden.",
        TextKey::NoBlockItems => "Keine Blockelemente.",
        TextKey::NoItems => "Keine Elemente.",
        TextKey::NoItemsToDisplay => "Keine Elemente zur Anzeige.",
        TextKey::NoAddonMediaListsYet => "Noch keine add-on-Listen vorhanden.",
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
        TextKey::Playout => "Playout",
        TextKey::PlayoutItemsAndHistory => "Playout-Elemente und Verlauf",
        TextKey::PlayoutNotFound => "Playout nicht gefunden.",
        TextKey::Playouts => "Playouts",
        TextKey::AddPlayout => "Playout hinzufügen",
        TextKey::ClassicScheduleMenu => "Klassischer Sendeplan",
        TextKey::BlockScheduleMenu => "Block-Sendeplan",
        TextKey::SequentialScheduleMenu => "Sequenzieller Sendeplan",
        TextKey::ScriptedScheduleMenu => "Skriptierter Sendeplan",
        TextKey::JsonDizqueTvScheduleMenu => "JSON (dizqueTV) Sendeplan",
        TextKey::ResetAllPlayouts => "Alle Playouts zurücksetzen",
        TextKey::PlayoutBuildQueued => "Playout-Wiederherstellung in der Warteschlange",
        TextKey::PlayoutBuildsQueued => "Playout-Wiederherstellungen in der Warteschlange",
        TextKey::SearchForPlayouts => "Playouts suchen",
        TextKey::PlayoutDetail => "Playout-Details",
        TextKey::ShowFiller => "Filler anzeigen",
        TextKey::EditPlayout => "Playout bearbeiten",
        TextKey::RefreshPlayout => "Playout aktualisieren",
        TextKey::ResetPlayout => "Playout zurücksetzen",
        TextKey::RebuildPlayout => "Playout aktualisieren",
        TextKey::PlayoutRebuildMode => "Aktion",
        TextKey::ContinuePlayout => "Playout verlängern",
        TextKey::PlayoutContinueHelp => "Programme nach dem bestehenden Zeitplan anhängen, ohne ihn zu ersetzen.",
        TextKey::PlayoutRefreshHelp => "Den zukünftigen Zeitplan neu erstellen und die gespeicherte Programmreihenfolge fortsetzen.",
        TextKey::PlayoutResetHelp => "Mit einer neuen Programmreihenfolge beginnen.",
        TextKey::KeepCurrentProgramme => "Laufendes Programm beibehalten",
        TextKey::KeepCurrentProgrammeHelp => "Das vollständige Programm und bereits von Playern vorbereitete Programme beibehalten. Änderungen gelten danach.",
        TextKey::ReplaceCurrentProgrammeHelp => "Das laufende Programm kann unterbrochen werden.",
        TextKey::QueuePlayoutBuild => "Aktualisierung einreihen",
        TextKey::InvalidPlayoutBuildMode => "Wählen Sie eine gültige Playout-Aktion.",
        TextKey::PlayoutContinueStateIncomplete => "Der gespeicherte Fortsetzungsstand ist unvollständig. Der bestehende Zeitplan bleibt erhalten. Wählen Sie Aktualisieren oder Zurücksetzen mit Beibehaltung des Programms.",
        TextKey::PlayoutClassicTimelineRejected => "Der Zeitplan enthält überlappende oder ungültige Programmzeiten. Veröffentlichte Programme bleiben unverändert. Prüfen Sie den Zeitplan und erstellen Sie ihn ab einer überprüften Programmgrenze neu.",
        TextKey::ScheduleReset => "Sendeplan zurücksetzen",
        TextKey::SchedulePlayoutReset => "Playout-Zurücksetzung planen",
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
        TextKey::ProgramSchedule => "Programm-Sendeplan",
        TextKey::SequentialScheduleFileHelp => {
            "Der vollständige Pfad zur YAML-Datei des sequenziellen Sendeplans"
        }
        TextKey::BlockTemplatesAddedLater => "Blockvorlagen werden später hinzugefügt",
        TextKey::UnsupportedPlayoutKind => "Nicht unterstützter Playout-Typ.",
        TextKey::ClassicPlayout => "Klassischer Playout",
        TextKey::ClassicPlayoutState => "Klassischer Playout-Zustand",
        TextKey::ScanRefreshState => "Status der Scan-Aktualisierung",
        TextKey::SubtitleCleanup => "Untertitel bereinigen",
        TextKey::SubtitleCleanupMessage => "Bereinigung ausstehend: {count} Untertiteldatensätze ohne Zuordnung oder externe Untertitel ohne Pfad. Die automatische Bereinigung erfolgt stündlich. Zwischengespeicherte Untertiteldateien bleiben erhalten.",
        TextKey::SubtitleCleanupBrief => "{count} Untertiteldatensätze warten auf Bereinigung",
        TextKey::SubtitleCacheRepairPendingMessage => "Die Reparatur des Untertitelcaches ist noch nicht abgeschlossen. Einige zwischengespeicherte Untertitel werden möglicherweise nicht angezeigt. Die automatische Cachewartung versucht es erneut. Wenn diese Warnung bestehen bleibt, prüfen Sie den Schreibzugriff auf den Untertitelcache und den verfügbaren Speicherplatz.",
        TextKey::SubtitleCacheRepairPendingBrief => "Reparatur des Untertitelcaches ausstehend",
        TextKey::ArtworkOrphans => "Verwaiste Bilder",
        TextKey::ArtworkOrphansMessage => "Bereinigung ausstehend: {actors} Schauspielerdatensätze und {artwork} Bilddatensätze ohne Zuordnung. Die automatische Bereinigung erfolgt stündlich. Verwenden Sie POST /api/maintenance/clean_artwork, um sie jetzt anzufordern. Zugeordnete Datensätze und referenzierte Bilder im Cache bleiben erhalten.",
        TextKey::ArtworkOrphansBrief => "{actors} verwaiste Schauspielerdatensätze und {artwork} verwaiste Bilddatensätze",
        TextKey::ArtworkCardinality => "Kardinalität des Bildmaterials",
        TextKey::Av1OutputCompatibility => "AV1-Ausgabe erfordert natives HLS. Ändern Sie Engine/Modus des Kanals oder wählen Sie H.264/HEVC im FFmpeg-Profil.",
        TextKey::HlsLiveDelivery => "HLS-Livebereitstellung",
        TextKey::BlockPlayout => "Block-Playout",
        TextKey::ScriptedPlayout => "Skriptierter Sendeplan",
        TextKey::AlternateSchedules => "Alternative Sendepläne",
        TextKey::EditAlternateSchedules => "Alternative Sendepläne bearbeiten",
        TextKey::PlayoutTemplates => "Playout-Vorlagen",
        TextKey::EditTemplates => "Vorlagen bearbeiten",
        TextKey::DefaultDeco => "Standard-Deko",
        TextKey::EnableDefaultDeco => "Standard-Deko aktivieren",
        TextKey::SaveDefaultDeco => "Standard-Deko speichern",
        TextKey::SaveScriptedSchedule => "Skriptierten Sendeplan speichern",
        TextKey::EraseItems => "Elemente löschen",
        TextKey::ErasedPlayoutItems => "Playout-Elemente gelöscht.",
        TextKey::SavedDefaultDeco => "Standard-Deko gespeichert.",
        TextKey::SavedScriptedSchedule => "Skriptierter Sendeplan gespeichert.",
        TextKey::Maintenance => "Wartung",
        TextKey::SaveAlternateSchedules => "Alternative Sendepläne speichern",
        TextKey::AddAlternateSchedule => "Alternativen Sendeplan hinzufügen",
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
        TextKey::RerunCollections => "Wiederholungssammlungen",
        TextKey::RerunCollection => "Wiederholungssammlung",
        TextKey::RerunCollectionActionFailed => {
            "Die Aktion für die Wiederholungssammlung ist fehlgeschlagen."
        }
        TextKey::RerunCollectionDeleted => "Wiederholungssammlung gelöscht.",
        TextKey::RerunCollectionNameRequired => {
            "Der Name der Wiederholungssammlung ist erforderlich!"
        }
        TextKey::RerunCollectionSaved => "Wiederholungssammlung gespeichert.",
        TextKey::RerunPlaybackOrder => "Reihenfolge der Wiedergabe bei Wiederholungen",
        TextKey::RemoteStreams => "Ferngesteuerte Streams",
        TextKey::RemoteStream => "Ferngesteuerter Stream",
        TextKey::PublishedOptions => "Veröffentlichte Optionen",
        TextKey::PlaybackHealth => "Wiedergabestatus",
        TextKey::Configure => "Konfigurieren",
        TextKey::Watch => "Ansehen",
        TextKey::RemoteStreamPlaylists => "Wiedergabelisten für Ferngesteuerte Streams",
        TextKey::PlaylistUrl => "Wiedergabeliste URL",
        TextKey::Extractor => "Exadd-onor",
        TextKey::DestinationFolder => "Zielordner",
        TextKey::SyncInterval => "Synchronisationsintervall",
        TextKey::MaxItems => "Maximale Anzahl der Elemente",
        TextKey::RemovedEntries => "Entfernte Einträge",
        TextKey::CreateOrderedPlaylist => "Eine geordnete Wiedergabeliste erstellen",
        TextKey::SyncNow => "Jetzt synchronisieren",
        TextKey::LastSync => "Letzte Synchronisation",
        TextKey::NeverSynced => "Nie synchronisiert",
        TextKey::ManagedManifestReadOnly => {
            "Diese manuell erstellte Manifestdatei ist hier schreibgeschützt."
        }
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
        TextKey::SaveMultiCollection => "Mehrsammlung speichern",
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
        TextKey::Schedules => "Sendepläne",
        TextKey::AddSchedule => "Sendeplan hinzufügen",
        TextKey::AddScheduleItem => "Sendeplanelement hinzufügen",
        TextKey::AddSmartCollection => "Intelligente Sammlung hinzufügen",
        TextKey::Album => "Album",
        TextKey::Chronological => "Chronologisch",
        TextKey::CollectionSize => "Sammlungsgröße",
        TextKey::CollectionType => "Sammlungstyp",
        TextKey::Count => "Anzahl",
        TextKey::CopySchedule => "Sendeplan kopieren",
        TextKey::CopyScheduleItem => "Sendeplanelement kopieren",
        TextKey::CustomTitle => "Benutzerdefinierter Titel",
        TextKey::DeleteSchedule => "Sendeplan löschen",
        TextKey::DeleteScheduleItem => "Sendeplanelement löschen",
        TextKey::DeleteSmartCollection => "Intelligente Sammlung löschen",
        TextKey::DetailRowsPerPage => "Detailzeilen pro Seite",
        TextKey::Director => "Regisseur",
        TextKey::Directors => "Regisseure",
        TextKey::DiscardToFillAttempts => "Versuche zum Verwerfen zum Auffüllen",
        TextKey::Duration => "Dauer",
        TextKey::Dynamic => "Dynamisch",
        TextKey::EditFillerPreset => "Filler-Vorlage bearbeiten",
        TextKey::EditProperties => "Eigenschaften bearbeiten",
        TextKey::EditScheduleItems => "Sendeplanelemente bearbeiten",
        TextKey::EditSmartCollection => "Intelligente Sammlung bearbeiten",
        TextKey::EnterNameForNewSchedule => "Geben Sie einen Namen für den neuen Sendeplan ein",
        TextKey::Fixed => "Fest",
        TextKey::FixedStartTimeBehavior => "Verhalten bei festgelegter Startzeit",
        TextKey::Flood => "Überflutung",
        TextKey::Flexible => "Fixiert",
        TextKey::FlexibleStartSoonAsPossible => "Flexibel - So bald wie möglich starten",
        TextKey::FlexibleStartSoonAsPossibleAfterStartTime => {
            "Flexibel - So bald wie möglich nach Startzeit starten"
        }
        TextKey::FillWithGroupMode => "Füllmodus für Gruppen (Sendung oder Künstler)",
        TextKey::GraphicsElements => "Grafikelemente",
        TextKey::AddGraphicsElement => "Grafikelement hinzufügen",
        TextKey::DrawtextRendererOption => "Drawtext — dynamisch/dateibasiert",
        TextKey::GraphicsMetadataRefreshSeconds => "Programmmetadaten aktualisieren (Sekunden)",
        TextKey::GraphicsMetadataRefreshHelp => "0 behält die Metadaten vom Start. 5–60 lädt Programmdetails und Elementvariablen neu. Drawtext-Uhr und FFmpeg-Dateinachladen sind getrennt; drawvg verwendet die Momentaufnahme beim Start.",
        TextKey::GraphicsMetadataRefreshBounds => "Verwenden Sie 0 für eine Momentaufnahme beim Start oder 5–60 Sekunden für aktuelle Metadaten.",
        TextKey::ProgrammeTemplateHelp => "CurrentProgramme, NextProgramme, ContinuingProgramme und LaterToday bezeichnen vollständige Programme. Fehlenden Text mit {{ NextProgramme.Title | default(\"\", true) }} ausblenden. Das bisherige Epg bleibt verfügbar.",
        TextKey::GraphicsItemVariables => "Variablen für dieses Element (JSON)",
        TextKey::GraphicsItemVariablesHelp => "Überschreibungen gelten für die ausgewählten Grafiken dieses geplanten Elements. Ein leeres Objekt verwendet native Programmmetadaten. Verzögertes Speichern gilt für neu erzeugte Elemente.",
        TextKey::GraphicsPreviewAmbiguous => "Mehrere geplante Einträge überlappen zu diesem Zeitpunkt. Ein Programm kann nicht sicher gewählt werden.",
        TextKey::GraphicsContextPreview => "Vorschau des Programmtexts",
        TextKey::GraphicsContextPreviewHelp => "Den Entwurf mit dem gewählten Kanal, dem tatsächlich geplanten Eintrag und dessen Variablen auflösen. Diese Textvorschau zeigt keine Schriftmaße oder Platzierung.",
        TextKey::GraphicsPreviewTimeHelp => "Einen ISO-Zeitstempel mit Zeitzonenversatz eingeben, zum Beispiel 2026-10-05T21:30:00+02:00. Historische Zeitstempel verwenden den gespeicherten Sendeplan.",
        TextKey::GraphicsPreviewSavedHelp => "Ohne JavaScript verwendet die Vorschau den gespeicherten Text. Den Entwurf vor der Vorschau speichern.",
        TextKey::GraphicsItemVariablesInvalid => "JSON-Objekte für ausgewählte Grafikelemente verwenden, höchstens 64 KiB pro Element.",
        TextKey::DrawtextSettings => "Drawtext-Einstellungen",
        TextKey::DrawvgRendererOption => "Drawvg — statisch/Vektor",
        TextKey::EditGraphicsElement => "Grafikelement bearbeiten",
        TextKey::FontFile => "Schriftdatei",
        TextKey::FontFileOptionalHelp => {
            "Optional. Leer lassen, um die mitgelieferte Roboto-Schrift zu verwenden."
        }
        TextKey::GraphicsElement => "Grafikelement",
        TextKey::GraphicsElementConfigInvalid => {
            "Dieses Text-Grafikelement hat eine ungültige Konfiguration und wurde nicht geändert."
        }
        TextKey::GraphicsElementKindImage => "Bild",
        TextKey::GraphicsElementKindMotion => "Bewegung",
        TextKey::GraphicsElementKindScript => "Skript",
        TextKey::GraphicsElementKindSubtitle => "Untertitel",
        TextKey::GraphicsElementKindText => "Text",
        TextKey::GraphicsElementNameTooLong => "Der Name darf höchstens 50 Zeichen lang sein.",
        TextKey::GraphicsElementNotEditable => "Hier nicht bearbeitbar",
        TextKey::GraphicsElementNotFound => "Grafikelement nicht gefunden.",
        TextKey::GraphicsElementNotText => "Hier können nur Text-Grafikelemente bearbeitet werden.",
        TextKey::GraphicsElementSaved => "Grafikelement gespeichert.",
        TextKey::GraphicsElementSaveFailed => "Das Grafikelement konnte nicht gespeichert werden.",
        TextKey::GraphicsElementType => "Typ",
        TextKey::GraphicsExistingDrawvgHelp => {
            "Vorhandene und neue Elemente verwenden standardmäßig Drawvg."
        }
        TextKey::GraphicsStreamRestartHelp => {
            "Änderungen gelten für neu gestartete Streams und starten aktive Streams nicht neu."
        }
        TextKey::NoGraphicsElements => "Keine Grafikelemente gefunden.",
        TextKey::RefreshPeriodBounds => {
            "Der Aktualisierungszeitraum muss zwischen 250 und 60000 Millisekunden liegen."
        }
        TextKey::RefreshPeriodMilliseconds => "Aktualisierungszeitraum (Millisekunden)",
        TextKey::ReloadFrameInterval => "Frame-Intervall zum Neuladen",
        TextKey::ReloadFrameIntervalBounds => {
            "Das Frame-Intervall zum Neuladen muss mindestens 1 sein."
        }
        TextKey::Renderer => "Renderer",
        TextKey::SaveGraphicsElement => "Grafikelement speichern",
        TextKey::TextTemplate => "Textvorlage",
        TextKey::TextTemplateRequired => "Eine Textvorlage ist erforderlich.",
        TextKey::TextTemplateTooLong => {
            "Die Textvorlage muss gültiger UTF-8-Text mit höchstens 64 KiB sein."
        }
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
        }
        TextKey::MarathonGroupBy => "Marathon-Gruppierung nach",
        TextKey::MarathonShuffleGroups => "Marathon-Gruppen mischen",
        TextKey::MarathonShuffleItems => "Marathon-Elemente mischen",
        TextKey::MidRollFiller => "Mid-Roll-Filler",
        TextKey::MidRoll => "Mid-Roll",
        TextKey::MoveDown => "Nach unten verschieben",
        TextKey::MoveUp => "Nach oben verschieben",
        TextKey::ReorderPlaylistItem => "Playlisteneintrag zum Sortieren ziehen",
        TextKey::MultiEpisodeGroupSize => "Größe der Mehr-Episoden-Gruppe",
        TextKey::MultiEpisodeShuffle => "Mehr-Episoden-Mischen",
        TextKey::Multiple => "Mehrfach",
        TextKey::MultipleCount => "Mehrfach-Anzahl",
        TextKey::MultipleCountHelp => {
            "Kann ein Ausdruck mit 'count' (Sammlungsgröße) und 'random' (0 bis count-1) sein"
        }
        TextKey::MultipleMode => "Mehrfach-Modus",
        TextKey::NewScheduleName => "Neuer Sendeplanname",
        TextKey::NewSmartCollection => "Neue intelligente Sammlung",
        TextKey::NoScheduleItems => "Keine Sendeplanelemente.",
        TextKey::NoSchedules => "Keine Sendepläne.",
        TextKey::Normal => "Normal",
        TextKey::Offline => "Offline\nZX",
        TextKey::One => "Eins",
        TextKey::OrderedGroups => "Geordnete Gruppen",
        TextKey::Playback => "Wiedergabe",
        TextKey::PlaybackOrder => "Wiedergabereihenfolge",
        TextKey::PlaybackOrderUpdateFailed => {
            "Aktualisierung der Wiedergabereihenfolge fehlgeschlagen"
        }
        TextKey::PlaybackOrderUpdated => "Wiedergabereihenfolge aktualisiert.",
        TextKey::PlaylistItemSize => "Größe der Wiedergabelistenelemente",
        TextKey::PlayoutDurationHours => "Playout-Dauer (Stunden)",
        TextKey::PlayoutDurationMinutes => "Playout-Dauer (Minuten)",
        TextKey::PostRollFiller => "Post-Roll-Filler",
        TextKey::PostRoll => "Post-Roll",
        TextKey::PreRollFiller => "Pre-Roll-Filler",
        TextKey::PreRoll => "Pre-Roll",
        TextKey::Random => "Zufällig",
        TextKey::RandomCount => "Zufallsanzahl",
        TextKey::RandomRotation => "Zufällige Rotation",
        TextKey::RemainingDurationHelp => {
            "Dauer des Inhalts nach diesem Mid-Roll-Punkt, in Sekunden"
        }
        TextKey::RandomStartPoint => "Zufälliger Startpunkt",
        TextKey::RemoveFailed => "Entfernen fehlgeschlagen",
        TextKey::RemovedFromCollection => "Aus der Sammlung entfernt.",
        TextKey::RemoveFromCollection => "Aus Sammlung entfernen",
        TextKey::Rename => "Umbenennen",
        TextKey::RenameFailed => "Umbenennen fehlgeschlagen",
        TextKey::ReorderFailed => "Neuanordnung fehlgeschlagen",
        TextKey::RerunFirstRun => "Wiederholung: Erstausstrahlung",
        TextKey::RerunRerun => "Wiederholung: Wiederholung",
        TextKey::ReadOnlyTwentyFourHourPlayoutView => {
            "Schreibgeschützte 24-Stunden-Ansicht der geplanten Playout-Elemente."
        }
        TextKey::SaveSchedule => "Sendeplan speichern",
        TextKey::SaveScheduleItems => "Sendeplanelemente speichern",
        TextKey::ScheduleSaveDeferred => "Für nächste Erweiterung speichern",
        TextKey::ScheduleSaveAndRefresh => "Speichern und aktualisieren",
        TextKey::ScheduleDeferredHelp => "Das Speichern für die nächste Erweiterung erhält veröffentlichte Programme und die Wiedergabereihenfolge. Füller, Grafiken, Programmdaten und Spuren können geändert werden. Änderungen an Quellen, Reihenfolge oder Programmstruktur erfordern Speichern und aktualisieren.",
        TextKey::ScheduleDeferredPending => "Wartet auf die nächste Erweiterung",
        TextKey::ScheduleDeferredApplied => "Angewendet ab",
        TextKey::ScheduleDeferredBoundary => "Bereits geplant bis",
        TextKey::ScheduleDeferredLastBuild => "Letzter Aufbau",
        TextKey::ScheduleDeferredBuildSucceeded => "Erfolgreich",
        TextKey::ScheduleDeferredSaved => "Gespeichert; veröffentlichte Programme bleiben unverändert.",
        TextKey::ScheduleDeferredIncompatible => "Verzögertes Speichern kann Quellen, Wiedergabereihenfolge oder Programmstruktur nicht ändern. Verwenden Sie dafür Speichern und aktualisieren.",
        TextKey::ScheduleDeferredBusy => "Ein Playout wird aufgebaut. Warten Sie auf den Abschluss und versuchen Sie es erneut.",
        TextKey::ScheduleDeferredInvalidTimeline => "Die veröffentlichte Planung enthält ungültige Programmzeiten. Reparieren Sie sie vor dem verzögerten Speichern.",
        TextKey::SaveSmartCollection => "Intelligente Sammlung speichern",
        TextKey::Schedule => "Sendeplan",
        TextKey::ScheduleActionFailed => "Sendeplanaktion fehlgeschlagen.",
        TextKey::ScheduleAsGroup => "Als Gruppe planen",
        TextKey::ScheduleKind => "Sendeplantyp",
        TextKey::ScheduleKindBlock => "Block",
        TextKey::ScheduleKindClassic => "Klassisch",
        TextKey::ScheduleKindExternalJson => "JSON (dizqueTV)",
        TextKey::ScheduleKindScripted => "Skriptiert",
        TextKey::ScheduleKindSequential => "Sequenziell",
        TextKey::SchedulingContext => "Planungskontext",
        TextKey::ScheduleItem => "Sendeplanelement",
        TextKey::ScheduleItemActionFailed => "Aktion für Sendeplanelement fehlgeschlagen.",
        TextKey::ScheduleItemFiller => "Filler des Sendeplanelements",
        TextKey::ScheduleItemFillerHelp => {
            "Pre-, Mid- und Post-Roll-Filler gelten für jedes Medienelement, das von diesem Sendeplanelement ausgewählt wird. Um Inhalte nach einem Multiple-Block einmalig abzuspielen, fügen Sie sie als nächstes Sendeplanelement hinzu."
        }
        TextKey::ScheduleItemOverrides => "Überschreibungen des Sendeplanelements",
        TextKey::ScheduleItems => "Sendeplanelemente",
        TextKey::ScheduleNotFound => "Sendeplan nicht gefunden.",
        TextKey::ScheduleSaveFailed => "Speichern des Sendeplans fehlgeschlagen.",
        TextKey::SmartShuffle => "Intelligentes Mischen",
        TextKey::SmartShufflePreset => "Abstands-Voreinstellung",
        TextKey::Balanced => "Ausgewogen",
        TextKey::SmartShuffleScope => "Abstände teilen über",
        TextKey::SmartShuffleScopePlayout => "Playout",
        TextKey::SmartShuffleScopeScheduleItem => "Sendeplanelement",
        TextKey::SmartShuffleValidation => {
            "Wählen Sie einen Abstandsschwellenwert und mindestens eine Ähnlichkeitsregel."
        }
        TextKey::SmartShuffleScopeContentSource => "Inhaltsquelle",
        TextKey::SmartShuffleAfterItems => "Videos zwischen ähnlichen Elementen",
        TextKey::SmartShuffleAfterDurationMinutes => "Minuten zwischen ähnlichen Elementen",
        TextKey::SmartShuffleIncludeFiller => "Filler einbeziehen",
        TextKey::SmartShuffleInheritedBlock => {
            "Für dieses Element gibt es keine Überschreibung für intelligentes Mischen; eine vorhandene ältere Playout-Richtlinie bleibt erhalten."
        }
        TextKey::SmartShuffleInactive => {
            "Intelligentes Mischen ist inaktiv, weil diese Wiedergabereihenfolge nicht zufällig ist."
        }
        TextKey::SmartShuffleOverrideBlock => "Überschreibung für dieses Block-Element",
        TextKey::SimilarityRules => "Ähnlichkeitsregeln",
        TextKey::AlbumArtist => "Album-Interpret",
        TextKey::Apply => "Anwenden",
        TextKey::ContentParent => "Übergeordnet oder Variante",
        TextKey::SmartShuffleFilterRules => "Filterregeln",
        TextKey::SearchForSchedules => "Nach Sendeplänen suchen",
        TextKey::SearchForManualCollections => "Nach manuellen Sammlungen suchen",
        TextKey::SearchForRerunCollections => "Nach Wiederholungssammlungen suchen",
        TextKey::SearchForSmartCollections => "Nach intelligenten Sammlungen suchen",
        TextKey::SearchForTemplates => "Nach Vorlagen suchen",
        TextKey::SearchForBlocks => "Nach Blöcken suchen",
        TextKey::SearchForMultiCollections => "Nach Mehrsammlungen suchen",
        TextKey::SearchQuery => "Suchabfrage",
        TextKey::BlockSearchQueryPlaceholder => "z. B. genre:action",
        TextKey::SearchTitle => "Suchtitel",
        TextKey::SearchTitleDisplay => "Suchtitel (Anzeige)",
        TextKey::SearchSmartCollection => "Intelligente Sammlung suchen",
        TextKey::Season => "Staffel",
        TextKey::SeasonEpisode => "Staffel, Folge",
        TextKey::Unnumbered => "Nicht nummeriert",
        TextKey::SeasonNotFound => "Staffel nicht gefunden",
        TextKey::Select => "Auswählen",
        TextKey::SelectCollection => "Sammlung auswählen",
        TextKey::SelectGroupForNewTemplate => "Wählen Sie eine Gruppe für die neue Vorlage",
        TextKey::SelectScheduleItemPrompt => {
            "Wählen Sie ein Sendeplanelement zum Bearbeiten aus oder fügen Sie ein neues Element hinzu."
        }
        TextKey::SelectSchedule => "Sendeplan auswählen",
        TextKey::SelectSmartCollection => "Intelligente Sammlung auswählen",
        TextKey::Shuffle => "Mischen",
        TextKey::ShuffleInOrder => "Mischen in Reihenfolge",
        TextKey::ShuffleScheduleItems => "Sendeplanelemente mischen",
        TextKey::ShuffleScheduleItemsHelp => {
            "Hinweis: Dies deaktiviert feste Startzeiten und den Flood-Modus"
        }
        TextKey::ShuffledGroups => "Gemischte Gruppen",
        TextKey::Show => "Serie",
        TextKey::ShowNotFound => "Serie nicht gefunden",
        TextKey::Shows => "Serien",
        TextKey::Start => "Start",
        TextKey::StartTime => "Startzeit",
        TextKey::StartTimeOnOrAfter => "Startzeit am oder nach",
        TextKey::StartType => "Starttyp",
        TextKey::Strict => "Strikt",
        TextKey::StrictAlwaysWaitExactStartTime => "Strikt – immer auf die exakte Startzeit warten",
        TextKey::StrictWaitExactStartTime => "Strikt – auf die exakte Startzeit warten",
        TextKey::TailFiller => "Tail-Filler",
        TextKey::TailMode => "Tail-Modus",
        TextKey::Tail => "Tail",
        TextKey::TelevisionSeason => "Fernsehstaffel",
        TextKey::TelevisionShow => "Fernsehserie",
        TextKey::TreatCollectionsAsShows => "Sammlungen als Serien behandeln",
        TextKey::TreatCollectionsAsShowsHelp => {
            "Dies ist nützlich für mehrteilige Crossover-Folgen"
        }
        TextKey::Type => "Typ",
        TextKey::TypeToSearch => "Zum Suchen tippen …",
        TextKey::Search => "Suchen",
        TextKey::Filters => "Filter",
        TextKey::ActiveFilters => "aktive Filter",
        TextKey::ClearFilters => "Filter löschen",
        TextKey::CreateSmartCollection => "Intelligente Sammlung erstellen",
        TextKey::FromYear => "Ab Jahr",
        TextKey::ToYear => "Bis Jahr",
        TextKey::CollectionComposer => "Editor für intelligente Sammlungen",
        TextKey::CollectionDetails => "Sammlungsdetails",
        TextKey::Rules => "Regeln",
        TextKey::Overview => "Übersicht",
        TextKey::MatchAll => "Alle erfüllen",
        TextKey::MatchAllHelp => "Jede Regel in dieser Gruppe muss erfüllt sein.",
        TextKey::MatchAny => "Mindestens eine erfüllen",
        TextKey::MatchAnyHelp => "Mindestens eine Regel in dieser Gruppe muss erfüllt sein.",
        TextKey::ExcludeAny => "Treffer ausschließen",
        TextKey::ExcludeAnyHelp => {
            "Elemente, die eine Regel dieser Gruppe erfüllen, werden ausgeschlossen."
        }
        TextKey::AddRule => "Regel hinzufügen",
        TextKey::EditRule => "Regel bearbeiten",
        TextKey::RemoveRule => "Regel entfernen",
        TextKey::RuleField => "Feld",
        TextKey::Genre => "Genre",
        TextKey::Studio => "Studio",
        TextKey::Network => "Sender",
        TextKey::Actor => "Darsteller",
        TextKey::Writer => "Drehbuchautor",
        TextKey::SubtitleLanguage => "Untertitelsprache",
        TextKey::ShowTitle => "Serientitel",
        TextKey::ContentKindLabel => "Inhaltstyp",
        TextKey::SubKindLabel => "Untertyp",
        TextKey::HasCommentary => "Hat Kommentarspur",
        TextKey::AllFields => "Alle Felder",
        TextKey::MatchType => "Vergleich",
        TextKey::MatchValue => "Wert",
        TextKey::TextContains => "Text enthält",
        TextKey::ExactPhrase => "Exakte Wortgruppe",
        TextKey::Wildcard => "Platzhalter",
        TextKey::Fuzzy => "Unscharf",
        TextKey::Range => "Bereich",
        TextKey::RangeStart => "Bereichsanfang",
        TextKey::RangeEnd => "Bereichsende",
        TextKey::NestedGroup => "Verschachtelte Gruppe",
        TextKey::NestedGroupHelp => {
            "Diese Gruppe im Fokusmodus bearbeiten, damit jeweils nur eine boolesche Ebene sichtbar ist."
        }
        TextKey::AddNestedGroup => "Verschachtelte Gruppe hinzufügen",
        TextKey::EditNestedGroup => "Verschachtelte Gruppe bearbeiten",
        TextKey::BackToRules => "Zurück zu den Regeln",
        TextKey::BackToSearch => "Zurück zur Suche",
        TextKey::CanonicalQuery => "Kanonische Abfrage",
        TextKey::CanonicalQueryHelp => {
            "Diese Abfrage wird von Suche, Planung und gespeicherter Sammlung verwendet."
        }
        TextKey::LivePreview => "Live-Vorschau",
        TextKey::MatchingItems => "passende Elemente",
        TextKey::QueryValid => "Abfrage gültig",
        TextKey::QueryInvalid => "Abfrage muss geprüft werden",
        TextKey::SearchRuleValueRequired => "Jede Suchregel benötigt einen Wert.",
        TextKey::SearchGroupRequired => "Fügen Sie dieser Gruppe mindestens eine Regel hinzu.",
        TextKey::SearchWildcardRequired => "Eine Platzhalterregel muss * oder ? enthalten.",
        TextKey::SavedSearches => "Gespeicherte Suchen",
        TextKey::SavedSearchName => "Name der gespeicherten Suche",
        TextKey::SavedSearchHelp => {
            "Speichern Sie diese komplexe Suche, um sie später aufzurufen und zu ändern."
        }
        TextKey::ChooseSavedSearch => "Gespeicherte Suche auswählen",
        TextKey::LoadSavedSearch => "Gespeicherte Suche laden",
        TextKey::SaveSavedSearch => "Suche speichern",
        TextKey::UpdateSavedSearch => "Gespeicherte Suche aktualisieren",
        TextKey::NewSavedSearch => "Neue gespeicherte Suche",
        TextKey::DeleteSavedSearch => "Gespeicherte Suche löschen",
        TextKey::SavedSearchSaved => "Gespeicherte Suche gespeichert.",
        TextKey::SavedSearchDeleted => "Gespeicherte Suche gelöscht.",
        TextKey::SavedSearchLoaded => "Gespeicherte Suche geladen.",
        TextKey::SavedSearchNameRequired => "Ein Name für die gespeicherte Suche ist erforderlich.",
        TextKey::SavedSearchNameTooLong => "Der Name darf höchstens 80 Zeichen lang sein.",
        TextKey::SavedSearchNameUnique => "Der Name der gespeicherten Suche muss eindeutig sein.",
        TextKey::NoSavedSearches => "Noch keine gespeicherten Suchen.",
        TextKey::RefreshPreview => "Vorschau aktualisieren",
        TextKey::NoRules => "Keine Regeln in dieser Gruppe.",
        TextKey::SearchFiltersHelp => {
            "Nur häufige Filter. Verschachtelte Logik im Sammlungseditor erstellen."
        }
        TextKey::SearchMedia => "Medien suchen",
        TextKey::SearchMoviesOrShows => "Filme oder Serien suchen …",
        TextKey::SearchAddonMediaList => "add-on-Liste suchen",
        TextKey::SearchingMinimumLogLevel => "Mindest-Protokollebene bei der Suche",
        TextKey::Settings => "Einstellungen",
        TextKey::SettingsSaveFailed => "Speichern der Einstellungen fehlgeschlagen",
        TextKey::ShowMediaInfo => "Medieninfo anzeigen",
        TextKey::ScriptedScheduleTimeout => "Zeitlimit für skriptierten Sendeplan",
        TextKey::SkipMissingItems => "Fehlende Elemente überspringen",
        TextKey::SkipMissingItemsHelp => {
            "Legt fest, ob Elemente mit fehlender Datei oder nicht verfügbare Elemente in Playouts aufgenommen werden"
        }
        TextKey::SmartCollections => "Intelligente Sammlungen",
        TextKey::SmartCollection => "Intelligente Sammlung",
        TextKey::SmartCollectionDeleted => "Intelligente Sammlung gelöscht.",
        TextKey::SmartCollectionQueryPlaceholder => "z. B. genre:action OR genre:comedy",
        TextKey::SmartCollectionSaved => "Intelligente Sammlung gespeichert.",
        TextKey::SaveAddonMediaList => "add-on-Liste speichern",
        TextKey::Songs => "Lieder",
        TextKey::SplitTimeEvenly => "Zeit gleichmäßig aufteilen",
        TextKey::StreamingMinimumLogLevel => "Mindest-Protokollebene beim Streaming",
        TextKey::Studios => "Studios",
        TextKey::Support => "Support",
        TextKey::Template => "Vorlage",
        TextKey::TemplateCopied => "Vorlage kopiert.",
        TextKey::TemplateCreated => "Vorlage erstellt.",
        TextKey::TemplateDeleted => "Vorlage gelöscht.",
        TextKey::TemplateGroup => "Vorlagengruppe",
        TextKey::TemplateGroupCreated => "Vorlagengruppe erstellt.",
        TextKey::TemplateGroupDeleted => "Vorlagengruppe gelöscht.",
        TextKey::TemplateGroupName => "Name der Vorlagengruppe",
        TextKey::TemplateGroups => "Vorlagengruppen",
        TextKey::TemplateName => "Vorlagenname",
        TextKey::TemplateNotFound => "Vorlage nicht gefunden.",
        TextKey::Templates => "Vorlagen",
        TextKey::Theme => "Design",
        TextKey::ThisActionCannotBeUndone => "Diese Aktion kann nicht rückgängig gemacht werden.",
        TextKey::ThisCollectionHasNoItemsYet => "Diese Sammlung enthält noch keine Elemente.",
        TextKey::Thumbnail => "Miniaturbild",
        TextKey::Title => "Titel",
        TextKey::ToggleNavigation => "Navigation umschalten",
        TextKey::TotalDurationHelp => "Gesamtdauer des Inhalts, in Sekunden",
        TextKey::TotalPointsHelp => "Gesamtzahl möglicher Mid-Roll-Punkte",
        TextKey::TotalProgressHelp => "normalisierte Position von 0 bis 1",
        TextKey::AddonMediaList => "add-on-Liste",
        TextKey::AddonMediaListActionFailed => "add-on-Listenaktion fehlgeschlagen.",
        TextKey::AddonMediaListAdded => "add-on-Liste hinzugefügt.",
        TextKey::AddonMediaListDeleted => "add-on-Liste gelöscht.",
        TextKey::AddonMediaListMatched => "add-on-Liste abgeglichen.",
        TextKey::AddonMediaListNotFound => "add-on-Liste nicht gefunden.",
        TextKey::AddonMediaListSaveFailed => "Speichern der add-on-Liste fehlgeschlagen.",
        TextKey::AddonMediaListSaved => "add-on-Liste gespeichert.",
        TextKey::AddonMediaLists => "add-on-Listen",
        TextKey::AddonToolMaintenance => "Werkzeugwartung",
        TextKey::AddonToolMaintenanceNone => "Noch kein Wartungsversuch.",
        TextKey::AddonToolUpdatePermission => "Native Updates konfigurierter Programme erlauben",
        TextKey::AddonToolMaintenanceQueued => "In Warteschlange",
        TextKey::AddonToolMaintenanceRunning => "Wird ausgeführt",
        TextKey::AddonToolMaintenanceWaiting => "Wartet auf Wiederholung",
        TextKey::AddonToolUpdated => "Aktualisiert",
        TextKey::AddonToolAlreadyCurrent => "Bereits aktuell",
        TextKey::AddonToolManualUpdate => "Manuelles Update erforderlich",
        TextKey::AddonToolManualHelp => "Aktualisieren Sie das konfigurierte Programm manuell mit der ursprünglichen Installationsmethode und prüfen Sie danach das Add-on.",
        TextKey::AddonToolFailedHelp => "Das Programmupdate ist fehlgeschlagen. Prüfen Sie Programmverfügbarkeit und Netzwerkzugriff; die automatische Wartung versucht es später erneut.",
        TextKey::AddonToolUpdateFailedStale => "Das Programm konnte nicht aktualisiert werden und eine neuere Version ist verfügbar. Die automatische Wartung versucht es später erneut. Verwenden Sie zur sofortigen Aktualisierung die ursprüngliche Installationsmethode und prüfen Sie anschließend das Add-on.",
        TextKey::AddonToolUpdateVersions => "{program}: installiert {installed}, neueste Version {latest}.",
        TextKey::AddonToolVersionHelp => "Die Programmversion konnte nicht gelesen werden. Prüfen Sie das konfigurierte Programm und seine Berechtigungen.",
        TextKey::AddonToolUnavailableHelp => "Das konfigurierte Programm ist nicht verfügbar. Prüfen Sie die Programmeinstellung und Installation.",
        TextKey::AddonToolBusyHelp => "Wartet, bis das Programm frei ist; die laufende Wiedergabe wird nicht unterbrochen.",
        TextKey::AddonToolCleanupHelp => "Beenden Sie Server, Scanner und alle zugehörigen Anbieter- und Programmprozesse. Führen Sie im Serverinstallationsverzeichnis den Offline-Wiederherstellungsbefehl für dieses Programm mit ausdrücklicher Bestätigung aus; ersetzen Sie <absolute-executable-path> durch den konfigurierten absoluten Pfad. Verwenden Sie unter Windows ersatzrs-web.exe. Geben Sie bei einem eigenen Profil zusätzlich --state-dir mit dessen absolutem Verzeichnis an. Starten Sie erst danach neu. Dies hebt die Prozesssicherheitssperre auf; das Programm wird nicht repariert.",
        TextKey::AddonListDiskWarnings => "Warnungen zur Festplattennutzung",
        TextKey::AddonListDiskWarningGb => "Diese Liste: warnen ab (GB)",
        TextKey::AddonListsCombinedDiskWarningGb => "Alle Add-on-Listen zusammen: warnen ab (GB)",
        TextKey::AddonListDiskWarningHelp => "Ganze dezimale GB (1 GB = 1.000.000.000 Bytes). 0 deaktiviert nur diese Warnung. Beim Erreichen oder Überschreiten des Grenzwerts erscheint lediglich eine Zustandswarnung; Downloads werden nicht gestoppt und Dateien nicht gelöscht.",
        TextKey::AddonListDiskWarningFailed => "Die Speicherwarnung konnte nicht geladen oder gespeichert werden. Laden Sie die Seite neu und versuchen Sie es erneut.",
        TextKey::ScanWorkerTitle => "Scan-Dienst",
        TextKey::ScanWorkerUnavailableBrief => "Der Scan-Dienst ist nicht verfügbar",
        TextKey::ScanWorkerUnavailableHelp => "Der Scan-Dienst ist nicht verfügbar. Neue Scans können nicht ausgeführt werden. Prüfen Sie die Protokolle und stellen Sie sicher, dass alle verbleibenden lokalen FFprobe-Prozesse beendet sind, bevor Sie ErsatzRS neu starten.",
        TextKey::PlaybackDataAccessTitle => "Zugriff auf Wiedergabedaten",
        TextKey::LegacyMpegTsCapacityBrief => "Legacy-MPEG-TS benötigt mindestens zwei aktive Transcodierungsplätze",
        TextKey::LegacyMpegTsCapacityHelp => "Ein aktivierter Kanal verwendet den nativen Legacy-MPEG-TS-Modus. Sein Wrapper und der Programmencoder benötigen zwei separate aktive Transcodierungsplätze. Erhöhen Sie das Limit für aktive Transcodierungen auf mindestens 2 oder wählen Sie einen anderen Streamingmodus.",
        TextKey::TeletextChannelTitle => "Kanal-Teletext",
        TextKey::TeletextChannelBaseHelp => "Diese Seite ändert die gespeicherten Grundeinstellungen. Aktive Lua-Überschreibungen behalten ihre Laufzeit. Seitenprüfung und Veröffentlichung sind separate Schritte; Speichern sendet keine Seiten und aktiviert keine Pakete.",
        TextKey::TeletextChannelEmpty => "Keine Teletext-Add-ons oder gespeicherten Auswahlen verfügbar. Verwalte installierte Add-ons, um ein Profil anzulegen.",
        TextKey::TeletextChannelProfiles => "Profile",
        TextKey::TeletextProfileCreate => "Profil anlegen",
        TextKey::TeletextProfileSave => "Profil speichern",
        TextKey::TeletextProfileName => "Profilname",
        TextKey::TeletextProfileSharedHelp => "Profile können gemeinsam verwendet werden. Das Bearbeiten eines Profils macht abhängige Kanalauswahlen ungültig; verwende separate Profile für unabhängige Kanaleinstellungen.",
        TextKey::TeletextBindingProfile => "Ausgewähltes Profil",
        TextKey::TeletextBindingSave => "Kanalauswahl speichern",
        TextKey::TeletextBindingOff => "Teletext ausschalten",
        TextKey::TeletextBindingNone => "Kein Profil ausgewählt",
        TextKey::TeletextProfileInheritHelp => "Vererben verwendet die Installationseinstellung oder den Add-on-Standard. Wähle Überschreiben für einen Profilwert; ein leerer Wert unterscheidet sich vom Vererben.",
        TextKey::TeletextProfileOverride => "Überschreiben",
        TextKey::TeletextProfileValue => "Eigener Wert",
        TextKey::TeletextProfileReference => "Speicherort des Geheimnisses",
        TextKey::TeletextProfileConfigured => "Konfiguration gültig",
        TextKey::TeletextProfileMissingInstallation => "Paket ist nicht installiert",
        TextKey::TeletextProfileDisabledInstallation => "Paket ist deaktiviert",
        TextKey::TeletextProfileIncompatibleInstallation => "Paket ist inkompatibel",
        TextKey::TeletextProfileInvalidSettings => "Profileinstellungen sind ungültig",
        TextKey::TeletextProfileDeleted => "Profil wurde gelöscht",
        TextKey::TeletextProfileStaleInstallation => "Paket geändert; prüfe und speichere dieses Profil erneut",
        TextKey::TeletextAdminConflict => "Diese Konfiguration wurde geändert. Prüfe die aktuellen Werte vor dem erneuten Speichern.",
        TextKey::TeletextAdminInvalid => "Diese Einstellungen konnten nicht gespeichert werden. Prüfe Profil, Auswahl und Feldwerte.",
        TextKey::TeletextAdminUnavailable => "Die Teletext-Konfiguration ist nicht verfügbar. Prüfe Anwendung und Datenbank und versuche es erneut.",
        TextKey::TeletextAdminSaved => "Grundkonfiguration gespeichert. Seitenprüfung und Veröffentlichung bleiben separate Schritte.",
        TextKey::TeletextAdminWrongAddon => "Das Profil gehört nicht zu diesem Add-on. Prüfe die Auswahl.",
        TextKey::TeletextAdminLimit => "Diese Einstellungen überschreiten die unterstützten Konfigurationsgrenzen. Reduziere Profilwerte oder Auswahlen.",
        TextKey::TeletextProfileRedacted => "Ein gespeicherter geheimer Wert kann hier nicht angezeigt werden. Vererbe eine gültige Einstellung oder ersetze ihn durch den Speicherort eines Geheimnisses.",
        TextKey::TeletextProfileRepairHelp => "Dieses Profil enthält Einstellungen, die mit dem aktuellen Add-on nicht sicher bearbeitet werden können. Lege ein neues Profil mit geprüften Werten an und wähle es ausdrücklich aus. Die vorhandene Kanalauswahl kann weiterhin ausgeschaltet werden.",
        TextKey::TeletextEffectiveAuthoringTitle => "Seitenentwürfe der aktiven Scripted-Auswahl",
        TextKey::TeletextEffectiveAuthoringHelp => "Prüfen Sie die vom aktuellen Skript gewählten Profile und speichern Sie einen statischen Entwurf. Es wird kein TV-Programm übergeben. Speichern veröffentlicht keine Seiten.",
        TextKey::TeletextEffectiveAuthoringUnavailable => "Eine aktive Scripted-Auswahl und gültige Entwurfseinstellungen sind erforderlich.",
        TextKey::TeletextEffectiveAuthoringAbsent => "Das aktuelle Skript schaltet den gesamten Teletext aus. Speichern Sie einen Entwurf ohne Teletext und ohne Seitenadresse.",
        TextKey::TeletextEffectiveScopeExpiry => "Diese Scripted-Auswahl ist gültig bis:",
        TextKey::TeletextPreviewAbsent => "Dieser Entwurf wählt keine Teletextseiten aus. Es gibt keine anzuzeigende Seite.",
        TextKey::TeletextEffectiveEditorUnavailable => "Editor-Export und -Import für Scripted-Entwürfe sind noch nicht verbunden. Der gespeicherte Entwurf bleibt für Vorschau und geprüfte Veröffentlichung verfügbar.",
        TextKey::TeletextAuthoringTitle => "Seitenentwürfe",
        TextKey::TeletextAuthoringHelp => "Prüfen Sie die ausgewählten Profile und speichern Sie einen neuen Entwurf. Die Ausstrahlung bleibt unverändert.",
        TextKey::TeletextAuthoringStaticHelp => "Statische Seiten: In diesem Schritt wird kein TV-Programmführer bereitgestellt.",
        TextKey::TeletextAuthoringUnavailable => "Die Entwurfsprüfung erfordert gültige Gestaltungsvorgaben und eine aktivierte Profilauswahl.",
        TextKey::TeletextDesignName => "Entwurfsname",
        TextKey::TeletextEntryProducer => "Add-on für die Startseite",
        TextKey::TeletextEntryPage => "Startseite (100–899)",
        TextKey::TeletextEntrySubpage => "Startunterseite (hex)",
        TextKey::TeletextV1Region => "Zeichenregion für Level 1",
        TextKey::TeletextV1WesternLatin => "Westliches Latein",
        TextKey::TeletextResourceLifecycle => "Aktualisierungsdatensätze für Ressourcen (JSON)",
        TextKey::TeletextResourceLifecycleHelp => "Geben Sie für jede Ressourcenseite einen ausdrücklichen Aktualisierungsdatensatz an. [] ist gültig, wenn keine Ressourcen vorhanden sind.",
        TextKey::TeletextValidateSaveDraft => "Prüfen und Entwurf speichern",
        TextKey::TeletextDraftSaved => "Entwurf gespeichert. Vorschau und Veröffentlichung sind separate Schritte.",
        TextKey::TeletextEditorTitle => "Mit QTeletextMaker bearbeiten",
        TextKey::TeletextEditorHelp => "Laden Sie eine ZIP-Datei herunter, entpacken Sie sie und bearbeiten Sie die TTI-Seiten in QTeletextMaker. Der Export enthält die ursprünglichen Metadaten und benötigten Ressourcendateien.",
        TextKey::TeletextEditorOwnershipHelp => "Nur freie Inhalte und definierte benutzerdefinierte Zonen dürfen geändert werden. Generierte Programminhalte, Seitenidentitäten und Ressourcendeklarationen sind geschützt. Nicht unterstützte Änderungen werden abgewiesen.",
        TextKey::TeletextEditorTemplateHelp => "Änderungen an Addon-Vorlagen erfordern ein ausdrückliches Addon-Versionsupdate; ein Entwurfsimport verändert keine installierten Pakete.",
        TextKey::TeletextEditorImportHelp => "Wählen Sie die unveränderten .ersatzrs.json-Metadaten und alle .tti- und .dvb-Dateien desselben Exports, auch unveränderte Dateien. Behalten Sie die ursprünglichen Dateinamen bei.",
        TextKey::TeletextEditorNoPublicationHelp => "Der Import prüft und speichert einen unveröffentlichten Entwurf zur Vorschau. Veröffentlichung anfordern und bestätigen bleiben separate Aktionen.",
        TextKey::TeletextEditorUnavailable => "Editor-Export und -Import sind nicht verfügbar, da der aktuelle Designstand nicht geprüft werden konnte. Laden Sie die Vorschau neu.",
        TextKey::TeletextEditorExportPage => "Diese Seite und Unterseite herunterladen",
        TextKey::TeletextEditorExportSet => "Vollständigen Seitensatz herunterladen",
        TextKey::TeletextEditorSidecar => "Exportmetadaten (.ersatzrs.json)",
        TextKey::TeletextEditorFiles => "Seiten- und Ressourcendateien (.tti, .dvb)",
        TextKey::TeletextEditorImport => "Prüfen und als Entwurf importieren",
        TextKey::TeletextObjectCopyTitle => "Objektdefinition kopieren",
        TextKey::TeletextObjectCopyHelp => "Erstellen Sie eine unveröffentlichte Kopie in derselben Ressource. Bearbeiten Sie einen neuen Export in QTeletextMaker und veröffentlichen Sie ihn separat.",
        TextKey::TeletextObjectCopySource => "Quellobjekt",
        TextKey::TeletextObjectCopyNumber => "Freie Objektnummer",
        TextKey::TeletextObjectCopyAction => "Entwurfskopie erstellen",
        TextKey::TeletextObjectCopyCurrentHelp => "Öffnen Sie die aktuelle Entwurfsversion, um ein Objekt zu kopieren.",
        TextKey::TeletextObjectCopyInvalid => "Wählen Sie ein vorhandenes Quellobjekt und eine freie Objektnummer in derselben Ressource.",
        TextKey::TeletextObjectAddTitle => "Objektdefinition hinzufügen",
        TextKey::TeletextObjectAddHelp => "Erstellen Sie eine leere Definition in einer vorhandenen POP/GPOP-Ressource. Daraus wird ein neuer Entwurf; die Veröffentlichung bleibt ein separater Schritt.",
        TextKey::TeletextObjectAddResource => "Ressource",
        TextKey::TeletextObjectAddType => "Objekttyp",
        TextKey::TeletextObjectAddAction => "Leere Definition erstellen",
        TextKey::TeletextObjectAddInvalid => "Die Ressource, der Typ oder die neue Objektnummer ist ungültig, gehört nicht zu diesem Entwurf oder wird bereits verwendet.",
        TextKey::TeletextResourceBankTitle => "Ressourcenbank erstellen",
        TextKey::TeletextResourceBankHelp => "Erstellen Sie eine leere POP/GPOP-Bank in einem neuen Entwurf. Geben Sie alle Aktualisierungswerte ausdrücklich an. Quellseiten und installierte Pakete bleiben unverändert; veröffentlichen Sie separat.",
        TextKey::TeletextResourceDisplay => "Anzeigeseite / Unterseite",
        TextKey::TeletextResourceBankKind => "Ressourcentyp",
        TextKey::TeletextResourceBankPage => "Ressourcenseite (drei Hexadezimalziffern)",
        TextKey::TeletextResourceBankSubpage => "Logische Unterseite (S1)",
        TextKey::TeletextResourceBankCounter => "Aktualisierungszähler",
        TextKey::TeletextResourceBankUpdate => "Aktualisierungsstatus",
        TextKey::TeletextResourceBankUnchanged => "Unverändert",
        TextKey::TeletextResourceBankChanged => "Geändert",
        TextKey::TeletextResourceBankMoreSubpages => "Weitere Unterseiten folgen",
        TextKey::TeletextResourceBankFinalSubpage => "Letzte Unterseite",
        TextKey::TeletextResourceBankLastSubpage => "Letzte logische Unterseite",
        TextKey::TeletextResourceBankAction => "Entwurfsbank erstellen",
        TextKey::TeletextResourceBankInvalid => "Prüfen Sie Anzeigeseite, POP/GPOP-Adresse und ausdrückliche Aktualisierungswerte. Die Bank muss neu und gültig sein.",
        TextKey::TeletextSubpageAnnouncementTitle => "Ressourcenunterseite ankündigen",
        TextKey::TeletextSubpageAnnouncementHelp => "Wählen Sie eine oder mehrere Anzeigeseiten und eine vorhandene Ressourcenunterseite. Nur eine bereits ausgewählte Ressourcenseite kann erweitert werden. Dies erstellt einen Entwurf; veröffentlichen Sie separat.",
        TextKey::TeletextSubpageAnnouncementDisplays => "Anzeigeseiten / Unterseiten (eine oder mehrere auswählen)",
        TextKey::TeletextSubpageAnnouncementAction => "Entwurfsankündigung speichern",
        TextKey::TeletextSubpageAnnouncementInvalid => "Wählen Sie gültige Anzeigeseiten und eine vorhandene Unterseite ihrer ausgewählten POP/GPOP-Ressource. Es wurden keine Änderungen gespeichert.",
        TextKey::TeletextObjectAttachmentTitle => "Eine vorhandene Ressourcenseite zuordnen",
        TextKey::TeletextObjectAttachmentHelp => "Wählen Sie eine vorhandene Seite für eine Rolle ohne ausgewählte Seite. Bestehende Objektaufrufe bleiben unverändert. Dies speichert einen Entwurf. Veröffentlichen Sie ihn separat.",
        TextKey::TeletextObjectAttachmentAction => "Zuordnung als Entwurf speichern",
        TextKey::TeletextObjectAttachmentRestoreTitle => "Eine Ressourcenseitenzuordnung rückgängig machen",
        TextKey::TeletextObjectAttachmentRestoreHelp => "Setzen Sie diese Rolle auf den ursprünglichen Zustand ohne ausgewählte Seite zurück. Andere Entwurfsänderungen bleiben erhalten. Dies speichert einen Entwurf. Veröffentlichen Sie ihn separat.",
        TextKey::TeletextObjectAttachmentRestoreAction => "Rücknahme als Entwurf speichern",
        TextKey::TeletextObjectSelectionTitle => "Ressourcenseiten und Objektaufrufe auswählen",
        TextKey::TeletextObjectSelectionHelp => "Wählen Sie vorhandene POP/GPOP- und DRCS/GDRCS-Seiten und passen Sie bestehende Ressourcenaufrufe ausdrücklich gemeinsam an. Aufrufe werden nie automatisch geändert; stimmen Sie alle betroffenen Verwendungen ab. Dies speichert einen Entwurf. Veröffentlichen Sie ihn separat.",
        TextKey::TeletextObjectSelectionPageTarget => "Ressourcenseite",
        TextKey::TeletextObjectSelectionMask => "Erforderliche Unterseiten (Hexadezimalmaske)",
        TextKey::TeletextObjectSelectionMaskHelp => "Geben Sie 1–4 Hexadezimalziffern ein. Bit 0 kündigt S1=0 an, bis Bit 15 für S1=15. Kündigen Sie alle erforderlichen Unterseiten der ausgewählten Seite an.",
        TextKey::TeletextObjectSelectionCalls => "Ressourcenaufruf",
        TextKey::TeletextObjectSelectionMainCalls => "Expliziter Main-Aufruf",
        TextKey::TeletextObjectSelectionCaller => "Aufrufendes Objekt",
        TextKey::TeletextObjectSelectionCommand => "Befehlsindex (beginnt bei 0)",
        TextKey::TeletextObjectSelectionCurrentTarget => "Aktuelles Ziel",
        TextKey::TeletextObjectSelectionOffset => "Ursprungsversatz (Zeile, Spalte)",
        TextKey::TeletextObjectSelectionNoOffset => "Kein ausdrücklicher Versatz",
        TextKey::TeletextObjectSelectionCallTarget => "Aufrufziel",
        TextKey::TeletextObjectSelectionAction => "Auswahl als Entwurf speichern",
        TextKey::TeletextObjectSelectionInvalid => "Die ausgewählten Seiten, Unterseitenmasken oder abgestimmten Aufrufe sind ungültig oder nicht mehr aktuell. Es wurden keine Änderungen gespeichert.",
        TextKey::TeletextResourceEditCurrentHelp => "Öffnen Sie die aktuelle Entwurfsversion, um Ressourcenbanken oder Ankündigungen zu ändern.",
        TextKey::TeletextObjectAddCurrentHelp => "Öffnen Sie die aktuelle Entwurfsrevision, um eine Objektdefinition hinzuzufügen.",
        TextKey::TeletextObjectTypeActive => "Aktiv",
        TextKey::TeletextObjectTypeAdaptive => "Adaptiv",
        TextKey::TeletextObjectTypePassive => "Passiv",
        TextKey::TeletextEditorLimit => "Die Editordateien überschreiten die konfigurierten Größen- oder Verarbeitungslimits. Verkleinern Sie den unterstützten Seitensatz oder prüfen Sie die konfigurierten Limits. Die Ausstrahlung bleibt unverändert.",
        TextKey::TeletextEditorMismatch => "Import abgewiesen: Dateien oder Metadaten passen nicht zum ursprünglichen Export oder aktuellen Design. Laden Sie die Vorschau neu und verwenden Sie Dateien desselben Exports.",
        TextKey::TeletextEditorProtected => "Import abgewiesen: Ein geschützter oder nicht unterstützter Seitenbereich wurde geändert. Bearbeiten Sie nur freie Inhalte oder definierte benutzerdefinierte Zonen.",
        TextKey::TeletextEditorNoChanges => "Der Import enthält keine unterstützten Seitenänderungen. Bearbeiten Sie vor dem Import freie Inhalte oder eine benutzerdefinierte Zone.",
        TextKey::TeletextEditorSyntax => "Import abgewiesen: Eine TTI-Seite ist ungültig oder verwendet nicht unterstützte Syntax. Prüfen Sie die angegebene Seite und Position.",
        TextKey::TeletextEditorMissingReceipt => "Der ursprüngliche Exportnachweis ist nicht verfügbar. Exportieren Sie das Design vor dem Import erneut.",
        TextKey::TeletextDeliveryTitle => "Ausgabe aktiver Streams",
        TextKey::TeletextDeliveryHelp => "Jeder Eintrag beschreibt eine beobachtete Ausgabe. Die Lieferung bestätigt weder die Clientanzeige noch andere Zuschauer. Zum Aktualisieren neu laden; die letzte bestätigte Lieferung ist eine frühere Beobachtung.",
        TextKey::TeletextDeliveryUnavailable => "Ausgabebeobachtung nicht verfügbar.",
        TextKey::TeletextDeliveryEmpty => "Keine aktive Ausgabe beobachtet.",
        TextKey::TeletextDeliveryIncomplete => "Einige Ausgaben konnten nicht erfasst werden. Diese Liste ist unvollständig.",
        TextKey::TeletextDeliveryLabel => "Ausgabe",
        TextKey::TeletextDeliveryRequested => "Angeforderte Ausgabe",
        TextKey::TeletextDeliveryConfirmed => "Letzte bestätigte Lieferung",
        TextKey::TeletextDeliveryWaiting => "Warten auf vollständige Lieferung.",
        TextKey::TeletextDeliveryApplied => "Angeforderte Ausgabe geliefert.",
        TextKey::TeletextDeliveryRejected => "Angeforderte Änderung abgelehnt.",
        TextKey::TeletextDeliveryUnconfirmed => "Keine Lieferung bestätigt.",
        TextKey::TeletextDeliveryAbsent => "Keine selbst erstellten Seiten",
        TextKey::TeletextDeliveryGeneration => "Generation",
        TextKey::TeletextPublicationTitle => "Diesen gespeicherten Entwurf veröffentlichen",
        TextKey::TeletextPublicationHelp => "Fordere die Veröffentlichung dieser gespeicherten Revision an und veröffentliche anschließend den geprüften Antrag. Speichern oder Vorschau veröffentlicht keinen Entwurf.",
        TextKey::TeletextPublicationUnrequested => "Für diese gespeicherte Revision liegt kein offener Veröffentlichungsantrag vor. Bereits ausgewählte Seiten bleiben unverändert.",
        TextKey::TeletextPublicationPending => "Die Veröffentlichung dieser gespeicherten Revision wurde angefordert. Sie ist noch nicht für Streams ausgewählt.",
        TextKey::TeletextPublicationSelected => "Diese gespeicherte Revision ist ausgewählt. Das bestätigt nicht ihre Anwendung in einem Stream; jede geeignete Ausgabe prüft Änderungen vor der Anwendung.",
        TextKey::TeletextPublicationUnavailable => "Für diese gespeicherte Revision ist die Veröffentlichung nicht verfügbar. Prüfe die aktuellen Einstellungen und die Validierung vor einem neuen Antrag.",
        TextKey::TeletextPublicationRequest => "Veröffentlichung anfordern",
        TextKey::TeletextPublicationPublish => "Angeforderte Seiten veröffentlichen",
        TextKey::TeletextPublicationConflict => "Entwurf, Auswahl oder Veröffentlichungsstatus wurde geändert. Lade genau diese Revision erneut und prüfe sie vor einem weiteren Versuch.",
        TextKey::TeletextPublicationRequestSaved => "Veröffentlichungsantrag gespeichert. Die für Streams ausgewählten Seiten wurden nicht geändert.",
        TextKey::TeletextPublicationSelectedSaved => "Die angeforderten Seiten sind ausgewählt. Die tatsächliche Anwendung wird für jede geeignete Ausgabe separat geprüft.",
        TextKey::TeletextPublicationReplacePending => "Das Anfordern dieser Revision ersetzt den offenen Antrag für einen anderen Entwurf. Die derzeit für Streams ausgewählten Seiten bleiben unverändert.",
        TextKey::TeletextPreviewTitle => "Entwurfsvorschau",
        TextKey::TeletextPreviewHelp => "Diese Vorschau verwendet das geprüfte Modell. Empfängerkopfzeile, Video und nicht aufgelöste Bereiche werden separat gekennzeichnet.",
        TextKey::TeletextPreviewPage => "Seite",
        TextKey::TeletextPreviewSubpage => "Unterseite",
        TextKey::TeletextPreviewLoad => "Vorschau anzeigen",
        TextKey::TeletextPreviewDiagnostics => "Prüfungs- und Darstellungsprobleme",
        TextKey::TeletextPreviewNoDiagnostics => "Für diese Seite liegen keine ungelösten Darstellungsprobleme vor.",
        TextKey::TeletextPreviewReceiverHeader => "Empfängerkopfzeile",
        TextKey::TeletextPreviewVideo => "Video",
        TextKey::TeletextPreviewUnresolved => "Nicht aufgelöst",
        TextKey::TeletextDraftMissing => "Der angeforderte Entwurf oder die Version ist für diesen Kanal nicht verfügbar.",
        TextKey::TeletextWorkBusy => "Die Teletext-Verarbeitung ist ausgelastet. Versuchen Sie es erneut.",
        TextKey::TeletextWorkExpired => "Das ursprüngliche Zeitlimit ist abgelaufen. Ein bereits gespeicherter Entwurf bleibt erhalten.",
        TextKey::TeletextAuthoringRejected => "Die ausgewählten Seitendaten konnten nicht geprüft werden. Die laufende Ausstrahlung bleibt unverändert.",
        TextKey::TeletextPreviewPosition => "Position",
        TextKey::TeletextPreviewSource => "Originalquelle",
        TextKey::TeletextPreviewReload => "Gespeicherten Entwurf erneut öffnen",
        TextKey::TeletextPreviewEditorialTitle => "Bearbeitungsgrenzen",
        TextKey::TeletextPreviewEditorialHelp => "Diese Grenzen stammen aus diesem gespeicherten Original. Zellen außerhalb der festgelegten Bereiche bleiben geschützt; der Import eines Entwurfs ändert die Vorlage nicht.",
        TextKey::TeletextPreviewRegion => "Bereich",
        TextKey::TeletextPreviewArea => "Seitenbereich",
        TextKey::TeletextPreviewExtent => "Zeilen und Spalten (ab null gezählt)",
        TextKey::TeletextPreviewPermission => "Bearbeitung",
        TextKey::TeletextPreviewFree => "Freier Inhalt",
        TextKey::TeletextPreviewZone => "Ausfüllbarer Bereich",
        TextKey::TeletextPreviewGenerated => "Automatisch generiert",
        TextKey::TeletextPreviewDynamicField => "Dynamisches Feld",
        TextKey::TeletextPreviewBinding => "Zuordnung",
        TextKey::TeletextPreviewNoEditorialMetadata => "Für diese gespeicherte Seite wurden keine Bearbeitungsbereiche festgelegt.",
        TextKey::TeletextPreviewPanelLeft => "Linker Seitenbereich",
        TextKey::TeletextPreviewPanelMain => "Hauptseite",
        TextKey::TeletextPreviewPanelRight => "Rechter Seitenbereich",
        TextKey::TeletextNavIndex => "Index",
        TextKey::TeletextNavRed => "Rot",
        TextKey::TeletextNavGreen => "Grün",
        TextKey::TeletextNavYellow => "Gelb",
        TextKey::TeletextNavCyan => "Cyan",
        TextKey::TeletextAuthoringHealthTitle => "Konfiguration der Teletextgestaltung",
        TextKey::TeletextAuthoringHealthAbsentBrief => "Teletextgestaltung ist nicht konfiguriert",
        TextKey::TeletextAuthoringHealthAbsentDetail => "Es ist keine Richtlinie für die Teletextgestaltung konfiguriert. Entwurfsvorbereitung und Vorschau sind nicht verfügbar; die normale Wiedergabe bleibt unbeeinträchtigt.",
        TextKey::TeletextAuthoringHealthValidBrief => "Einstellungen der Teletextgestaltung sind gültig",
        TextKey::TeletextAuthoringHealthValidDetail => "Die Einstellungen der Teletextgestaltung sind gültig. Jeder Entwurf und jede Vorschau erfordern weiterhin eine begrenzte Validierung. Diese Prüfung bestätigt weder gestaltete Seiten noch Veröffentlichung, vollständige Level-2.5-Unterstützung oder Speichergrenzen.",
        TextKey::TeletextAuthoringHealthInvalidBrief => "Einstellungen der Teletextgestaltung sind ungültig",
        TextKey::TeletextAuthoringHealthInvalidDetail => "Die Einstellungen der Teletextgestaltung sind ungültig. Entwurfsvorbereitung und Vorschau sind nicht verfügbar; die normale Wiedergabe bleibt unbeeinträchtigt. Korrigieren Sie die Einstellungen der Teletextgestaltung.",
        TextKey::TeletextAuthoringHealthUnreadableBrief => "Einstellungen der Teletextgestaltung konnten nicht gelesen werden",
        TextKey::TeletextAuthoringHealthUnreadableDetail => "Die Einstellungen der Teletextgestaltung konnten nicht gelesen werden. Entwurfsvorbereitung und Vorschau sind nicht verfügbar; die normale Wiedergabe bleibt unbeeinträchtigt. Prüfen Sie Datenbankverfügbarkeit und Zugriff.",
        TextKey::TeletextWorkTitle => "Konfiguration der Teletext-Aufgaben",
        TextKey::TeletextWorkDefault => "Teletext-Aufgaben werden standardmäßig nacheinander ausgeführt. Dies ist keine Speichergrenze und bestätigt nicht, dass selbst erstellte Seiten sendebereit sind.",
        TextKey::TeletextWorkConfigured => "Die eingestellte Grenze beträgt {count} gleichzeitige Teletext-Aufgaben. Dies ist keine Speichergrenze und bestätigt nicht, dass selbst erstellte Seiten sendebereit sind.",
        TextKey::TeletextWorkInvalid => "Die Anzahl gleichzeitiger Teletext-Aufgaben muss eine positive ganze Zahl sein. Teletext-Aufgaben sind erst nach Korrektur dieser Einstellung verfügbar.",
        TextKey::TeletextSourceStorageInvalid => "Die Speicherkonfiguration für Anbieterquellen ist ungültig oder nicht verfügbar. Optionale Teletext-Untertitel von Anbietern sind nicht verfügbar; die normale Audio- und Videowiedergabe bleibt verfügbar.",
        TextKey::TeletextOutputTitle => "Konfiguration der Teletext-Ausgabe",
        TextKey::TeletextOutputAbsentBrief => "Die Teletext-Ausgabe ist nicht konfiguriert",
        TextKey::TeletextOutputAbsentHelp => "Es ist keine Richtlinie für die Teletext-Ausgabe konfiguriert. Die Kanäle werden weiterhin normal ohne zusätzlichen Teletext wiedergegeben.",
        TextKey::TeletextOutputConfiguredBrief => "Die Einstellungen für die Teletext-Ausgabe sind gültig",
        TextKey::TeletextOutputConfiguredHelp => "Die Einstellungen für die Teletext-Ausgabe sind gültig. Bei der Wiedergabe werden weiterhin die akzeptierten Seiten und die Stream-Kompatibilität jedes Kanals geprüft. Diese Prüfung bestätigt weder die Teletext-Übertragung noch die vollständige Unterstützung von Level 2.5 oder Speichergrenzen.",
        TextKey::TeletextOutputInvalidBrief => "Die Einstellungen für die Teletext-Ausgabe sind ungültig",
        TextKey::TeletextOutputInvalidHelp => "Die Einstellungen für die Teletext-Ausgabe sind ungültig. Die normale Wiedergabe wird ohne zusätzlichen Teletext fortgesetzt. Korrigieren Sie die Einstellungen, um die Teletext-Ausgabe zu aktivieren.",
        TextKey::TeletextOutputUnreadableBrief => "Die Einstellungen für die Teletext-Ausgabe konnten nicht gelesen werden",
        TextKey::TeletextOutputUnreadableHelp => "Die Einstellungen für die Teletext-Ausgabe konnten nicht gelesen werden. Die normale Wiedergabe wird ohne zusätzlichen Teletext fortgesetzt. Prüfen Sie die Verfügbarkeit und den Zugriff auf die Datenbank.",
        TextKey::TeletextDecoderTitle => "Teletext-Decodierung",
        TextKey::TeletextDecoderReady => "Die ausgewählte FFmpeg-Laufzeit hat die Prüfungen für Teletext-Seitenauswahl, Untertiteltext und Farbbitmap-Decodierung bestanden.",
        TextKey::TeletextDecoderMissing => "Teletext-Decodierung ist nicht verfügbar. Wählen Sie eine FFmpeg-9-Laufzeit mit libzvbi-Teletext-Unterstützung.",
        TextKey::TeletextDecoderOptions => "Dem Teletext-Decoder fehlen erforderliche Optionen für die Seitenauswahl oder Darstellung. Wählen Sie eine kompatible FFmpeg-9-Laufzeit.",
        TextKey::TeletextDecoderHelpFailed => "Die Teletext-Decoder-Prüfung konnte nicht abgeschlossen werden. Prüfen Sie die Verfügbarkeit und Zugriffsrechte der FFmpeg-Laufzeit.",
        TextKey::TeletextDecoderTextFailed => "Die Decodierung von Teletext-Seiten und Untertiteltext ist fehlgeschlagen. Wählen Sie eine kompatible FFmpeg-9-Laufzeit.",
        TextKey::TeletextDecoderOriginalPtsUnavailable => "Die Laufzeit kann Teletext dekodieren, aber die ursprünglichen Zeitstempel von Live-Untertiteln nicht erhalten. Live-Teletext-Untertitel sind nicht verfügbar; die normale Wiedergabe bleibt verfügbar.",
        TextKey::TeletextDecoderBitmapFailed => "Die Decodierung von Teletext-Farbbitmaps ist fehlgeschlagen. Wählen Sie eine kompatible FFmpeg-9-Laufzeit.",
        TextKey::NextRuntimeAvailability => "Verfügbarkeit der Next-Laufzeit",
        TextKey::NextOverlayInvalid => "Next-Konfigurationsüberlagerung ist nicht verfügbar oder ungültig",
        TextKey::NextOverlaySchema => "Next-Konfigurationsüberlagerungen benötigen eine unterstützte Versions-URI (0.1.0 oder 0.1.1). Prüfen Sie die Kopiereinstellungen, bevor Sie eine ältere Überlagerung aktualisieren.",
        TextKey::NextOverlayIncomplete => "Validierung der Next-Konfigurationsüberlagerung ist unvollständig",
        TextKey::JavascriptAudioSelectionTitle => "JavaScript-Audioauswahl",
        TextKey::JavascriptAudioSelectionHealthy => "Seit dem Start wurden keine Auswahlfehler erfasst. Noch nicht ausgeführte Skripte sind nicht validiert.",
        TextKey::JavascriptAudioSelectionFailures => "{count} Auswahlfehler seit dem Start; die integrierte Audioauswahl wurde verwendet. Prüfen Sie scripts/audio-stream-selector und die Grenzen in ADR 0096. Der Zähler wird beim Neustart zurückgesetzt.",
        TextKey::AddonListDiskUsageTitle => "Festplattennutzung der Add-on-Listen",
        TextKey::AddonListDiskUsageBrief => "Warnung zur Festplattennutzung der Add-on-Listen",
        TextKey::AddonListDiskUsageUnavailable => "Die Festplattennutzung der Add-on-Listen konnte nicht vollständig ermittelt werden. Prüfen Sie den Speicherzugriff und wiederholen Sie die Zustandsprüfung.",
        TextKey::AddonListDiskUsageUnavailableBrief => "Die Festplattennutzung der Add-on-Listen konnte nicht ermittelt werden.",
        TextKey::AddonListDiskUsageLine => "Liste #{id}: {used} GB belegt; Warnung ab {limit} GB.",
        TextKey::AddonListsCombinedDiskUsageLine => "Gesamt: {used} GB belegt; Warnung ab {limit} GB.",
        TextKey::AddonMediaListUrl => "add-on-Listen-URL",
        TextKey::AddonMediaListFolder => "Speicherordner",
        TextKey::AddonMediaListFolderHelp => "Optional; sonst aus dem Listennamen abgeleitet",
        TextKey::DestructiveActions => "Destruktive Aktionen",
        TextKey::ReconcileEntireSource => "Gesamte Quelle abgleichen",
        TextKey::ReconcileEntireSourceHelp => {
            "Einen vollständigen Quellenschnappschuss vergleichen, ohne die aktuelle Veröffentlichung oder die Eintragseinstellungen zu ändern."
        }
        TextKey::ReconcileQueued => "Vollständiger Quellenabgleich wurde eingereiht.",
        TextKey::ReconcileFailed => "Vollständiger Quellenabgleich fehlgeschlagen.",
        TextKey::ResetAndReload => "Zurücksetzen und neu laden",
        TextKey::ResetAddonMediaListHelp => {
            "Alle erzeugten Ergebnisse dieser Liste entfernen und anschließend einen vollständigen Import einreihen."
        }
        TextKey::SourceHost => "Quellhost",
        TextKey::ResetRemoves => "Dieses Zurücksetzen entfernt",
        TextKey::ResetRemovesItems => {
            "Importierte Einträge, Anbietermetadaten, Betreiberüberschreibungen, Episodennummerierung und Ersetzungen."
        }
        TextKey::ResetRemovesArtifacts => {
            "Erzeugte Medien, NFO-Dateien, Bilder, Untertitel, Caches und Archive."
        }
        TextKey::ResetRemovesRuntime => {
            "Aufträge, Prüfpunkte, Revisionsartefakte, Einträge der erzeugten Wiedergabeliste und projizierte Medien."
        }
        TextKey::ResetRetains => "Dieses Zurücksetzen behält bei",
        TextKey::ResetRetainsSettings => {
            "Listenname, Add-on-/Quell-/Bibliotheksbindung, Identität der erzeugten Wiedergabeliste, Aktualisierungs- und Veröffentlichungsrichtlinien, Artefaktkontingent und weitere Betreibereinstellungen."
        }
        TextKey::ResetReloadFailureWarning => {
            "Wenn das erneute Laden fehlschlägt, bleiben Kanäle und die erzeugte Wiedergabeliste leer."
        }
        TextKey::TypeListNameToConfirm => "Geben Sie zur Bestätigung den aktuellen Listennamen ein",
        TextKey::ResetQueued => "Zurücksetzen und erneutes Laden eingereiht.",
        TextKey::ResetAddonMediaListFailed => "Zurücksetzen und erneutes Laden fehlgeschlagen.",
        TextKey::ChapterInput => "Kapitel",
        TextKey::ChapterInputHelp => "Optional, eines pro Zeile: 0:00 Einführung",
        TextKey::RestoreProviderChapters => "Kapitel des Anbieters wiederherstellen",
        TextKey::ManagedByAddon => "Von einem Add-on verwaltet",
        TextKey::Trash => "Papierkorb",
        TextKey::TrashAlreadyEmpty => "Der Papierkorb ist bereits leer.",
        TextKey::TrashEmpty => "Der Papierkorb ist leer.",
        TextKey::TrashEmptyConfirm => {
            "Alle Elemente mit fehlender Datei aus der Datenbank löschen?"
        }
        TextKey::TrashEmptyConfirmHelp => {
            "Dadurch werden die derzeit fehlenden Medienelemente und ihre abhängigen Metadatenzeilen dauerhaft entfernt."
        }
        TextKey::TrashSubtitle => {
            "Elemente, deren Quelldateien derzeit auf dem Datenträger oder beim Anbieter fehlen."
        }
        TextKey::Troubleshooting => "Fehlerbehebung",
        TextKey::TunerCount => "Anzahl der Tuner",
        TextKey::HdhrDisabledByJwt => "HDHomeRun-Endpunkte sind deaktiviert, weil der JWT-Schutz für IPTV aktiviert ist. Diese Einstellungen haben keine Wirkung.",
        TextKey::TvShows => "Fernsehserien",
        TextKey::Ui => "Benutzeroberfläche",
        TextKey::UnavailableLocalMediaMessage => "{count} Elemente sind nicht verfügbar, weil ErsatzRS sie nicht auf dem Datenträger finden kann, darunter: {items}",
        TextKey::UnavailableRemoteStreamsMessage => "{count} Remote Streams sind nicht verfügbar, weil ihr Anbieter sie als nicht abspielbar meldet, darunter: {items}",
        TextKey::Unavailable => "Nicht verfügbar",
        TextKey::UseActualTimes => "Tatsächliche Zeiten verwenden",
        TextKey::UseCustomPlaybackOrder => "Benutzerdefinierte Wiedergabereihenfolge verwenden",
        TextKey::UseChaptersAsMediaItems => "Kapitel als Medienelemente verwenden",
        TextKey::UseChaptersAsMediaItemsHelp => "Einzelne Kapitel statt ganzer Dateien planen",
        TextKey::UseEmbeddedSubtitles => "Eingebettete Untertitel verwenden",
        TextKey::Uuid => "UUID",
        TextKey::UntitledItem => "Unbenanntes Element",
        TextKey::WindowsStartup => "Windows-Autostart",
        TextKey::WindowsStartupNeedsRepair => {
            "Der gespeicherte Autostart-Befehl verweist auf einen anderen Speicherort. Wenden Sie diese Einstellung an, um ihn zu reparieren."
        }
        TextKey::WindowsStartupUnavailable => {
            "Der Windows-Autostart ist nicht verfügbar, weil der mitgelieferte Launcher nicht gefunden oder die Registrierung nicht gelesen werden konnte."
        }
        TextKey::ActiveTranscodeLimit => {
            "Maximale Anzahl gleichzeitiger Streaming-Transkodierungen"
        }
        TextKey::ActiveTranscodeLimitHelp => {
            "Feste Obergrenze für FFmpeg-Streaming-Transkodierungen. Wenn alle Slots belegt sind, erhält ein neuer Zuschauer die Antwort, dass alle Streams in Benutzung sind, bis ein Slot frei wird; aktive Streams umgehen das Limit nicht."
        }
        TextKey::ActiveTranscodeSuggestionHardware => {
            "Hardware-Encoder erkannt; dies ist ein Vorschlagswert, keine automatische Änderung."
        }
        TextKey::ActiveTranscodeSuggestionSoftware => {
            "Nur Software-Kodierung; dies ist ein Vorschlagswert, keine automatische Änderung."
        }
        TextKey::Peak => "Spitze",
        TextKey::StreamsInUse => "Streams in Benutzung",
        TextKey::SuggestedTranscodeLimit => "Vorgeschlagener Wert",
        TextKey::Watermarks => "Wasserzeichen",
        TextKey::Width => "Breite",
        TextKey::WorkAheadSegmenterLimit => "Vorauslaufende HLS-Transkodierungen",
        TextKey::WorkAheadSegmenterLimitHelp => {
            "Wie viele HLS-Transkodierungs-Slots mit maximaler Geschwindigkeit vorauslaufen dürfen, um Puffer aufzubauen. Vorauslaufende Transkodierungen treten immer hinter angesehene Streams zurück."
        }
        TextKey::AccelerationKind => "Beschleunigungstyp",
        TextKey::AddFfmpegProfile => "FFmpeg-Profil hinzufügen",
        TextKey::AddProfile => "Profil hinzufügen",
        TextKey::AllowBFrames => "B-Frames zulassen",
        TextKey::AllowWatermarks => "Wasserzeichen zulassen",
        TextKey::Audio => "Audio",
        TextKey::AudioBitrate => "Audio-Bitrate (kBit/s)",
        TextKey::AudioBufferSize => "Audio-Puffergröße (kBit)",
        TextKey::AudioChannels => "Kanäle",
        TextKey::AudioFormat => "Audioformat",
        TextKey::BitDepth => "Bittiefe",
        TextKey::Copy => "Kopieren",
        TextKey::CopyBlock => "Block kopieren",
        TextKey::CopyFailed => "Kopieren fehlgeschlagen",
        TextKey::CopyNameSuffix => "Kopie",
        TextKey::CopyProfile => "Profil kopieren",
        TextKey::CopyTemplate => "Vorlage kopieren",
        TextKey::Crop => "Zuschneiden",
        TextKey::DeinterlaceVideo => "Video deinterlacen",
        TextKey::Edit => "Bearbeiten",
        TextKey::EditBlock => "Block bearbeiten",
        TextKey::EditCollection => "Sammlung bearbeiten",
        TextKey::EditFfmpegProfile => "FFmpeg-Profil bearbeiten",
        TextKey::EditMultiCollection => "Mehrsammlung bearbeiten",
        TextKey::EditRerunCollection => "Wiederholungssammlung bearbeiten",
        TextKey::EditTemplate => "Vorlage bearbeiten",
        TextKey::Expression => "Ausdruck",
        TextKey::Fallback => "Fallback",
        TextKey::FillerKind => "Filler-Typ",
        TextKey::General => "Allgemein",
        TextKey::HardwareAcceleration => "Hardwarebeschleunigung",
        TextKey::HardwareIfPossible => "Hardware, wenn möglich",
        TextKey::ProfileDeleteDefault => "Das Standard-FFmpeg-Profil kann nicht gelöscht werden.",
        TextKey::ProfileDeleteInUse => "Dieses FFmpeg-Profil wird von einem Kanal verwendet und kann nicht gelöscht werden.",
        TextKey::ProfileDeleteFailed => "Das FFmpeg-Profil konnte nicht gelöscht werden.",
        TextKey::NewProfileName => "Neuer Profilname",
        TextKey::NoneOption => "(Keine)",
        TextKey::NumHelp => "die Nummer des Mid-Roll-Punkts, beginnend bei 1",
        TextKey::OptionalDisplayTitle => "Optionaler Anzeigetitel …",
        TextKey::TranscodeAudio => "Audio transkodieren",
        TextKey::NormalizeColors => "Farben normalisieren",
        TextKey::NormalizeFramerate => "Bildrate normalisieren",
        TextKey::NormalizeLoudness => "Lautheit normalisieren",
        TextKey::TranscodeVideo => "Video transkodieren",
        TextKey::Off => "Aus",
        TextKey::Pad => "Auffüllen",
        TextKey::PadNearestFifteenMinutes => "15 (:00, :15, :30, :45)",
        TextKey::PadNearestFiveMinutes => "5 (:00, :05, :10, :15, :20 usw.)",
        TextKey::PadNearestTenMinutes => "10 (:00, :10, :20, :30, :40, :50)",
        TextKey::PadNearestThirtyMinutes => "30 (:00, :30)",
        TextKey::PadMode => "Auffüllmodus",
        TextKey::PadToNearestMinute => "Auf nächste Minute auffüllen",
        TextKey::PointHelp => "die Position des Mid-Roll-Punkts, in Sekunden",
        TextKey::ProfileCopied => "Profil kopiert.",
        TextKey::ProfileNotFound => "Profil nicht gefunden.",
        TextKey::QsvExtraHardwareFrames => "Zusätzliche QSV-Hardware-Frames",
        TextKey::Resolution => "Auflösung",
        TextKey::SampleRateKhz => "Abtastrate (kHz)",
        TextKey::SaveDecoTemplate => "Deko-Vorlage speichern",
        TextKey::SaveProfile => "Profil speichern",
        TextKey::SaveFillerPreset => "Filler-Vorlage speichern",
        TextKey::SaveRerunCollection => "Wiederholungssammlung speichern",
        TextKey::SaveSequentialSchedule => "Sequenziellen Sendeplan speichern",
        TextKey::SaveTemplate => "Vorlage speichern",
        TextKey::SaveBlock => "Block speichern",
        TextKey::ScaleAndPad => "Skalieren und auffüllen",
        TextKey::ScalingBehavior => "Skalierungsverhalten",
        TextKey::Software => "Software",
        TextKey::Stretch => "Strecken",
        TextKey::TargetLoudnessLufs => "Ziel-Lautheit (LUFS)",
        TextKey::ThreadCountAuto => "Thread-Anzahl (0 = automatisch)",
        TextKey::NativeEngineOnlyHelp => "Nur die native Engine von ErsatzRS verwendet diese Einstellung. ErsatzTV Next ignoriert sie.",
        TextKey::OutputDynamicRange => "Dynamikumfang der Ausgabe",
        TextKey::OutputDynamicRangeCopyHint => {
            "Das Video wird kopiert, daher behält die Ausgabe den Dynamikumfang der Quelle."
        }
        TextKey::HdrStaticMetadataHint => {
            "Leer lassen, um die dokumentierten HDR10-Standardwerte zu verwenden. Diese gelten nur, wenn die Quelle keine eigenen HDR-Metadaten mitbringt; das Mastering-Display und die Lichtwerte der Quelle haben immer Vorrang."
        }
        TextKey::HdrMasteringDisplay => "Mastering-Display",
        TextKey::HdrMaxCll => "MaxCLL (cd/m²)",
        TextKey::HdrMaxFall => "MaxFALL (cd/m²)",
        TextKey::TonemapAlgorithm => "Tonemap-Algorithmus",
        TextKey::VaapiDevicePath => "VAAPI-Gerätepfad",
        TextKey::VaapiDisplay => "VAAPI-Display",
        TextKey::VaapiDriver => "VAAPI-Treiber",
        TextKey::Video => "Video",
        TextKey::VideoBitrate => "Video-Bitrate (kBit/s)",
        TextKey::VideoBufferSize => "Video-Puffergröße (kBit)",
        TextKey::VideoFormat => "Videoformat",
        TextKey::VideoPreset => "Video-Voreinstellung",
        TextKey::VideoTune => "Video-Tune",
        TextKey::VideoProfile => "Videoprofil",
        TextKey::ViewCollection => "Sammlung anzeigen",
        TextKey::ViewPlaylist => "Wiedergabeliste anzeigen",
        TextKey::AddAll => "Alle hinzufügen",
        TextKey::AddAllToCollection => "Alle zur Sammlung hinzufügen",
        TextKey::AddAllToPlaylist => "Alle zur Wiedergabeliste hinzufügen",
        TextKey::AddedItemToPlaylist => "Element zur Wiedergabeliste hinzugefügt.",
        TextKey::AddedArtistToSchedule => "Interpret zum Sendeplan hinzugefügt",
        TextKey::AddedMovieToSchedule => "Film zum Sendeplan hinzugefügt",
        TextKey::AddedSeasonToSchedule => "Staffel zum Sendeplan hinzugefügt",
        TextKey::AddedShowToSchedule => "Serie zum Sendeplan hinzugefügt",
        TextKey::AddingItem => "Element wird hinzugefügt …",
        TextKey::AddingItemToPlaylist => "Element wird zur Wiedergabeliste hinzugefügt …",
        TextKey::Artists => "Interpreten",
        TextKey::ClearSelection => "Auswahl aufheben",
        TextKey::Collection => "Sammlung",
        TextKey::CollectionNameRequired => "Der Sammlungsname ist erforderlich",
        TextKey::CollectionNotFound => "Sammlung nicht gefunden",
        TextKey::CollectionRenamed => "Sammlung umbenannt.",
        TextKey::EditAddonMediaListProperties => "Eigenschaften der add-on-Liste bearbeiten",
        TextKey::EnterAddonMediaListUrl => "add-on-Listen-URL eingeben",
        TextKey::EnterSearchQuery => "Geben Sie eine Suchabfrage ein.",
        TextKey::Episodes => "Folgen",
        TextKey::LoadingCollections => "Sammlungen werden geladen …",
        TextKey::LoadingPlaylistGroups => "Wiedergabelistengruppen werden geladen …",
        TextKey::LoadingPlaylists => "Wiedergabelisten werden geladen …",
        TextKey::GeneratePlaylist => "Wiedergabeliste in Originalreihenfolge erstellen",
        TextKey::GeneratePlaylistFromSortedAddonMediaListItems => {
            "Die ursprüngliche Anbieterreihenfolge nach jeder Veröffentlichung beibehalten"
        }
        TextKey::MusicVideos => "Musikvideos",
        TextKey::NewCollectionName => "Neuer Sammlungsname",
        TextKey::NoMediaMatched => "Keine Medien gefunden.",
        TextKey::NoItemsToAdd => "Keine Elemente zum Hinzufügen.",
        TextKey::NoItemsToPreview => "Keine Elemente für die Vorschau.",
        TextKey::NoPlaylistsInGroup => "Keine Wiedergabelisten in dieser Gruppe.",
        TextKey::PleaseChoosePlaylist => "Bitte wählen Sie eine Wiedergabeliste.",
        TextKey::RequestFailed => "Anfrage fehlgeschlagen",
        TextKey::Released => "Veröffentlicht",
        TextKey::SaveAs => "Speichern unter",
        TextKey::Seasons => "Staffeln",
        TextKey::SeeAll => "Alle anzeigen >>",
        TextKey::SelectAll => "Alle auswählen",
        TextKey::SelectGroup => "(Gruppe auswählen)",
        TextKey::SelectPlaylist => "(Wiedergabeliste auswählen)",
        TextKey::SelectSearchCollectionHelp => {
            "Wählen Sie die Sammlung, zu der die Suchergebnisse hinzugefügt werden sollen."
        }
        TextKey::SelectSearchPlaylistHelp => {
            "Wählen Sie die Wiedergabeliste, zu der die Suchergebnisse hinzugefügt werden sollen."
        }
        TextKey::Selected => "Ausgewählt",
        TextKey::SelectCollectionToAddThisMovie => {
            "Wählen Sie eine Sammlung, um diesen Film hinzuzufügen."
        }
        TextKey::SelectPlaylistToAddThisMovie => {
            "Wählen Sie eine Wiedergabeliste, um diesen Film hinzuzufügen."
        }
        TextKey::Add => "Hinzufügen",
        TextKey::AddLibrary => "Bibliothek hinzufügen",
        TextKey::Address => "Adresse",
        TextKey::AddressRequired => "Die Adresse ist erforderlich.",
        TextKey::ApiKey => "API-Schlüssel",
        TextKey::ApiKeyRequired => "Der API-Schlüssel ist erforderlich.",
        TextKey::AuthorizedRedirecting => "Autorisiert – Weiterleitung …",
        TextKey::CheckingEveryTwoSeconds => "Prüfung alle 2 Sekunden …",
        TextKey::ClickAuthorizePlex => "Klicken Sie hier, um sich auf Plex.tv zu autorisieren",
        TextKey::ClickPlayoutItemHelp => {
            "Klicken Sie auf ein Playout-Element, um Titel, Zeiten und Planungskontext einzusehen."
        }
        TextKey::ConfigureConnection => "Verbindung konfigurieren",
        TextKey::Connection => "Verbindung",
        TextKey::DeepScanCollections => "Sammlungen tiefgehend scannen",
        TextKey::DeepScanLibrary => "Bibliothek tiefgehend scannen",
        TextKey::DeepScanConfirmationQuestion => "Möchten Sie {name} wirklich tief scannen?",
        TextKey::DeepScanConfirmationWarning => "Ein tiefer Scan aktualisiert alles, auch scheinbar unveränderte Elemente, und kann wesentlich länger als ein normaler Scan dauern.",
        TextKey::LibraryScanJavascriptRequired => "Zum Scannen von Bibliotheken und externen Sammlungen ist JavaScript erforderlich. Aktivieren Sie JavaScript, um einen tiefen Scan zu prüfen und zu bestätigen.",
        TextKey::DeepScan => "Tiefer Scan",
        TextKey::EditLibrary => "Bibliothek bearbeiten",
        TextKey::EditPlaylist => "Wiedergabeliste bearbeiten",
        TextKey::EnterOnePathPerLine => "Geben Sie einen Pfad pro Zeile ein.",
        TextKey::Error => "Fehler",
        TextKey::ExternalCollections => "Externe Sammlungen",
        TextKey::Information => "Information",
        TextKey::Kind => "Typ",
        TextKey::Library => "Bibliothek",
        TextKey::LibraryKind => "Bibliothekstyp",
        TextKey::LibraryPaths => "Bibliothekspfade",
        TextKey::FolderPreview => "TV-Ordner prüfen",
        TextKey::FolderPreviewHelp => "Für TV-Serien: vor dem Speichern bis zu acht absolute Ordnerpfade prüfen. Die Vorschau liest höchstens 10.000 Einträge, 32 Ordnerebenen und zwei Sekunden Dateisystemarbeit; Dateien und Bibliothekseinstellungen bleiben unverändert.",
        TextKey::FolderPreviewCandidates => "Dies sind gefundene Kandidaten, keine dekodierten Medien. Pro Ordner werden bis zu 40 Diagnosebeispiele angezeigt. Erwartete Struktur: Bibliothek/Serie/Staffel/Dateien; Paketordner werden nicht automatisch durchsucht.",
        TextKey::FolderPreviewPartial => "Unvollständige Vorschau: ein Grenzwert wurde erreicht. Die Zahlen erfassen nicht den gesamten Ordner.",
        TextKey::FolderPreviewUnavailable => "Vorschau nicht verfügbar. TV-Serien wählen und ein bis acht absolute Pfade eingeben (je höchstens 4.096 Bytes). Erreichbarkeit der Ordner prüfen und erneut versuchen; eine andere Vorschau läuft möglicherweise noch.",
        TextKey::FolderIgnoredExtension => "Ignorierte Dateiendung",
        TextKey::FolderUnsupportedNesting => "Nicht unterstützte Verschachtelung oder falsch abgelegtes Video",
        TextKey::FolderEmptyShow => "Serien ohne gefundene Medien",
        TextKey::FolderUnreadable => "Nicht lesbarer Eintrag",
        TextKey::FolderRepeated => "Wiederholter Ordner oder Verknüpfungsschleife",
        TextKey::FolderDepthLimit => "Maximale Ordnertiefe",
        TextKey::FolderInspected => "Geprüfte Einträge",
        TextKey::FolderExtras => "Gefundene Zusatzvideos",
        TextKey::FolderHidden => "Versteckte Einträge",
        TextKey::LocalLibraryPathAbsolute => "Pfad [{path}] muss ein absoluter Pfad sein.",
        TextKey::LocalLibraryPathExists => "Pfad [{path}] muss ein vorhandener Ordner sein.",
        TextKey::LocalLibraryPathConflict => "Pfad [{path}] überschneidet sich mit einem Pfad einer anderen Bibliothek.",
        TextKey::LocalLibraries => "Lokale Bibliotheken",
        TextKey::LocalPath => "Lokaler Pfad",
        TextKey::MediaKind => "Medientyp",
        TextKey::NoLibrariesConfigured => {
            "Keine Bibliotheken konfiguriert. Fügen Sie zuerst eine Medienquelle hinzu."
        }
        TextKey::NoSchedulingContext => "Kein Planungskontext",
        TextKey::NoPlayoutItemsInWindow => "Keine Playout-Elemente in diesem 24-Stunden-Fenster",
        TextKey::NoPlayoutsFound => "Keine Playouts gefunden.",
        TextKey::NoPathReplacements => "Keine Pfadersetzungen.",
        TextKey::Paths => "Pfade",
        TextKey::PathReplacements => "Pfadersetzungen",
        TextKey::PlexSignInHelp => {
            "Melden Sie sich bei Plex an, um eine Medienquelle hinzuzufügen."
        }
        TextKey::Remote => "Remote",
        TextKey::RemotePath => "Remote-Pfad",
        TextKey::Remove => "Entfernen",
        TextKey::RemoveBlockFromTemplate => "Block aus Vorlage entfernen",
        TextKey::RemoveContent => "Inhalt entfernen",
        TextKey::Retry => "Erneut versuchen",
        TextKey::Saved => "Gespeichert.",
        TextKey::Saving => "Wird gespeichert …",
        TextKey::QuickScan => "Schnellscan",
        TextKey::ScanCollections => "Sammlungen scannen",
        TextKey::ScanFailed => "Scan fehlgeschlagen",
        TextKey::ScanLibrary => "Bibliothek scannen",
        TextKey::ScanQueued => "Scan in Warteschlange",
        TextKey::Scanning => "Wird gescannt …",
        TextKey::Server => "Server",
        TextKey::ServerAddress => "Serveradresse",
        TextKey::PlexRefreshFailed => "Plex-Bibliotheken konnten nicht aktualisiert werden. Prüfe die Serververbindung und versuche es erneut.",
        TextKey::ReauthenticatePlex => "Erneut bei Plex anmelden",
        TextKey::PlexMissingServer => "Server fehlt bei Plex",
        TextKey::PlexMissingHelp => "Dieser Server ist nicht mehr in deinem Plex-Konto aufgeführt. Beanspruche ihn erneut in Plex oder entferne ihn hier. Inhalte bleiben erhalten und Scans sind pausiert.",
        TextKey::PlexSignedOutHelp => "Melde dich erneut an, um den Plex-Zugriff wiederherzustellen. Medienquellen und synchronisierte Inhalte bleiben erhalten.",
        TextKey::PlexRemoveServerHelp => "Beim Entfernen dieses Servers werden seine Bibliotheken und synchronisierten Inhalte aus ErsatzRS gelöscht. Dateien auf dem Plex-Server werden nicht gelöscht.",
        TextKey::PlexSignInFailed => "Die Plex-Anmeldung ist fehlgeschlagen oder abgelaufen. Versuche es erneut.",
        TextKey::SignInToPlex => "Bei Plex anmelden",
        TextKey::SignOutOfPlex => "Von Plex abmelden",
        TextKey::Sync => "Synchronisieren",
        TextKey::WaitingForPlexAuthorization => "Warten auf die Plex-Autorisierung.",
        TextKey::AddChannel => "Kanal hinzufügen",
        TextKey::Any => "Beliebig",
        TextKey::BlockPlayoutTroubleshooting => "Fehlerbehebung für Block-Playout",
        TextKey::BlockPlayoutHistory => "Verlauf des Block-Playouts",
        TextKey::Categories => "Kategorien",
        TextKey::CategoriesHelp => "Kommagetrennte Liste von Kategorien",
        TextKey::Channel => "Kanal",
        TextKey::ChannelCouldNotBeSaved => {
            "Der Kanal konnte nicht gespeichert werden. Überprüfen Sie die Formularwerte und versuchen Sie es erneut."
        }
        TextKey::ChannelDisabled => "Kanal deaktiviert",
        TextKey::ChannelHiddenFromEpg => "Kanal im EPG ausgeblendet",
        TextKey::ChannelLogo => "Kanallogo",
        TextKey::ChannelLogoPreview => "Vorschau des Kanallogos",
        TextKey::ChannelLogoUploadFailed => {
            "Der Upload des Kanallogos ist fehlgeschlagen. Überprüfen Sie die Datei und versuchen Sie es erneut."
        }
        TextKey::Check => "Prüfen",
        TextKey::Continuous => "Fortlaufend",
        TextKey::DateAndTime => "Datum und Uhrzeit",
        TextKey::DefaultOption => "Standard",
        TextKey::DefaultSchedule => "Standard-Sendeplan",
        TextKey::DeleteChannel => "Kanal löschen",
        TextKey::DownloadMediaSample => "Medienprobe herunterladen",
        TextKey::DownloadResults => "Ergebnisse herunterladen",
        TextKey::DisableWatermarks => "Wasserzeichen deaktivieren",
        TextKey::EditChannel => "Kanal bearbeiten",
        TextKey::EditChannelNumbers => "Kanalnummern bearbeiten",
        TextKey::EditNumbers => "Nummern bearbeiten",
        TextKey::ExternalLogoUrl => "Externe Logo-URL",
        TextKey::Fail => "Nicht bestanden",
        TextKey::FallbackFiller => "Fallback-Filler",
        TextKey::FfmpegProfile => "FFmpeg-Profil",
        TextKey::FirstPage => "Erste Seite",
        TextKey::FirstRunPlaybackOrder => "Wiedergabereihenfolge der Erstausstrahlung",
        TextKey::Forced => "Erzwungen",
        TextKey::GenerateSubtitles => "Untertitel erzeugen",
        TextKey::Generated => "Erzeugt",
        TextKey::Details => "Details",
        TextKey::Full => "Vollständig",
        TextKey::Group => "Gruppe",
        TextKey::FfmpegRuntime => "FFmpeg-Laufzeitumgebung",
        TextKey::FfmpegRuntimeDownloaded => "FFmpeg-Laufzeitumgebung heruntergeladen",
        TextKey::FfmpegRuntimeLatestRelease => "Neuestes Release",
        TextKey::FfmpegRuntimeRelease => "Installiertes Release",
        TextKey::FfmpegRuntimeSource => "Quelle",
        TextKey::FfmpegRuntimeSourceDownloaded => "Heruntergeladen",
        TextKey::FfmpegRuntimeSourceManaged => "Verwaltet",
        TextKey::FfmpegRuntimeSourceOverride => "Überschreibung",
        TextKey::FfmpegRuntimeSourcePath => "PATH",
        TextKey::FfmpegRuntimeSourceUnknown => "Unbekannt",
        TextKey::FfmpegRuntimeUpdateAvailable => "Update verfügbar",
        TextKey::FfmpegRuntimeUpdateCheckFailed => "Update-Prüfung fehlgeschlagen",
        TextKey::FfmpegRuntimeUpdateFailed => "Update der FFmpeg-Laufzeitumgebung fehlgeschlagen",
        TextKey::FfmpegRuntimeUpToDate => "Aktuell",
        TextKey::FfmpegRuntimeUpdated => "FFmpeg-Laufzeitumgebung aktualisiert",
        TextKey::FfmpegRuntimeVersion => "Version",
        TextKey::HealthNoPersistedRun => "Derzeit sind keine Systemprüfungen anwendbar.",
        TextKey::HealthPersistedRunHelp => {
            "Ergebnisse werden fünf Minuten zwischengespeichert. Mit Aktualisieren können Sie sofort erneut prüfen. Diese Seite wird auch aktualisiert, wenn Medienscans den Elementstatus ändern."
        }
        TextKey::HistoryDetails => "Verlaufsdetails",
        TextKey::HlsDirect => "HLS direkt",
        TextKey::HlsSegmenter => "HLS-Segmenter",
        TextKey::Hours => "Stunden",
        TextKey::IdleBehavior => "Leerlaufverhalten",
        TextKey::IdleBehaviorHelp => {
            "Legt fest, wie sich der Transkodierungsprozess verhält, nachdem sich alle Clients getrennt haben"
        }
        TextKey::Info => "Info",
        TextKey::Import => "Importieren",
        TextKey::IsEnabled => "Ist aktiviert",
        TextKey::Json => "JSON",
        TextKey::KeepRunning => "Weiterlaufen lassen",
        TextKey::Key => "Schlüssel",
        TextKey::LastMidFillerHelp => "Sekunden seit dem letzten Mid-Roll-Filler",
        TextKey::LastPage => "Letzte Seite",
        TextKey::Level => "Ebene",
        TextKey::LoadingMediaInfo => "Medieninfo wird geladen …",
        TextKey::Logs => "Protokolle",
        TextKey::PlaybackSpeed => "Geschwindigkeit",
        TextKey::Logo => "Logo",
        TextKey::MediaItem => "Medienelement",
        TextKey::Message => "Meldung",
        TextKey::MissingMediaItemQueryParameter => "Fehlender Abfrageparameter mediaItem",
        TextKey::Mirror => "Spiegelung",
        TextKey::MirrorOptionsHelp => {
            "Nur verfügbar, wenn die Playout-Quelle auf Spiegelung gesetzt ist."
        }
        TextKey::MirrorSourceChannel => "Quellkanal der Spiegelung",
        TextKey::MusicVideoCreditsMode => "Modus für Musikvideo-Credits",
        TextKey::MusicVideoCreditsTemplate => "Vorlage für Musikvideo-Credits",
        TextKey::NoMatchingRecordsFound => "Keine passenden Datensätze gefunden",
        TextKey::NoFfmpegProfiles => "Keine FFmpeg-Profile gefunden. Erstellen Sie zuerst eines.",
        TextKey::NoBlockPlayoutSelected => {
            "Wählen Sie ein Block-Playout, um dessen Blöcke einzusehen."
        }
        TextKey::NoBlocks => "Keine Blöcke gefunden.",
        TextKey::NoHistory => "Kein Verlauf gefunden.",
        TextKey::NoMessages => "Keine Meldungen.",
        TextKey::NvidiaCapabilitiesNotCollected => "NVIDIA-Fähigkeiten wurden noch nicht erfasst.",
        TextKey::Number => "Nummer",
        TextKey::OnDemand => "Bei Bedarf",
        TextKey::MatchedPointsHelp => {
            "Anzahl der Mid-Roll-Punkte, die bereits auf den Ausdruck zutreffen"
        }
        TextKey::Pass => "Bestanden",
        TextKey::ParityDebt => "ParityDebt",
        TextKey::PlaybackTroubleshooting => "Fehlerbehebung bei der Wiedergabe",
        TextKey::Play => "Abspielen",
        TextKey::PlayoutTimeline => "Playout-Zeitleiste",
        TextKey::PlayoutMode => "Playout-Modus",
        TextKey::PlayoutModeHelp => "Legt den Ablauf des Kanal-Playouts fest",
        TextKey::PlayoutOffset => "Playout-Versatz",
        TextKey::PlayoutSource => "Playout-Quelle",
        TextKey::PlayoutSourceHelp => {
            "Legt die Inhaltsquelle des Kanals fest. Dies kann nicht geändert werden, wenn der Kanal bereits ein Playout hat."
        }
        TextKey::Preview => "Vorschau",
        TextKey::PreviewBlockPlayout => "Vorschau des Block-Playouts",
        TextKey::PreviewChannel => "Kanalvorschau",
        TextKey::ChannelPreview => "Kanalvorschau",
        TextKey::ReloadStream => "Stream neu laden",
        TextKey::StopPreview => "Vorschau beenden",
        TextKey::OpenFullPlayer => "Vollständigen Player öffnen",
        TextKey::BackToChannels => "Zurück zu den Kanälen",
        TextKey::NowPlaying => "Läuft jetzt",
        TextKey::UpNext => "Als Nächstes",
        TextKey::ProgrammeGuide => "Programmführer",
        TextKey::EpgPreview => "EPG-Vorschau",
        TextKey::EpgPreviewDescription => {
            "Durchsuchen Sie den veröffentlichten Programmführer mit zwei Stunden Verlauf und 24 Stunden Vorausschau."
        }
        TextKey::EpgZoom => "Zoom",
        TextKey::EpgNow => "Jetzt",
        TextKey::NoEpgChannels => "Im EPG sind keine aktivierten Kanäle sichtbar.",
        TextKey::ScheduleGap => "Sendeplanlücke",
        TextKey::Filler => "Filler",
        TextKey::NoProgramming => "In diesem Zeitfenster ist kein Programm geplant.",
        TextKey::ChannelDisabledPreview => "Dieser Kanal ist deaktiviert.",
        TextKey::ChannelNotAiring => "Dieser Kanal sendet derzeit nicht.",
        TextKey::ChannelNoPlayout => "Dieser Kanal hat kein Playout.",
        TextKey::PreviewIdle => "Die Vorschau startet automatisch stummgeschaltet.",
        TextKey::PreviewConnecting => "Verbindung zum Live-Stream wird hergestellt …",
        TextKey::PreviewErrorSlate => "Fehlerbild: Das Programm kann nicht wiedergegeben werden.",
        TextKey::PreviewOutputUnknown => "Der Ausgabestatus des Servers ist nicht verfügbar.",
        TextKey::PreviewLive => "Die Live-Wiedergabe läuft.",
        TextKey::PreviewPaused => "Vorschau pausiert.",
        TextKey::PreviewRecovering => "Wiedergabe wird wiederhergestellt …",
        TextKey::PreviewStopped => "Vorschau beendet.",
        TextKey::PreviewUnsupported => "Dieser Browser kann kein HLS-Video wiedergeben.",
        TextKey::PreviewAuthorizationFailed => "Die Autorisierung der Vorschau ist fehlgeschlagen.",
        TextKey::PreviewQueued => "Vorschau eingereiht",
        TextKey::PreviewRunning => "Vorschau wird gerendert",
        TextKey::PreviewCompleted => "Rendering abgeschlossen",
        TextKey::PreviewCancelling => "Vorschau wird gestoppt; Bereinigung läuft",
        TextKey::PreviewProgrammeSeek => "Sekunden ab Programmbeginn",
        TextKey::PreviewProgrammeSeekHelp => "Leer lassen, um zur gewählten UTC-Zeit einzusteigen. Die Suche bleibt im gewählten Programm.",
        TextKey::PreviewStartHelp => "Der Zeitpunkt im Sendeplan des Kanals für die Vorschau. Datum und Uhrzeit in UTC sind erforderlich.",
        TextKey::PreviewStartRequired => "Datum und Uhrzeit sind erforderlich.",
        TextKey::PreviewStartInvalid => "Wählen Sie ein gültiges Datum und eine gültige Uhrzeit in UTC.",
        TextKey::PreviewTimestamp => "Vorschauzeitpunkt (UTC):",
        TextKey::PreviewAlreadyRunning => "Eine Diagnosevorschau läuft bereits. Warten Sie auf den Abschluss, bevor Sie eine neue starten.",
        TextKey::PreviewCleanupUnconfirmed => "Die Bereinigung der Vorschau konnte nicht bestätigt werden. Starten Sie den Server vor einem erneuten Versuch neu.",
        TextKey::PreviewConfigurationInvalid => "Der Streamingmodus des Kanals ist ungültig. Speichern Sie einen unterstützten Modus im Kanaleditor und versuchen Sie es erneut.",
        TextKey::PreviewStartupFailed => "Die Vorschau konnte nicht starten. Prüfen Sie das Serverprotokoll auf den Startfehler sowie die Medien und das FFmpeg-Profil.",
        TextKey::PreviewUnavailable => "Der Live-Stream ist nicht verfügbar.",
        TextKey::PreviewCapacityReached => "Alle Transkodierungs-Slots sind derzeit belegt.",
        TextKey::PreviewNetworkError => "Die Vorschau hat ihre Netzwerkverbindung verloren.",
        TextKey::PreviewMediaError => {
            "Der Browser konnte die konfigurierte Ausgabe dieses Kanals nicht dekodieren."
        }
        TextKey::PreviewDerivativeNote => {
            "Die Admin-Vorschau verwendet ein browserkompatibles H.264/AAC-Derivat; die konfigurierte Kanalausgabe bleibt unverändert."
        }
        TextKey::JavascriptRequiredPlayback => {
            "Für die interaktive Wiedergabe ist JavaScript erforderlich."
        }
        TextKey::ConfirmStopTranscodeSession => {
            "Die gemeinsame Transkodierungssitzung für diesen Kanal beenden?"
        }
        TextKey::PreviewPlaylistPlayout => "Vorschau des Wiedergabelisten-Playouts",
        TextKey::PreferredAudioTitle => "Bevorzugter Audiotitel",
        TextKey::PreferredSubtitleLanguage => "Bevorzugte Untertitelsprache",
        TextKey::QsvCapabilitiesNotCollected => "QSV-Fähigkeiten wurden noch nicht erfasst.",
        TextKey::RemoveStoredLogo => "Gespeichertes Logo entfernen",
        TextKey::SaveChannel => "Kanal speichern",
        TextKey::SaveChannels => "Kanäle speichern",
        TextKey::Second => "Sekunde",
        TextKey::Seconds => "Sekunden",
        TextKey::SelectProfile => "– Profil auswählen –",
        TextKey::SequentialSchedule => "Sequenzieller Sendeplan",
        TextKey::SequentialScheduleValidation => "Validierung des sequenziellen Sendeplans",
        TextKey::SeekSeconds => "Suchposition (Sekunden)",
        TextKey::Schema => "Schema",
        TextKey::ShowDisabled => "Deaktivierte anzeigen",
        TextKey::ShowInEpg => "Im EPG anzeigen",
        TextKey::SlugSeconds => "Slug-Sekunden",
        TextKey::SlugSecondsHelp => {
            "Sekunden schwarzes Video und stille Tonspur, die zwischen jedem Playout-Element eingefügt werden"
        }
        TextKey::Song => "Lied",
        TextKey::SongVideoMode => "Video-Modus für Lieder",
        TextKey::SortById => "Nach Kennung sortieren",
        TextKey::SortByLevel => "Nach Ebene sortieren",
        TextKey::SortByName => "Nach Name sortieren",
        TextKey::SortByTimestamp => "Nach Zeitstempel sortieren",
        TextKey::StopOnDisconnect => "Bei Trennung beenden",
        TextKey::StopScheduling => "Planung beenden",
        TextKey::StopSchedulingBlockItems => "Planung von Block-Elementen beenden",
        TextKey::StopTranscodeSession => "Transkodierungssitzung beenden",
        TextKey::StartFromBeginning => "Von vorn beginnen",
        TextKey::StreamSelector => "Stream-Selektor",
        TextKey::StreamSelectorMode => "Stream-Selektor-Modus",
        TextKey::InvalidNextEngineTextSubtitleMode => "Ungültiger Textuntertitelmodus der Next-Engine",
        TextKey::SourceTeletextDelivery => "Untertitel aus Quell-Teletext",
        TextKey::SourceTeletextDeliveryHelp => "Wählen Sie die Ausgabe ausgewählter Untertitel aus Quell-Teletext. DVB-Untertitelspuren erfordern derzeit native MPEG-TS- oder Hybrid-Ausgabe. Untertitelauswahl und selbst erstellte Teletextseiten bleiben unverändert.",
        TextKey::SourceTeletextDeliveryBurn => "In das Video einbrennen",
        TextKey::SourceTeletextDeliveryDvb => "DVB-Untertitelspur",
        TextKey::InvalidSourceTeletextDelivery => "Ungültige Ausgabe von Quell-Teletext-Untertiteln",
        TextKey::SourceTeletextDeliveryUnsupported => "DVB-Untertitel aus Quell-Teletext erfordern native MPEG-TS- oder Hybrid-Ausgabe",
        TextKey::TeletextCarriageReady => "Die Voraussetzungen für die DVB-Untertitelausgabe sind vorhanden. Die Untertitelaufbereitung je Quelle und die Client-Anzeige müssen noch geprüft werden.",
        TextKey::TeletextCarriageMissing => "Die DVB-Untertitelausgabe ist konfiguriert, aber die erforderliche FFmpeg-Unterstützung fehlt. Wählen Sie eine kompatible Laufzeit; die normale Wiedergabe bleibt verfügbar.",
        TextKey::TeletextCarriageInvalid => "Ein Kanal hat ungültige Einstellungen für die Quell-Teletext-Ausgabe. Prüfen Sie die gewählte Untertitelausgabe; die normale Wiedergabe bleibt verfügbar.",
        TextKey::TeletextCarriageOutput => "Ein Kanal verwendet DVB-Untertitel mit einer nicht unterstützten Engine oder Ausgabe. Wählen Sie natives MPEG-TS oder ändern Sie die Untertitelausgabe; die normale Wiedergabe bleibt verfügbar.",
        TextKey::NextEngineTextSubtitleModeHelp => "Konvertierte Textuntertitel sind nur über HLS verfügbar. Wählen Sie Einbrennen für Untertitel in MPEG-TS.",
        TextKey::NextEngineTextSubtitleMode => "Textuntertitelmodus der Next-Engine",
        TextKey::SubtitleBurn => "Einbrennen",
        TextKey::SubtitleConvertIfPossible => "Wenn möglich konvertieren",
        TextKey::PreviewCaptions => "Untertitel",
        TextKey::StreamingEngine => "Streaming-Engine",
        TextKey::InvalidStreamingEngine => "Ungültige Streaming-Engine",
        TextKey::StreamingMode => "Streaming-Modus",
        TextKey::Styles => "Stile",
        TextKey::Subtitle => "Untertitel",
        TextKey::SubtitleMode => "Untertitelmodus",
        TextKey::SubtitleModeHelp => {
            "„Erzwungen“ zeigt nur Spuren mit erzwungener Handlung. Um Untertitel in Ihrer bevorzugten Sprache immer anzuzeigen, wählen Sie „Beliebig“."
        }
        TextKey::Timestamp => "Zeitstempel",
        TextKey::Timeline => "Zeitleiste",
        TextKey::Target => "Ziel",
        TextKey::Tags => "Tags",
        TextKey::Tools => "Werkzeuge",
        TextKey::TranscodeMode => "Transkodierungsmodus",
        TextKey::TranscodeModeHelp => "Legt fest, wann der Transkodierungsprozess aktiv ist",
        TextKey::TroubleshootPlayback => "Wiedergabe untersuchen",
        TextKey::TroubleshootChannel => "Kanal untersuchen",
        TextKey::TroubleshootingReports => "Fehlerberichte",
        TextKey::MpegTsTroubleshooting => "MPEG-TS-Fehlerbehebung",
        TextKey::InternalMpegTs => "Internes MPEG-TS",
        TextKey::PublicMpegTs => "Öffentliches MPEG-TS",
        TextKey::RetainRawSample => "Rohprobe aufbewahren",
        TextKey::TsdDuckVersion => "TSDuck-Version",
        TextKey::Report => "Bericht",
        TextKey::RunId => "Lauf-ID",
        TextKey::Status => "Status",
        TextKey::Summary => "Zusammenfassung",
        TextKey::SampleBytes => "Probenbytes",
        TextKey::RawSampleArchived => "Rohprobe archiviert",
        TextKey::ArchivePath => "Archivpfad",
        TextKey::SampleUrl => "Proben-URL",
        TextKey::StabilityMeter => "Stabilitätsanzeige",
        TextKey::Stable => "Stabil",
        TextKey::Packets => "Pakete",
        TextKey::Services => "Dienste",
        TextKey::Pids => "PIDs",
        TextKey::ContinuityFindings => "Kontinuitätsbefunde",
        TextKey::PcrFindings => "PCR-Befunde",
        TextKey::Bitrate => "Bitrate",
        TextKey::AnalysisJson => "Analyse-JSON",
        TextKey::StandardOutput => "Standardausgabe",
        TextKey::StandardError => "Standardfehler",
        TextKey::NotAvailable => "Nicht verfügbar",
        TextKey::NotApplicable => "Nicht zutreffend",
        TextKey::Completed => "Abgeschlossen",
        TextKey::Failed => "Fehlgeschlagen",
        TextKey::TroubleshootingPlaybackStartDebt => {
            "Die Wiedergabevorschau für Medienelemente und Kanäle schreibt jetzt FFmpeg-Berichte, Protokolle, media_info.json, troubleshooting_info.json, ausgewähltes Stream-Selektor-YAML und Fähigkeitsdateien, wendet Stream-Selektor-Regeln an und leitet auf /iptv/session/.troubleshooting/live.m3u8 weiter."
        }
        TextKey::UnableToLoadMediaInfo => "Medieninfo konnte nicht geladen werden",
        TextKey::UnableToLoadPlaybackTroubleshooting => {
            "Kontext für die Wiedergabefehlerbehebung konnte nicht geladen werden"
        }
        TextKey::UploadLogo => "Logo hochladen",
        TextKey::UploadLogoHelp => {
            "Wählen Sie ein lokales Bild, das beim Speichern das gespeicherte Kanallogo ersetzt"
        }
        TextKey::Warning => "Warnung",
        TextKey::Watermark => "Wasserzeichen",
        TextKey::WatermarkIds => "Wasserzeichen-IDs",
        TextKey::Window => "Fenster",
        TextKey::WithProgress => "Mit Fortschritt",
        TextKey::Writers => "Autoren",
        TextKey::Validate => "Validieren",
        TextKey::Yaml => "YAML",
        TextKey::YamlFile => "YAML-Datei",
        TextKey::Messages => "Meldungen",
        TextKey::CollectionName => "Sammlungsname",
        TextKey::MediaItemType => "Medienelementtyp",
        TextKey::MediaItemTitle => "Titel des Medienelements",
        TextKey::ActualSize => "Tatsächliche Größe",
        TextKey::AddWatermark => "Wasserzeichen hinzufügen",
        TextKey::AspectRatioGuides => "Hilfslinien für Seitenverhältnisse",
        TextKey::BottomLeft => "Unten links",
        TextKey::BottomMiddle => "Unten Mitte",
        TextKey::BottomRight => "Unten rechts",
        TextKey::CopyWatermark => "Wasserzeichen kopieren",
        TextKey::CopyOf => "Kopie von",
        TextKey::CurrentImage => "Aktuelles Bild",
        TextKey::Custom => "Benutzerdefiniert",
        TextKey::ChannelLogoPreviewUnavailable => {
            "Vorschauen des Kanallogos erfordern einen Kanalkontext. Wählen Sie ein benutzerdefiniertes Bild, um es hier in der Vorschau zu sehen."
        }
        TextKey::DeleteWatermark => "Wasserzeichen löschen",
        TextKey::DurationSeconds => "Dauer (Sekunden)",
        TextKey::EditWatermark => "Wasserzeichen bearbeiten",
        TextKey::EnableWatermarkToPreview => {
            "Aktivieren Sie das Wasserzeichen, um seine Platzierung zu sehen."
        }
        TextKey::Frequency => "Häufigkeit",
        TextKey::GeneratingPreview => "Vorschau wird erzeugt …",
        TextKey::HorizontalMarginPercent => "Horizontaler Rand (%)",
        TextKey::GuideFourThree => "4:3-Schutzbereich",
        TextKey::Identity => "Identität",
        TextKey::Image => "Bild",
        TextKey::ImageSource => "Bildquelle",
        TextKey::Intermittent => "Zeitweise",
        TextKey::IntermittentTiming => "Zeitsteuerung für zeitweise Anzeige",
        TextKey::LeftMiddle => "Links Mitte",
        TextKey::Location => "Position",
        TextKey::MiddleCenter => "Mitte zentriert",
        TextKey::Mode => "Modus",
        TextKey::Minutes => "Minuten",
        TextKey::Moods => "Stimmungen",
        TextKey::NewName => "Neuer Name",
        TextKey::NativeAssetDimensions => "Native Asset- und Anzeigegröße",
        TextKey::NextWatermark => "Nächstes Wasserzeichen",
        TextKey::Opacity => "Deckkraft",
        TextKey::OpacityExpression => "Deckkraft-Ausdruck",
        TextKey::OpacityOneToHundred => "Deckkraft (1–100)",
        TextKey::Permanent => "Dauerhaft",
        TextKey::PerPass => "pro Durchlauf",
        TextKey::PlaceWithinSourceContent => "Innerhalb des Quellinhalts platzieren",
        TextKey::Placement => "Platzierung",
        TextKey::PlacementPreview => "Platzierungsvorschau",
        TextKey::PreviewFailed => "Vorschau fehlgeschlagen",
        TextKey::PreviewReferenceFullHd => "Full-HD-Referenzbild · 1920×1080",
        TextKey::PreviousWatermark => "Vorheriges Wasserzeichen",
        TextKey::RightMiddle => "Rechts Mitte",
        TextKey::SaveWatermark => "Wasserzeichen speichern",
        TextKey::Scaled => "Skaliert",
        TextKey::SelectCustomImageToPreview => {
            "Wählen Sie ein benutzerdefiniertes Bild aus oder laden Sie eines hoch, um es in der Vorschau zu sehen."
        }
        TextKey::Size => "Größe",
        TextKey::TopLeft => "Oben links",
        TextKey::TopMiddle => "Oben Mitte",
        TextKey::TopRight => "Oben rechts",
        TextKey::UploadFailed => "Upload fehlgeschlagen.",
        TextKey::UploadImage => "Bild hochladen",
        TextKey::UnsavedChanges => "Nicht gespeicherte Änderungen",
        TextKey::UnsavedWatermarkChangesMessage => {
            "Ihre Wasserzeichen-Änderungen verwerfen und das ausgewählte Wasserzeichen öffnen?"
        }
        TextKey::VerticalMarginPercent => "Vertikaler Rand (%)",
        TextKey::WatermarkCopied => "Wasserzeichen kopiert.",
        TextKey::WatermarkNotFound => "Wasserzeichen nicht gefunden.",
        TextKey::WatermarkPlacementPreview => "Vorschau der Wasserzeichenplatzierung",
        TextKey::WatermarkPreview => "Wasserzeichen-Vorschau",
        TextKey::WatermarkNavigation => "Wasserzeichen-Navigation",
        TextKey::WidthPercent => "Breite (%)",
        TextKey::ZIndex => "Z-Index",
        TextKey::DiscardChanges => "Änderungen verwerfen",
        TextKey::XmltvBlockBehavior => "XMLTV-Blockverhalten",
        TextKey::XmltvTimeZone => "XMLTV-Zeitzone",
        TextKey::Xmltv => "XMLTV",
        TextKey::Badge => "Badge",
        TextKey::Buttons => "Schaltflächen",
        TextKey::CheckValue => "Kontrollkästchen",
        TextKey::ChannelName => "Kanalname",
        TextKey::ChannelNumber => "Kanalnummer",
        TextKey::ComponentGallery => "ErsatzRS-Komponentengalerie",
        TextKey::DangerButton => "Gefahr",
        TextKey::DeleteDialog => "Löschdialog",
        TextKey::Decrement => "Verringern",
        TextKey::Dialogs => "Dialoge",
        TextKey::DisabledButton => "Deaktiviert",
        TextKey::EnterName => "Name eingeben",
        TextKey::ErrorOccurred => "Ein Fehler ist aufgetreten.",
        TextKey::ErrorToast => "Fehler-Toast",
        TextKey::ExampleModal => "Beispiel-Modal",
        TextKey::ExampleModalBody => "Dies ist ein Beispielinhalt für einen Modal-Dialog.",
        TextKey::FormInputs => "Formulareingaben",
        TextKey::Icons => "Symbole",
        TextKey::IncrementBadge => "Badge erhöhen",
        TextKey::InfoToast => "Info-Toast",
        TextKey::JustFyi => "Nur zur Information.",
        TextKey::LinkButton => "Link-Schaltfläche",
        TextKey::ModalCancelled => "Abgebrochen.",
        TextKey::ModalConfirmed => "Bestätigt!",
        TextKey::NumValue => "Zahl",
        TextKey::OpenModal => "Modal öffnen",
        TextKey::OperationSucceeded => "Vorgang erfolgreich.",
        TextKey::PagedTableDemo => "PagedTable (5 Zeilen pro Seite, 7 insgesamt)",
        TextKey::PrimaryButton => "Primär",
        TextKey::SecondaryButton => "Sekundär",
        TextKey::SelectValue => "Auswahl",
        TextKey::SomethingMightBeWrong => "Möglicherweise stimmt etwas nicht.",
        TextKey::SuccessToast => "Erfolgs-Toast",
        TextKey::TextButton => "Text",
        TextKey::TextValue => "Text",
        TextKey::Toasts => "Toasts",
        TextKey::WarningToast => "Warn-Toast",
        TextKey::YamlScript => "YAML-Skript",
        TextKey::Yes => "Ja",
    }
}

fn schedule_error_message(error: &str) -> String {
    match error.trim() {
        "Schedule name is required!" => "Der Sendeplanname ist erforderlich!".to_owned(),
        "New Schedule Name is required." => "Der neue Sendeplanname ist erforderlich.".to_owned(),
        "Schedule name must be 50 characters or less." => {
            "Der Sendeplanname darf höchstens 50 Zeichen lang sein.".to_owned()
        }
        "Schedule name must be unique" => "Der Sendeplanname muss eindeutig sein".to_owned(),
        "Schedule does not exist" => "Der Sendeplan existiert nicht".to_owned(),
        "[ProgramScheduleId] does not exist." => "[ProgramScheduleId] existiert nicht.".to_owned(),
        "A collection must use the same playback order in every schedule item." => {
            "Eine Sammlung muss in jedem Sendeplanelement dieselbe Wiedergabereihenfolge verwenden.".to_owned()
        }
        "Shuffle In Order cannot be used with Fill With Group Mode." => {
            "Mischen in Reihenfolge kann nicht mit dem Modus Fill With Group verwendet werden.".to_owned()
        }
        "Fixed schedule items require a start time." => {
            "Feste Sendeplanelemente erfordern eine Startzeit.".to_owned()
        }
        "Multi Collection items can only use Shuffle or Shuffle In Order." => {
            "Mehrsammlungselemente können nur Mischen oder Mischen in Reihenfolge verwenden.".to_owned()
        }
        "Playlist Item Size can only be used with Playlist schedule items." => {
            "Die Größe der Wiedergabelistenelemente kann nur bei Sendeplanelementen vom Typ Wiedergabeliste verwendet werden.".to_owned()
        }
        "Multiple playout mode with Count requires a count expression." => {
            "Der Playout-Modus Multiple mit Count erfordert einen Anzahl-Ausdruck.".to_owned()
        }
        "Duration playout mode requires a duration between 1 minute and 24 hours." => {
            "Der Playout-Modus Duration erfordert eine Dauer zwischen 1 Minute und 24 Stunden.".to_owned()
        }
        "Duration playout mode requires discard-to-fill attempts." => {
            "Der Playout-Modus Duration erfordert Discard-to-Fill-Versuche.".to_owned()
        }
        "Tail Mode Filler requires a tail filler preset." => {
            "Der Tail-Modus Filler erfordert eine Tail-Filler-Vorlage.".to_owned()
        }
        "Tail filler can only be set when Tail Mode is Filler." => {
            "Ein Tail-Filler kann nur festgelegt werden, wenn der Tail-Modus Filler ist.".to_owned()
        }
        "Collection is required." => "Die Sammlung ist erforderlich.".to_owned(),
        "Media item is required." => "Das Medienelement ist erforderlich.".to_owned(),
        "Multi Collection is required." => "Die Mehrsammlung ist erforderlich.".to_owned(),
        "Smart Collection is required." => "Die intelligente Sammlung ist erforderlich.".to_owned(),
        "Playlist is required." => "Die Wiedergabeliste ist erforderlich.".to_owned(),
        "Rerun Collection is required." => "Die Wiederholungssammlung ist erforderlich.".to_owned(),
        "Search Query is required." => "Die Suchabfrage ist erforderlich.".to_owned(),
        "Only one pre-, mid-, or post-roll filler can pad to the nearest minute." => {
            "Nur ein Pre-, Mid- oder Post-Roll-Filler kann auf die nächste Minute auffüllen.".to_owned()
        }
        "Padding filler requires a fallback filler." => {
            "Ein Auffüll-Filler erfordert einen Fallback-Filler.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn scheduling_content_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("Block group name \"") && trimmed.ends_with("\" is invalid") {
        return "Der Name der Block-Gruppe darf höchstens 50 Zeichen lang sein.".to_owned();
    }
    if trimmed.starts_with("Block name \"") && trimmed.ends_with("\" is invalid") {
        return "Der Block-Name darf höchstens 50 Zeichen lang sein.".to_owned();
    }
    if trimmed.starts_with("Template name \"") && trimmed.ends_with("\" is invalid") {
        return "Der Vorlagenname darf höchstens 50 Zeichen lang sein.".to_owned();
    }
    if trimmed.starts_with("A template named \"")
        && trimmed.ends_with("\" already exists in that template group")
    {
        return "In dieser Vorlagengruppe existiert bereits eine Vorlage mit diesem Namen."
            .to_owned();
    }
    if trimmed.starts_with("Block from ") && trimmed.contains(" intersects block from ") {
        return "Der Block überschneidet sich mit einem anderen Block.".to_owned();
    }
    if trimmed.starts_with("TemplateGroup ") && trimmed.ends_with(" does not exist.") {
        return "Die Vorlagengruppe existiert nicht.".to_owned();
    }
    if trimmed.starts_with("Template ") && trimmed.ends_with(" does not exist.") {
        return "Die Vorlage existiert nicht.".to_owned();
    }
    if trimmed.starts_with("DecoGroup ") && trimmed.ends_with(" does not exist.") {
        return "Die Deko-Gruppe existiert nicht.".to_owned();
    }
    if trimmed.starts_with("Deco ") && trimmed.ends_with(" does not exist.") {
        return "Die Deko existiert nicht.".to_owned();
    }
    if trimmed.starts_with("DecoTemplateGroup ") && trimmed.ends_with(" does not exist.") {
        return "Die Deko-Vorlagengruppe existiert nicht.".to_owned();
    }
    if trimmed.starts_with("DecoTemplate ") && trimmed.ends_with(" does not exist.") {
        return "Die Deko-Vorlage existiert nicht.".to_owned();
    }
    if trimmed.starts_with("Deco name \"") && trimmed.ends_with("\" is invalid") {
        return "Der Deko-Name darf höchstens 50 Zeichen lang sein.".to_owned();
    }
    if trimmed.starts_with("A deco named \"")
        && trimmed.ends_with("\" already exists in that deco group")
    {
        return "In dieser Deko-Gruppe existiert bereits eine Deko mit diesem Namen.".to_owned();
    }
    if trimmed.starts_with("Deco template name \"") && trimmed.ends_with("\" is invalid") {
        return "Der Name der Deko-Vorlage darf höchstens 50 Zeichen lang sein.".to_owned();
    }
    if trimmed.starts_with("A deco template named \"")
        && trimmed.ends_with("\" already exists in that deco template group")
    {
        return "In dieser Deko-Vorlagengruppe existiert bereits eine Deko-Vorlage mit diesem Namen."
            .to_owned();
    }
    if trimmed.starts_with("Watermark ") && trimmed.ends_with(" does not exist.") {
        return "Das Wasserzeichen existiert nicht.".to_owned();
    }
    if trimmed.starts_with("Graphics element ") && trimmed.ends_with(" does not exist.") {
        return "Das Grafikelement existiert nicht.".to_owned();
    }
    if trimmed.starts_with("Deco from ") && trimmed.contains(" intersects deco from ") {
        return "Die Deko überschneidet sich mit einer anderen Deko.".to_owned();
    }

    match trimmed {
        "Block does not exist." => "Der Block existiert nicht.".to_owned(),
        "Block group name is required" => "Der Name der Block-Gruppe ist erforderlich".to_owned(),
        "Block group name must be unique" => {
            "Der Name der Block-Gruppe muss eindeutig sein".to_owned()
        }
        "Block name is required" => "Der Block-Name ist erforderlich".to_owned(),
        "A block named with that value already exists in that block group" => {
            "In dieser Block-Gruppe existiert bereits ein Block mit diesem Namen".to_owned()
        }
        "Block duration must be between 5 minutes and 24 hours" => {
            "Die Block-Dauer muss zwischen 5 Minuten und 24 Stunden liegen".to_owned()
        }
        "[Collection] is required for collection type 'Collection'" => {
            "[Collection] ist für den Sammlungstyp 'Collection' erforderlich".to_owned()
        }
        "[MediaItem] is required for collection type 'TelevisionShow'" => {
            "[MediaItem] ist für den Sammlungstyp 'TelevisionShow' erforderlich".to_owned()
        }
        "[MediaItem] is required for collection type 'TelevisionSeason'" => {
            "[MediaItem] ist für den Sammlungstyp 'TelevisionSeason' erforderlich".to_owned()
        }
        "[MultiCollection] is required for collection type 'MultiCollection'" => {
            "[MultiCollection] ist für den Sammlungstyp 'MultiCollection' erforderlich".to_owned()
        }
        "[SmartCollection] is required for collection type 'SmartCollection'" => {
            "[SmartCollection] ist für den Sammlungstyp 'SmartCollection' erforderlich".to_owned()
        }
        "[SearchQuery] is required for collection type 'SearchQuery'" => {
            "[SearchQuery] ist für den Sammlungstyp 'SearchQuery' erforderlich".to_owned()
        }
        "[CollectionType] is invalid" => "[CollectionType] ist ungültig".to_owned(),
        "[BlockId] does not exist." => "[BlockId] existiert nicht.".to_owned(),
        "Template does not exist." => "Die Vorlage existiert nicht.".to_owned(),
        "Template group name is required" => {
            "Der Name der Vorlagengruppe ist erforderlich".to_owned()
        }
        "Template group name must be 50 characters or fewer" => {
            "Der Name der Vorlagengruppe darf höchstens 50 Zeichen lang sein".to_owned()
        }
        "Template group name must be unique" => {
            "Der Name der Vorlagengruppe muss eindeutig sein".to_owned()
        }
        "Template group does not exist." => "Die Vorlagengruppe existiert nicht.".to_owned(),
        "[TemplateId] does not exist." => "[TemplateId] existiert nicht.".to_owned(),
        "Template name is required" => "Der Vorlagenname ist erforderlich".to_owned(),
        "Template items cannot cross midnight." => {
            "Vorlagenelemente dürfen Mitternacht nicht überschreiten.".to_owned()
        }
        "Deco group name is required" => "Der Name der Deko-Gruppe ist erforderlich".to_owned(),
        "Deco group name must be 50 characters or fewer" => {
            "Der Name der Deko-Gruppe darf höchstens 50 Zeichen lang sein".to_owned()
        }
        "Deco group name must be unique" => {
            "Der Name der Deko-Gruppe muss eindeutig sein".to_owned()
        }
        "Deco group does not exist." => "Die Deko-Gruppe existiert nicht.".to_owned(),
        "Deco name is required" => "Der Deko-Name ist erforderlich".to_owned(),
        "Deco does not exist" => "Die Deko existiert nicht.".to_owned(),
        "Default filler has unsupported collection type." => {
            "Der Standard-Filler hat einen nicht unterstützten Sammlungstyp.".to_owned()
        }
        "Dead air fallback has unsupported collection type." => {
            "Der Totzeit-Fallback hat einen nicht unterstützten Sammlungstyp.".to_owned()
        }
        "Break content has unsupported collection type." => {
            "Der Pauseninhalt hat einen nicht unterstützten Sammlungstyp.".to_owned()
        }
        "Default filler must have valid item" => {
            "Der Standard-Filler muss ein gültiges Element haben".to_owned()
        }
        "Dead air fallback must have valid item" => {
            "Der Totzeit-Fallback muss ein gültiges Element haben".to_owned()
        }
        "Break content collection must have valid item" => {
            "Die Sammlung des Pauseninhalts muss ein gültiges Element haben".to_owned()
        }
        "Break content multi collection must have valid item" => {
            "Die Mehrsammlung des Pauseninhalts muss ein gültiges Element haben".to_owned()
        }
        "Break content smart collection must have valid item" => {
            "Die intelligente Sammlung des Pauseninhalts muss ein gültiges Element haben".to_owned()
        }
        "Break content media item must have valid item" => {
            "Das Medienelement des Pauseninhalts muss ein gültiges Element haben".to_owned()
        }
        "Break content playlist must have valid item" => {
            "Die Wiedergabeliste des Pauseninhalts muss ein gültiges Element haben".to_owned()
        }
        "Default filler item does not exist." => {
            "Das Element des Standard-Fillers existiert nicht.".to_owned()
        }
        "Dead air fallback item does not exist." => {
            "Das Element des Totzeit-Fallbacks existiert nicht.".to_owned()
        }
        "Break content collection item does not exist." => {
            "Das Sammlungselement des Pauseninhalts existiert nicht.".to_owned()
        }
        "Break content multi collection item does not exist." => {
            "Das Mehrsammlungselement des Pauseninhalts existiert nicht.".to_owned()
        }
        "Break content smart collection item does not exist." => {
            "Das Element der intelligenten Sammlung des Pauseninhalts existiert nicht.".to_owned()
        }
        "Break content media item item does not exist." => {
            "Das Medienelement des Pauseninhalts existiert nicht.".to_owned()
        }
        "Break content playlist item does not exist." => {
            "Die Wiedergabeliste des Pauseninhalts existiert nicht.".to_owned()
        }
        "Deco may only contain one [Block Start] break content" => {
            "Eine Deko darf nur einen Pauseninhalt vom Typ „Block Start“ enthalten.".to_owned()
        }
        "Deco may only contain one [Between Block Items] break content" => {
            "Eine Deko darf nur einen Pauseninhalt vom Typ „Between Block Items“ enthalten."
                .to_owned()
        }
        "Deco may only contain one [At Chapter Markers] break content" => {
            "Eine Deko darf nur einen Pauseninhalt vom Typ „At Chapter Markers“ enthalten."
                .to_owned()
        }
        "Deco may only contain one [Block Finish] break content" => {
            "Eine Deko darf nur einen Pauseninhalt vom Typ „Block Finish“ enthalten.".to_owned()
        }
        "Deco template group name is required" => {
            "Der Name der Deko-Vorlagengruppe ist erforderlich".to_owned()
        }
        "Deco template group name must be 50 characters or fewer" => {
            "Der Name der Deko-Vorlagengruppe darf höchstens 50 Zeichen lang sein".to_owned()
        }
        "Deco template group name must be unique" => {
            "Der Name der Deko-Vorlagengruppe muss eindeutig sein".to_owned()
        }
        "Deco template group does not exist." => {
            "Die Deko-Vorlagengruppe existiert nicht.".to_owned()
        }
        "[DecoTemplateId] does not exist." => "Die Deko-Vorlage existiert nicht.".to_owned(),
        "Deco template name is required" => "Der Name der Deko-Vorlage ist erforderlich".to_owned(),
        "[DecoId] does not exist." => "Die Deko existiert nicht.".to_owned(),
        "Deco template items cannot be empty." => {
            "Deko-Vorlagenelemente dürfen nicht leer sein.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn watermark_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("Watermark ") && trimmed.ends_with(" does not exist.") {
        return "Wasserzeichen nicht gefunden.".to_owned();
    }

    match trimmed {
        "Name is required" => "Der Name ist erforderlich".to_owned(),
        "Name must be 50 characters or fewer" => {
            "Der Name darf höchstens 50 Zeichen lang sein".to_owned()
        }
        "Watermark image is required" | "Watermark image is required!" => {
            "Ein Wasserzeichenbild ist erforderlich".to_owned()
        }
        "Width must be between 0 and 100%" => {
            "Die Breite muss zwischen 0 und 100 % liegen".to_owned()
        }
        "Horizontal margin must be between 0 and 50%" => {
            "Der horizontale Rand muss zwischen 0 und 50 % liegen".to_owned()
        }
        "Vertical margin must be between 0 and 50%" => {
            "Der vertikale Rand muss zwischen 0 und 50 % liegen".to_owned()
        }
        "Duration must be positive and less than frequency window" => {
            "Die Dauer muss positiv und kürzer als das Häufigkeitsfenster sein".to_owned()
        }
        "Opacity must be between 1 and 100" => {
            "Die Deckkraft muss zwischen 1 und 100 liegen".to_owned()
        }
        "Watermark name must be unique" | "ChannelWatermark name must be unique" => {
            "Der Name des Wasserzeichens muss eindeutig sein".to_owned()
        }
        "Watermark not found" | "Source watermark not found" | "Watermark does not exist." => {
            "Wasserzeichen nicht gefunden.".to_owned()
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
        _ => return format!("Unbekannt ({value})"),
    }
    .to_owned()
}

fn ffmpeg_profile_video_format_label(format: i32) -> String {
    match format {
        0 => "Keine",
        1 => "H264",
        2 => "H265 (HEVC)",
        3 => "MPEG-2",
        4 => "AV1",
        99 => text(TextKey::Copy),
        _ => return format!("Unbekannt ({format})"),
    }
    .to_owned()
}

fn ffmpeg_profile_audio_format_label(format: i32) -> String {
    match format {
        0 => "Keine",
        1 => "AAC",
        2 => "AC3",
        3 => "AAC (LATM)",
        99 => text(TextKey::Copy),
        _ => return format!("Unbekannt ({format})"),
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
            "Die Anzahl der zu erstellenden XMLTV-Tage ({current}) darf die Anzahl der zu erstellenden Playout-Tage ({limit}) nicht überschreiten"
        );
    }
    if let Some(name) = trimmed.strip_suffix(" path does not exist") {
        return format!("Der Pfad {name} existiert nicht");
    }
    if let Some(name) = trimmed
        .strip_prefix("Unable to verify ")
        .and_then(|value| value.strip_suffix(" version"))
    {
        return format!("Die {name}-Version kann nicht überprüft werden");
    }
    if let Some(value) = trimmed.strip_prefix("Invalid option: ") {
        return format!("Ungültige Option: {value}");
    }
    if trimmed.starts_with("Resolution ") && trimmed.ends_with(" does not exist.") {
        return "Die Auflösung existiert nicht.".to_owned();
    }

    match trimmed {
        "Windows startup registration is not supported on this platform" => {
            "Die Windows-Autostart-Registrierung wird auf dieser Plattform nicht unterstützt".to_owned()
        }
        "ErsatzRS.exe was not found next to ersatzrs-web.exe" => {
            "ErsatzRS.exe wurde nicht neben ersatzrs-web.exe gefunden".to_owned()
        }
        "Unable to update Windows startup registration" => {
            "Die Windows-Autostart-Registrierung konnte nicht aktualisiert werden".to_owned()
        }
        "Tuner count must be greater than zero" => {
            "Die Tuner-Anzahl muss größer als null sein".to_owned()
        }
        "Library refresh interval must be 0 (do not refresh) or greater than zero" => {
            "Das Aktualisierungsintervall der Bibliothek muss 0 (nicht aktualisieren) oder größer als null sein".to_owned()
        }
        "Library refresh interval must be less than 1,000,000. Use 0 to disable automatic refresh" => {
            "Das Aktualisierungsintervall der Bibliothek muss kleiner als 1.000.000 sein. Verwenden Sie 0, um die automatische Aktualisierung zu deaktivieren".to_owned()
        }
        "Playout days to build must be greater than zero" => {
            "Die Anzahl der zu erstellenden Playout-Tage muss größer als null sein".to_owned()
        }
        "Scripted schedule timeout seconds must be at least 10" => {
            "Das Zeitlimit für skriptierte Sendepläne muss mindestens 10 Sekunden betragen".to_owned()
        }
        "XMLTV days to build must be greater than zero" => {
            "Die Anzahl der zu erstellenden XMLTV-Tage muss größer als null sein".to_owned()
        }
        "Resolution width or height is invalid" => {
            "Die Breite oder Höhe der Auflösung ist ungültig".to_owned()
        }
        "Resolution width and height must be unique" => {
            "Breite und Höhe der Auflösung müssen eindeutig sein".to_owned()
        }
        "HLS Segmenter idle timeout must be greater than or equal to 30" => {
            "Das Leerlauf-Zeitlimit des HLS-Segmenters muss mindestens 30 betragen".to_owned()
        }
        "Work-Ahead HLS Segmenter limit must be greater than or equal to 0" => {
            "Das Limit für vorauslaufende HLS-Segmenter muss mindestens 0 betragen".to_owned()
        }
        "Active HLS transcode limit must be greater than or equal to 1" => {
            "Das Limit für aktive HLS-Transkodierungen muss mindestens 1 betragen".to_owned()
        }
        "HLS Segmenter initial segment count must be greater than or equal to 1" => {
            "Die anfängliche Segmentanzahl des HLS-Segmenters muss mindestens 1 betragen".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn media_action_error_message(error: &str) -> String {
    match error.trim() {
        "Collection name is required" => "Der Sammlungsname ist erforderlich".to_owned(),
        "Collection name must be 50 characters or less" => {
            "Der Sammlungsname darf höchstens 50 Zeichen lang sein".to_owned()
        }
        "Collection name must be unique" => "Der Sammlungsname muss eindeutig sein".to_owned(),
        "Media item does not exist" => "Das Medienelement existiert nicht".to_owned(),
        "Collection does not exist" | "Collection does not exist." => {
            "Die Sammlung existiert nicht.".to_owned()
        }
        "Playlist does not exist" | "Playlist does not exist." => {
            "Die Wiedergabeliste existiert nicht.".to_owned()
        }
        "Unable to locate media item" => {
            "Das Medienelement konnte nicht gefunden werden".to_owned()
        }
        "Media action failed" => "Die Medienaktion ist fehlgeschlagen".to_owned(),
        _ => error.to_owned(),
    }
}

fn filler_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("FillerPreset ") && trimmed.ends_with(" does not exist.") {
        return "Die Filler-Vorlage existiert nicht.".to_owned();
    }

    match trimmed {
        "Filler preset does not exist" => "Die Filler-Vorlage existiert nicht.".to_owned(),
        "Filler preset name must be unique" => {
            "Der Name der Filler-Vorlage muss eindeutig sein".to_owned()
        }
        "Name must not be empty" => "Der Name darf nicht leer sein".to_owned(),
        "Name must be 50 characters or fewer" => {
            "Der Name darf höchstens 50 Zeichen lang sein".to_owned()
        }
        "Filler kind must not be None" => "Der Filler-Typ darf nicht None sein".to_owned(),
        "Filler mode must not be None" => "Der Filler-Modus darf nicht None sein".to_owned(),
        "Count must be greater than or equal to 0" => {
            "Die Anzahl muss mindestens 0 betragen".to_owned()
        }
        "Duration must not be empty" => "Die Dauer darf nicht leer sein".to_owned(),
        "Pad To Nearest Minute must not be empty" => {
            "„Auf nächste Minute auffüllen“ darf nicht leer sein".to_owned()
        }
        "Use Chapters As Media Items must not be enabled for fallback filler" => {
            "„Kapitel als Medienelemente verwenden“ darf für Fallback-Filler nicht aktiviert sein"
                .to_owned()
        }
        "Collection must not be empty" => "Die Sammlung darf nicht leer sein".to_owned(),
        "Multi Collection must not be empty" => "Die Mehrsammlung darf nicht leer sein".to_owned(),
        "Smart Collection must not be empty" => {
            "Die intelligente Sammlung darf nicht leer sein".to_owned()
        }
        "Media item must not be empty" => "Das Medienelement darf nicht leer sein".to_owned(),
        _ => error.to_owned(),
    }
}

fn media_browser_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.ends_with("Collection does not contain the selected item.") {
        return "Die Sammlung enthält das ausgewählte Element nicht.".to_owned();
    }
    if trimmed.contains("collection with id ") && trimmed.ends_with(" not found") {
        return "Die Sammlung existiert nicht.".to_owned();
    }
    if trimmed == "Pinned stream selection is invalid or no longer available." {
        return "Die angeheftete Streamauswahl ist ungültig oder nicht mehr verfügbar.".to_owned();
    }
    if trimmed == "Movie was not found." {
        return "Der Film wurde nicht gefunden.".to_owned();
    }
    if trimmed.starts_with("Collection ") && trimmed.ends_with(" is already present.") {
        return "Die Sammlung ist bereits vorhanden.".to_owned();
    }
    if trimmed.starts_with("Collection ") && trimmed.ends_with(" does not exist.") {
        return "Die Sammlung existiert nicht.".to_owned();
    }
    if trimmed.starts_with("Smart collection ") && trimmed.ends_with(" is already present.") {
        return "Die intelligente Sammlung ist bereits vorhanden.".to_owned();
    }
    if trimmed.starts_with("Smart collection ") && trimmed.ends_with(" does not exist.") {
        return "Die intelligente Sammlung existiert nicht.".to_owned();
    }
    if trimmed.starts_with("MultiCollection ") && trimmed.ends_with(" does not exist.") {
        return "Die Mehrsammlung existiert nicht.".to_owned();
    }
    if trimmed.starts_with("Rerun collection ") && trimmed.ends_with(" does not exist.") {
        return "Die Wiederholungssammlung existiert nicht.".to_owned();
    }
    if trimmed.starts_with("Referenced item ") && trimmed.ends_with(" does not exist.") {
        return "Das referenzierte Element existiert nicht.".to_owned();
    }
    if trimmed.starts_with("PlaylistGroup ") && trimmed.ends_with(" does not exist.") {
        return "Die Wiedergabelistengruppe existiert nicht.".to_owned();
    }
    if trimmed.starts_with("Playlist ") && trimmed.ends_with(" does not exist.") {
        return "Die Wiedergabeliste existiert nicht.".to_owned();
    }
    if trimmed.starts_with("Playlist name \"") && trimmed.ends_with("\" is invalid") {
        return "Der Name der Wiedergabeliste ist ungültig.".to_owned();
    }
    if let Some(error) = trimmed.strip_prefix("Playlist preview failed: ") {
        return format!("Vorschau der Wiedergabeliste fehlgeschlagen: {error}");
    }
    if trimmed.starts_with("Media item ") && trimmed.ends_with(" does not exist.") {
        return "Das Medienelement existiert nicht.".to_owned();
    }

    match trimmed {
        "Collection name is required" => "Der Sammlungsname ist erforderlich".to_owned(),
        "Collection name must be 50 characters or less" => {
            "Der Sammlungsname darf höchstens 50 Zeichen lang sein".to_owned()
        }
        "Collection name must be unique" => "Der Sammlungsname muss eindeutig sein".to_owned(),
        "Collection does not exist." => "Die Sammlung existiert nicht.".to_owned(),
        "Smart collection name is required" => {
            "Der Name der intelligenten Sammlung ist erforderlich".to_owned()
        }
        "Smart collection query is required" => {
            "Die Abfrage der intelligenten Sammlung ist erforderlich".to_owned()
        }
        "SmartCollection name must be unique" => {
            "Der Name der intelligenten Sammlung muss eindeutig sein".to_owned()
        }
        "MultiCollection does not exist." => "Die Mehrsammlung existiert nicht.".to_owned(),
        "Multi-collection name is required!" => {
            "Der Name der Mehrsammlung ist erforderlich.".to_owned()
        }
        "Multi-collection name cannot be longer than 50 characters." => {
            "Der Name der Mehrsammlung darf nicht länger als 50 Zeichen sein.".to_owned()
        }
        "MultiCollection name must be unique" => {
            "Der Name der Mehrsammlung muss eindeutig sein".to_owned()
        }
        "Each multi-collection item must reference exactly one collection." => {
            "Jedes Mehrsammlungselement muss genau eine Sammlung referenzieren.".to_owned()
        }
        "Rerun collection does not exist." => {
            "Die Wiederholungssammlung existiert nicht.".to_owned()
        }
        "[Name] is an empty string" | "[Name] must not be empty." => {
            "Der Name darf nicht leer sein.".to_owned()
        }
        "[Name] must not be longer than 50" | "[Name] must not be longer than 50." => {
            "Der Name darf nicht länger als 50 Zeichen sein.".to_owned()
        }
        "Rerun collection name must be unique" => {
            "Der Name der Wiederholungssammlung muss eindeutig sein".to_owned()
        }
        "Unsupported collection type." => "Nicht unterstützter Sammlungstyp.".to_owned(),
        "[ProgramScheduleId] does not exist." => "Der Sendeplan existiert nicht.".to_owned(),
        "[MediaItem] does not exist." => "Das Medienelement existiert nicht.".to_owned(),
        "[PlaylistGroupId] does not exist." => {
            "Die Wiedergabelistengruppe existiert nicht.".to_owned()
        }
        "Cannot delete system playlist group" => {
            "Die System-Wiedergabelistengruppe kann nicht gelöscht werden".to_owned()
        }
        "Cannot delete system (generated) playlist" => {
            "Die (generierte) System-Wiedergabeliste kann nicht gelöscht werden".to_owned()
        }
        "[PlaylistId] does not exist." => "Die Wiedergabeliste existiert nicht.".to_owned(),
        "Cannot save system (generated) playlist" => {
            "Die (generierte) System-Wiedergabeliste kann nicht gespeichert werden".to_owned()
        }
        "Playlist playout preview has no items" => {
            "Die Vorschau des Wiedergabelisten-Playouts enthält keine Elemente".to_owned()
        }
        "Playlist name is required" => "Der Name der Wiedergabeliste ist erforderlich".to_owned(),
        "[Collection] is required for collection type 'Collection'" => {
            "Die Sammlung ist erforderlich.".to_owned()
        }
        "[MultiCollection] is required for collection type 'MultiCollection'" => {
            "Die Mehrsammlung ist erforderlich.".to_owned()
        }
        "[SmartCollection] is required for collection type 'SmartCollection'" => {
            "Die intelligente Sammlung ist erforderlich.".to_owned()
        }
        "[CollectionType] is invalid" => "Der Sammlungstyp ist ungültig.".to_owned(),
        _ => error.to_owned(),
    }
}

fn media_source_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if let Some(error) = trimmed.strip_prefix("Could not reach Jellyfin: ") {
        return format!("Jellyfin konnte nicht erreicht werden: {error}");
    }

    match trimmed {
        "Address is required." => "Die Adresse ist erforderlich.".to_owned(),
        "API key is required." => "Der API-Schlüssel ist erforderlich.".to_owned(),
        "'Address' must be a valid URL" => "'Address' muss eine gültige URL sein".to_owned(),
        _ => error.to_owned(),
    }
}

fn addon_operation_error_message(code: &str, detail: &str) -> String {
    let summary = match code {
        "missing-setting" => "Konfigurieren Sie die erforderliche Add-on-Einstellung.",
        "missing-secret" => {
            "Konfigurieren Sie den erforderlichen Geheimnisverweis in den Add-on-Einstellungen."
        }
        "provider-unreachable" => "Das Add-on konnte den Anbieter nicht erreichen.",
        "provider-rejected" => "Der Anbieter hat die Add-on-Anfrage abgelehnt.",
        "provider-rate-limited" => "Der Anbieter hat das Add-on vorübergehend begrenzt.",
        "not-found" => "Das angeforderte Add-on-Element wurde nicht gefunden.",
        "unsupported-url" => "Das Add-on unterstützt diese URL nicht.",
        _ => "Das Add-on hat einen Fehler gemeldet.",
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
        return format!("Add-on hat ungültige Listendaten zurückgegeben: {error}");
    }
    if normalized.starts_with("add-on media list with id ") && normalized.ends_with(" not found") {
        return "Die add-on-Liste existiert nicht.".to_owned();
    }

    if let Some(folder) =
        normalized.strip_prefix("another list of this add-on already uses the folder ")
    {
        return format!(
            "Eine andere Liste dieses Add-ons verwendet den Ordner \"{folder}\" bereits."
        );
    }
    if let Some(folder) = normalized.strip_suffix(" is not usable as a folder name") {
        return format!("\"{folder}\" ist als Ordnername nicht verwendbar.");
    }

    match normalized {
        "Invalid add-on list url" => "Ungültige add-on-Listen-URL.".to_owned(),
        "add-on API client is not configured for this operation" => {
            "Der add-on-API-Client ist für diesen Vorgang nicht konfiguriert.".to_owned()
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
            Some("Serie - S02E05".to_owned())
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
        assert_eq!(STRINGS.text(TextKey::Addons), "Add-ons");
    }

    #[test]
    fn glossary_terms_are_used_consistently() {
        assert_eq!(STRINGS.text(TextKey::Schedule), "Sendeplan");
        assert_eq!(STRINGS.text(TextKey::Schedules), "Sendepläne");
        assert_eq!(STRINGS.text(TextKey::ScheduleItem), "Sendeplanelement");
        assert_eq!(
            STRINGS.schedule_item_fallback_label(7),
            "Sendeplanelement #7"
        );
        assert_eq!(STRINGS.text(TextKey::Item), "Element");
        assert_eq!(STRINGS.text(TextKey::Items), "Elemente");
        assert_eq!(STRINGS.text(TextKey::FillerPreset), "Filler-Vorlage");
        assert_eq!(STRINGS.text(TextKey::DecoTemplate), "Deko-Vorlage");
        // Product nouns ErsatzTV keeps verbatim stay verbatim in German too.
        assert_eq!(STRINGS.text(TextKey::Playout), "Playout");
        assert_eq!(STRINGS.text(TextKey::Block), "Block");
        assert_eq!(STRINGS.text(TextKey::Filler), "Filler");
    }

    #[test]
    fn stable_workflow_errors_are_german() {
        assert_eq!(
            STRINGS.schedule_error_message("Schedule name is required!"),
            "Der Sendeplanname ist erforderlich!"
        );
        assert_eq!(
            STRINGS.scheduling_content_error_message("DecoGroup 3 does not exist."),
            "Die Deko-Gruppe existiert nicht."
        );
        assert_eq!(
            STRINGS.settings_error_message("Tuner count must be greater than zero"),
            "Die Tuner-Anzahl muss größer als null sein"
        );
        assert_eq!(
            STRINGS.watermark_error_message("Opacity must be between 1 and 100"),
            "Die Deckkraft muss zwischen 1 und 100 liegen"
        );
        assert_eq!(
            STRINGS.addon_media_list_error_message("Invalid add-on list url"),
            "Ungültige add-on-Listen-URL."
        );
    }

    #[test]
    fn technical_identifiers_survive_translation() {
        assert_eq!(
            STRINGS.ffmpeg_profile_video_format_label(1, 2),
            "H264 / NVENC"
        );
        assert_eq!(STRINGS.ffmpeg_profile_audio_format_label(2), "AC3");
        assert_eq!(
            STRINGS.settings_error_message("Invalid option: hls-direct"),
            "Ungültige Option: hls-direct"
        );
    }

    #[test]
    fn unknown_diagnostic_values_pass_through() {
        assert_eq!(
            STRINGS.media_browser_error_message("custom database failure"),
            "custom database failure"
        );
        assert_eq!(STRINGS.channel_streaming_mode_label(99), "Unbekannt (99)");
        assert_eq!(STRINGS.channel_language_label("tlh"), "tlh");
    }
}
