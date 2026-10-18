use crate::admin::i18n::{LanguageStrings, TextKey};

pub const SORT_ORDER: u16 = 40;
pub const CODE: &str = "nl";
pub const LABEL: &str = "Nederlands";
pub const ALIASES: &[&str] = &["nl", "nl-nl", "nl_nl"];

pub struct DutchStrings;

pub static STRINGS: DutchStrings = DutchStrings;

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

impl LanguageStrings for DutchStrings {
    fn text(&self, key: TextKey) -> &'static str {
        text(key)
    }

    fn selected_count(&self, count: usize) -> String {
        format!("{count} geselecteerd")
    }

    fn item_count(&self, count: u64) -> String {
        if count == 1 {
            "1 item".to_owned()
        } else {
            format!("{count} items")
        }
    }

    fn range_of(&self, start: u32, end: u32, total: u64) -> String {
        format!("{start}-{end} van {total}")
    }

    fn match_status(&self, matched: u64, total: u64) -> String {
        format!("{matched} van {total}")
    }

    fn rows_per_page(&self, page_size: u32) -> String {
        format!("Rijen per pagina: {page_size}")
    }

    fn page_of(&self, page: u32, total: u32) -> String {
        format!("Pagina {page} van {total}")
    }

    fn save_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Opslaan mislukt: {error}")
    }

    fn delete_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Verwijderen mislukt: {error}")
    }

    fn empty_trash_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Prullenbak legen mislukt: {error}")
    }

    fn search_count(&self, total: u32, label: &'static str) -> String {
        format!("{total} {label}")
    }

    fn selected_items_count(&self, count: usize) -> String {
        if count == 1 {
            "1 item geselecteerd".to_owned()
        } else {
            format!("{count} items geselecteerd")
        }
    }

    fn delete_selected_count(&self, count: usize) -> String {
        format!("Geselecteerde verwijderen ({count})")
    }

    fn deleted_items_from_database(&self, count: u64) -> String {
        if count == 1 {
            "1 item uit de database verwijderd.".to_owned()
        } else {
            format!("{count} items uit de database verwijderd.")
        }
    }

    fn season_display_name(&self, number: i64) -> String {
        if number == 0 {
            "Specials".to_owned()
        } else {
            format!("Seizoen {number}")
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
        format!("Media-item #{id}")
    }

    fn schedule_item_fallback_label(&self, id: i64) -> String {
        format!("Schema-item #{id}")
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
            1 => "MPEG-TS (verouderd)".to_owned(),
            2 => "HLS direct".to_owned(),
            4 => "HLS-segmenter".to_owned(),
            5 => "MPEG-TS".to_owned(),
            _ => format!("Onbekend ({mode})"),
        }
    }

    fn channel_language_label(&self, code: &str) -> String {
        match code.trim().to_ascii_lowercase().as_str() {
            "" => String::new(),
            "eng" | "en" => "Engels".to_owned(),
            "nld" | "dut" | "nl" => "Nederlands".to_owned(),
            "deu" | "ger" | "de" => "Duits".to_owned(),
            "fra" | "fre" | "fr" => "Frans".to_owned(),
            "spa" | "es" => "Spaans".to_owned(),
            "ita" | "it" => "Italiaans".to_owned(),
            "jpn" | "ja" => "Japans".to_owned(),
            "kor" | "ko" => "Koreaans".to_owned(),
            "zho" | "chi" | "zh" => "Chinees".to_owned(),
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
            _ => "Onbekend",
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
            format!("{label} (niet beschikbaar op dit systeem)")
        }
    }

    fn ffmpeg_tonemap_algorithm_label(&self, algorithm: i32) -> String {
        match algorithm {
            0 => "Lineair",
            1 => "Clippen",
            2 => "Gamma",
            3 => "Reinhard",
            4 => "Mobius",
            5 => "Hable",
            _ => return format!("Onbekend ({algorithm})"),
        }
        .to_owned()
    }

    fn ffmpeg_output_dynamic_range_label(&self, range: i32) -> String {
        match range {
            0 => "SDR (BT.709)",
            1 => "HDR10 (BT.2020 PQ)",
            _ => return format!("Onbekend ({range})"),
        }
        .to_owned()
    }

    fn ffmpeg_loudness_mode_label(&self, mode: i32) -> String {
        match mode {
            0 => text(TextKey::Off),
            1 => "loudnorm",
            _ => return format!("Onbekend ({mode})"),
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
        format!("Prullenbak geleegd: {count} items verwijderd.")
    }

    fn select_collection_to_add_selected_items(&self, count: usize) -> String {
        format!("Selecteer de collectie om {count} geselecteerde items toe te voegen")
    }

    fn select_playlist_to_add_selected_items(&self, count: usize) -> String {
        format!("Selecteer de afspeellijst om {count} geselecteerde items toe te voegen")
    }

    fn added_items_to_collection(&self, count: usize) -> String {
        format!("{count} items toegevoegd aan collectie")
    }

    fn added_items_to_playlist(&self, count: usize) -> String {
        format!("{count} items toegevoegd aan afspeellijst")
    }

    fn add_to_collection_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Toevoegen aan collectie mislukt: {error}")
    }

    fn add_to_playlist_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Toevoegen aan afspeellijst mislukt: {error}")
    }

    fn no_media_source_configured(&self, provider: &'static str) -> String {
        format!(
            "Geen {provider}-bron geconfigureerd. Klik op Verbinding configureren om er een toe te voegen."
        )
    }

    fn provider_connection_title(&self, provider: &'static str) -> String {
        format!("{provider}-verbinding")
    }

    fn provider_libraries_title(&self, provider: &'static str) -> String {
        format!("{provider}-bibliotheken")
    }

    fn provider_path_replacements_title(&self, provider: &'static str) -> String {
        format!("{provider}-padvervangingen")
    }

    fn provider_path_label(&self, provider: &'static str) -> String {
        format!("{provider}-pad")
    }

    fn health_summary(&self, errors: u32, warnings: u32) -> String {
        format!("{errors} fout(en), {warnings} waarschuwing(en)")
    }

    fn classic_playout_state_message(&self, playouts: &str) -> String {
        format!(
            "De klassieke uitzending vereist aandacht voor: {playouts}. Controleer de programmatijden en de opgeslagen voortzettingsstatus. Vernieuwen vereist een gecontroleerde programmagrens en een volledig controlepunt; onveilige verzoeken laten gepubliceerde programma’s ongewijzigd."
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
            "De status van scanvernieuwing vereist aandacht: {unresolved_resolutions} onvoltooide impactbepaling(en), {stale_resolutions} vastgelopen impactbepaling(en), {failed_intents} mislukte vernieuwingsopdracht(en) voor uitzendingen en {stale_intents} vastgelopen vernieuwingsopdracht(en) voor uitzendingen. Controleer de scanner- en planningslogboeken en probeer daarna de betreffende scan of uitzendingsvernieuwing opnieuw."
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
            "Bij {recent} kana(a)l(en) werd live HLS-media in de afgelopen {minutes} minuten langzamer dan de werkelijke tijd geleverd; {active} was/waren bij de laatste meting nog niet hersteld; {fallbacks} kana(a)l(en) gebruiken maximaal {fallback_hours} uur een geverifieerde terugval naar software-encoding en/of lagere kwaliteit. Controleer de streaminglogboeken en voer de leveringsbewuste TSDuck-afspeeltest uit voordat u providerinstellingen wijzigt."
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
            "De cardinaliteit van afbeeldingen vereist aandacht: {show_duplicate_rows} dubbele afbeeldingsrij(en) voor series, {season_duplicate_rows} dubbele afbeeldingsrij(en) voor seizoenen en {episode_duplicate_rows} dubbele afbeeldingsrij(en) voor afleveringen; de aantallen voor de televisieprojectie zijn {published_items} gepubliceerde afleveringitem(s) uit add-onlijsten, {projection_rows} projectierij(en), {distinct_episodes} unieke geprojecteerde aflevering(en), {complete_hierarchy_rows} volledige hiërarchieprojectie(s) en {hierarchy_episodes} hiërarchieaflevering(en). Vernieuw of publiceer de betreffende add-ontelevisielijsten opnieuw."
        )
    }

    fn unable_to_load_playback_troubleshooting(&self, error: &dyn std::fmt::Display) -> String {
        format!("Kan context voor afspeelprobleemoplossing niet laden: {error}")
    }

    fn playout_reset_prompt(&self, channel: &str, schedule: &str) -> String {
        format!(
            "Voer de tijd in waarop de uitzending op kanaal {channel} met schema {schedule} elke dag opnieuw moet worden opgebouwd"
        )
    }
}

fn schedule_error_message(error: &str) -> String {
    match error.trim() {
        "Schedule name is required!" => "Schemanaam is verplicht!".to_owned(),
        "New Schedule Name is required." => "Nieuwe schemanaam is verplicht.".to_owned(),
        "Schedule name must be 50 characters or less." => {
            "Schemanaam mag maximaal 50 tekens zijn.".to_owned()
        }
        "Schedule name must be unique" => "Schemanaam moet uniek zijn".to_owned(),
        "Schedule does not exist" => "Schema bestaat niet".to_owned(),
        "[ProgramScheduleId] does not exist." => "[ProgramScheduleId] bestaat niet.".to_owned(),
        "A collection must use the same playback order in every schedule item." => {
            "Een collectie moet in elk schema-item dezelfde afspeelvolgorde gebruiken.".to_owned()
        }
        "Shuffle In Order cannot be used with Fill With Group Mode." => {
            "Shuffle in volgorde kan niet worden gebruikt met Fill With Group Mode.".to_owned()
        }
        "Fixed schedule items require a start time." => {
            "Vaste schema-items vereisen een starttijd.".to_owned()
        }
        "Multi Collection items can only use Shuffle or Shuffle In Order." => {
            "Multi-collectie-items kunnen alleen Shuffle of Shuffle in volgorde gebruiken.".to_owned()
        }
        "Playlist Item Size can only be used with Playlist schedule items." => {
            "Playlistitemgrootte kan alleen worden gebruikt met schema-items van het type playlist.".to_owned()
        }
        "Multiple playout mode with Count requires a count expression." => {
            "Meervoudige uitzendmodus met Aantal vereist een aantalexpressie.".to_owned()
        }
        "Duration playout mode requires a duration between 1 minute and 24 hours." => {
            "Duur-uitzendmodus vereist een duur tussen 1 minuut en 24 uur.".to_owned()
        }
        "Duration playout mode requires discard-to-fill attempts." => {
            "Duur-uitzendmodus vereist pogingen voor discard-to-fill.".to_owned()
        }
        "Tail Mode Filler requires a tail filler preset." => {
            "Tailmodus Filler vereist een tail-fillerpreset.".to_owned()
        }
        "Tail filler can only be set when Tail Mode is Filler." => {
            "Tail-filler kan alleen worden ingesteld wanneer Tailmodus Filler is.".to_owned()
        }
        "Collection is required." => "Collectie is verplicht.".to_owned(),
        "Media item is required." => "Media-item is verplicht.".to_owned(),
        "Multi Collection is required." => "Multi-collectie is verplicht.".to_owned(),
        "Smart Collection is required." => "Slimme collectie is verplicht.".to_owned(),
        "Playlist is required." => "Playlist is verplicht.".to_owned(),
        "Rerun Collection is required." => "Herhalingscollectie is verplicht.".to_owned(),
        "Search Query is required." => "Zoekquery is verplicht.".to_owned(),
        "Only one pre-, mid-, or post-roll filler can pad to the nearest minute." => {
            "Slechts een pre-, mid- of post-roll filler kan aanvullen tot de dichtstbijzijnde minuut.".to_owned()
        }
        "Padding filler requires a fallback filler." => {
            "Aanvulfiller vereist een fallback-filler.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn scheduling_content_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("Block group name \"") && trimmed.ends_with("\" is invalid") {
        return "Blokgroepnaam mag maximaal 50 tekens zijn.".to_owned();
    }
    if trimmed.starts_with("Block name \"") && trimmed.ends_with("\" is invalid") {
        return "Bloknaam mag maximaal 50 tekens zijn.".to_owned();
    }
    if trimmed.starts_with("Template name \"") && trimmed.ends_with("\" is invalid") {
        return "Sjabloonnaam mag maximaal 50 tekens zijn.".to_owned();
    }
    if trimmed.starts_with("A template named \"")
        && trimmed.ends_with("\" already exists in that template group")
    {
        return "Er bestaat al een sjabloon met die naam in die sjabloongroep.".to_owned();
    }
    if trimmed.starts_with("Block from ") && trimmed.contains(" intersects block from ") {
        return "Blok overlapt met een ander blok.".to_owned();
    }
    if trimmed.starts_with("TemplateGroup ") && trimmed.ends_with(" does not exist.") {
        return "Sjabloongroep bestaat niet.".to_owned();
    }
    if trimmed.starts_with("Template ") && trimmed.ends_with(" does not exist.") {
        return "Sjabloon bestaat niet.".to_owned();
    }
    if trimmed.starts_with("DecoGroup ") && trimmed.ends_with(" does not exist.") {
        return "Decoratiegroep bestaat niet.".to_owned();
    }
    if trimmed.starts_with("Deco ") && trimmed.ends_with(" does not exist.") {
        return "Decoratie bestaat niet.".to_owned();
    }
    if trimmed.starts_with("DecoTemplateGroup ") && trimmed.ends_with(" does not exist.") {
        return "Decoratiesjabloongroep bestaat niet.".to_owned();
    }
    if trimmed.starts_with("DecoTemplate ") && trimmed.ends_with(" does not exist.") {
        return "Decoratiesjabloon bestaat niet.".to_owned();
    }
    if trimmed.starts_with("Deco name \"") && trimmed.ends_with("\" is invalid") {
        return "Decoratienaam mag maximaal 50 tekens zijn.".to_owned();
    }
    if trimmed.starts_with("A deco named \"")
        && trimmed.ends_with("\" already exists in that deco group")
    {
        return "Er bestaat al een decoratie met die naam in die decoratiegroep.".to_owned();
    }
    if trimmed.starts_with("Deco template name \"") && trimmed.ends_with("\" is invalid") {
        return "Decoratiesjabloonnaam mag maximaal 50 tekens zijn.".to_owned();
    }
    if trimmed.starts_with("A deco template named \"")
        && trimmed.ends_with("\" already exists in that deco template group")
    {
        return "Er bestaat al een decoratiesjabloon met die naam in die decoratiesjabloongroep."
            .to_owned();
    }
    if trimmed.starts_with("Watermark ") && trimmed.ends_with(" does not exist.") {
        return "Zenderlogo bestaat niet.".to_owned();
    }
    if trimmed.starts_with("Graphics element ") && trimmed.ends_with(" does not exist.") {
        return "Grafisch element bestaat niet.".to_owned();
    }
    if trimmed.starts_with("Deco from ") && trimmed.contains(" intersects deco from ") {
        return "Decoratie overlapt met een andere decoratie.".to_owned();
    }

    match trimmed {
        "Block does not exist." => "Blok bestaat niet.".to_owned(),
        "Block group name is required" => "Blokgroepnaam is verplicht".to_owned(),
        "Block group name must be unique" => "Blokgroepnaam moet uniek zijn".to_owned(),
        "Block name is required" => "Bloknaam is verplicht".to_owned(),
        "A block named with that value already exists in that block group" => {
            "Er bestaat al een blok met die naam in die blokgroep.".to_owned()
        }
        "Block duration must be between 5 minutes and 24 hours" => {
            "Blokduur moet tussen 5 minuten en 24 uur liggen.".to_owned()
        }
        "[Collection] is required for collection type 'Collection'" => {
            "Collectie is verplicht voor collectietype Collectie.".to_owned()
        }
        "[MediaItem] is required for collection type 'TelevisionShow'" => {
            "Media-item is verplicht voor collectietype Tv-serie.".to_owned()
        }
        "[MediaItem] is required for collection type 'TelevisionSeason'" => {
            "Media-item is verplicht voor collectietype Tv-seizoen.".to_owned()
        }
        "[MultiCollection] is required for collection type 'MultiCollection'" => {
            "Multi-collectie is verplicht voor collectietype Multi-collectie.".to_owned()
        }
        "[SmartCollection] is required for collection type 'SmartCollection'" => {
            "Slimme collectie is verplicht voor collectietype Slimme collectie.".to_owned()
        }
        "[SearchQuery] is required for collection type 'SearchQuery'" => {
            "Zoekquery is verplicht voor collectietype Zoekquery.".to_owned()
        }
        "[CollectionType] is invalid" => "Collectietype is ongeldig.".to_owned(),
        "[BlockId] does not exist." => "Blok bestaat niet.".to_owned(),
        "Template does not exist." => "Sjabloon bestaat niet.".to_owned(),
        "Template group name is required" => "Sjabloongroepnaam is verplicht".to_owned(),
        "Template group name must be 50 characters or fewer" => {
            "Sjabloongroepnaam mag maximaal 50 tekens zijn.".to_owned()
        }
        "Template group name must be unique" => "Sjabloongroepnaam moet uniek zijn".to_owned(),
        "Template group does not exist." => "Sjabloongroep bestaat niet.".to_owned(),
        "[TemplateId] does not exist." => "Sjabloon bestaat niet.".to_owned(),
        "Template name is required" => "Sjabloonnaam is verplicht".to_owned(),
        "Template items cannot cross midnight." => {
            "Sjabloonitems mogen middernacht niet overschrijden.".to_owned()
        }
        "Deco group name is required" => "Decoratiegroepnaam is verplicht".to_owned(),
        "Deco group name must be 50 characters or fewer" => {
            "Decoratiegroepnaam mag maximaal 50 tekens zijn.".to_owned()
        }
        "Deco group name must be unique" => "Decoratiegroepnaam moet uniek zijn".to_owned(),
        "Deco group does not exist." => "Decoratiegroep bestaat niet.".to_owned(),
        "Deco name is required" => "Decoratienaam is verplicht".to_owned(),
        "Deco does not exist" => "Decoratie bestaat niet.".to_owned(),
        "Default filler has unsupported collection type." => {
            "Standaardfiller heeft een niet-ondersteund collectietype.".to_owned()
        }
        "Dead air fallback has unsupported collection type." => {
            "Dead-air-fallback heeft een niet-ondersteund collectietype.".to_owned()
        }
        "Break content has unsupported collection type." => {
            "Onderbrekingsinhoud heeft een niet-ondersteund collectietype.".to_owned()
        }
        "Default filler must have valid item" => {
            "Standaardfiller moet een geldig item hebben.".to_owned()
        }
        "Dead air fallback must have valid item" => {
            "Dead-air-fallback moet een geldig item hebben.".to_owned()
        }
        "Break content collection must have valid item" => {
            "Collectie voor onderbrekingsinhoud moet een geldig item hebben.".to_owned()
        }
        "Break content multi collection must have valid item" => {
            "Multi-collectie voor onderbrekingsinhoud moet een geldig item hebben.".to_owned()
        }
        "Break content smart collection must have valid item" => {
            "Slimme collectie voor onderbrekingsinhoud moet een geldig item hebben.".to_owned()
        }
        "Break content media item must have valid item" => {
            "Media-item voor onderbrekingsinhoud moet geldig zijn.".to_owned()
        }
        "Break content playlist must have valid item" => {
            "Afspeellijst voor onderbrekingsinhoud moet een geldig item hebben.".to_owned()
        }
        "Default filler item does not exist." => {
            "Item voor standaardfiller bestaat niet.".to_owned()
        }
        "Dead air fallback item does not exist." => {
            "Item voor dead-air-fallback bestaat niet.".to_owned()
        }
        "Break content collection item does not exist." => {
            "Collectie-item voor onderbrekingsinhoud bestaat niet.".to_owned()
        }
        "Break content multi collection item does not exist." => {
            "Multi-collectie-item voor onderbrekingsinhoud bestaat niet.".to_owned()
        }
        "Break content smart collection item does not exist." => {
            "Slimme-collectie-item voor onderbrekingsinhoud bestaat niet.".to_owned()
        }
        "Break content media item item does not exist." => {
            "Media-item voor onderbrekingsinhoud bestaat niet.".to_owned()
        }
        "Break content playlist item does not exist." => {
            "Afspeellijst voor onderbrekingsinhoud bestaat niet.".to_owned()
        }
        "Deco may only contain one [Block Start] break content" => {
            "Decoratie mag maar een onderbrekingsinhoud aan het begin van het blok bevatten."
                .to_owned()
        }
        "Deco may only contain one [Between Block Items] break content" => {
            "Decoratie mag maar een onderbrekingsinhoud tussen blokitems bevatten.".to_owned()
        }
        "Deco may only contain one [At Chapter Markers] break content" => {
            "Decoratie mag maar een onderbrekingsinhoud bij hoofdstukmarkeringen bevatten."
                .to_owned()
        }
        "Deco may only contain one [Block Finish] break content" => {
            "Decoratie mag maar een onderbrekingsinhoud aan het einde van het blok bevatten."
                .to_owned()
        }
        "Deco template group name is required" => {
            "Decoratiesjabloongroepnaam is verplicht".to_owned()
        }
        "Deco template group name must be 50 characters or fewer" => {
            "Decoratiesjabloongroepnaam mag maximaal 50 tekens zijn.".to_owned()
        }
        "Deco template group name must be unique" => {
            "Decoratiesjabloongroepnaam moet uniek zijn".to_owned()
        }
        "Deco template group does not exist." => "Decoratiesjabloongroep bestaat niet.".to_owned(),
        "[DecoTemplateId] does not exist." => "Decoratiesjabloon bestaat niet.".to_owned(),
        "Deco template name is required" => "Decoratiesjabloonnaam is verplicht".to_owned(),
        "[DecoId] does not exist." => "Decoratie bestaat niet.".to_owned(),
        "Deco template items cannot be empty." => {
            "Decoratiesjabloonitems mogen niet leeg zijn.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn watermark_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("Watermark ") && trimmed.ends_with(" does not exist.") {
        return "Zenderlogo niet gevonden.".to_owned();
    }

    match trimmed {
        "Name is required" => "Naam is verplicht".to_owned(),
        "Name must be 50 characters or fewer" => "Naam mag maximaal 50 tekens bevatten".to_owned(),
        "Watermark image is required" | "Watermark image is required!" => {
            "Afbeelding voor zenderlogo is verplicht".to_owned()
        }
        "Width must be between 0 and 100%" => "Breedte moet tussen 0 en 100% liggen".to_owned(),
        "Horizontal margin must be between 0 and 50%" => {
            "Horizontale marge moet tussen 0 en 50% liggen".to_owned()
        }
        "Vertical margin must be between 0 and 50%" => {
            "Verticale marge moet tussen 0 en 50% liggen".to_owned()
        }
        "Duration must be positive and less than frequency window" => {
            "Duur moet positief zijn en korter dan het frequentievenster".to_owned()
        }
        "Opacity must be between 1 and 100" => {
            "Ondoorzichtigheid moet tussen 1 en 100 liggen".to_owned()
        }
        "Watermark name must be unique" | "ChannelWatermark name must be unique" => {
            "Naam van zenderlogo moet uniek zijn".to_owned()
        }
        "Watermark not found" | "Source watermark not found" | "Watermark does not exist." => {
            "Zenderlogo niet gevonden.".to_owned()
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
        _ => return format!("Onbekend ({value})"),
    }
    .to_owned()
}

fn ffmpeg_profile_video_format_label(format: i32) -> String {
    match format {
        0 => "Geen",
        1 => "H264",
        2 => "H265 (HEVC)",
        3 => "MPEG-2",
        4 => "AV1",
        99 => text(TextKey::Copy),
        _ => return format!("Onbekend ({format})"),
    }
    .to_owned()
}

fn ffmpeg_profile_audio_format_label(format: i32) -> String {
    match format {
        0 => "Geen",
        1 => "AAC",
        2 => "AC3",
        3 => "AAC (LATM)",
        99 => text(TextKey::Copy),
        _ => return format!("Onbekend ({format})"),
    }
    .to_owned()
}

fn ffmpeg_profile_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("An FFmpeg profile named \"") && trimmed.ends_with("\" already exists.")
    {
        if let Some(name) = trimmed
            .strip_prefix("An FFmpeg profile named \"")
            .and_then(|value| value.strip_suffix("\" already exists."))
        {
            return format!("Er bestaat al een FFmpeg-profiel met de naam \"{name}\".");
        }
    }

    match trimmed {
        "Profile name is required." => "Profielnaam is verplicht.".to_owned(),
        "Profile name must be 50 characters or fewer." => {
            "Profielnaam mag maximaal 50 tekens bevatten.".to_owned()
        }
        "Thread count must be 0 or greater." => "Aantal threads moet 0 of hoger zijn.".to_owned(),
        "QSV supports formats (h264, hevc, mpeg2video)" => {
            "QSV ondersteunt formaten (h264, hevc, mpeg2video).".to_owned()
        }
        "NVENC supports formats (h264, hevc, av1)" => {
            "NVENC ondersteunt formaten (h264, hevc, av1).".to_owned()
        }
        "VAAPI supports formats (h264, hevc, av1, mpeg2video)" => {
            "VAAPI ondersteunt formaten (h264, hevc, av1, mpeg2video).".to_owned()
        }
        "VideoToolbox supports formats (h264, hevc)" => {
            "VideoToolbox ondersteunt formaten (h264, hevc).".to_owned()
        }
        "Amf supports formats (h264, hevc)" => "Amf ondersteunt formaten (h264, hevc).".to_owned(),
        "V4L2 M2M supports formats (h264, hevc)" => {
            "V4L2 M2M ondersteunt formaten (h264, hevc).".to_owned()
        }
        "Rkmpp supports formats (h264, hevc)" => {
            "Rkmpp ondersteunt formaten (h264, hevc).".to_owned()
        }
        "HDR10 output requires hevc or av1; h264 and mpeg2video cannot carry HDR10 static metadata" => {
            "HDR10-uitvoer vereist hevc of av1; h264 en mpeg2video kunnen geen statische HDR10-metadata dragen.".to_owned()
        }
        "HDR10 output requires 10-bit; 8-bit cannot represent a PQ signal without banding" => {
            "HDR10-uitvoer vereist 10-bit; 8-bit kan een PQ-signaal niet zonder banding weergeven.".to_owned()
        }
        "Mastering display must look like G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1), with chromaticities up to 50000 and max luminance above min" => {
            "Het masteringscherm moet eruitzien als G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1), met chromaticiteiten tot 50000 en een max-luminantie boven de min.".to_owned()
        }
        "MaxCLL and MaxFALL must both be set (or both left empty), at most 65535, with MaxFALL no greater than MaxCLL" => {
            "MaxCLL en MaxFALL moeten allebei ingevuld zijn (of allebei leeg blijven), maximaal 65535, met MaxFALL niet groter dan MaxCLL.".to_owned()
        }
        "Mpeg2Video does not support 10-bit content" => {
            "Mpeg2Video ondersteunt geen 10-bit-content.".to_owned()
        }
        "VideoProfile must be high10 with 10-bit h264" => {
            "Videoprofiel moet high10 zijn bij 10-bit h264.".to_owned()
        }
        "VideoProfile must be high444p with NVIDIA 10-bit h264" => {
            "Videoprofiel moet high444p zijn bij NVIDIA 10-bit h264.".to_owned()
        }
        "VideoProfile cannot be high10 with 8-bit h264" => {
            "Videoprofiel mag niet high10 zijn bij 8-bit h264.".to_owned()
        }
        "Source profile not found." => "Bronprofiel niet gevonden.".to_owned(),
        "No FFmpeg resolutions are available; database bootstrap is incomplete" => {
            "Er zijn geen FFmpeg-resoluties beschikbaar; database-bootstrap is onvolledig"
                .to_owned()
        }
        _ => error.to_owned(),
    }
}

fn settings_error_message(error: &str) -> String {
    let trimmed = error.trim();
    let trimmed = trimmed
        .strip_prefix("update Windows startup registration is unavailable: ")
        .unwrap_or(trimmed);

    if let Some((current, limit)) = xmltv_days_error_parts(trimmed) {
        return format!(
            "Aantal XMLTV-dagen om te bouwen ({current}) mag niet groter zijn dan het aantal uitzenddagen om te bouwen ({limit})"
        );
    }
    if let Some(name) = trimmed.strip_suffix(" path does not exist") {
        return format!("{name}-pad bestaat niet");
    }
    if let Some(name) = trimmed
        .strip_prefix("Unable to verify ")
        .and_then(|value| value.strip_suffix(" version"))
    {
        return format!("Kan {name}-versie niet controleren");
    }
    if let Some(value) = trimmed.strip_prefix("Invalid option: ") {
        return format!("Ongeldige optie: {value}");
    }
    if trimmed.starts_with("Resolution ") && trimmed.ends_with(" does not exist.") {
        return "Resolutie bestaat niet.".to_owned();
    }

    match trimmed {
        "Windows startup registration is not supported on this platform" => {
            "Windows-opstartregistratie wordt niet ondersteund op dit platform.".to_owned()
        }
        "ErsatzRS.exe was not found next to ersatzrs-web.exe" => {
            "ErsatzRS.exe is niet naast ersatzrs-web.exe gevonden.".to_owned()
        }
        "Unable to update Windows startup registration" => {
            "De Windows-opstartregistratie kon niet worden bijgewerkt.".to_owned()
        }
        "Tuner count must be greater than zero" => "Aantal tuners moet groter zijn dan nul.".to_owned(),
        "Library refresh interval must be 0 (do not refresh) or greater than zero" => {
            "Bibliotheekverversingsinterval moet 0 (niet verversen) of groter dan nul zijn.".to_owned()
        }
        "Library refresh interval must be less than 1,000,000. Use 0 to disable automatic refresh" => {
            "Bibliotheekverversingsinterval moet kleiner zijn dan 1.000.000. Gebruik 0 om automatisch verversen uit te schakelen.".to_owned()
        }
        "Playout days to build must be greater than zero" => "Aantal uitzenddagen om te bouwen moet groter zijn dan nul.".to_owned(),
        "Scripted schedule timeout seconds must be at least 10" => {
            "Time-out voor Scripted schema moet minstens 10 seconden zijn.".to_owned()
        }
        "XMLTV days to build must be greater than zero" => "Aantal XMLTV-dagen om te bouwen moet groter zijn dan nul.".to_owned(),
        "Resolution width or height is invalid" => "Resolutiebreedte of -hoogte is ongeldig.".to_owned(),
        "Resolution width and height must be unique" => "Resolutiebreedte en -hoogte moeten uniek zijn.".to_owned(),
        "HLS Segmenter idle timeout must be greater than or equal to 30" => {
            "Inactiviteitstime-out van HLS-segmenter moet groter dan of gelijk aan 30 zijn.".to_owned()
        }
        "Work-Ahead HLS Segmenter limit must be greater than or equal to 0" => {
            "Work-ahead-limiet van HLS-segmenter moet groter dan of gelijk aan 0 zijn.".to_owned()
        }
        "Active HLS transcode limit must be greater than or equal to 1" => {
            "Limiet voor actieve HLS-transcodes moet groter dan of gelijk aan 1 zijn.".to_owned()
        }
        "HLS Segmenter initial segment count must be greater than or equal to 1" => {
            "Initieel segmentaantal van HLS-segmenter moet groter dan of gelijk aan 1 zijn.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn media_action_error_message(error: &str) -> String {
    match error.trim() {
        "Collection name is required" => "Collectienaam is verplicht.".to_owned(),
        "Collection name must be 50 characters or less" => {
            "Collectienaam mag maximaal 50 tekens zijn.".to_owned()
        }
        "Collection name must be unique" => "Collectienaam moet uniek zijn.".to_owned(),
        "Media item does not exist" => "Media-item bestaat niet.".to_owned(),
        "Collection does not exist" | "Collection does not exist." => {
            "Collectie bestaat niet.".to_owned()
        }
        "Playlist does not exist" | "Playlist does not exist." => {
            "Afspeellijst bestaat niet.".to_owned()
        }
        "Unable to locate media item" => "Kan media-item niet vinden.".to_owned(),
        "Media action failed" => "Media-actie mislukt.".to_owned(),
        _ => error.to_owned(),
    }
}

fn filler_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("FillerPreset ") && trimmed.ends_with(" does not exist.") {
        return "Fillerpreset bestaat niet.".to_owned();
    }

    match trimmed {
        "Filler preset does not exist" => "Fillerpreset bestaat niet.".to_owned(),
        "Filler preset name must be unique" => {
            "Naam van fillerpreset moet uniek zijn.".to_owned()
        }
        "Name must not be empty" => "Naam mag niet leeg zijn.".to_owned(),
        "Name must be 50 characters or fewer" => {
            "Naam mag maximaal 50 tekens zijn.".to_owned()
        }
        "Filler kind must not be None" => "Fillertype mag niet leeg zijn.".to_owned(),
        "Filler mode must not be None" => "Fillermodus mag niet leeg zijn.".to_owned(),
        "Count must be greater than or equal to 0" => {
            "Aantal moet groter dan of gelijk aan 0 zijn.".to_owned()
        }
        "Duration must not be empty" => "Duur mag niet leeg zijn.".to_owned(),
        "Pad To Nearest Minute must not be empty" => {
            "Afronden naar dichtstbijzijnde minuut mag niet leeg zijn.".to_owned()
        }
        "Use Chapters As Media Items must not be enabled for fallback filler" => {
            "Hoofdstukken als media-items gebruiken mag niet zijn ingeschakeld voor fallback-filler.".to_owned()
        }
        "Collection must not be empty" => "Collectie mag niet leeg zijn.".to_owned(),
        "Multi Collection must not be empty" => "Multi-collectie mag niet leeg zijn.".to_owned(),
        "Smart Collection must not be empty" => "Slimme collectie mag niet leeg zijn.".to_owned(),
        "Media item must not be empty" => "Media-item mag niet leeg zijn.".to_owned(),
        _ => error.to_owned(),
    }
}

fn media_browser_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.ends_with("Collection does not contain the selected item.") {
        return "De collectie bevat het geselecteerde item niet.".to_owned();
    }
    if trimmed.contains("collection with id ") && trimmed.ends_with(" not found") {
        return "Collectie bestaat niet.".to_owned();
    }
    if trimmed == "Pinned stream selection is invalid or no longer available." {
        return "De vastgezette streamselectie is ongeldig of niet meer beschikbaar.".to_owned();
    }
    if trimmed == "Movie was not found." {
        return "Film is niet gevonden.".to_owned();
    }
    if trimmed.starts_with("Collection ") && trimmed.ends_with(" is already present.") {
        return "Collectie is al aanwezig.".to_owned();
    }
    if trimmed.starts_with("Collection ") && trimmed.ends_with(" does not exist.") {
        return "Collectie bestaat niet.".to_owned();
    }
    if trimmed.starts_with("Smart collection ") && trimmed.ends_with(" is already present.") {
        return "Slimme collectie is al aanwezig.".to_owned();
    }
    if trimmed.starts_with("Smart collection ") && trimmed.ends_with(" does not exist.") {
        return "Slimme collectie bestaat niet.".to_owned();
    }
    if trimmed.starts_with("MultiCollection ") && trimmed.ends_with(" does not exist.") {
        return "Multi-collectie bestaat niet.".to_owned();
    }
    if trimmed.starts_with("Rerun collection ") && trimmed.ends_with(" does not exist.") {
        return "Herhalingscollectie bestaat niet.".to_owned();
    }
    if trimmed.starts_with("Referenced item ") && trimmed.ends_with(" does not exist.") {
        return "Verwezen item bestaat niet.".to_owned();
    }
    if trimmed.starts_with("PlaylistGroup ") && trimmed.ends_with(" does not exist.") {
        return "Afspeellijstgroep bestaat niet.".to_owned();
    }
    if trimmed.starts_with("Playlist ") && trimmed.ends_with(" does not exist.") {
        return "Afspeellijst bestaat niet.".to_owned();
    }
    if trimmed.starts_with("Playlist name \"") && trimmed.ends_with("\" is invalid") {
        return "Afspeellijstnaam is ongeldig.".to_owned();
    }
    if let Some(error) = trimmed.strip_prefix("Playlist preview failed: ") {
        return format!("Voorbeeld van afspeellijst mislukt: {error}");
    }
    if trimmed.starts_with("Media item ") && trimmed.ends_with(" does not exist.") {
        return "Media-item bestaat niet.".to_owned();
    }

    match trimmed {
        "Collection name is required" => "Collectienaam is verplicht.".to_owned(),
        "Collection name must be 50 characters or less" => {
            "Collectienaam mag maximaal 50 tekens zijn.".to_owned()
        }
        "Collection name must be unique" => "Collectienaam moet uniek zijn.".to_owned(),
        "Collection does not exist." => "Collectie bestaat niet.".to_owned(),
        "Smart collection name is required" => "Naam van slimme collectie is verplicht.".to_owned(),
        "Smart collection query is required" => {
            "Zoekopdracht voor slimme collectie is verplicht.".to_owned()
        }
        "SmartCollection name must be unique" => {
            "Naam van slimme collectie moet uniek zijn.".to_owned()
        }
        "MultiCollection does not exist." => "Multi-collectie bestaat niet.".to_owned(),
        "Multi-collection name is required!" => "Naam van multi-collectie is verplicht.".to_owned(),
        "Multi-collection name cannot be longer than 50 characters." => {
            "Naam van multi-collectie mag maximaal 50 tekens zijn.".to_owned()
        }
        "MultiCollection name must be unique" => {
            "Naam van multi-collectie moet uniek zijn.".to_owned()
        }
        "Each multi-collection item must reference exactly one collection." => {
            "Elk multi-collectie-item moet precies een collectie verwijzen.".to_owned()
        }
        "Rerun collection does not exist." => "Herhalingscollectie bestaat niet.".to_owned(),
        "[Name] is an empty string" | "[Name] must not be empty." => {
            "Naam mag niet leeg zijn.".to_owned()
        }
        "[Name] must not be longer than 50" | "[Name] must not be longer than 50." => {
            "Naam mag maximaal 50 tekens zijn.".to_owned()
        }
        "Rerun collection name must be unique" => {
            "Naam van herhalingscollectie moet uniek zijn.".to_owned()
        }
        "Unsupported collection type." => "Niet-ondersteund collectietype.".to_owned(),
        "[ProgramScheduleId] does not exist." => "Schema bestaat niet.".to_owned(),
        "[MediaItem] does not exist." => "Media-item bestaat niet.".to_owned(),
        "[PlaylistGroupId] does not exist." => "Afspeellijstgroep bestaat niet.".to_owned(),
        "Cannot delete system playlist group" => {
            "Systeemafspeellijstgroep kan niet worden verwijderd.".to_owned()
        }
        "Cannot delete system (generated) playlist" => {
            "Systeemafspeellijst kan niet worden verwijderd.".to_owned()
        }
        "[PlaylistId] does not exist." => "Afspeellijst bestaat niet.".to_owned(),
        "Cannot save system (generated) playlist" => {
            "Systeemafspeellijst kan niet worden opgeslagen.".to_owned()
        }
        "Playlist playout preview has no items" => {
            "Voorbeeld van afspeellijst heeft geen items.".to_owned()
        }
        "Playlist name is required" => "Afspeellijstnaam is verplicht.".to_owned(),
        "[Collection] is required for collection type 'Collection'" => {
            "Collectie is verplicht.".to_owned()
        }
        "[MultiCollection] is required for collection type 'MultiCollection'" => {
            "Multi-collectie is verplicht.".to_owned()
        }
        "[SmartCollection] is required for collection type 'SmartCollection'" => {
            "Slimme collectie is verplicht.".to_owned()
        }
        "[CollectionType] is invalid" => "Collectietype is ongeldig.".to_owned(),
        _ => error.to_owned(),
    }
}

fn media_source_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if let Some(error) = trimmed.strip_prefix("Could not reach Jellyfin: ") {
        return format!("Kan Jellyfin niet bereiken: {error}");
    }

    match trimmed {
        "Address is required." => "Adres is verplicht.".to_owned(),
        "API key is required." => "API-sleutel is verplicht.".to_owned(),
        "'Address' must be a valid URL" => "Adres moet een geldige URL zijn.".to_owned(),
        _ => error.to_owned(),
    }
}

fn addon_operation_error_message(code: &str, detail: &str) -> String {
    let summary = match code {
        "missing-setting" => "Configureer de vereiste add-on-instelling.",
        "missing-secret" => "Configureer de vereiste geheime verwijzing in de add-on-instellingen.",
        "provider-unreachable" => "De add-on kon de provider niet bereiken.",
        "provider-rejected" => "De provider heeft het add-on-verzoek geweigerd.",
        "provider-rate-limited" => "De provider heeft de add-on tijdelijk beperkt.",
        "not-found" => "Het gevraagde add-on-item is niet gevonden.",
        "unsupported-url" => "De add-on ondersteunt deze URL niet.",
        _ => "De add-on heeft een fout gemeld.",
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
        return format!("Add-on gaf ongeldige lijstgegevens terug: {error}");
    }
    if normalized.starts_with("add-on media list with id ") && normalized.ends_with(" not found") {
        return "add-on-lijst bestaat niet.".to_owned();
    }

    if let Some(folder) =
        normalized.strip_prefix("another list of this add-on already uses the folder ")
    {
        return format!("Een andere lijst van deze add-on gebruikt de map \"{folder}\" al.");
    }
    if let Some(folder) = normalized.strip_suffix(" is not usable as a folder name") {
        return format!("\"{folder}\" is niet bruikbaar als mapnaam.");
    }

    match normalized {
        "Invalid add-on list url" => "Ongeldige add-on-lijst-URL.".to_owned(),
        "add-on API client is not configured for this operation" => {
            "add-on API-client is niet geconfigureerd voor deze bewerking.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn upload_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if let Some(error) = trimmed.strip_prefix("Cannot create upload directory: ") {
        return format!("Uploadmap kan niet worden gemaakt: {error}");
    }
    if let Some(error) = trimmed.strip_prefix("Write failed: ") {
        return format!("Schrijven mislukt: {error}");
    }
    if let Some(error) = trimmed.strip_prefix("Read field: ") {
        return format!("Veld lezen mislukt: {error}");
    }

    match trimmed {
        "Upload exceeds maximum allowed size" => {
            "Upload overschrijdt de maximaal toegestane grootte.".to_owned()
        }
        "No file field" => "Geen bestandsveld gevonden.".to_owned(),
        _ => error.to_owned(),
    }
}

fn xmltv_days_error_parts(error: &str) -> Option<(&str, &str)> {
    let rest = error.strip_prefix("XMLTV days to build (")?;
    let (current, rest) = rest.split_once(") cannot be greater than Playout days to build (")?;
    let limit = rest.strip_suffix(')')?;
    Some((current, limit))
}

pub fn text(key: TextKey) -> &'static str {
    match key {
        TextKey::Center => "Midden",
        TextKey::FilePath => "Bestandspad",
        TextKey::Format => "Formaat",
        TextKey::GraphicsAuthoringInvalid => "Controleer soort, naam, geometrie, timing en YAML-waarden.",
        TextKey::GraphicsAuthoringPreviewFailed => "De native graphicsrender mislukte of overschreed de tijdslimiet. Controleer renderer en invoerbestanden.",
        TextKey::GraphicsBaseStyle => "Basisstijl",
        TextKey::GraphicsDisappear => "Verdwijnen",
        TextKey::GraphicsEndBehavior => "Gedrag aan het einde",
        TextKey::GraphicsEpgEntries => "Gidsitems",
        TextKey::GraphicsExport => "YAML exporteren",
        TextKey::GraphicsFixedSize => "Vaste grootte",
        TextKey::GraphicsFontDirectory => "Lettertypemap",
        TextKey::GraphicsFontFamily => "Lettertypefamilie",
        TextKey::GraphicsFontSize => "Lettergrootte (pixels)",
        TextKey::GraphicsGeometry => "Plaatsing en lagen",
        TextKey::GraphicsHold => "Vasthouden",
        TextKey::GraphicsHoldSeconds => "Vasthouden (seconden)",
        TextKey::GraphicsLineHeight => "Regelhoogte (pixels)",
        TextKey::GraphicsLoop => "Herhalen",
        TextKey::GraphicsLuaCommand => "Lua-bron of scriptbestand",
        TextKey::GraphicsMissingFont => "Het gekozen lettertypebestand ontbreekt of is leeg.",
        TextKey::GraphicsMissingMedia => "Het gekozen afbeeldings-, bewegings- of ondertitelbestand ontbreekt of is leeg.",
        TextKey::GraphicsMissingVariable => "Een sjabloonvariabele ontbreekt. Kies een kanaal/tijd of geef expliciete voorbeeldwaarden op.",
        TextKey::GraphicsPixelX => "X (pixels)",
        TextKey::GraphicsPixelY => "Y (pixels)",
        TextKey::GraphicsPreviewVariables => "Variabelen voor het voorbeeld (JSON-object)",
        TextKey::GraphicsRealPreview => "Werkelijk graphicsvoorbeeld",
        TextKey::GraphicsScaleWidth => "Schaalbreedte (%)",
        TextKey::GraphicsScriptArgs => "Scriptargumenten (één per regel)",
        TextKey::GraphicsSourceContent => "Binnen de broninhoud plaatsen",
        TextKey::GraphicsStartSeconds => "Start (seconden)",
        TextKey::GraphicsTextAlign => "Tekstuitlijning",
        TextKey::GraphicsTextColor => "Tekstkleur",
        TextKey::GraphicsTextFit => "Tekstaanpassing",
        TextKey::GraphicsTextWidth => "Tekstbreedte (%)",
        TextKey::GraphicsWrap => "Tekstomloop",
        TextKey::GraphicsYaml => "Graphics-YAML",
        TextKey::Left => "Links",
        TextKey::None => "Geen",
        TextKey::PixelFormat => "Pixelformaat",
        TextKey::ProgrammeSeekSeconds => "Zoeken vanaf programmastart (seconden)",
        TextKey::Right => "Rechts",
        TextKey::GraphicsDrawvgTypography => "Drawvg gebruikt ingebouwde bitmapletters. Lettertypebestanden, families en mappen worden niet gebruikt. Breedte, omloop en schaling gebruiken die bitmapafmetingen.",
        TextKey::GraphicsDrawtextTypography => "Drawtext gebruikt het gekozen echte lettertype op vaste grootte. Breedte, omloop en schaling begrenzen de tekst niet; voeg zelf regelafbrekingen toe. Controleer lange titels en bijzondere tekens in het gerenderde voorbeeld. Een lettertypebestand gaat voor de familie. Bij familiezoekopdrachten kan het systeem een ander lettertype kiezen; gebruik een bestand voor het exacte lettertype. Zonder beide keuzes wordt de meegeleverde Roboto gebruikt.",
        TextKey::GraphicsJpegXlHelp => "Compacte geanimeerde JPEG XL wordt automatisch herkend. Image herhaalt vanaf de programmafase; Motion gebruikt Start en eindgedrag. Native decodering behoudt transparantie en zoekt vooruit door gedecodeerde frames.",
        TextKey::GraphicsGeometryHelp => "Image, Text en Motion gebruiken exacte uitvoerpixels wanneer zowel X als Y is ingevuld; deze vervangen ankers, marges en opvulling rond de broninhoud. Schaling en dekking volgen de mogelijkheden van de native renderer. De positie van Subtitle en Script komt uit ASS of Lua; gedeelde plaatsingswaarden blijven bewaard maar worden niet toegepast.",
        TextKey::GraphicsLuaHelp => "Lua draait in de eigen graphics-engine en geeft een VGS-tekenscript terug. Er wordt geen externe opdracht gestart. Start en duur regelen de zichtbaarheid. Formaat en pixelformaat blijven bewaard voor YAML-compatibiliteit maar wijzigen de VGS-uitvoer niet.",
        TextKey::GraphicsRealPreviewHelp => "Render de niet-opgeslagen definitie met de native engine op een transparant vlak. Kanaal en tijdstip kiezen echte programmacontext; zoeken schuift vanaf dat tijdstip op. Het voorbeeld slaat niets op en wijzigt de uitzending niet.",
        TextKey::GraphicsYamlHelp => "Import valideert en bewaart de gekozen soort en vervangt deze definitie bij bewerken. Export bevat de niet-opgeslagen velden. Onbekende YAML-velden worden geweigerd.",

        TextKey::ExternalJsonPlayoutState => "Status van External JSON-uitzending",
        TextKey::ExternalJsonDirectHelp => "Het bronbestand wordt rechtstreeks ingelezen. Kies {refresh} of {reset} en schakel programmabehoud eenmalig uit om gecontroleerde updates te activeren. Dit kan de uitzending onderbreken.",
        TextKey::ExternalJsonAcceptedHelp => "{continue} voegt alleen toe. {refresh} en {reset} nemen toekomstige programma’s over zonder externe tijden te wijzigen. {reset} kan de volgorde van de externe maker niet terugzetten.",
        TextKey::ExternalJsonSourceHelp => "Na activering van gecontroleerde updates blijft de geaccepteerde planning gelden als je dit pad wijzigt. Gebruik {refresh} of {reset} om het nieuwe bestand over te nemen.",
        TextKey::ExternalJsonInvalidHelp => "De geaccepteerde planning is ongeldig of wordt niet ondersteund. Controleer het bronbestand en vervang de planning met {refresh} of {reset}, met programmabehoud uitgeschakeld.",
        TextKey::ExternalJsonSourceChangedHelp => "Het bronpad is gewijzigd. De geaccepteerde planning blijft gelden; gebruik {refresh} of {reset} om het nieuwe bestand over te nemen.",
        TextKey::ExternalJsonNextHelp => "External JSON-updates vereisen de ingebouwde afspeelengine op dit kanaal en de spiegelkanalen. Next leest geen externe planning.",
        TextKey::ExternalJsonExpiredHelp => "De geaccepteerde planning bevat geen toekomstige programma’s. Controleer en verleng het bronbestand en werk daarna de uitzending bij.",
        TextKey::ExternalJsonBuildFailedHelp => "De externe planning kon niet worden overgenomen. Controleer het bronbestand, de programmaovergangen en de gekozen actie. De laatst geaccepteerde planning blijft in gebruik.",
        TextKey::ExternalJsonDirect => "Rechtstreeks uit bestand",
        TextKey::ExternalJsonControlled => "Gecontroleerde updates",
        TextKey::AcceptedRevision => "Geaccepteerde versie",
        TextKey::PlayoutContinuityUnavailable => "Het hervatpunt van het programma ontbreekt of is niet compatibel. Gebruik Reset om geldige afspeelstatus op te bouwen; Refresh behoudt tot die tijd de bestaande planning.",
        TextKey::BlockPlayoutState => "Status blokuitzending",
        TextKey::YamlPlayoutState => "Status YAML-uitzending",
        TextKey::ScriptedPlayoutState => "Status scriptgestuurde uitzending",
        TextKey::ScriptedTeletextOverrideUnavailable => "Kanaal {channel}: de actieve teletekstselectie is niet beschikbaar. De gepubliceerde uitzendplanning blijft behouden.",
        TextKey::ScriptedTeletextGenerationUnavailable => "Kanaal {channel}: de pagina’s voor het actieve teletekstprofiel zijn niet gereed. De gepubliceerde uitzendplanning blijft behouden.",
        TextKey::ScriptedTimelineRejected => "De tijdlijn is afgewezen. Gepubliceerde programma’s blijven ongewijzigd. Corrigeer het script en probeer opnieuw.",
        TextKey::ItemDetailsNotRequested => "Details niet opgehaald",
        TextKey::ItemDetailsFailed => "Details ophalen mislukt",
        TextKey::ItemDetailsRetry => "Details wachten op nieuwe poging",
        TextKey::ItemProviderPlayback => "Afspelen volgens provider",
        TextKey::ItemPosition => "Positie",
        TextKey::ItemPrevious => "Vorige",
        TextKey::ItemNext => "Volgende",
        TextKey::ItemBack => "Terug naar items",
        TextKey::ItemTools => "Itemhulpmiddelen",
        TextKey::ItemEpisodePosition => "Afleveringspositie",
        TextKey::ItemNumbered => "Seizoen en aflevering",
        TextKey::ItemUnnumberedHelp => "Dit item heeft na opslaan geen seizoen- of afleveringsnummer.",
        TextKey::ItemNumberingHelp => "Een genummerd item vereist beide waarden. Seizoen 0 is voor specials.",
        TextKey::ItemMetadata => "Samenvatting, genres en tags",
        TextKey::ItemMetadataHelp => "Aanvullende titels, talen en credits",
        TextKey::ItemSaveHelp => "Sla wijzigingen op om ze automatisch toe te passen.",
        TextKey::ItemEditNumbering => "Nummering in lijst bewerken",
        TextKey::ItemReplacementHelp => "Een vervanging kiezen of verwijderen wijzigt dit concept. Gebruik Item opslaan om dit te bewaren.",
        TextKey::ItemReplacementPending => "De vervangingswijziging is niet opgeslagen.",
        TextKey::ItemSearchReplacement => "Lokale media zoeken",
        TextKey::ItemRemoveReplacement => "Vervanging verwijderen",
        TextKey::ItemInformation => "Media-informatie en herkomst",
        TextKey::ItemInformationHelp => "Audio, ondertitels, streamdetails en provideropties",
        TextKey::ItemSavedToolsHelp => "Deze acties gebruiken het opgeslagen item. Niet-opgeslagen wijzigingen tellen niet mee.",
        TextKey::ItemViewMedia => "Mediadetails bekijken",
        TextKey::ItemSavedValue => "Opgeslagen",
        TextKey::AddonWorkspaceSelectFirst => "Selecteer eerst items om te bewerken.",
        TextKey::AddonsNoneInstalled => "Er zijn nog geen add-ons geïnstalleerd.",
        TextKey::AddonsNoneAvailable => "Uw bronnen bieden nog geen add-ons aan.",
        TextKey::AddonsSetupHelp => "Open Bronnen om een repository toe te voegen of uit een bestand te installeren.",
        TextKey::AddonPackageHelp => "Gebruik deze rij voor een ZIP met één add-on.",
        TextKey::AddonRepositoryPackageHelp => "Gebruik deze rij voor een repository-ZIP, zoals ErsatzRS-addons.zip, met een catalogus van add-ons.",
        TextKey::AddonPackageUploadHelp => "Dit add-onpakket kon niet worden geïnstalleerd. Gebruik voor een repository-ZIP de rij Repositorypakket hieronder.",
        TextKey::AddonRepositoryUploadHelp => "Dit repositorypakket kon niet worden toegevoegd. Gebruik voor een ZIP met één add-on de rij Add-onpakket hierboven.",
        TextKey::AddonConfigurationSavedRecheck => "Opgeslagen. De add-on is uitgeschakeld totdat u deze opnieuw controleert en inschakelt.",
        TextKey::AddonCheckRequired => "Configuratie gewijzigd. Controleer de add-on opnieuw voordat u deze inschakelt.",
        TextKey::AddonCheckPending => "Add-on controleren…",
        TextKey::AddonWorkspaceIncludeSelected => "Geselecteerde items opnemen",
        TextKey::AddonWorkspaceExcludeSelected => "Geselecteerde items uitsluiten",
        TextKey::AddonWorkspaceInclusionHelp => "Sla selectiewijzigingen op om ze automatisch toe te passen.",
        TextKey::AddonWorkspacePublicationFailed => "Publicatie mislukt",
        TextKey::AddonWorkspacePublicationRestoredHelp => "De vorige versie is hersteld. Opgeslagen wijzigingen blijven behouden en worden automatisch opnieuw geprobeerd. Activiteit toont technische fouten.",

        TextKey::AddonWorkspaceParentMetadataPreserved => "De verwerking is gestopt om gewijzigde of ongeldige bovenliggende NFO-metadata te behouden. Herstel het bestand voordat je opnieuw probeert.",

        TextKey::AddonWorkspaceImporting => "Lijst importeren…",
        TextKey::AddonWorkspaceImportingHelp => "De lijst opent automatisch zodra het ophalen is voltooid. Gebruik Vernieuwen zonder JavaScript.",
        TextKey::AddonWorkspaceImportSteps => "Importeer de lijst. Metadata en benodigde media worden automatisch verwerkt en gepubliceerd.",
        TextKey::AddonWorkspaceChooseProvider => "Kies hieronder een add-on als er geen wordt gevonden voor deze URL.",
        TextKey::AddonWorkspaceExcluded => "Uitgesloten van publicatie",
        TextKey::AddonWorkspaceSort => "Sorteervolgorde",
        TextKey::AddonWorkspaceAscending => "Oplopend",
        TextKey::AddonWorkspaceDescending => "Aflopend",
        TextKey::AddonWorkspaceItemDate => "Datum",
        TextKey::AddonWorkspaceSelectVisible => "Zichtbare items selecteren",
        TextKey::AddonWorkspaceDeselectVisible => "Zichtbare items deselecteren",
        TextKey::ItemFromList => "Van lijst",
        TextKey::ItemUseListSetting => "Lijstinstelling gebruiken",
        TextKey::ItemOverride => "Itemafwijking",
        TextKey::ItemPackageDefault => "Pakketstandaard; nog niet geactiveerd",
        TextKey::ItemPreviewOptions => "Optiewijzigingen bekijken",
        TextKey::ItemAccessActive => "Er wordt al een toegangscontrole uitgevoerd.",
        TextKey::OpenItemInNewTab => "Openen in een nieuw tabblad",
        TextKey::BulkItemsToChange => "Te wijzigen items",
        TextKey::BulkScopeHelp => "Nummeren, seizoenen herkennen en mediatype wijzigen gebruiken deze items. Het lengtefilter geldt voor nummeren en seizoenen herkennen.",
        TextKey::BulkDetectSeasons => "Seizoenen herkennen",
        TextKey::BulkFirstSeason => "Eerste seizoen",
        TextKey::BulkReviewChanges => "Wijzigingen controleren",
        TextKey::BulkPreviewSaves => "Toepassen slaat deze waarden op; opgeslagen wijzigingen worden automatisch gepubliceerd.",
        TextKey::BulkPreviewNothing => "Dit voorstel wijzigt niets.",
        TextKey::BulkApplyNumbering => "Nummering toepassen",
        TextKey::BulkApplyChanges => "Wijzigingen toepassen",
        TextKey::BulkCancelPreview => "Voorbeeld annuleren",
        TextKey::ActiveFilter => "Actief filter",
        TextKey::AddonWorkspaceLengthFilter => "Filteren op medialengte",
        TextKey::AddonWorkspaceMinimumLength => "Minimumlengte",
        TextKey::AddonWorkspaceMaximumLength => "Maximumlengte",
        TextKey::AddonWorkspaceUnknownLength => "Onbekende lengtes meenemen",
        TextKey::AddonWorkspaceLengthHelp => "Minuten:seconden. Laat leeg voor geen limiet.",
        TextKey::AddonWorkspaceReviewLatest => "Nieuwste opgeslagen versie bekijken",
        TextKey::AddonWorkspaceReviewRetained => "Je invoer blijft behouden. Vergelijk deze met de nieuwste opgeslagen waarden voordat je opslaat. Controleer andere open taken afzonderlijk.",
        TextKey::AddonWorkspaceStreamHelp => "Rechtstreeks vanaf de bron afspelen.",
        TextKey::AddonWorkspaceDownloadHelp => "Elke video voorbereiden voordat deze wordt afgespeeld.",
        TextKey::AddonWorkspaceNoOptions => "Deze add-on biedt geen instellingen voor toegang of segmenten.",
        TextKey::AddonWorkspaceNewHelp => "Nieuwe geïmporteerde video’s downloaden; bestaande video’s streamen.",
        TextKey::AddonWorkspaceSizeLimit => "Dit formulier is te groot. Ga terug naar je invoer en sla een kleinere taak op.",
        TextKey::AddonWorkspaceItems => "Items",
        TextKey::AddonWorkspaceSettings => "Lijstinstellingen",
        TextKey::AddonWorkspacePlayback => "Afspelen en toegang",
        TextKey::AddonWorkspaceActivity => "Activiteit en herstel",
        TextKey::AddonWorkspaceSections => "Lijstonderdelen",
        TextKey::AddonWorkspaceSaveSettings => "Instellingen opslaan",
        TextKey::AddonWorkspaceSaveItem => "Item opslaan",
        TextKey::AddonWorkspaceSaveNumbering => "Nummering opslaan",
        TextKey::AddonWorkspacePublish => "Opgeslagen wijzigingen publiceren",
        TextKey::AddonWorkspacePublished => "Gepubliceerde lijst is actueel",
        TextKey::AddonWorkspacePending => "Opgeslagen wijzigingen worden automatisch toegepast",
        TextKey::AddonWorkspacePreparationRequired => "Benodigde media worden automatisch voorbereid",
        TextKey::AddonWorkspacePreparing => "Media worden voorbereid voor publicatie",
        TextKey::AddonWorkspaceAddIncomplete => "De lijst is toegevoegd, maar een deel van de instellingen is niet opgeslagen. Open Lijstinstellingen, controleer ze en sla opnieuw op.",
        TextKey::AddonWorkspacePreparationFailed => "Mediavoorbereiding mislukt",
        TextKey::RetryPreparation => "Voorbereiding opnieuw proberen",
        TextKey::PreparationFailedItems => "Voorbereidingstaken",
        TextKey::PreparationFailureFailed => "De add-on kon de media niet voorbereiden. Probeer de voorbereiding opnieuw en controleer de add-on als dit blijft mislukken.",
        TextKey::PreparationFailureInputChanged => "Het voorbereidingsverzoek kwam niet meer overeen met de opgeslagen lijst of de geïnstalleerde add-on. Probeer de voorbereiding opnieuw.",
        TextKey::PreparationFailureInvalidRequest => "De opgeslagen opties passen niet bij de geïnstalleerde add-on. Controleer Afspelen en toegang, sla op en probeer de voorbereiding opnieuw.",
        TextKey::PreparationFailureCancelled => "De voorbereiding is geannuleerd. Probeer de voorbereiding opnieuw om opnieuw te beginnen.",
        TextKey::PreparationFailureAttemptsExhausted => "De voorbereiding is gestopt na herhaalde mislukte pogingen. Probeer de voorbereiding opnieuw en controleer de add-on als dit blijft mislukken.",
        TextKey::PreparationFailureStorageFull => "Er was niet genoeg opslagruimte om de media voor te bereiden. Maak ruimte vrij en probeer de voorbereiding opnieuw.",
        TextKey::PreparationFailureUnsupported => "De add-on kan deze media niet voorbereiden.",
        TextKey::PreviousSavedRevision => "Vorige opgeslagen revisie",
        TextKey::AddonWorkspacePreparationActivity => "Mediavoorbereiding",
        TextKey::AddonWorkspaceBulk => "Meerdere items bewerken",
        TextKey::AddonWorkspaceSelection => "Geselecteerd voor bewerking",
        TextKey::AddonWorkspaceOffPage => "Op andere pagina’s",
        TextKey::AddonWorkspaceDiscard => "Bewerkingen verwerpen",
        TextKey::AddonWorkspaceSelected => "Geselecteerde items",
        TextKey::AddonWorkspaceMatching => "Alle overeenkomende items",
        TextKey::AddonWorkspacePage => "Huidige pagina",
        TextKey::AddonWorkspaceAccess => "Toegang en segmenten",
        TextKey::AddonWorkspaceSaveConflict => "De opgeslagen instellingen zijn gewijzigd. Controleer de huidige versie voordat u opslaat.",
        TextKey::AddonWorkspaceSaveInvalid => "Deze instellingen konden niet worden gevalideerd. Controleer uw invoer en probeer opnieuw.",
        TextKey::AddonWorkspaceSaveStorage => "De instellingen konden niet worden opgeslagen. Uw invoer is behouden; probeer opnieuw op te slaan.",
        TextKey::AddonWorkspaceLoadIncomplete => "Sommige editorinformatie kon niet consistent worden geladen. Uw invoer is behouden. Controleer de huidige versie om opnieuw te proberen.",
        TextKey::TechnicalDetails => "Technische details",
        TextKey::AddonPermissionNetworkHelp => "Verbinding maken met de vermelde netwerkhosts",
        TextKey::AddonPermissionMediaHelp => "Media lezen die beschikbaar zijn voor ErsatzRS",
        TextKey::AddonPermissionFfmpegHelp => "De beheerde FFmpeg-runtime uitvoeren",
        TextKey::AddonPermissionCommandHelp => "De vermelde externe opdrachten uitvoeren",
        TextKey::PreparationTasks => "Voorbereidingstaken",
        TextKey::PreparationAffectedItems => "Betrokken items",
        TextKey::PreparationIntervals => "Interval- / SponsorBlock-taken",
        TextKey::PreparationDownloads => "Downloadtaken",
        TextKey::PreparationSubtitles => "Ondertiteltaken",
        TextKey::PreparationQueued => "In wachtrij",
        TextKey::PreparationRunning => "Actief",
        TextKey::PreparationWaiting => "Wachtend",
        TextKey::PreparationNotQueued => "Niet ingepland",
        TextKey::LastActivity => "Laatste activiteit",
        TextKey::AddonAccessUnknownFailure => "De oorzaak van deze toegangsfout is onbekend. Controleer de opgeslagen toegangsinstellingen, test de bron in uw browser en probeer de toegangscontrole opnieuw.",
        TextKey::AddonWorkspaceRetained => "Wijzigingen konden niet worden opgeslagen. Je invoer is bewaard; controleer de fouten of laad de huidige revisie opnieuw.",
        TextKey::AddonWorkspaceNavigation => "Navigatie behoudt niet-opgeslagen wijzigingen. Opgeslagen wijzigingen worden automatisch toegepast.",
        TextKey::AddonWorkspaceSeasonOnly => "Alleen seizoen wijzigen",
        TextKey::AddonWorkspaceSeasonPerYear => "Nieuw seizoen per kalenderjaar",
        TextKey::AddonWorkspaceYearDatesIncomplete => "Uploaddatums ontbreken nog. Wacht op de details of vul de ontbrekende datums in voordat je seizoenen per jaar groepeert.",
        TextKey::AddonWorkspacePageSize => "Items per pagina",

        TextKey::AddonAccessTest => "Toegang testen",
        TextKey::AddonAccessHelp => {
            "Controleer dit item met de opgeslagen conceptopties. Niet-opgeslagen wijzigingen tellen niet mee; gepubliceerde weergave blijft gelijk."
        }
        TextKey::AddonAccessSaveFirst => {
            "Sla eerst een optie op om de conceptinstellingen te activeren voordat u de toegang test."
        }
        TextKey::AddonAccessError => {
            "De toegangscontrole kon niet worden geladen of gewijzigd. Vernieuw het item en probeer het opnieuw."
        }
        TextKey::AddonAccessNotChecked => "Niet gecontroleerd",
        TextKey::AddonAccessQueued => "Toegangscontrole in wachtrij",
        TextKey::AddonAccessRunning => "Toegang controleren",
        TextKey::AddonAccessWaiting => "Wachten om de toegangscontrole opnieuw te proberen",
        TextKey::AddonAccessCancelled => "Toegangscontrole geannuleerd",
        TextKey::AddonAccessStale => "Toegangsresultaat verlopen of instellingen gewijzigd",
        TextKey::AddonAccessAccessible => "Toegankelijk",
        TextKey::AddonAccessProfileUnavailable => "Het geselecteerde browserprofiel is niet beschikbaar voor de server. Controleer het opgeslagen browserprofiel en controleer de toegang opnieuw.",
        TextKey::AddonAccessProfileLocked => "Het browserprofiel is vergrendeld. Sluit de browser die dit profiel gebruikt en probeer de toegangscontrole opnieuw.",
        TextKey::AddonAccessDecryptionFailed => "De server kon de browserreferenties niet ontsleutelen. Controleer het browserprofiel en het account waarmee de server draait en probeer opnieuw.",
        TextKey::AddonDerivedMetadataLimited => "De metadata van de serie heeft de maximale omvang bereikt. De afleveringsmetadata blijft volledig.",
        TextKey::AddonLocalPlaybackAvailable => "Lokale kopie afspeelbaar",
        TextKey::AddonAccessRestricted => "De provider heeft de toegang beperkt. Open de bron in je browser om account- of regiobeperkingen te controleren en probeer de toegangscontrole opnieuw.",
        TextKey::AddonAccessNextAttempt => "Volgende poging:",
        TextKey::AddonAccessExpires => "Resultaat verloopt:",
        TextKey::AddonAccessRefresh => "Status vernieuwen",
        TextKey::AddonOptionsCurrentFilter => "Huidig filter",
        TextKey::AddonOptions => "Provideropties",
        TextKey::AddonOptionsDraftHelp => "Opgeslagen opties worden automatisch toegepast. Bestaande playback blijft beschikbaar tijdens de verwerking.",
        TextKey::AddonOptionsProvisional => {
            "Deze pakketstandaarden zijn voorlopig. Door een optie op te slaan activeert u ze voor dit concept."
        }
        TextKey::AddonOptionsListDefault => "Lijststandaard",
        TextKey::AddonOptionsInherited => "Overgenomen",
        TextKey::AddonOptionsOverride => "Overschrijving",
        TextKey::AddonOptionsFiniteOnly => {
            "Niet beschikbaar voor live-items of items met onbekende duur: deze optie vereist media met een vaste duur. Direct afspelen blijft beschikbaar."
        }
        TextKey::AddonOptionsResetPackage => "Pakketstandaard herstellen",
        TextKey::AddonOptionsResetList => "Lijststandaard herstellen",
        TextKey::AddonOptionsMixed => "Verschillende waarden — kies een waarde",
        TextKey::AddonOptionsPreviewHelp => {
            "Controleer deze specifieke items voordat u toepast. Bij een tussentijdse concept- of schemawijziging is een nieuw voorbeeld vereist."
        }
        TextKey::AddonOptionsBefore => "Voor",
        TextKey::AddonOptionsAfter => "Na",
        TextKey::AddonOptionsConflict => {
            "Opties konden niet worden opgeslagen of geladen. Het concept, de selectie of het pakketschema is mogelijk gewijzigd. Laad de editor opnieuw en probeer het nogmaals."
        }
        TextKey::AddonOptionsNone => "Dit pakket definieert geen opties.",
        TextKey::Addon => "Add-on",
        TextKey::PrimaryFeature => "Hoofdfilm",
        TextKey::MovieExtra => "Filmextra",
        TextKey::NeedReview => "Meldingen",

        TextKey::ReviewRecommended => "Beoordeling aanbevolen",
        TextKey::PublicationReadiness => "Gereedheid voor publicatie",
        TextKey::PublicationBlockers => "Blokkades",
        TextKey::PublicationActivity => "Activiteit",
        TextKey::PublicationJobs => "publicatietaken",
        TextKey::PublicationEvents => "gebeurtenissen",
        TextKey::NoPublicationActivity => "Er is nog geen publicatieactiviteit vastgelegd.",
        TextKey::CopyRedactedDiagnostics => "Geredigeerde diagnostiek kopiëren",
        TextKey::DownloadRedactedDiagnostics => "Geredigeerde diagnostiek downloaden",
        TextKey::RedactedDiagnosticsCopied => "Geredigeerde diagnostiek gekopieerd.",
        TextKey::PublicationGeneration => "Generatie",
        TextKey::CorrelationId => "Correlatie-ID",
        TextKey::TargetPublishedRevision => "Beoogde gepubliceerde revisie",
        TextKey::RelatedOperation => "Gerelateerde bewerking",
        TextKey::FilesystemTransition => "Bestandssysteemovergang",
        TextKey::RetryClass => "Herhaalclassificatie",
        TextKey::RecoveryAction => "Herstelactie",
        TextKey::OperatingSystemErrorCode => "Foutcode van besturingssysteem",
        TextKey::PublicationOutcome => "Resultaat",
        TextKey::PublicationExpired => "Verlopen",
        TextKey::EventExistingJobAdopted => "Bestaande publicatie overgenomen",
        TextKey::EventEnqueued => "Publicatie in wachtrij gezet",
        TextKey::EventLeaseWaitStarted => "Wachten op bibliotheektoegang",
        TextKey::EventLeaseAcquired => "Bibliotheektoegang verkregen",
        TextKey::EventLeaseReclaimed => "Publicatie hervat na een onderbreking of verlopen lease",
        TextKey::EventPhaseAdvanced => "Publicatiefase voortgezet",
        TextKey::EventChildScanStarted => "Verificatiescan gestart",
        TextKey::EventChildScanCompleted => "Verificatiescan voltooid",
        TextKey::EventChildScanFailed => "Verificatiescan mislukt",
        TextKey::EventOriginalFailure => "Oorspronkelijke publicatiefout",
        TextKey::EventRecoveryFailure => "Herstelfout",
        TextKey::EventRetryScheduled => "Nieuwe herstelpoging gepland",
        TextKey::EventRecoveryRequested => "Herstel aangevraagd",
        TextKey::EventCompleted => "Publicatie voltooid",
        TextKey::EventHistoryTruncated => "Oudere activiteit is afgekapt",
        TextKey::TriggerAutomaticRefresh => "Geplande vernieuwing",
        TextKey::TriggerOperator => "Aangevraagd door een beheerder",
        TextKey::TriggerRestoreRevision => "Aangevraagd door een revisie te herstellen",
        TextKey::TestSource => "Bron testen",
        TextKey::TestSourceHelp => {
            "Controleer de opgeslagen bron zonder de lijst te importeren of te wijzigen."
        }
        TextKey::SourceTestQueued => "Brontest staat in de wachtrij",
        TextKey::SourceTestRunning => "Brontest wordt uitgevoerd…",
        TextKey::SourceTestStale => {
            "De brontest is verouderd omdat de opgeslagen invoer niet meer overeenkomt met de huidige lijst, het add-onpakket of de standaardwaarden."
        }
        TextKey::SourceAvailable => "Bron is beschikbaar",
        TextKey::SourceUnsupported => "Bron wordt niet ondersteund",
        TextKey::SourceTestFailed => "Brontest mislukt",
        TextKey::SourceRevision => "Bronrevisie",
        TextKey::LastSuccessfulPlayoutBuild => "Laatste geslaagde opbouw",
        TextKey::PlayoutRefreshPending => "Vernieuwing staat klaar",
        TextKey::PlayoutRefreshFailed => "Vernieuwing mislukt; het gepubliceerde uitzendschema blijft behouden.",
        TextKey::PlayoutSourceCheckPending => "Bronwijzigingen controleren",
        TextKey::PlayoutSourceCheckFailed => "Bronvergelijking mislukt; een nieuwe poging staat klaar.",
        TextKey::FirstPageComplete => "Eerste pagina volledig",
        TextKey::ProviderIdentity => "Provideridentiteit",
        TextKey::ReturnedFirstPageItems => "Items op de eerste pagina",
        TextKey::ProviderTotalItemCount => "Totaal aantal items volgens de provider",
        TextKey::DraftChanges => "Opgeslagen conceptwijzigingen",
        TextKey::ExactDraftDiffHelp => {
            "Exacte verschillen tussen het opgeslagen concept en de huidige gepubliceerde revisie."
        }
        TextKey::DraftDiffUnavailable => {
            "Een exacte conceptvergelijking is beschikbaar nadat een volledige revisie is gepubliceerd."
        }
        TextKey::MetadataChanges => "Metadatawijzigingen",
        TextKey::ExcludedItems => "Uitgesloten items",
        TextKey::Conflicts => "Conflicten",
        TextKey::PendingRemovals => "Openstaande verwijderingen",
        TextKey::PendingRemovalHelp => {
            "Eén keer afwezig bij een volledige vergelijking. Bevestig de verwijdering nu, of wacht op een tweede volledige waarneming minstens 24 uur later."
        }
        TextKey::Reappearances => "Opnieuw verschenen items",
        TextKey::RemovalCandidates => "Kandidaten voor verwijdering",
        TextKey::ConfirmRemoval => "Verwijdering bevestigen",
        TextKey::RemovalConfirmed => "Verwijdering bevestigd.",
        TextKey::RemovalConfirmationFailed => "Bevestigen van verwijdering is mislukt.",
        TextKey::RevisionHistory => "Revisiegeschiedenis",
        TextKey::RevisionHistoryHelp => {
            "Herstellen vervangt het huidige concept en publiceert de gekozen momentopname als een nieuwe revisie."
        }
        TextKey::NoRevisionHistory => "Er worden nog geen gepubliceerde revisies bewaard.",
        TextKey::Revision => "Revisie",
        TextKey::CompareRevisions => "Revisies vergelijken",
        TextKey::RestoreRevision => "Revisie herstellen",
        TextKey::RestoreRevisionWarning => "Dit vervangt het huidige concept en publiceert revisie",
        TextKey::RestoreRevisionQueued => "Revisieherstel staat in de wachtrij",
        TextKey::RestoreRevisionFailed => "De revisie kon niet worden hersteld.",
        TextKey::PublishedItems => "Gepubliceerde items",
        TextKey::GeneratedPlaylists => "Gegenereerde afspeellijsten",
        TextKey::ActiveOrFuturePlayouts => "Actieve of toekomstige uitzendingen",
        TextKey::EffectiveOptions => "Effectieve opties",
        TextKey::Files => "Bestanden",
        TextKey::Artifacts => "Artefacten",
        TextKey::Added => "Toegevoegd",
        TextKey::Removed => "Verwijderd",
        TextKey::Changed => "Gewijzigd",
        TextKey::FilesystemPreviousBackedUp => "Reservekopie van vorige revisie gemaakt",
        TextKey::FilesystemCandidateCommitted => "Kandidaatrevisie vastgelegd",
        TextKey::FilesystemPreviousRestored => "Vorige revisie hersteld",
        TextKey::FilesystemProjectionRestored => "Databaseprojectie hersteld",
        TextKey::FilesystemRevisionVerified => "Revisie geverifieerd",
        TextKey::FilesystemCleanupCompleted => "Publicatiebestanden opgeruimd",
        TextKey::RetryTransientRecovery => "Tijdelijk herstel",
        TextKey::ErrorFilesystemIo => "Bestandssysteem-I/O",
        TextKey::ErrorPermissionDenied => "Toegang geweigerd",
        TextKey::ErrorSharingViolation => "Schending van bestandsdeling",
        TextKey::ErrorUnsafeFilesystemState => "Onveilige bestandssysteemstatus",
        TextKey::ErrorInvalidPublicationMarker => "Ongeldige publicatiemarkering",
        TextKey::ErrorPublicationIntegrity => "Integriteitscontrole van publicatie",
        TextKey::OutcomePublished => "Gepubliceerd",
        TextKey::OutcomeRestoredAfterFailure => "Vorige revisie hersteld na fout",
        TextKey::ReviewRecommendations => "Aanbevelingen voor beoordeling",
        TextKey::OptionalDetails => "Optionele details",
        TextKey::DraftSaved => "Concept opgeslagen",
        TextKey::SavedRevision => "Opgeslagen revisie",
        TextKey::PublishList => "Lijst publiceren",
        TextKey::PreparePublication => "Publicatie voorbereiden",
        TextKey::ProcessedMediaRequired => "Voorbereidingstaken",
        TextKey::ProcessedMediaQueued => "Mediavoorbereiding ingepland",
        TextKey::PreparingProcessedMedia => "Mediabestanden worden voorbereid",
        TextKey::PreparingPublication => "Publicatie wordt voorbereid",
        TextKey::Publishing => "Publiceren…",
        TextKey::PublicationQueued => "Publicatie staat in de wachtrij",
        TextKey::RestoringPublication => "Vorige revisie herstellen…",
        TextKey::Published => "Gepubliceerd",
        TextKey::PublishSavedRevisionHelp => {
            "Publiceren gebruikt exact de opgeslagen revisie. Sla lijstwijzigingen eerst op."
        }
        TextKey::ChangesSavedImmediately => {
            "Item- en bulkwijzigingen worden direct opgeslagen en kunnen ongedaan worden gemaakt."
        }
        TextKey::ReviewItem => "Item beoordelen",
        TextKey::ShowFullDescription => "Volledige beschrijving tonen",
        TextKey::ShowLess => "Minder tonen",
        TextKey::ImportAList => "Een lijst importeren",
        TextKey::ImportOverview => "Overzicht importeren",
        TextKey::AddonContextSelected => "Geselecteerde add-on",
        TextKey::CompletedIn => "Voltooid in",
        TextKey::NoPublicationBlockers => "Geen publicatieblokkades.",
        TextKey::BlockerOverviewIncomplete => "Het volledige overzicht moet zijn geverifieerd.",
        TextKey::BlockerPublicationInProgress => "Publicatie is al bezig.",
        TextKey::BlockerRecoveryRequired => "Een eerdere publicatie moet nog worden hersteld.",
        TextKey::BlockerCollectionTitleMissing => "De collectie heeft een titel nodig.",
        TextKey::BlockerInvalidArtwork => {
            "Afbeeldingen moeten een geldige begrensde HTTPS-URL gebruiken."
        }
        TextKey::BlockerNoIncludedItems => "Neem ten minste één item op.",
        TextKey::BlockerMoviePrimaryCount => "Filmpublicatie vereist precies één hoofdfilm.",
        TextKey::BlockerPlaybackDecisionRequired => "Kies eindig of live afspeelgedrag.",
        TextKey::BlockerPlaybackDetailsUnavailable => {
            "Vereiste afspeeldetails zijn niet beschikbaar via deze add-on."
        }
        TextKey::BlockerPlaybackDetailsFailed => "Providerdetails bevatten geen positieve duur.",
        TextKey::BlockerUnavailableWithoutReplacement => {
            "Het niet-beschikbare item heeft een geldige lokale vervanging nodig."
        }
        TextKey::BlockerItemTitleMissing => "Het item heeft een titel nodig.",
        TextKey::BlockerSeasonEpisodeMissing => "Voer een geldig seizoen- en afleveringsnummer in.",
        TextKey::BlockerArtistMissing => "Voer ten minste één artiest in.",
        TextKey::PublishUnsavedChanges => "Sla wijzigingen op voordat u deze revisie publiceert.",
        TextKey::PublicationBlocked => "Publicatie is geblokkeerd.",
        TextKey::PublishBlocked => "Publiceren geblokkeerd",
        TextKey::RetryRecovery => "Herstel opnieuw proberen",
        TextKey::RestorePreviousRevision => "Vorige revisie herstellen",
        TextKey::PublicationPhase => "Publicatiefase",
        TextKey::PublicationAttempts => "Pogingen",
        TextKey::LastProgress => "Laatste voortgang",
        TextKey::FailureReason => "Oorspronkelijke reden",
        TextKey::RestrictedItemsExcluded => "Beperkte items uitgesloten van deze revisie",
        TextKey::RecoveryQueued => "Herstel staat in de wachtrij",
        TextKey::RecoveryFailed => "Herstel kon niet in de wachtrij worden gezet.",
        TextKey::PhaseAwaitingLease => "Wachten op bibliotheektoegang",
        TextKey::PhaseStaging => "Revisie voorbereiden",
        TextKey::PhaseScanning => "Voorbereide revisie scannen",
        TextKey::PhaseCommitting => "Revisie vastleggen",
        TextKey::PhaseRestoringFiles => "Bestanden herstellen",
        TextKey::PhaseRestoringProjection => "Databaseprojectie herstellen",
        TextKey::PhaseReconciling => "Herstelde revisie afstemmen",
        TextKey::PhaseVerifying => "Revisie verifiëren",
        TextKey::PhaseCleaningUp => "Publicatiebestanden opruimen",
        TextKey::PhaseComplete => "Voltooid",
        TextKey::InstallLeavesDisabled => {
            "Na installatie blijft deze add-on uitgeschakeld totdat u hem inschakelt."
        }
        TextKey::EnableActivatesPermissions => {
            "Inschakelen activeert de opgegeven machtigingen van de add-on."
        }
        TextKey::FirstAired => "Eerste uitzending",
        TextKey::LoadItemDetailsAfterPublishing => "Itemdetails automatisch laden",
        TextKey::LoadItemDetailsHelp => {
            "Importeer eerst het overzicht. Technische details die nodig zijn voor afspelen worden vóór de eerste publicatie geladen; de overige uitgebreide metadata volgt in een beheerst tempo."
        }
        TextKey::OverviewOnlyManualDetailsHelp => {
            "Importeer alleen het overzicht en vraag beschrijvende details handmatig op. Technische details die nodig zijn voor afspelen worden vóór publicatie wel automatisch voorbereid."
        }
        TextKey::PlaybackDetailsQueued => "Afspeeldetails in wachtrij",
        TextKey::PublishAgainAfterDetails => "Voltooide details worden automatisch toegepast.",
TextKey::FetchDetails => "Details ophalen",
        TextKey::DetailsQueued => "Details in wachtrij",
        TextKey::DetailsLoading => "Details laden",
        TextKey::RetryDetails => "Details opnieuw proberen",
        TextKey::DetailsComplete => "Details compleet",
        TextKey::DetailsSkipped => "Details overgeslagen",
        TextKey::FindingProgrammes => "Programma's zoeken",
        TextKey::CheckingOverview => "Overzicht controleren",
        TextKey::ReadyToReview => "Wordt automatisch verwerkt",

        TextKey::LoadingItemDetails => "Itemdetails laden",
        TextKey::WaitingForProvider => "Wachten op aanbieder",
        TextKey::OverviewOnly => "Alleen overzicht",
        TextKey::PauseImport => "Import pauzeren",
        TextKey::ResumeImport => "Import hervatten",
        TextKey::RetryImportNow => "Import nu opnieuw proberen",
        TextKey::EmptyImportNeedsRetry => "Lege import moet opnieuw worden geprobeerd",
        TextKey::EmptyImportRetryHelp => {
            "Deze voltooide import bevat geen items. Werk de add-on bij en probeer opnieuw; je lijstinstellingen en bewerkingen blijven behouden."
        }
        TextKey::CancelImport => "Import annuleren",
        TextKey::OtherPageItemsAffected => "Meer betrokken items staan op andere pagina's",
        TextKey::ElapsedTime => "Verstreken tijd",
        TextKey::LastSuccessfulProgress => "Laatste geslaagde voortgang",
        TextKey::EstimatedRemainingTime => "Geschatte resterende tijd",
        TextKey::PlaybackBehavior => "Afspeelgedrag",
        TextKey::UseProviderValue => "Waarde van aanbieder gebruiken",
        TextKey::FiniteMedia => "Eindige media",
        TextKey::LiveSource => "Livebron",
        TextKey::ProviderValueChanged => "Waarde van aanbieder gewijzigd",
        TextKey::ApplyToSelected => "Toepassen op selectie",
        TextKey::ApplyToAllRemoteStreams => "Toepassen op alle remote streams",
        TextKey::SelectItem => "Item selecteren",
        TextKey::NumberEpisodes => "Afleveringen nummeren",
        TextKey::NumberEpisodesHelp => {
            "Ken één seizoen en oplopende afleveringsnummers toe in de oorspronkelijke volgorde van de add-on. Uitgesloten items en hoofdstukfragmenten worden overgeslagen."
        }
        TextKey::FirstEpisode => "Eerste aflevering",
        TextKey::NumberToAssign => "Aantal toewijzen",
        TextKey::AllRemaining => "Alle resterende",
        TextKey::ReplaceExistingEpisodeNumbers => "Bestaande afleveringsnummers vervangen",
        TextKey::AutoDetectSeasons => "Seizoenen automatisch herkennen aan uitzenddatums",
        TextKey::AutoDetectSeasonsHelp => {
            "Een nieuw seizoen begint zodra het verschil tussen twee bekende uitzenddatums minimaal het ingestelde aantal dagen is. Items zonder datum blijven in het huidige seizoen."
        }
        TextKey::MinimumSeasonGapDays => "Minimale tijd tussen seizoenen (dagen)",
        TextKey::NumberingStartsAt => "Nummering begint bij",
        TextKey::FirstIncludedItem => "Eerste opgenomen item",
        TextKey::StartWithFirstItem => "Begin bij eerste item",
        TextKey::StartHere => "Begin hier",
        TextKey::SaveSeasonEpisode => "Seizoen en aflevering opslaan",
        TextKey::BulkEdit => "Bulk bewerken",
        TextKey::Scope => "Bereik",
        TextKey::AllItems => "Alle items",
        TextKey::CurrentPage => "Huidige pagina",
        TextKey::ItemsNeedingReview => "Items met meldingen",

        TextKey::KeepCurrentValue => "Huidige waarde behouden",
        TextKey::UndoLastChange => "Laatste wijziging ongedaan maken",
        TextKey::Undo => "Ongedaan maken",
        TextKey::ChangeUndone => "Wijziging ongedaan gemaakt",
        TextKey::MetadataConflicts => "Metadataconflicten",
        TextKey::MetadataConflictsHelp => "De aanbieder heeft velden gewijzigd die je hebt aangepast. Je opgeslagen waarden blijven gelden.",
TextKey::PreviousProviderValue => "Vorige aanbiederwaarde",
        TextKey::NewProviderValue => "Nieuwe aanbiederwaarde",
        TextKey::OperatorValue => "Beheerderswaarde",
        TextKey::Discovered => "Gevonden",
        TextKey::Examined => "Onderzocht",
        TextKey::ArchivedSkipped => "Overgeslagen als gearchiveerd",
        TextKey::Emitted => "Uitgestuurd",
        TextKey::EditMetadata => "Metadata bewerken",
        TextKey::OneTagPerLine => "Eén tag per regel",
        TextKey::OriginalTitle => "Oorspronkelijke titel",
        TextKey::ReleaseDate => "Releasedatum",
        TextKey::Year => "Jaar",
        TextKey::SortTitle => "Sorteertitel",
        TextKey::MovieFilesAndExtras => "Filmbestanden en extra's",
        TextKey::MovieFilesAndExtrasHelp => {
            "Wijs precies één hoofdfilm toe. De bronvolgorde bepaalt de rol niet, dus een trailer mag vooraan staan."
        }
        TextKey::EpisodesMetadataHelp => {
            "Controleer de identiteit en nummering van afleveringen; de bronvolgorde blijft ongewijzigd."
        }
        TextKey::MusicVideosMetadataHelp => {
            "Artiest, album en track horen bij elke video, zodat lijsten met meerdere artiesten geldig blijven."
        }
        TextKey::OtherVideosMetadataHelp => {
            "Gebruik dit type voor zelfstandige video's, niet voor één film met extra's."
        }
        TextKey::SongsMetadataHelp => {
            "Track, schijf, artiest en album worden per nummer bewerkt; de lijstvolgorde vervangt de tracknummering niet."
        }
        TextKey::ImagesMetadataHelp => {
            "Bijschrift en maker zijn bewerkbaar; formaat, afmetingen en bronidentiteit blijven feitelijke brongegevens."
        }
        TextKey::RemoteStreamsMetadataHelp => {
            "Bewerk hier de presentatiemetadata; verbinding en protocol blijven feiten van de providerbron."
        }
        TextKey::CastAndCredits => "Cast en medewerkers",
        TextKey::OneCreditPerLine => "Eén persoon per regel: naam | rol",
        TextKey::Poster => "Poster",
        TextKey::Artwork => "Afbeeldingen",
        TextKey::PublishMetadata => "Publiceren",
        TextKey::IncludeItem => "Opnemen",
        TextKey::IncludeInPublication => "Opnemen bij publicatie",
        TextKey::MetadataDraftNotice => "Opgeslagen metadata en wijzigingen in de selectie worden automatisch toegepast.",
TextKey::ApplyToAllItemsPrompt => {
            "Elk item krijgt de gekozen mediasoort, waarmee waarden die de add-on heeft afgeleid of die je zelf hebt ingesteld worden overschreven."
        }
        TextKey::TelevisionEpisode => "Televisieaflevering",
        TextKey::CollectionTypeAuto => "Bepalen uit de items",
        TextKey::CurrentLibraryType => "Huidig bibliotheektype",
        TextKey::CollectionTypeChangeHelp => "Sla op om bestaande items automatisch te reorganiseren. Volgorde, selectie en metadatawijzigingen blijven behouden.",
TextKey::NonPlayableItems => "Niet-afspeelbare items",
        TextKey::UnknownItems => "Onbekende items",
        TextKey::PlayableItems => "Afspeelbare items",
        TextKey::ApplyToAllItems => "Op alle items toepassen",
        TextKey::ChangeAddon => "Add-on wijzigen",
        TextKey::ChangeAddonHelp => {
            "Kies een andere add-on om de lijst daarmee opnieuw te importeren."
        }
        TextKey::ChangeAddonPrompt => {
            "De lijst wordt opnieuw geïmporteerd via de gekozen add-on. Itemmetadata, vervangingen en hoofdstukcorrecties worden opnieuw afgeleid van de nieuwe provider."
        }
        TextKey::AddonAutoDetect => "Bepalen uit de URL",
        TextKey::AddonAutoDetected => "Automatisch gekozen op basis van de URL.",
        TextKey::AddonMediaListNoAddonForUrl => {
            "Geen enkele geïnstalleerde add-on verwerkt deze URL. Kies er een of stel een standaard remote stream-verwerker in."
        }
        TextKey::DefaultRemoteStreamAddon => "Standaard remote stream-verwerker",
        TextKey::DefaultRemoteStreamAddonHelp => {
            "Wordt gebruikt voor HTTP- en HTTPS-bronnen die geen enkele geïnstalleerde add-on claimt. De add-on moet de host nog steeds mogen benaderen."
        }
        TextKey::AddonDetailWait => "Tempo voor itemdetails",
        TextKey::MinimumDetailWait => "Minimale wachttijd voor details",
        TextKey::MaximumDetailWait => "Maximale wachttijd voor details",
        TextKey::AddonDetailWaitHelp => {
            "Na elke poging om itemdetails in te lezen wacht ErsatzRS een nieuw willekeurig aantal seconden binnen dit globale bereik voordat een lijst dezelfde providerhost weer mag gebruiken."
        }
        TextKey::AddonDetailWaitInvalid => {
            "Vul positieve hele seconden in en zorg dat het minimum niet hoger is dan het maximum."
        }
        TextKey::Addons => "Add-ons",
        TextKey::AddonOverview => "Overzicht",
        TextKey::AddonUpdates => "Updates",
        TextKey::AddonSources => "Bronnen",
        TextKey::AddonNotFound => "Add-on niet gevonden",
        TextKey::BackToAddons => "Terug naar add-ons",
        TextKey::AvailableVersion => "Beschikbare versie",
        TextKey::BrowseAddons => "Add-ons bekijken",
        TextKey::CheckForUpdates => "Controleren op updates",
        TextKey::InstallFromFile => "Installeren vanuit bestand",
        TextKey::InstallRepositoryFromFile => "Repository installeren vanuit bestand",
        TextKey::License => "Licentie",
        TextKey::UnknownSources => "Onbekende bronnen",
        TextKey::UnknownSourcesHelp => {
            "Sta add-ons uit lokale Git-checkouts en ZIP-bestanden toe. Add-ons draaien met de toegang van de ErsatzRS-servicegebruiker."
        }
        TextKey::Verified => "Geverifieerd",
        TextKey::Unverified => "Niet geverifieerd",
        TextKey::Sideloaded => "Lokaal geïnstalleerd",
        TextKey::GitCheckout => "Git-checkout",
        TextKey::OfflineBundle => "Offlinebundel",
        TextKey::LegacyUnmanaged => "Verouderde catalogus · onbeheerd",
        TextKey::LegacyAdoptionHelp => {
            "Dit vervangt de onbeheerde catalogusinstallatie door de geselecteerde lokale bron, met behoud van instellingen en gegevens."
        }
        TextKey::Enabled => "Ingeschakeld",
        TextKey::Disabled => "Uitgeschakeld",
        TextKey::Source => "Bron",
        TextKey::ViewSourceCode => "Broncode bekijken",
        TextKey::Version => "Versie",
        TextKey::NextVersion => "Versie van Next-engine",
        TextKey::NoAddonsMatch => "Geen add-ons voldoen aan deze filters.",
        TextKey::AddonsUpToDate => "Alles is up-to-date.",
        TextKey::AddonRepositories => "Add-onrepositories",
        TextKey::AvailableAddons => "Beschikbare add-ons",
        TextKey::InstalledAddons => "Geïnstalleerde add-ons",
        TextKey::AddRepository => "Repository toevoegen",
        TextKey::LastRefresh => "Laatst gescand",
        TextKey::NeverRefreshed => "Nooit",
        TextKey::ChooseFile => "Bestand kiezen…",
        TextKey::NoFileSelected => "Geen bestand gekozen",
        TextKey::EditRepository => "Repository bewerken",
        TextKey::OfficialRepositoryProtected => {
            "De officiële repository kan niet worden verwijderd."
        }
        TextKey::UnknownSourcesRequired => "Schakel eerst onbekende bronnen in.",
        TextKey::NoRepositories => {
            "Geen lokale repositories gevonden. Clone een repository naar de checkoutmap hierboven en ververs daarna."
        }
        TextKey::AddonPackage => "Add-onpakket",
        TextKey::RepositoryPackage => "Repositorypakket",
        TextKey::NoSigningKeyPinned => "Er is geen openbare ondertekeningssleutel vastgelegd.",
        TextKey::RepositoryUrl => "Repository-URL",
        TextKey::PublicSigningKey => "Publieke ondertekeningssleutel",
        TextKey::DeploymentManagedRepositories => {
            "Repositories worden door de installatieconfiguratie beheerd."
        }
        TextKey::Refresh => "Vernieuwen",
        TextKey::Install => "Installeren",
        TextKey::UpdateAddon => "Bijwerken",
        TextKey::Uninstall => "Deïnstalleren",
        TextKey::Configuration => "Configuratie",
        TextKey::SecretReference => "Geheimverwijzing",
        TextKey::EnvironmentVariable => "Omgevingsvariabele",
        TextKey::SecretFile => "Geheimbestand",
        TextKey::Incompatible => "Niet compatibel",
        TextKey::PackageUnavailable => "Pakket niet beschikbaar",
        TextKey::Official => "Officieel",
        TextKey::ThirdParty => "Externe repository",
        TextKey::Permissions => "Rechten",
        TextKey::Capabilities => "Mogelijkheden",
        TextKey::Dependencies => "Afhankelijkheden",
        TextKey::Optional => "Optioneel",
        TextKey::MediaReadAccess => "Leestoegang tot media",
        TextKey::ExternalCommands => "Externe programma's",
        TextKey::ThirdPartyRepositoryWarning => {
            "Laad alleen lokale repositories en ZIP-bestanden die je vertrouwt; add-ons draaien als de ErsatzRS-servicegebruiker."
        }
        TextKey::AddCustomResolution => "Aangepaste resolutie toevoegen",
        TextKey::AddBlock => "Blok toevoegen",
        TextKey::AddBlockGroup => "Blokgroep toevoegen",
        TextKey::AddBlockItem => "Blokitem toevoegen",
        TextKey::AddBlockToTemplate => "Blok aan sjabloon toevoegen",
        TextKey::AddCollection => "Collectie toevoegen",
        TextKey::AddContent => "Inhoud toevoegen",
        TextKey::AddFailed => "Toevoegen mislukt",
        TextKey::AddMultiCollection => "Multi-collectie toevoegen",
        TextKey::AddPlaylist => "Afspeellijst toevoegen",
        TextKey::AddPlaylistGroup => "Afspeellijstgroep toevoegen",
        TextKey::AddPlaylistItem => "Afspeellijstitem toevoegen",
        TextKey::AddRerunCollection => "Herhalingscollectie toevoegen",
        TextKey::AddTemplate => "Sjabloon toevoegen",
        TextKey::AddTemplateGroup => "Sjabloongroep toevoegen",
        TextKey::AddAddonMediaList => "Add-on-lijst toevoegen",
        TextKey::AddToCollection => "Toevoegen aan collectie",
        TextKey::AddToEllipsis => "Toevoegen aan ...",
        TextKey::AddToPlaylist => "Toevoegen aan afspeellijst",
        TextKey::AddToSchedule => "Toevoegen aan schema",
        TextKey::Actions => "Acties",
        TextKey::All => "Alles",
        TextKey::AreYouSureDelete => "Weet u zeker dat u wilt verwijderen",
        TextKey::Actors => "Acteurs",
        TextKey::Artist => "Artiest",
        TextKey::ArtistNotFound => "Artiest niet gevonden",
        TextKey::AfterDurationEnd => "Na einde duur",
        TextKey::AutomaticRefresh => "Automatisch vernieuwen",
        TextKey::AddonDeliveryMode => "Levering",
        TextKey::AddonDeliveryStream => "Streamen",
        TextKey::AddonDeliveryDownloadAhead => "Vooraf downloaden",
        TextKey::AddonDeliveryNewVideos => "Vooraf downloaden — alleen nieuwe video's",
        TextKey::AddonDeliveryHelp => "Wijzigingen in de levering gelden na de volgende publicatie. Alleen nieuwe video's downloadt video's die voor het eerst na het opgeslagen tijdstip zijn geïmporteerd; al bekende video's worden gestreamd. Opnieuw inschakelen stelt een nieuw tijdstip in. Alleen media met een eindige duur kunnen vooraf worden gedownload. Live-items en items waarvan de duur onbekend is, blijven streamen.",
        TextKey::AddonDeliveryUnavailable => "Downloaden vereist een compatibele, ingeschakelde add-on en media met een eindige duur.",
        TextKey::AddonSelectionInitialReview => "Items selecteren vóór de eerste publicatie",
        TextKey::AddonSelectionInitialReviewHelp => "Importeer eerst alleen het overzicht. Bekijk en sla de selectie op voordat media wordt opgehaald of de lijst wordt gepubliceerd.",
        TextKey::AddonSelectionReviewRequired => "Bekijk en sla de eerste selectie op. Automatische publicatie en het ophalen van media wachten hierop.",
        TextKey::AddonSelectionEvaluatedAt => "Laatste selectiebeoordeling",
        TextKey::AddonSelection => "Doorlopende selectie",
        TextKey::AddonSelectionHelp => "Lege grenzen laten elke waarde toe. Termen zoeken in de titel zonder hoofdletterverschil. Pins gaan voor alle filters en tellen niet mee voor nieuwste N. Gearchiveerde mediabestanden blijven bewaard.",
        TextKey::AddonSelectionEnabled => "Selectiefilters inschakelen",
        TextKey::AddonSelectionPause => "Automatisch ophalen pauzeren",
        TextKey::AddonSelectionAge => "Maximale ouderdom (dagen)",
        TextKey::AddonSelectionNewest => "Nieuwste items (zonder pins)",
        TextKey::AddonSelectionMinimum => "Minimale duur (seconden)",
        TextKey::AddonSelectionMaximum => "Maximale duur (seconden)",
        TextKey::AddonSelectionInclude => "Titeltermen opnemen (één per regel)",
        TextKey::AddonSelectionExclude => "Titeltermen uitsluiten (één per regel)",
        TextKey::AddonSelectionActive => "Actieve selectie",
        TextKey::AddonSelectionArchived => "Gearchiveerde selectie",
        TextKey::AddonSelectionPreviewHelp => "Het voorbeeld publiceert of downloadt niets. De tabel toont maximaal 100 items; alle beslissingen tellen mee. Opslaan past de beoordeelde filters via de bestaande worker toe.",
        TextKey::AddonSelectionManual => "Handmatige selectie",
        TextKey::AddonSelectionPinned => "Vastgezet",
        TextKey::AddonSelectionExcluded => "Handmatig uitgesloten",
        TextKey::AddonSelectionAutomatic => "Selectiefilters volgen",
        TextKey::AddonSelectionCutoff => "Effectieve ouderdomsgrens",
        TextKey::AddonSelectionMissingDate => "Publicatiedatum ontbreekt",
        TextKey::AddonSelectionFutureDate => "Publicatiedatum ligt in de toekomst",
        TextKey::AddonSelectionTooOld => "Buiten het ouderdomsvenster",
        TextKey::AddonSelectionMissingDuration => "Duur ontbreekt",
        TextKey::AddonSelectionTooShort => "Korter dan de minimale duur",
        TextKey::AddonSelectionTooLong => "Langer dan de maximale duur",
        TextKey::AddonSelectionTitleNotIncluded => "Geen opnameterm gevonden",
        TextKey::AddonSelectionTitleExcluded => "Een uitsluitingsterm gevonden",
        TextKey::AddonSelectionNewestLimit => "Buiten nieuwste N",
        TextKey::AddonSelectionInvalid => "Controleer de selectiegrenzen en termen, of herlaad na een gelijktijdige wijziging.",
        TextKey::AddonDeliveryCutoff => "Grensdatum voor nieuwe video's",
        TextKey::AddonDeliveryFreshCutoff => "Bij het opslaan wordt een nieuwe grensdatum vastgelegd.",
        TextKey::AdaptiveRefreshUnavailable => {
            "Adaptief vernieuwen is niet beschikbaar omdat deze add-on broncontroles niet meer ondersteunt. Kies een ander interval om automatisch vernieuwen te hervatten."
        }
        TextKey::RefreshPublicationPolicy => "Publicatie na vernieuwing",
        TextKey::RefreshPublicationReview => "Elke vernieuwing beoordelen",
        TextKey::RefreshPublicationAutomatic => "Opgeslagen wijzigingen automatisch toepassen",
        TextKey::RefreshPublicationPolicyHelp => "Imports, verversingen, metadata en opgeslagen wijzigingen worden automatisch toegepast.",
        TextKey::AutoPublicationHeld => "Automatische verwerking heeft aandacht nodig",
        TextKey::PreparationWorkerUnavailable => {
            "De achtergrondtaak voor mediavoorbereiding is niet beschikbaar. Start de server opnieuw voordat u de voorbereiding opnieuw probeert."
        }
        TextKey::PreparationProcessRecoveryRequired => {
            "Mediavoorbereiding vereist procesherstel. Controleer of eerdere providerprocessen zijn gestopt voordat u gereserveerde opslag herstelt en de voorbereiding opnieuw probeert."
        }
        TextKey::AutoPublicationBaselineUnavailable => {
            "Er is geen gepubliceerde basis beschikbaar."
        }
        TextKey::AutoPublicationExistingChanges => "Bestaande items zijn gewijzigd.",
        TextKey::AutoPublicationConflicts => {
            "Metadata- of vervangingsconflicten vereisen beoordeling."
        }
        TextKey::AutoPublicationRemovals => "Items zijn verwijderd.",
        TextKey::AutoPublicationRestricted => "Sommige toevoegingen hebben toegangsbeperkingen.",
        TextKey::AutoPublicationUnavailable => "Sommige toevoegingen zijn niet beschikbaar.",
        TextKey::AutoPublicationOptionsChanged => "Publicatieopties zijn gewijzigd.",
        TextKey::AutoPublicationPreparationFailed => {
            "Het voorbereiden van de publicatie is mislukt."
        }
        TextKey::AutoPublicationConcurrentChange => {
            "De lijst is tijdens de voorbereiding gewijzigd."
        }
        TextKey::RefreshCadenceAdaptive => "Adaptief",
        TextKey::RefreshCadenceDaily => "Dagelijks",
        TextKey::RefreshCadenceWeekly => "Wekelijks",
        TextKey::RefreshCadenceMonthly => "Maandelijks",
        TextKey::RefreshCadenceYearly => "Jaarlijks",
        TextKey::RefreshCadenceNever => "Nooit",
        TextKey::LastSuccessfulRefresh => "Laatste geslaagde vernieuwing",
        TextKey::NextScheduledRefresh => "Volgende geplande vernieuwing",
        TextKey::RefreshRetryAt => "Opnieuw proberen vanaf",
        TextKey::Back => "Terug",
        TextKey::BackToSettings => "Terug naar instellingen",
        TextKey::BackToArtists => "Terug naar artiesten",
        TextKey::BackToCollections => "Terug naar collecties",
        TextKey::BackToMovies => "Terug naar films",
        TextKey::BackToOtherVideos => "Terug naar andere video's",
        TextKey::BackToTvShows => "Terug naar series",
        TextKey::BeforeDurationEnd => "Voor einde duur",
        TextKey::BackToTemplates => "Terug naar sjablonen",
        TextKey::Block => "Blok",
        TextKey::BlockGroup => "Blokgroep",
        TextKey::BlockCannotBeMoved => {
            "Het blok kan daar niet worden geplaatst omdat het zou overlappen of middernacht zou passeren."
        }
        TextKey::BlockCopied => "Blok gekopieerd.",
        TextKey::BlockCreated => "Blok aangemaakt.",
        TextKey::BlockDeleted => "Blok verwijderd.",
        TextKey::BlockGroupCreated => "Blokgroep aangemaakt.",
        TextKey::BlockGroupDeleted => "Blokgroep verwijderd.",
        TextKey::BlockGroupName => "Naam van blokgroep",
        TextKey::BlockGroups => "Blokgroepen",
        TextKey::BlockItem => "Blokitem",
        TextKey::BlockItems => "Blokitems",
        TextKey::BlockName => "Bloknaam",
        TextKey::BlockNotFound => "Blok niet gevonden.",
        TextKey::BlockPreviewFailed => {
            "Het blokvoorbeeld kon niet worden gemaakt. Controleer de blokitems en probeer het opnieuw."
        }
        TextKey::Blocks => "Blokken",
        TextKey::BlockToRemove => "Te verwijderen blok",
        TextKey::BlockWillNotFit => "Het blok past niet op of na de gekozen starttijd.",
        TextKey::Cancel => "Annuleren",
        TextKey::Clear => "Wissen",
        TextKey::Available => "Beschikbaar",
        TextKey::UnknownAvailability => "Onbekend",
        TextKey::LocalReplacement => "Lokale vervanging",
        TextKey::NoReplacement => "Geen vervanging",
        TextKey::InvalidReplacement => "Ongeldige vervanging",
        TextKey::Channels => "Kanalen",
        TextKey::Close => "Sluiten",
        TextKey::Collections => "Collecties",
        TextKey::CollectionDeleted => "Collectie verwijderd.",
        TextKey::CollectionSaved => "Collectie opgeslagen.",
        TextKey::SaveCollection => "Collectie opslaan",
        TextKey::ComputingPreview => "Voorbeeld berekenen...",
        TextKey::Confirm => "Bevestigen",
        TextKey::ConfirmDelete => "Verwijderen bevestigen",
        TextKey::CommentaryTracks => "Commentaartracks",
        TextKey::Content => "Inhoud",
        TextKey::ContentVariantAcoustic => "Akoestisch",
        TextKey::ContentVariantAlternateVersion => "Versie",
        TextKey::ContentVariantBehindTheScenes => "Achter de schermen",
        TextKey::ContentVariantDeletedScene => "Verwijderde scène",
        TextKey::ContentVariantDirectorsCut => "Director's cut",
        TextKey::ContentVariantExtended => "Uitgebreid",
        TextKey::ContentVariantExtra => "Extra",
        TextKey::ContentVariantFeaturette => "Featurette",
        TextKey::ContentVariantInterview => "Interview",
        TextKey::ContentVariantLive => "Live",
        TextKey::ContentVariantOfficialVideo => "Officiële video",
        TextKey::ContentVariantOther => "Overig",
        TextKey::ContentVariantRemaster => "Remaster",
        TextKey::ContentVariantRemix => "Remix",
        TextKey::ContentVariantScene => "Scène",
        TextKey::ContentVariantSearchHelp => {
            "Varianttokens: content_kind:extra, content_kind:alternate_version, sub_kind:featurette, sub_kind:unplugged, parent_title:\"Filmtitel\", audio_title:commentary, subtitle_title:commentary, has_commentary:true"
        }
        TextKey::ContentVariantShort => "Kort",
        TextKey::ContentVariantSpecialFeatures => "Extra's / special features",
        TextKey::ContentVariantTrailer => "Trailer",
        TextKey::ContentVariantUnplugged => "Unplugged",
        TextKey::ContentVariantVersions => "Versies",
        TextKey::ContentRatings => "Kijkwijzers",
        TextKey::CustomResolutionHeight => "Hoogte van aangepaste resolutie",
        TextKey::CustomResolutionWidth => "Breedte van aangepaste resolutie",
        TextKey::CustomResolutions => "Aangepaste resoluties",
        TextKey::Dark => "Donker",
        TextKey::DaysToBuild => "Dagen om te bouwen",
        TextKey::AddBreakContent => "Breakinhoud toevoegen",
        TextKey::AddDeco => "Decoratie toevoegen",
        TextKey::AddDecoGroup => "Decoratiegroep toevoegen",
        TextKey::AddDecoTemplate => "Decoratiesjabloon toevoegen",
        TextKey::AddDecoTemplateGroup => "Decoratiesjabloongroep toevoegen",
        TextKey::AtChapterMarkers => "Bij hoofdstukmarkeringen",
        TextKey::BackToDecos => "Terug naar decoraties",
        TextKey::BackToDecoTemplates => "Terug naar decoratiesjablonen",
        TextKey::BetweenBlockItems => "Tussen blokitems",
        TextKey::BlockFinish => "Blokeinde",
        TextKey::BlockStart => "Blokstart",
        TextKey::BreakContent => "Breakinhoud",
        TextKey::BreakContentBody => {
            "Breakinhoud kan specifieke items afspelen (met playlists) en blokken aanvullen tot hun volledige ingestelde duur."
        }
        TextKey::BreakContentItem => "Breakinhouditem",
        TextKey::BreakContentMode => "Breakinhoudmodus",
        TextKey::DeadAirFallback => "Dead-air fallback",
        TextKey::DeadAirFallbackBody => {
            "Wanneer er voor de huidige tijd geen uitzenditem wordt gevonden, wordt willekeurig een item uit deze collectie gekozen, herhaald en precies passend ingekort tot de start van het volgende uitzenditem. Dit vervangt de afbeelding Kanaal is offline die anders zou worden getoond."
        }
        TextKey::DeadAirFallbackCollectionType => "Collectietype voor dead-air fallback",
        TextKey::DeadAirFallbackMode => "Dead-air fallbackmodus",
        TextKey::Deco => "Decoratie",
        TextKey::DecoCannotBeAdded => {
            "Decoratie kan niet worden toegevoegd omdat deze zou overlappen of middernacht zou passeren."
        }
        TextKey::DecoCannotBeMoved => {
            "Decoratie kan daar niet naartoe worden verplaatst omdat deze zou overlappen of middernacht zou passeren."
        }
        TextKey::DecoCreated => "Decoratie aangemaakt.",
        TextKey::DecoDeleted => "Decoratie verwijderd.",
        TextKey::DecoGroup => "Decoratiegroep",
        TextKey::DecoGroupCreated => "Decoratiegroep aangemaakt.",
        TextKey::DecoGroupDeleted => "Decoratiegroep verwijderd.",
        TextKey::DecoGroupName => "Naam van decoratiegroep",
        TextKey::DecoGroups => "Decoratiegroepen",
        TextKey::DecoName => "Decoratienaam",
        TextKey::DecoNotFound => "Decoratie niet gevonden.",
        TextKey::DecoSaved => "Decoratie opgeslagen.",
        TextKey::Decos => "Decoraties",
        TextKey::DecoTemplate => "Decoratiesjabloon",
        TextKey::DecoTemplateCreated => "Decoratiesjabloon aangemaakt.",
        TextKey::DecoTemplateDeleted => "Decoratiesjabloon verwijderd.",
        TextKey::DecoTemplateGroup => "Decoratiesjabloongroep",
        TextKey::DecoTemplateGroupCreated => "Decoratiesjabloongroep aangemaakt.",
        TextKey::DecoTemplateGroupDeleted => "Decoratiesjabloongroep verwijderd.",
        TextKey::DecoTemplateGroupName => "Naam van decoratiesjabloongroep",
        TextKey::DecoTemplateGroups => "Decoratiesjabloongroepen",
        TextKey::DecoTemplateName => "Decoratiesjabloonnaam",
        TextKey::DecoTemplateNotFound => "Decoratiesjabloon niet gevonden.",
        TextKey::DecoTemplateSaved => "Decoratiesjabloon opgeslagen.",
        TextKey::AddDecoToTemplate => "Decoratie aan sjabloon toevoegen",
        TextKey::DecoToRemove => "Te verwijderen decoratie",
        TextKey::DefaultFiller => "Standaard filler",
        TextKey::DefaultFillerBody => {
            "Nadat alle blokken zijn gepland, vult een tweede ronde ongeplande tijd met willekeurige items uit deze collectie."
        }
        TextKey::DefaultFillerCollectionType => "Collectietype voor standaard filler",
        TextKey::DefaultFillerMode => "Standaard fillermodus",
        TextKey::DeleteDeco => "Decoratie verwijderen",
        TextKey::DeleteDecoGroup => "Decoratiegroep verwijderen",
        TextKey::DeleteDecoTemplate => "Decoratiesjabloon verwijderen",
        TextKey::DeleteDecoTemplateGroup => "Decoratiesjabloongroep verwijderen",
        TextKey::Disable => "Uitschakelen",
        TextKey::EditDeco => "Decoratie bewerken",
        TextKey::EditDecoTemplate => "Decoratiesjabloon bewerken",
        TextKey::GraphicsElementsMode => "Modus voor grafische elementen",
        TextKey::Inherit => "Overnemen",
        TextKey::Merge => "Samenvoegen",
        TextKey::NoDecosFound => "Geen decoraties gevonden.",
        TextKey::NoDecoTemplatesFound => "Geen decoratiesjablonen gevonden.",
        TextKey::Override => "Overschrijven",
        TextKey::RemoveBreakContent => "Breakinhoud verwijderen",
        TextKey::RemoveDecoFromTemplate => "Decoratie uit sjabloon verwijderen",
        TextKey::Replace => "Vervangen",
        TextKey::SavedDecos => "Opgeslagen decoraties",
        TextKey::SavedDecoTemplates => "Opgeslagen decoratiesjablonen",
        TextKey::SearchForDecos => "Decoraties zoeken",
        TextKey::SearchForDecoTemplates => "Decoratiesjablonen zoeken",
        TextKey::TrimToFit => "Passend inkorten",
        TextKey::UseGraphicsElementsDuringFiller => "Grafische elementen gebruiken tijdens filler",
        TextKey::UseWatermarkDuringFiller => "Zenderlogo gebruiken tijdens filler",
        TextKey::WatermarkMode => "Zenderlogomodus",
        TextKey::DecoTemplates => "Decoratiesjablonen",
        TextKey::DefaultFfmpegProfile => "Standaard FFmpeg-profiel",
        TextKey::DefaultFfmpegProfileHelp => {
            "Het FFmpeg-profiel dat wordt gebruikt bij het maken van nieuwe kanalen"
        }
        TextKey::DefaultMinimumLogLevel => "Standaard minimaal logniveau",
        TextKey::DefaultMpegTsScript => "Standaard MPEG-TS-script",
        TextKey::DefaultMpegTsScriptHelp => "Het MPEG-TS-script dat wordt gebruikt bij streaming",
        TextKey::Decorations => "Decoraties",
        TextKey::Debug => "Debug",
        TextKey::Delete => "Verwijderen",
        TextKey::DeleteAll => "Alles verwijderen",
        TextKey::DeleteBlock => "Blok verwijderen",
        TextKey::DeleteBlockGroup => "Blokgroep verwijderen",
        TextKey::AddFillerPreset => "Opvulvoorinstelling toevoegen",
        TextKey::DeleteCustomResolution => "Aangepaste resolutie verwijderen",
        TextKey::DeleteCollection => "Collectie verwijderen",
        TextKey::DeleteFillerPreset => "Opvulvoorinstelling verwijderen",
        TextKey::DeleteMultiCollection => "Multi-collectie verwijderen",
        TextKey::DeletePlaylist => "Afspeellijst verwijderen",
        TextKey::DeletePlaylistGroup => "Afspeellijstgroep verwijderen",
        TextKey::DeleteRerunCollection => "Herhalingscollectie verwijderen",
        TextKey::DeleteTemplate => "Sjabloon verwijderen",
        TextKey::DeleteTemplateGroup => "Sjabloongroep verwijderen",
        TextKey::DeleteAddonMediaList => "add-on-lijst verwijderen",
        TextKey::DisabledCollectionsAlreadyPresent => {
            "Uitgeschakelde collecties staan al in deze multi-collectie"
        }
        TextKey::EmptyTrash => "Prullenbak legen",
        TextKey::ExtractEmbeddedSubtitles => "Ingesloten (tekst)ondertitels extraheren",
        TextKey::ErasedPlayoutItemsAndHistory => "Uitzenditems en geschiedenis zijn gewist.",
        TextKey::EraseFailed => "Wissen mislukt",
        TextKey::EraseItemsAndHistory => "Items en geschiedenis wissen",
        TextKey::ErasePlayoutItemsAndHistoryConfirm => {
            "Hiermee worden alle uitzenditems en geschiedenis voor deze sequentiële uitzending verwijderd. Doorgaan?"
        }
        TextKey::Episode => "Aflevering",
        TextKey::FanArt => "fanart",
        TextKey::Ffmpeg => "FFmpeg",
        TextKey::FileNotFound => "Bestand niet gevonden",
        TextKey::Finish => "Einde",
        TextKey::Fatal => "Fataal",
        TextKey::FfmpegPath => "FFmpeg-pad",
        TextKey::FfmpegPathHelp => "Het volledige pad naar het uitvoerbare ffmpeg-bestand",
        TextKey::FfmpegProfiles => "FFmpeg-profielen",
        TextKey::FfprobePath => "FFprobe-pad",
        TextKey::FfprobePathHelp => "Het volledige pad naar het uitvoerbare ffprobe-bestand",
        TextKey::FillerPreset => "Opvulvoorinstelling",
        TextKey::FillerPresetActionFailed => "Actie voor opvulvoorinstelling mislukt.",
        TextKey::FillerPresetCouldNotBeSaved => {
            "Opvulvoorinstelling kon niet worden opgeslagen. Controleer de formulierwaarden en probeer het opnieuw."
        }
        TextKey::FillerPresetDeleted => "Opvulvoorinstelling verwijderd.",
        TextKey::FillerPresetExpressionHelp => {
            "Voeg bij mid-roll-opvulling alleen opvulling toe wanneer deze expressie waar is voor een bepaald mid-roll-punt."
        }
        TextKey::FillerPresetSaved => "Opvulvoorinstelling opgeslagen.",
        TextKey::FillerPresets => "Opvulvoorinstellingen",
        TextKey::Flags => "Vlaggen",
        TextKey::GlobalFallbackFiller => "Globale fallback-opvulling",
        TextKey::GlobalWatermark => "Globaal zenderlogo",
        TextKey::GoHome => "Naar startpagina",
        TextKey::HdHomeRun => "HDHomeRun",
        TextKey::HealthChecks => "Gezondheidscontroles",
        TextKey::Height => "Hoogte",
        TextKey::HlsDirectOutputFormat => "Direct HLS-uitvoerformaat",
        TextKey::HlsSegmenterIdleTimeout => "Time-out voor inactieve HLS-segmenter",
        TextKey::HlsSegmenterIdleTimeoutHelp => {
            "Het aantal seconden dat een kanaal blijft transcoderen wanneer geen enkele client nog verzoeken heeft ontvangen"
        }
        TextKey::HlsSegmenterInitialSegmentCount => "Aanvankelijk aantal HLS-segmenten",
        TextKey::HlsSegmenterInitialSegmentCountHelp => {
            "Stelt de start van de stream uit totdat het opgegeven aantal segmenten van 4 seconden is getranscodeerd; een groter aantal betekent een tragere start maar mogelijk minder buffering"
        }
        TextKey::HomeRunning => "IPTV-server is actief.",
        TextKey::ReleaseNotes => "Release notes",
        TextKey::FullChangelog => "Volledige changelog",
        TextKey::HttpMinimumLogLevel => "Minimaal logniveau voor verzoeklogging",
        TextKey::Id => "Id",
        TextKey::Images => "Afbeeldingen",
        TextKey::Item => "Item",
        TextKey::ItemActions => "Itemacties",
        TextKey::ItemType => "Itemtype",
        TextKey::Items => "Items",
        TextKey::Language => "Taal",
        TextKey::Languages => "Talen",
        TextKey::MusicLabels => "Muzieklabels",
        TextKey::LeaveEmptyToAutoDetect => "Laat leeg voor automatische detectie",
        TextKey::Libraries => "Bibliotheken",
        TextKey::LibraryRefreshInterval => "Vernieuwingsinterval bibliotheek",
        TextKey::Light => "Licht",
        TextKey::Lists => "Lijsten",
        TextKey::Loading => "Laden...",
        TextKey::LoadingMembers => "Leden laden...",
        TextKey::Local => "Lokaal",
        TextKey::Logging => "Logboek",
        TextKey::M3u => "M3U",
        TextKey::Media => "Media",
        TextKey::MediaInfo => "Media-informatie",
        TextKey::MediaSources => "Mediabronnen",
        TextKey::Movie => "Film",
        TextKey::MovieNotFound => "Film niet gevonden",
        TextKey::Movies => "Films",
        TextKey::MusicVideo => "Muziekvideo",
        TextKey::MultiCollections => "Multi-collecties",
        TextKey::ManualCollections => "Handmatige collecties",
        TextKey::MatchFailed => "Koppelen mislukt",
        TextKey::MatchStatus => "Koppelstatus",
        TextKey::MatchAddonMediaListItems => "Items van add-on-lijst koppelen",
        TextKey::Members => "Leden",
        TextKey::MultiCollection => "Multi-collectie",
        TextKey::MultiCollectionDeleted => "Multi-collectie verwijderd.",
        TextKey::MultiCollectionItems => "Multi-collectie-items",
        TextKey::MultiCollectionNameRequired => "Naam van multi-collectie is verplicht!",
        TextKey::MultiCollectionNameTooLong => {
            "Naam van multi-collectie mag niet langer zijn dan 50 tekens."
        }
        TextKey::MultiCollectionSaved => "Multi-collectie opgeslagen.",
        TextKey::Music => "Muziek",
        TextKey::Name => "Naam",
        TextKey::NameRequired => "Naam is verplicht",
        TextKey::Networks => "Netwerken",
        TextKey::NewCollection => "(Nieuwe collectie)",
        TextKey::NewCollectionTitle => "Nieuwe collectie",
        TextKey::NewFillerPreset => "Nieuwe opvulvoorinstelling",
        TextKey::NewMultiCollection => "Nieuwe multi-collectie",
        TextKey::NewTemplateGroup => "Nieuwe sjabloongroep",
        TextKey::NewTemplateName => "Nieuwe sjabloonnaam",
        TextKey::Next24Hours => "Volgende 24 uur",
        TextKey::NextPage => "Volgende pagina",
        TextKey::NoFillerPresets => "Nog geen opvulvoorinstellingen.",
        TextKey::NoBlockItems => "Geen blokitems.",
        TextKey::NoItems => "Geen items.",
        TextKey::NoItemsToDisplay => "Geen items om weer te geven.",
        TextKey::NoAddonMediaListsYet => "Nog geen add-on-lijsten.",
        TextKey::No => "Nee",
        TextKey::NoCollections => "Nog geen collecties.",
        TextKey::NoMembers => "Nog geen leden.",
        TextKey::NoMultiCollections => "Nog geen multi-collecties.",
        TextKey::NoTemplatesFound => "Geen sjablonen gevonden.",
        TextKey::NotFound => "Pagina niet gevonden.",
        TextKey::OtherVideo => "Andere video",
        TextKey::OtherVideoNotFound => "Andere video niet gevonden",
        TextKey::OtherVideos => "Andere video's",
        TextKey::OpenBrowser => "Browser openen",
        TextKey::Playout => "Uitzending",
        TextKey::PlayoutItemsAndHistory => "Uitzenditems en geschiedenis",
        TextKey::PlayoutNotFound => "Uitzending niet gevonden.",
        TextKey::Playouts => "Uitzendingen",
        TextKey::AddPlayout => "Uitzending toevoegen",
        TextKey::ClassicScheduleMenu => "Klassiek schema",
        TextKey::BlockScheduleMenu => "Blokschema",
        TextKey::SequentialScheduleMenu => "Sequentieel schema",
        TextKey::ScriptedScheduleMenu => "Scripted schema",
        TextKey::JsonDizqueTvScheduleMenu => "JSON-schema (dizqueTV)",
        TextKey::ResetAllPlayouts => "Alle uitzendingen opnieuw opbouwen",
        TextKey::PlayoutBuildQueued => "Heropbouw van uitzending in wachtrij geplaatst",
        TextKey::PlayoutBuildsQueued => "uitzendingsheropbouwen in wachtrij geplaatst",
        TextKey::SearchForPlayouts => "Zoeken naar uitzendingen",
        TextKey::PlayoutDetail => "Uitzenddetails",
        TextKey::ShowFiller => "Opvulling tonen",
        TextKey::EditPlayout => "Uitzending bewerken",
        TextKey::RefreshPlayout => "Uitzending vernieuwen",
        TextKey::ResetPlayout => "Uitzending opnieuw opbouwen",
        TextKey::RebuildPlayout => "Uitzending bijwerken",
        TextKey::PlayoutRebuildMode => "Actie",
        TextKey::ContinuePlayout => "Uitzending verlengen",
        TextKey::PlayoutContinueHelp => "Voeg programma’s toe na de bestaande planning zonder deze te vervangen.",
        TextKey::PlayoutRefreshHelp => "Bouw de toekomstige planning opnieuw op en vervolg de opgeslagen programmavolgorde.",
        TextKey::PlayoutResetHelp => "Begin met een nieuwe programmavolgorde.",
        TextKey::KeepCurrentProgramme => "Lopend programma behouden",
        TextKey::KeepCurrentProgrammeHelp => "Behoud het volledige programma en programma’s die spelers al hebben voorbereid. Wijzigingen gaan daarna in.",
        TextKey::ReplaceCurrentProgrammeHelp => "Het lopende programma kan worden onderbroken.",
        TextKey::QueuePlayoutBuild => "Bijwerken inplannen",
        TextKey::InvalidPlayoutBuildMode => "Kies een geldige actie voor de uitzending.",
        TextKey::PlayoutContinueStateIncomplete => "De opgeslagen voortgang is onvolledig. De bestaande planning is behouden. Kies Vernieuwen of Opnieuw opbouwen met programmabehoud.",
        TextKey::PlayoutClassicTimelineRejected => "De planning bevat overlappende of ongeldige programmatijden. Gepubliceerde programma’s blijven ongewijzigd. Controleer de planning en bouw opnieuw op vanaf een gecontroleerde programmagrens.",
        TextKey::ScheduleReset => "Heropbouw plannen",
        TextKey::SchedulePlayoutReset => "Heropbouw van uitzending plannen",
        TextKey::EditExternalJsonFile => "Extern JSON-bestand bewerken",
        TextKey::DeletePlayout => "Uitzending verwijderen",
        TextKey::Troubleshoot => "Problemen oplossen",
        TextKey::DailyResetTime => "Dagelijkse heropbouwtijd",
        TextKey::DoNotAutomaticallyReset => "Niet automatisch opnieuw opbouwen",
        TextKey::SaveChanges => "Wijzigingen opslaan",
        TextKey::ExternalJsonFileLabel => "Extern JSON-bestand",
        TextKey::EditExternalJsonFilePrompt => "Bewerk het externe JSON-bestand van de uitzending",
        TextKey::UnscheduledPlayoutItem => "NIET GEPLAND",
        TextKey::DisabledChannelsAlreadyHavePlayout => {
            "Uitgeschakelde kanalen hebben al een uitzending"
        }
        TextKey::ProgramSchedule => "Programmaschema",
        TextKey::SequentialScheduleFileHelp => {
            "Het volledige pad naar het sequentiële schema (YAML)-bestand"
        }
        TextKey::BlockTemplatesAddedLater => "Bloksjablonen worden later toegevoegd",
        TextKey::UnsupportedPlayoutKind => "Niet-ondersteund uitzendingstype.",
        TextKey::ClassicPlayout => "Klassieke uitzending",
        TextKey::ClassicPlayoutState => "Status klassieke uitzending",
        TextKey::ScanRefreshState => "Status van scanvernieuwing",
        TextKey::SubtitleCleanup => "Ondertitels opruimen",
        TextKey::SubtitleCleanupMessage => "Opruimen staat klaar: {count} ondertitelrecords zonder eigenaar of externe ondertitelrecords zonder pad. Automatisch opruimen gebeurt elk uur. Ondertitelbestanden in de cache blijven behouden.",
        TextKey::SubtitleCleanupBrief => "{count} ondertitelrecords wachten op opruimen",
        TextKey::ArtworkOrphans => "Verweesde afbeeldingen",
        TextKey::ArtworkOrphansMessage => "Opruimen staat klaar: {actors} acteurrecords en {artwork} afbeeldingsrecords zonder eigenaar. Automatisch opruimen gebeurt elk uur. Gebruik POST /api/maintenance/clean_artwork om nu op te ruimen. Gekoppelde records en gebruikte afbeeldingen in de cache blijven behouden.",
        TextKey::ArtworkOrphansBrief => "{actors} verweesde acteurrecords en {artwork} verweesde afbeeldingsrecords",
        TextKey::ArtworkCardinality => "Cardinaliteit van afbeeldingen",
        TextKey::Av1OutputCompatibility => "AV1-uitvoer vereist native HLS. Wijzig de engine/modus van het kanaal of kies H.264/HEVC in het FFmpeg-profiel.",
        TextKey::HlsLiveDelivery => "HLS-livelevering",
        TextKey::BlockPlayout => "Blokuitzending",
        TextKey::ScriptedPlayout => "Scripted uitzending",
        TextKey::AlternateSchedules => "Alternatieve schema's",
        TextKey::EditAlternateSchedules => "Alternatieve schema's bewerken",
        TextKey::PlayoutTemplates => "Uitzendsjablonen",
        TextKey::EditTemplates => "Sjablonen bewerken",
        TextKey::DefaultDeco => "Standaarddecoratie",
        TextKey::EnableDefaultDeco => "Standaarddecoratie inschakelen",
        TextKey::SaveDefaultDeco => "Standaarddecoratie opslaan",
        TextKey::SaveScriptedSchedule => "Scripted schema opslaan",
        TextKey::EraseItems => "Items wissen",
        TextKey::ErasedPlayoutItems => "Uitzenditems zijn gewist.",
        TextKey::SavedDefaultDeco => "Standaarddecoratie is opgeslagen.",
        TextKey::SavedScriptedSchedule => "Scripted schema is opgeslagen.",
        TextKey::Maintenance => "Onderhoud",
        TextKey::SaveAlternateSchedules => "Alternatieve schema's opslaan",
        TextKey::AddAlternateSchedule => "Alternatief schema toevoegen",
        TextKey::SaveTemplates => "Sjablonen opslaan",
        TextKey::PreviewCalendar => "Kalender bekijken",
        TextKey::EditContent => "Content bewerken",
        TextKey::PlayoutNestedCatchAllHelp => {
            "In prioriteitsvolgorde van boven naar beneden. De onderste regel komt altijd overeen met alle dagen en maanden als vangnet."
        }
        TextKey::PlayoutNestedPriorityHelp => "In prioriteitsvolgorde van boven naar beneden.",
        TextKey::DaysOfWeek => "Dagen van de week",
        TextKey::DaysOfMonth => "Dagen van de maand",
        TextKey::Months => "Maanden",
        TextKey::LimitToDateRange => "Beperken tot datumbereik",
        TextKey::StartDay => "Startdag",
        TextKey::EndDay => "Einddag",
        TextKey::Enable => "Inschakelen",
        TextKey::Up => "Omhoog",
        TextKey::Down => "Omlaag",
        TextKey::Weekdays => "Weekdagen",
        TextKey::Weekends => "Weekenden",
        TextKey::Sunday => "Zondag",
        TextKey::Monday => "Maandag",
        TextKey::Tuesday => "Dinsdag",
        TextKey::Wednesday => "Woensdag",
        TextKey::Thursday => "Donderdag",
        TextKey::Friday => "Vrijdag",
        TextKey::Saturday => "Zaterdag",
        TextKey::January => "Januari",
        TextKey::February => "Februari",
        TextKey::March => "Maart",
        TextKey::April => "April",
        TextKey::May => "Mei",
        TextKey::June => "Juni",
        TextKey::July => "Juli",
        TextKey::August => "Augustus",
        TextKey::September => "September",
        TextKey::October => "Oktober",
        TextKey::November => "November",
        TextKey::December => "December",
        TextKey::JanuaryShort => "Jan",
        TextKey::FebruaryShort => "Feb",
        TextKey::MarchShort => "Mrt",
        TextKey::AprilShort => "Apr",
        TextKey::MayShort => "Mei",
        TextKey::JuneShort => "Jun",
        TextKey::JulyShort => "Jul",
        TextKey::AugustShort => "Aug",
        TextKey::SeptemberShort => "Sep",
        TextKey::OctoberShort => "Okt",
        TextKey::NovemberShort => "Nov",
        TextKey::DecemberShort => "Dec",
        TextKey::PlayAll => "Alles afspelen",
        TextKey::Playlist => "Afspeellijst",
        TextKey::PlaylistAdded => "Afspeellijst toegevoegd.",
        TextKey::PlaylistDeleted => "Afspeellijst verwijderd.",
        TextKey::PlaylistGroup => "Afspeellijstgroep",
        TextKey::PlaylistGroupAdded => "Afspeellijstgroep toegevoegd.",
        TextKey::PlaylistGroupDeleted => "Afspeellijstgroep verwijderd.",
        TextKey::PlaylistGroupName => "Naam van afspeellijstgroep",
        TextKey::PlaylistGroups => "Afspeellijstgroepen",
        TextKey::PlaylistItem => "Afspeellijstitem",
        TextKey::PlaylistItems => "Afspeellijstitems",
        TextKey::PlaylistName => "Afspeellijstnaam",
        TextKey::PlaylistNotFound => "Afspeellijst niet gevonden.",
        TextKey::PlaylistPlayoutPreview => "Voorbeeld van afspeellijstuitzending",
        TextKey::PlaylistPreviewEmpty => "Voorbeeld van afspeellijstuitzending heeft geen items",
        TextKey::Playlists => "Afspeellijsten",
        TextKey::ProbeForInterlacedFrames => "Controleren op geïnterlinieerde frames",
        TextKey::PreferredAudioLanguage => "Voorkeurstaal voor audio",
        TextKey::DefaultSubtitleLanguage => "Standaardtaal voor ondertitels",
        TextKey::FallbackSubtitleLanguage => "Terugvaltaal voor ondertitels",
        TextKey::OriginalLanguageOption => "Origineel (und)",
        TextKey::UndefinedLanguageOption => "Ongedefinieerd (und)",
        TextKey::Previous24Hours => "Vorige 24 uur",
        TextKey::PreviousPage => "Vorige pagina",
        TextKey::RerunCollections => "Herhalingscollecties",
        TextKey::RerunCollection => "Herhalingscollectie",
        TextKey::RerunCollectionActionFailed => "Herhalingscollectieactie mislukt.",
        TextKey::RerunCollectionDeleted => "Herhalingscollectie verwijderd.",
        TextKey::RerunCollectionNameRequired => "Naam van herhalingscollectie is verplicht!",
        TextKey::RerunCollectionSaved => "Herhalingscollectie opgeslagen.",
        TextKey::RerunPlaybackOrder => "Afspeelvolgorde voor herhalingen",
        TextKey::RemoteStreams => "Externe streams",
        TextKey::RemoteStream => "Externe stream",
        TextKey::PublishedOptions => "Gepubliceerde opties",
        TextKey::PlaybackHealth => "Afspeelstatus",
        TextKey::Configure => "Configureren",
        TextKey::Watch => "Bekijken",
        TextKey::RemoteStreamPlaylists => "Afspeellijsten met externe streams",
        TextKey::PlaylistUrl => "Afspeellijst-URL",
        TextKey::Extractor => "Extractor",
        TextKey::DestinationFolder => "Doelmap",
        TextKey::SyncInterval => "Synchronisatie-interval",
        TextKey::MaxItems => "Maximumaantal items",
        TextKey::RemovedEntries => "Verwijderde items",
        TextKey::CreateOrderedPlaylist => "Geordende afspeellijst maken",
        TextKey::SyncNow => "Nu synchroniseren",
        TextKey::LastSync => "Laatste synchronisatie",
        TextKey::NeverSynced => "Nooit gesynchroniseerd",
        TextKey::ManagedManifestReadOnly => {
            "Dit handmatig geschreven manifest is hier alleen-lezen."
        }
        TextKey::Truncated => "Ingekort",
        TextKey::MissingDuration => "Duur ontbreekt",
        TextKey::Orphaned => "Niet meer aanwezig",
        TextKey::Materialized => "Aangemaakt",
        TextKey::State => "Status",
        TextKey::Upstream => "Bronvolgorde",
        TextKey::NewestFirst => "Nieuwste eerst",
        TextKey::Keep => "Behouden",
        TextKey::ResetChannels => "Kanalen resetten",
        TextKey::RowsPerPage => "Rijen per pagina",
        TextKey::Save => "Opslaan",
        TextKey::SaveMultiCollection => "Multi-collectie opslaan",
        TextKey::SavePlaylist => "Afspeellijst opslaan",
        TextKey::SaveTroubleshootingReportsToDisk => {
            "Probleemoplossingsrapporten op schijf opslaan"
        }
        TextKey::SaveAsSmartCollection => "Opslaan als slimme collectie",
        TextKey::SearchQueryRequired => "Zoekopdracht is verplicht",
        TextKey::SaveSettings => "Instellingen opslaan",
        TextKey::Scanner => "Scanner",
        TextKey::ScanningMinimumLogLevel => "Minimaal logniveau voor scannen",
        TextKey::Scheduling => "Planning",
        TextKey::SchedulingMinimumLogLevel => "Minimaal logniveau voor planning",
        TextKey::StartWithWindows => "ErsatzRS starten wanneer ik me bij Windows aanmeld",
        TextKey::StartWithWindowsHelp => {
            "Start de tray-launcher automatisch voor de huidige Windows-gebruiker."
        }
        TextKey::Schedules => "Schema's",
        TextKey::AddSchedule => "Schema toevoegen",
        TextKey::AddScheduleItem => "Schema-item toevoegen",
        TextKey::AddSmartCollection => "Slimme collectie toevoegen",
        TextKey::Album => "Album",
        TextKey::Chronological => "Chronologisch",
        TextKey::CollectionSize => "Collectiegrootte",
        TextKey::CollectionType => "Collectietype",
        TextKey::Count => "Aantal",
        TextKey::CopySchedule => "Schema kopiëren",
        TextKey::CopyScheduleItem => "Schema-item kopiëren",
        TextKey::CustomTitle => "Aangepaste titel",
        TextKey::DeleteSchedule => "Schema verwijderen",
        TextKey::DeleteScheduleItem => "Schema-item verwijderen",
        TextKey::DeleteSmartCollection => "Slimme collectie verwijderen",
        TextKey::DetailRowsPerPage => "Detailrijen per pagina",
        TextKey::Director => "Regisseur",
        TextKey::Directors => "Regisseurs",
        TextKey::DiscardToFillAttempts => "Pogingen om te vullen door overslaan",
        TextKey::Duration => "Duur",
        TextKey::Dynamic => "Dynamisch",
        TextKey::EditProperties => "Eigenschappen bewerken",
        TextKey::EditScheduleItems => "Schema-items bewerken",
        TextKey::EditSmartCollection => "Slimme collectie bewerken",
        TextKey::EnterNameForNewSchedule => "Voer een naam in voor het nieuwe schema",
        TextKey::Fixed => "Vast",
        TextKey::FixedStartTimeBehavior => "Gedrag voor vaste starttijd",
        TextKey::Flood => "Flood",
        TextKey::Flexible => "Flexibel",
        TextKey::FlexibleStartSoonAsPossible => "Flexibel - start zo snel mogelijk",
        TextKey::FlexibleStartSoonAsPossibleAfterStartTime => {
            "Flexibel - start zo snel mogelijk na de starttijd"
        }
        TextKey::FillWithGroupMode => "Vullen met groepsmodus (serie of artiest)",
        TextKey::GraphicsElements => "Grafische elementen",
        TextKey::AddGraphicsElement => "Grafisch element toevoegen",
        TextKey::DrawtextRendererOption => "Drawtext — dynamisch/bestandsgebaseerd",
        TextKey::GraphicsMetadataRefreshSeconds => "Programmametadata vernieuwen (seconden)",
        TextKey::GraphicsMetadataRefreshHelp => "0 bewaart metadata van het startmoment. 5–60 leest programmadetails en itemvariabelen opnieuw. De Drawtext-klok en het herladen van het FFmpeg-bestand werken afzonderlijk; drawvg gebruikt een momentopname bij de start.",
        TextKey::GraphicsMetadataRefreshBounds => "Gebruik 0 voor een momentopname bij de start of 5–60 seconden voor actuele metadata.",
        TextKey::ProgrammeTemplateHelp => "CurrentProgramme, NextProgramme, ContinuingProgramme en LaterToday verwijzen naar volledige programma’s. Laat ontbrekende tekst weg met {{ NextProgramme.Title | default(\"\", true) }}. De bestaande Epg blijft beschikbaar.",
        TextKey::GraphicsItemVariables => "Variabelen voor dit item (JSON)",
        TextKey::GraphicsItemVariablesHelp => "Overrides gelden voor de geselecteerde graphics van dit schedule-item. Een leeg object gebruikt de native programmametadata. Uitgesteld opslaan past ze toe op nieuw gegenereerde items.",
        TextKey::GraphicsPreviewAmbiguous => "Op dit tijdstip overlappen meerdere geplande items. Er kan niet veilig een programma worden gekozen.",
        TextKey::GraphicsContextPreview => "Voorbeeld van programmatekst",
        TextKey::GraphicsContextPreviewHelp => "Vul de concepttekst in met het gekozen kanaal, het werkelijke geplande item en de variabelen van dat item. Dit tekstvoorbeeld toont geen lettermetingen of plaatsing.",
        TextKey::GraphicsPreviewTimeHelp => "Voer een ISO-tijdstip met tijdzoneverschil in, bijvoorbeeld 2026-10-05T21:30:00+02:00. Historische tijdstippen gebruiken de opgeslagen planning.",
        TextKey::GraphicsPreviewSavedHelp => "Zonder JavaScript gebruikt het voorbeeld de opgeslagen tekst. Sla concepttekst op voordat u deze bekijkt.",
        TextKey::GraphicsItemVariablesInvalid => "Gebruik JSON-objecten voor geselecteerde grafische elementen, met maximaal 64 KiB per element.",
        TextKey::DrawtextSettings => "Drawtext-instellingen",
        TextKey::DrawvgRendererOption => "Drawvg — statisch/vector",
        TextKey::EditGraphicsElement => "Grafisch element bewerken",
        TextKey::FontFile => "Lettertypebestand",
        TextKey::FontFileOptionalHelp => {
            "Optioneel. Laat leeg om het meegeleverde Roboto-lettertype te gebruiken."
        }
        TextKey::GraphicsElement => "Grafisch element",
        TextKey::GraphicsElementConfigInvalid => {
            "Dit grafische tekstelement heeft een ongeldige configuratie en is niet gewijzigd."
        }
        TextKey::GraphicsElementKindImage => "Afbeelding",
        TextKey::GraphicsElementKindMotion => "Beweging",
        TextKey::GraphicsElementKindScript => "Script",
        TextKey::GraphicsElementKindSubtitle => "Ondertiteling",
        TextKey::GraphicsElementKindText => "Tekst",
        TextKey::GraphicsElementNameTooLong => "De naam mag maximaal 50 tekens bevatten.",
        TextKey::GraphicsElementNotEditable => "Hier niet bewerkbaar",
        TextKey::GraphicsElementNotFound => "Grafisch element niet gevonden.",
        TextKey::GraphicsElementNotText => {
            "Alleen grafische tekstelementen kunnen hier worden bewerkt."
        }
        TextKey::GraphicsElementSaved => "Grafisch element opgeslagen.",
        TextKey::GraphicsElementSaveFailed => "Het grafische element kon niet worden opgeslagen.",
        TextKey::GraphicsElementType => "Type",
        TextKey::GraphicsExistingDrawvgHelp => {
            "Bestaande en nieuwe elementen gebruiken standaard Drawvg."
        }
        TextKey::GraphicsStreamRestartHelp => {
            "Wijzigingen gelden voor nieuw gestarte streams en herstarten actieve streams niet."
        }
        TextKey::NoGraphicsElements => "Geen grafische elementen gevonden.",
        TextKey::RefreshPeriodBounds => {
            "De vernieuwingsperiode moet tussen 250 en 60000 milliseconden liggen."
        }
        TextKey::RefreshPeriodMilliseconds => "Vernieuwingsperiode (milliseconden)",
        TextKey::ReloadFrameInterval => "Frame-interval voor herladen",
        TextKey::ReloadFrameIntervalBounds => {
            "Het frame-interval voor herladen moet minimaal 1 zijn."
        }
        TextKey::Renderer => "Renderer",
        TextKey::SaveGraphicsElement => "Grafisch element opslaan",
        TextKey::TextTemplate => "Tekstsjabloon",
        TextKey::TextTemplateRequired => "Een tekstsjabloon is verplicht.",
        TextKey::TextTemplateTooLong => {
            "Het sjabloon moet geldige UTF-8-tekst van maximaal 64 KiB zijn."
        }
        TextKey::GraphicsIds => "Grafische ID's",
        TextKey::Genres => "Genres",
        TextKey::GroupName => "Groepsnaam",
        TextKey::GroupMultiPartEpisodesHelp => "Groepeer meerdelige afleveringen bij shuffle",
        TextKey::GuideMode => "Gidsmodus",
        TextKey::KeepMultiPartEpisodesTogether => "Meerdelige afleveringen bij elkaar houden",
        TextKey::Marathon => "Marathon",
        TextKey::MarathonBatchSize => "Marathonbatchgrootte",
        TextKey::MarathonBatchSizeHelp => {
            "Hoeveel items uit elke groep worden afgespeeld voordat wordt doorgegaan; leeg of nul speelt alle items af"
        }
        TextKey::MarathonGroupBy => "Marathon groeperen op",
        TextKey::MarathonShuffleGroups => "Shuffle op marathongroepen",
        TextKey::MarathonShuffleItems => "Shuffle op marathonitems",
        TextKey::EditFillerPreset => "Opvulvoorinstelling bewerken",
        TextKey::MidRollFiller => "Mid-roll filler",
        TextKey::MidRoll => "Mid-roll",
        TextKey::MoveDown => "Omlaag verplaatsen",
        TextKey::MoveUp => "Omhoog verplaatsen",
        TextKey::ReorderPlaylistItem => "Sleep om het afspeellijstitem te verplaatsen",
        TextKey::MultiEpisodeGroupSize => "Groepsgrootte voor meerdere afleveringen",
        TextKey::MultiEpisodeShuffle => "Shuffle op meerdere afleveringen",
        TextKey::Multiple => "Meerdere",
        TextKey::MultipleCount => "Aantal meerdere",
        TextKey::MultipleCountHelp => {
            "Kan een expressie zijn met 'count' (collectiegrootte) en 'random' (0 tot count-1)"
        }
        TextKey::MultipleMode => "Modus voor meerdere",
        TextKey::NewScheduleName => "Nieuwe schemanaam",
        TextKey::NewSmartCollection => "Nieuwe slimme collectie",
        TextKey::NoScheduleItems => "Geen schema-items.",
        TextKey::NoSchedules => "Geen schema's.",
        TextKey::Normal => "Normaal",
        TextKey::Offline => "Offline",
        TextKey::One => "Een",
        TextKey::OrderedGroups => "Geordende groepen",
        TextKey::Playback => "Afspelen",
        TextKey::PlaybackOrder => "Afspeelvolgorde",
        TextKey::PlaybackOrderUpdateFailed => "Afspeelvolgorde bijwerken mislukt",
        TextKey::PlaybackOrderUpdated => "Afspeelvolgorde bijgewerkt.",
        TextKey::PlaylistItemSize => "Playlistitemgrootte",
        TextKey::PlayoutDurationHours => "Uitzendduur in uren",
        TextKey::PlayoutDurationMinutes => "Uitzendduur in minuten",
        TextKey::PostRollFiller => "Post-roll filler",
        TextKey::PostRoll => "Post-roll",
        TextKey::PreRollFiller => "Pre-roll filler",
        TextKey::PreRoll => "Pre-roll",
        TextKey::Random => "Willekeurig",
        TextKey::RandomCount => "Willekeurig aantal",
        TextKey::RandomRotation => "Willekeurige rotatie",
        TextKey::RemainingDurationHelp => "duur van de inhoud na dit mid-roll-punt, in seconden",
        TextKey::RandomStartPoint => "Willekeurig startpunt",
        TextKey::RemoveFailed => "Verwijderen mislukt",
        TextKey::RemovedFromCollection => "Uit collectie verwijderd.",
        TextKey::RemoveFromCollection => "Uit collectie verwijderen",
        TextKey::Rename => "Naam wijzigen",
        TextKey::RenameFailed => "Naam wijzigen mislukt",
        TextKey::ReorderFailed => "Volgorde wijzigen mislukt",
        TextKey::RerunFirstRun => "Herhaling - eerste uitzending",
        TextKey::RerunRerun => "Herhaling - herhaling",
        TextKey::ReadOnlyTwentyFourHourPlayoutView => {
            "Alleen-lezen 24-uursweergave van geplande uitzenditems."
        }
        TextKey::SaveSchedule => "Schema opslaan",
        TextKey::SaveScheduleItems => "Schema-items opslaan",
        TextKey::ScheduleSaveDeferred => "Opslaan voor volgende uitbreiding",
        TextKey::ScheduleSaveAndRefresh => "Opslaan en vernieuwen",
        TextKey::ScheduleDeferredHelp => "Opslaan voor de volgende uitbreiding behoudt gepubliceerde programma’s en de afspeelvolgorde. Dit ondersteunt wijzigingen aan opvulling, graphics, gidsgegevens en tracks. Gebruik Opslaan en vernieuwen voor wijzigingen aan bronnen, volgorde of programmaopbouw.",
        TextKey::ScheduleDeferredPending => "Wacht op de volgende uitbreiding",
        TextKey::ScheduleDeferredApplied => "Toegepast vanaf",
        TextKey::ScheduleDeferredBoundary => "Al gepland tot",
        TextKey::ScheduleDeferredLastBuild => "Laatste opbouw",
        TextKey::ScheduleDeferredBuildSucceeded => "Geslaagd",
        TextKey::ScheduleDeferredSaved => "Opgeslagen; gepubliceerde programma’s blijven ongewijzigd.",
        TextKey::ScheduleDeferredIncompatible => "Uitgesteld opslaan kan bronnen, afspeelvolgorde of programmaopbouw niet wijzigen. Gebruik hiervoor Opslaan en vernieuwen.",
        TextKey::ScheduleDeferredBusy => "Er wordt een uitzending opgebouwd. Wacht tot dit klaar is en probeer opnieuw.",
        TextKey::ScheduleDeferredInvalidTimeline => "De gepubliceerde planning bevat ongeldige programmatijden. Herstel deze voordat je een uitgestelde wijziging opslaat.",
        TextKey::SaveSmartCollection => "Slimme collectie opslaan",
        TextKey::Schedule => "Schema",
        TextKey::ScheduleActionFailed => "Schema-actie mislukt.",
        TextKey::ScheduleAsGroup => "Als groep plannen",
        TextKey::ScheduleKind => "Schematype",
        TextKey::ScheduleKindBlock => "Blok",
        TextKey::ScheduleKindClassic => "Klassiek",
        TextKey::ScheduleKindExternalJson => "JSON (dizqueTV)",
        TextKey::ScheduleKindScripted => "Scripted",
        TextKey::ScheduleKindSequential => "Sequentieel",
        TextKey::SchedulingContext => "Planningscontext",
        TextKey::ScheduleItem => "Schema-item",
        TextKey::ScheduleItemActionFailed => "Schema-itemactie mislukt.",
        TextKey::ScheduleItemFiller => "Filler voor schema-item",
        TextKey::ScheduleItemFillerHelp => {
            "Pre-, mid- en post-roll filler wordt toegepast op elk media-item dat door dit schema-item wordt geselecteerd. Voeg content als volgend schema-item toe om deze eenmaal na een blok met Meerdere af te spelen."
        }
        TextKey::ScheduleItemOverrides => "Overschrijvingen voor schema-item",
        TextKey::ScheduleItems => "Schema-items",
        TextKey::ScheduleNotFound => "Schema niet gevonden.",
        TextKey::ScheduleSaveFailed => "Schema opslaan mislukt.",
        TextKey::SmartShuffle => "Smart Shuffle",
        TextKey::SmartShufflePreset => "Afstandsvoorinstelling",
        TextKey::Balanced => "Gebalanceerd",
        TextKey::SmartShuffleScope => "Smart Shuffle-bereik",
        TextKey::SmartShuffleScopePlayout => "Uitzending",
        TextKey::SmartShuffleScopeScheduleItem => "Schema-item",
        TextKey::SmartShuffleValidation => {
            "Kies een afstandsdrempel en ten minste één gelijkenisregel."
        }
        TextKey::SmartShuffleScopeContentSource => "Inhoudsbron",
        TextKey::SmartShuffleAfterItems => "Video's tussen vergelijkbare items",
        TextKey::SmartShuffleAfterDurationMinutes => "Smart Shuffle-duur (minuten)",
        TextKey::SmartShuffleIncludeFiller => "Filler meetellen",
        TextKey::SmartShuffleInheritedBlock => {
            "Dit item heeft geen Smart Shuffle-overschrijving en behoudt eventueel verouderd uitzendbeleid."
        }
        TextKey::SmartShuffleInactive => {
            "Smart Shuffle is niet actief omdat deze afspeelvolgorde niet willekeurig is."
        }
        TextKey::SmartShuffleOverrideBlock => "Overschrijven voor dit blokitem",
        TextKey::SimilarityRules => "Gelijkenisregels",
        TextKey::AlbumArtist => "Albumartiest",
        TextKey::Apply => "Toepassen",
        TextKey::ContentParent => "Bovenliggend item of variant",
        TextKey::SmartShuffleFilterRules => "Filterregels",
        TextKey::SearchForSchedules => "Schema's zoeken",
        TextKey::SearchForManualCollections => "Handmatige collecties zoeken",
        TextKey::SearchForRerunCollections => "Herhalingscollecties zoeken",
        TextKey::SearchForSmartCollections => "Slimme collecties zoeken",
        TextKey::SearchForTemplates => "Sjablonen zoeken",
        TextKey::SearchForBlocks => "Blokken zoeken",
        TextKey::SearchForMultiCollections => "Multi-collecties zoeken",
        TextKey::SearchQuery => "Zoekopdracht",
        TextKey::BlockSearchQueryPlaceholder => "bijv. genre:action",
        TextKey::SearchTitle => "Zoektitel",
        TextKey::SearchTitleDisplay => "Zoektitel (weergave)",
        TextKey::SearchSmartCollection => "Slimme collectie zoeken",
        TextKey::Season => "Seizoen",
        TextKey::SeasonEpisode => "Seizoen, aflevering",
        TextKey::Unnumbered => "Ongenummerd",
        TextKey::SeasonNotFound => "Seizoen niet gevonden",
        TextKey::Select => "Selecteren",
        TextKey::SelectCollection => "Collectie selecteren",
        TextKey::SelectGroupForNewTemplate => "Selecteer een groep voor het nieuwe sjabloon",
        TextKey::SelectScheduleItemPrompt => {
            "Selecteer een schema-item om te bewerken of voeg een nieuw item toe."
        }
        TextKey::SelectSchedule => "Selecteer een schema",
        TextKey::SelectSmartCollection => "Slimme collectie selecteren",
        TextKey::Shuffle => "Shuffle",
        TextKey::ShuffleInOrder => "Shuffle in volgorde",
        TextKey::ShuffleScheduleItems => "Shuffle op schema-items",
        TextKey::ShuffleScheduleItemsHelp => {
            "Opmerking: dit schakelt vaste starttijden en flood-modus uit"
        }
        TextKey::ShuffledGroups => "Shuffle-groepen",
        TextKey::Show => "Serie",
        TextKey::ShowNotFound => "Serie niet gevonden",
        TextKey::Shows => "Series",
        TextKey::Start => "Start",
        TextKey::StartTime => "Starttijd",
        TextKey::StartTimeOnOrAfter => "Starttijd op of na",
        TextKey::StartType => "Starttype",
        TextKey::Strict => "Strikt",
        TextKey::StrictAlwaysWaitExactStartTime => "Strikt - altijd wachten op de exacte starttijd",
        TextKey::StrictWaitExactStartTime => "Strikt - wachten op de exacte starttijd",
        TextKey::TailFiller => "Eindfiller",
        TextKey::TailMode => "Eindmodus",
        TextKey::Tail => "Einde",
        TextKey::TelevisionSeason => "TV-seizoen",
        TextKey::TelevisionShow => "TV-serie",
        TextKey::TreatCollectionsAsShows => "Collecties als series behandelen",
        TextKey::TreatCollectionsAsShowsHelp => "Nuttig voor meerdelige crossover-afleveringen",
        TextKey::Type => "Type",
        TextKey::TypeToSearch => "Typ om te zoeken...",
        TextKey::Search => "Zoeken",
        TextKey::Filters => "Filters",
        TextKey::ActiveFilters => "actieve filters",
        TextKey::ClearFilters => "Filters wissen",
        TextKey::CreateSmartCollection => "Slimme collectie maken",
        TextKey::FromYear => "Vanaf jaar",
        TextKey::ToYear => "Tot jaar",
        TextKey::CollectionComposer => "Composer voor slimme collecties",
        TextKey::CollectionDetails => "Collectiegegevens",
        TextKey::Rules => "Regels",
        TextKey::Overview => "Overzicht",
        TextKey::MatchAll => "Aan alles voldoen",
        TextKey::MatchAllHelp => "Elke regel in deze groep moet overeenkomen.",
        TextKey::MatchAny => "Aan één of meer voldoen",
        TextKey::MatchAnyHelp => "Ten minste één regel in deze groep moet overeenkomen.",
        TextKey::ExcludeAny => "Overeenkomsten uitsluiten",
        TextKey::ExcludeAnyHelp => {
            "Items die aan een regel in deze groep voldoen, worden uitgesloten."
        }
        TextKey::AddRule => "Regel toevoegen",
        TextKey::EditRule => "Regel bewerken",
        TextKey::RemoveRule => "Regel verwijderen",
        TextKey::RuleField => "Veld",
        TextKey::Genre => "Genre",
        TextKey::Studio => "Studio",
        TextKey::Network => "Netwerk",
        TextKey::Actor => "Acteur",
        TextKey::Writer => "Scenarioschrijver",
        TextKey::SubtitleLanguage => "Ondertiteltaal",
        TextKey::ShowTitle => "Serietitel",
        TextKey::ContentKindLabel => "Inhoudstype",
        TextKey::SubKindLabel => "Subtype",
        TextKey::HasCommentary => "Heeft commentaar",
        TextKey::AllFields => "Alle velden",
        TextKey::MatchType => "Vergelijking",
        TextKey::MatchValue => "Waarde",
        TextKey::TextContains => "Tekst bevat",
        TextKey::ExactPhrase => "Exacte woordgroep",
        TextKey::Wildcard => "Jokerteken",
        TextKey::Fuzzy => "Benaderend",
        TextKey::Range => "Bereik",
        TextKey::RangeStart => "Begin van bereik",
        TextKey::RangeEnd => "Einde van bereik",
        TextKey::NestedGroup => "Geneste groep",
        TextKey::NestedGroupHelp => {
            "Bewerk deze groep in focusmodus, zodat steeds één logisch niveau zichtbaar is."
        }
        TextKey::AddNestedGroup => "Geneste groep toevoegen",
        TextKey::EditNestedGroup => "Geneste groep bewerken",
        TextKey::BackToRules => "Terug naar regels",
        TextKey::BackToSearch => "Terug naar zoeken",
        TextKey::CanonicalQuery => "Canonieke zoekopdracht",
        TextKey::CanonicalQueryHelp => {
            "Dit is de zoekopdracht die zoeken, planning en de opgeslagen collectie gebruiken."
        }
        TextKey::LivePreview => "Live voorbeeld",
        TextKey::MatchingItems => "overeenkomende items",
        TextKey::QueryValid => "Zoekopdracht is geldig",
        TextKey::QueryInvalid => "Zoekopdracht vereist aandacht",
        TextKey::SearchRuleValueRequired => "Iedere zoekregel heeft een waarde nodig.",
        TextKey::SearchGroupRequired => "Voeg ten minste één regel toe aan deze groep.",
        TextKey::SearchWildcardRequired => "Een jokertekenregel moet * of ? bevatten.",
        TextKey::SavedSearches => "Opgeslagen zoekopdrachten",
        TextKey::SavedSearchName => "Naam opgeslagen zoekopdracht",
        TextKey::SavedSearchHelp => {
            "Sla deze complexe zoekopdracht op om haar later te hervatten en wijzigen."
        }
        TextKey::ChooseSavedSearch => "Kies een opgeslagen zoekopdracht",
        TextKey::LoadSavedSearch => "Zoekopdracht laden",
        TextKey::SaveSavedSearch => "Zoekopdracht opslaan",
        TextKey::UpdateSavedSearch => "Opgeslagen zoekopdracht bijwerken",
        TextKey::NewSavedSearch => "Nieuwe opgeslagen zoekopdracht",
        TextKey::DeleteSavedSearch => "Opgeslagen zoekopdracht verwijderen",
        TextKey::SavedSearchSaved => "Zoekopdracht opgeslagen.",
        TextKey::SavedSearchDeleted => "Opgeslagen zoekopdracht verwijderd.",
        TextKey::SavedSearchLoaded => "Opgeslagen zoekopdracht geladen.",
        TextKey::SavedSearchNameRequired => {
            "Een naam voor de opgeslagen zoekopdracht is verplicht."
        }
        TextKey::SavedSearchNameTooLong => "De naam mag maximaal 80 tekens bevatten.",
        TextKey::SavedSearchNameUnique => "De naam van de opgeslagen zoekopdracht moet uniek zijn.",
        TextKey::NoSavedSearches => "Nog geen opgeslagen zoekopdrachten.",
        TextKey::RefreshPreview => "Voorbeeld vernieuwen",
        TextKey::NoRules => "Deze groep bevat geen regels.",
        TextKey::SearchFiltersHelp => {
            "Alleen veelgebruikte filters. Gebruik de collectiecomposer voor geneste logica."
        }
        TextKey::SearchMedia => "Media zoeken",
        TextKey::SearchMoviesOrShows => "Films of series zoeken...",
        TextKey::SearchAddonMediaList => "add-on-lijst zoeken",
        TextKey::SearchingMinimumLogLevel => "Minimaal logniveau voor zoeken",
        TextKey::Settings => "Instellingen",
        TextKey::SettingsSaveFailed => "Instellingen opslaan mislukt",
        TextKey::ShowMediaInfo => "Media-info weergeven",
        TextKey::ScriptedScheduleTimeout => "Time-out voor Scripted schema",
        TextKey::SkipMissingItems => "Ontbrekende items overslaan",
        TextKey::SkipMissingItemsHelp => {
            "Bepaalt of bestand-niet-gevonden of niet-beschikbare items in uitzendingen worden opgenomen"
        }
        TextKey::SmartCollections => "Slimme collecties",
        TextKey::SmartCollection => "Slimme collectie",
        TextKey::SmartCollectionDeleted => "Slimme collectie verwijderd.",
        TextKey::SmartCollectionQueryPlaceholder => "bijv. genre:action OR genre:comedy",
        TextKey::SmartCollectionSaved => "Slimme collectie opgeslagen.",
        TextKey::SaveAddonMediaList => "add-on-lijst opslaan",
        TextKey::Songs => "Nummers",
        TextKey::SplitTimeEvenly => "Tijd gelijk verdelen",
        TextKey::StreamingMinimumLogLevel => "Minimaal logniveau voor streaming",
        TextKey::Studios => "Studio's",
        TextKey::Support => "Ondersteuning",
        TextKey::Template => "Sjabloon",
        TextKey::TemplateCopied => "Sjabloon gekopieerd.",
        TextKey::TemplateCreated => "Sjabloon aangemaakt.",
        TextKey::TemplateDeleted => "Sjabloon verwijderd.",
        TextKey::TemplateGroup => "Sjabloongroep",
        TextKey::TemplateGroupCreated => "Sjabloongroep aangemaakt.",
        TextKey::TemplateGroupDeleted => "Sjabloongroep verwijderd.",
        TextKey::TemplateGroupName => "Naam van sjabloongroep",
        TextKey::TemplateGroups => "Sjabloongroepen",
        TextKey::TemplateName => "Sjabloonnaam",
        TextKey::TemplateNotFound => "Sjabloon niet gevonden.",
        TextKey::Templates => "Sjablonen",
        TextKey::Theme => "Thema",
        TextKey::ThisActionCannotBeUndone => "Deze actie kan niet ongedaan worden gemaakt.",
        TextKey::ThisCollectionHasNoItemsYet => "Deze collectie bevat nog geen items.",
        TextKey::Thumbnail => "miniatuur",
        TextKey::Title => "Titel",
        TextKey::ToggleNavigation => "Navigatie omschakelen",
        TextKey::TotalDurationHelp => "totale duur van de inhoud, in seconden",
        TextKey::TotalPointsHelp => "totaal aantal mogelijke mid-roll-punten",
        TextKey::TotalProgressHelp => "genormaliseerde positie van 0 tot 1",
        TextKey::AddonMediaList => "Add-on-lijst",
        TextKey::AddonMediaListActionFailed => "Actie voor add-on-lijst mislukt.",
        TextKey::AddonMediaListAdded => "add-on-lijst toegevoegd.",
        TextKey::AddonMediaListDeleted => "add-on-lijst verwijderd.",
        TextKey::AddonMediaListMatched => "add-on-lijst gekoppeld.",
        TextKey::AddonMediaListNotFound => "add-on-lijst niet gevonden.",
        TextKey::AddonMediaListSaveFailed => "Opslaan van add-on-lijst mislukt.",
        TextKey::AddonMediaListSaved => "add-on-lijst opgeslagen.",
        TextKey::AddonMediaLists => "Add-on-lijsten",
        TextKey::AddonToolMaintenance => "Toolonderhoud",
        TextKey::AddonToolMaintenanceNone => "Nog geen onderhoudspoging.",
        TextKey::AddonToolUpdatePermission => "Native updates van ingestelde programma’s toestaan",
        TextKey::AddonToolMaintenanceQueued => "In wachtrij",
        TextKey::AddonToolMaintenanceRunning => "Bezig",
        TextKey::AddonToolMaintenanceWaiting => "Wacht op nieuwe poging",
        TextKey::AddonToolUpdated => "Bijgewerkt",
        TextKey::AddonToolAlreadyCurrent => "Al actueel",
        TextKey::AddonToolManualUpdate => "Handmatige update nodig",
        TextKey::AddonToolManualHelp => "Werk het ingestelde programma handmatig bij via de oorspronkelijke installatiemethode en controleer daarna de add-on.",
        TextKey::AddonToolFailedHelp => "De programma-update is mislukt. Controleer de beschikbaarheid en netwerktoegang; automatisch onderhoud probeert het later opnieuw.",
        TextKey::AddonToolUpdateFailedStale => "Het programma kon niet worden bijgewerkt en er is een nieuwere versie beschikbaar. Automatisch onderhoud probeert het later opnieuw. Werk het programma desgewenst nu bij via de oorspronkelijke installatiemethode en controleer daarna de add-on.",
        TextKey::AddonToolUpdateVersions => "{program}: geïnstalleerd {installed}, nieuwste {latest}.",
        TextKey::AddonToolVersionHelp => "De programmaversie kon niet worden gelezen. Controleer het ingestelde programma en de rechten.",
        TextKey::AddonToolUnavailableHelp => "Het ingestelde programma is niet beschikbaar. Controleer de programma-instelling en installatie.",
        TextKey::AddonToolBusyHelp => "Wacht tot het programma vrij is; actieve weergave wordt niet onderbroken.",
        TextKey::AddonToolCleanupHelp => "Stop de server, scanner en alle bijbehorende provider- en programmaprocessen. Voer vanuit de serverinstallatiemap de offline herstelopdracht voor dit programma uit met expliciete bevestiging; vervang <absolute-executable-path> door het ingestelde absolute pad. Gebruik op Windows ersatzrs-web.exe. Geef bij een aangepast profiel ook --state-dir met de absolute profielmap op. Start pas daarna opnieuw. Dit heft de procesveiligheidsblokkade op; het repareert het programma niet.",
        TextKey::AddonListDiskWarnings => "Waarschuwingen voor schijfgebruik",
        TextKey::AddonListDiskWarningGb => "Deze lijst: waarschuwen vanaf (GB)",
        TextKey::AddonListsCombinedDiskWarningGb => "Alle add-on-lijsten samen: waarschuwen vanaf (GB)",
        TextKey::AddonListDiskWarningHelp => "Hele decimale GB (1 GB = 1.000.000.000 bytes). 0 schakelt alleen deze waarschuwing uit. Bij het bereiken of overschrijden van de grens verschijnt alleen een Gezondheidswaarschuwing; downloads stoppen niet en bestanden worden niet verwijderd.",
        TextKey::AddonListDiskWarningFailed => "De schijfwaarschuwing kon niet worden geladen of opgeslagen. Laad de pagina opnieuw en probeer het nogmaals.",
        TextKey::ScanWorkerTitle => "Scanservice",
        TextKey::ScanWorkerUnavailableBrief => "De scanservice is niet beschikbaar",
        TextKey::ScanWorkerUnavailableHelp => "De scanservice is niet beschikbaar. Nieuwe scans kunnen niet starten. Controleer de logboeken en bevestig dat eventuele resterende lokale FFprobe-processen zijn gestopt voordat u ErsatzRS opnieuw start.",
        TextKey::PlaybackDataAccessTitle => "Toegang tot afspeelgegevens",
        TextKey::LegacyMpegTsCapacityBrief => "Legacy MPEG-TS vereist minstens twee actieve transcodeslots",
        TextKey::LegacyMpegTsCapacityHelp => "Een ingeschakeld kanaal gebruikt de native legacy MPEG-TS-modus. De wrapper en programma-encoder vereisen twee afzonderlijke actieve transcodeslots. Verhoog de limiet voor actieve transcodes naar minstens 2 of kies een andere streamingmodus.",
        TextKey::TeletextChannelTitle => "Teletekst per kanaal",
        TextKey::TeletextChannelBaseHelp => "Deze pagina wijzigt de opgeslagen basisinstellingen. Actieve Lua-overrides behouden hun eigen looptijd. Paginavalidatie en publicatie zijn aparte stappen; opslaan maakt pagina’s niet live en activeert geen pakketten.",
        TextKey::TeletextChannelEmpty => "Er zijn geen teletekst-addons of bewaarde selecties beschikbaar. Beheer de geïnstalleerde addons om een profiel te maken.",
        TextKey::TeletextChannelProfiles => "Profielen",
        TextKey::TeletextProfileCreate => "Profiel maken",
        TextKey::TeletextProfileSave => "Profiel opslaan",
        TextKey::TeletextProfileName => "Profielnaam",
        TextKey::TeletextProfileSharedHelp => "Profielen kunnen gedeeld worden. Een profielwijziging maakt afhankelijke kanaalselecties ongeldig; gebruik aparte profielen voor onafhankelijke kanaalinstellingen.",
        TextKey::TeletextBindingProfile => "Geselecteerd profiel",
        TextKey::TeletextBindingSave => "Kanaalselectie opslaan",
        TextKey::TeletextBindingOff => "Teletekst uitschakelen",
        TextKey::TeletextBindingNone => "Geen profiel geselecteerd",
        TextKey::TeletextProfileInheritHelp => "Overnemen gebruikt de installatie-instelling of addonstandaard. Kies Aanpassen voor een profielwaarde; een lege waarde is iets anders dan overnemen.",
        TextKey::TeletextProfileOverride => "Aanpassen",
        TextKey::TeletextProfileValue => "Aangepaste waarde",
        TextKey::TeletextProfileReference => "Locatie van geheim",
        TextKey::TeletextProfileConfigured => "Configuratie geldig",
        TextKey::TeletextProfileMissingInstallation => "Pakket is niet geïnstalleerd",
        TextKey::TeletextProfileDisabledInstallation => "Pakket is uitgeschakeld",
        TextKey::TeletextProfileIncompatibleInstallation => "Pakket is niet compatibel",
        TextKey::TeletextProfileInvalidSettings => "Profielinstellingen zijn ongeldig",
        TextKey::TeletextProfileDeleted => "Profiel is verwijderd",
        TextKey::TeletextProfileStaleInstallation => "Pakket gewijzigd; controleer dit profiel en sla het opnieuw op",
        TextKey::TeletextAdminConflict => "Deze configuratie is gewijzigd. Controleer de huidige waarden voordat je opnieuw opslaat.",
        TextKey::TeletextAdminInvalid => "Deze instellingen konden niet worden opgeslagen. Controleer het profiel, de selectie en de veldwaarden.",
        TextKey::TeletextAdminUnavailable => "De teletekstconfiguratie is niet beschikbaar. Controleer de applicatie en database en probeer opnieuw.",
        TextKey::TeletextAdminSaved => "Basisconfiguratie opgeslagen. Paginavalidatie en publicatie blijven aparte stappen.",
        TextKey::TeletextAdminWrongAddon => "Het profiel hoort niet bij deze addon. Controleer de selectie.",
        TextKey::TeletextAdminLimit => "Deze instellingen overschrijden de ondersteunde configuratielimieten. Verklein de profielwaarden of selecties.",
        TextKey::TeletextProfileRedacted => "Een opgeslagen geheime waarde kan hier niet worden getoond. Neem een geldige instelling over of vervang deze door een locatie van een geheim.",
        TextKey::TeletextProfileRepairHelp => "Dit profiel bevat instellingen die niet veilig met de huidige addon kunnen worden bewerkt. Maak een nieuw profiel met gecontroleerde waarden en selecteer het expliciet. De bestaande kanaalselectie kan nog steeds worden uitgeschakeld.",
        TextKey::TeletextEffectiveAuthoringTitle => "Paginaontwerpen voor de actieve Scripted-selectie",
        TextKey::TeletextEffectiveAuthoringHelp => "Valideer de profielen die het huidige script kiest en sla een statisch concept op. Er wordt geen tv-gids aangeleverd. Opslaan publiceert geen pagina’s.",
        TextKey::TeletextEffectiveAuthoringUnavailable => "Een actieve Scripted-selectie en geldige ontwerpinstellingen zijn vereist.",
        TextKey::TeletextEffectiveAuthoringAbsent => "Het huidige script zet alle teletext uit. Sla een concept zonder teletext op, zonder pagina-adres.",
        TextKey::TeletextEffectiveScopeExpiry => "Deze Scripted-selectie is geldig tot:",
        TextKey::TeletextPreviewAbsent => "Dit concept selecteert geen teletextpagina’s. Er is geen pagina om weer te geven.",
        TextKey::TeletextEffectiveEditorUnavailable => "Exporteren en importeren voor Scripted-concepten zijn nog niet aangesloten. Het opgeslagen concept blijft beschikbaar voor preview en gecontroleerde publicatie.",
        TextKey::TeletextAuthoringTitle => "Paginaontwerpen",
        TextKey::TeletextAuthoringHelp => "Valideer de geselecteerde profielen en sla een nieuw concept op. Dit verandert de uitzending niet.",
        TextKey::TeletextAuthoringStaticHelp => "Statische pagina’s: in deze stap wordt geen tv-gids aangeleverd.",
        TextKey::TeletextAuthoringUnavailable => "Conceptvalidatie vereist geldige ontwerpinstellingen en een ingeschakelde profielselectie.",
        TextKey::TeletextDesignName => "Conceptnaam",
        TextKey::TeletextEntryProducer => "Add-on voor de openingspagina",
        TextKey::TeletextEntryPage => "Openingspagina (100–899)",
        TextKey::TeletextEntrySubpage => "Openingssubpagina (hex)",
        TextKey::TeletextV1Region => "Tekenregio voor Level 1",
        TextKey::TeletextV1WesternLatin => "Westers Latijn",
        TextKey::TeletextResourceLifecycle => "Updaterecords voor resources (JSON)",
        TextKey::TeletextResourceLifecycleHelp => "Geef voor elke resourcepagina een expliciet updaterecord op. [] is geldig als er geen resources zijn.",
        TextKey::TeletextValidateSaveDraft => "Valideren en concept opslaan",
        TextKey::TeletextDraftSaved => "Concept opgeslagen. Preview en publicatie zijn afzonderlijke stappen.",
        TextKey::TeletextEditorTitle => "Bewerken met QTeletextMaker",
        TextKey::TeletextEditorHelp => "Download een ZIP, pak deze uit en bewerk de TTI-pagina’s in QTeletextMaker. De export bevat de oorspronkelijke metadata en benodigde bronbestanden.",
        TextKey::TeletextEditorOwnershipHelp => "Alleen vrije inhoud en gedefinieerde custom zones mogen wijzigen. Gegenereerde gidsinhoud, pagina-identiteiten en brondeclaraties zijn beschermd. Niet-ondersteunde wijzigingen worden afgewezen.",
        TextKey::TeletextEditorTemplateHelp => "Wijzigingen aan addontemplates vereisen een expliciete update van de addonversie; een concept importeren wijzigt geen geïnstalleerde pakketten.",
        TextKey::TeletextEditorImportHelp => "Selecteer de ongewijzigde .ersatzrs.json-metadata en alle .tti- en .dvb-bestanden uit dezelfde export, ook ongewijzigde bestanden. Behoud de oorspronkelijke bestandsnamen.",
        TextKey::TeletextEditorNoPublicationHelp => "Import valideert en bewaart een ongepubliceerd concept voor preview. Publicatie aanvragen en bevestigen blijven afzonderlijke acties.",
        TextKey::TeletextEditorUnavailable => "Export en import voor de editor zijn niet beschikbaar omdat de actuele ontwerpstatus niet kon worden gecontroleerd. Herlaad de preview.",
        TextKey::TeletextEditorExportPage => "Deze pagina en subpagina downloaden",
        TextKey::TeletextEditorExportSet => "Volledige paginaset downloaden",
        TextKey::TeletextEditorSidecar => "Exportmetadata (.ersatzrs.json)",
        TextKey::TeletextEditorFiles => "Pagina- en bronbestanden (.tti, .dvb)",
        TextKey::TeletextEditorImport => "Valideren en importeren als concept",
        TextKey::TeletextObjectCopyTitle => "Objectdefinitie kopiëren",
        TextKey::TeletextObjectCopyHelp => "Maak een ongepubliceerde kopie in dezelfde resource. Bewerk een nieuwe export in QTeletextMaker en publiceer apart.",
        TextKey::TeletextObjectCopySource => "Bronobject",
        TextKey::TeletextObjectCopyNumber => "Ongebruikt objectnummer",
        TextKey::TeletextObjectCopyAction => "Conceptkopie maken",
        TextKey::TeletextObjectCopyCurrentHelp => "Open de huidige conceptversie om een object te kopiëren.",
        TextKey::TeletextObjectCopyInvalid => "Kies een bestaand bronobject en een ongebruikt objectnummer in dezelfde resource.",
        TextKey::TeletextEditorLimit => "De editorbestanden overschrijden de ingestelde grootte- of verwerkingslimieten. Verklein de ondersteunde paginaset of controleer de ingestelde limieten. De uitzending blijft ongewijzigd.",
        TextKey::TeletextEditorMismatch => "Import afgewezen: bestanden of metadata komen niet overeen met de oorspronkelijke export of het actuele ontwerp. Herlaad de preview en gebruik bestanden uit dezelfde export.",
        TextKey::TeletextEditorProtected => "Import afgewezen: een beschermd of niet-ondersteund paginadeel is gewijzigd. Bewerk alleen vrije inhoud of gedefinieerde custom zones.",
        TextKey::TeletextEditorNoChanges => "De import bevat geen ondersteunde paginawijzigingen. Bewerk vrije inhoud of een custom zone voordat je importeert.",
        TextKey::TeletextEditorSyntax => "Import afgewezen: een TTI-pagina is ongeldig of gebruikt niet-ondersteunde syntaxis. Controleer de aangegeven pagina en positie.",
        TextKey::TeletextEditorMissingReceipt => "Het oorspronkelijke exportbewijs is niet beschikbaar. Exporteer het ontwerp opnieuw voordat je importeert.",
        TextKey::TeletextPublicationTitle => "Dit opgeslagen ontwerp publiceren",
        TextKey::TeletextPublicationHelp => "Vraag publicatie aan voor deze opgeslagen revisie en publiceer daarna de gecontroleerde aanvraag. Een concept opslaan of bekijken publiceert het niet.",
        TextKey::TeletextPublicationUnrequested => "Voor deze opgeslagen revisie staat geen publicatieaanvraag open. De bestaande geselecteerde pagina’s blijven behouden.",
        TextKey::TeletextPublicationPending => "Publicatie aangevraagd voor deze opgeslagen revisie. Deze is nog niet geselecteerd voor streams.",
        TextKey::TeletextPublicationSelected => "Deze opgeslagen revisie is geselecteerd. Dit bevestigt niet dat een stream haar heeft toegepast; elke geschikte uitvoer controleert wijzigingen vóór toepassing.",
        TextKey::TeletextPublicationUnavailable => "Publicatie is niet beschikbaar voor deze opgeslagen revisie. Controleer de huidige instellingen en validatie voordat je opnieuw publicatie aanvraagt.",
        TextKey::TeletextPublicationRequest => "Publicatie aanvragen",
        TextKey::TeletextPublicationPublish => "Aangevraagde pagina’s publiceren",
        TextKey::TeletextPublicationConflict => "Het ontwerp, de selectie of de publicatiestatus is gewijzigd. Herlaad deze exacte revisie en controleer deze voordat je opnieuw probeert.",
        TextKey::TeletextPublicationRequestSaved => "Publicatieaanvraag opgeslagen. De voor streams geselecteerde pagina’s zijn niet gewijzigd.",
        TextKey::TeletextPublicationSelectedSaved => "De aangevraagde pagina’s zijn geselecteerd. Daadwerkelijke toepassing wordt per geschikte uitvoer afzonderlijk gecontroleerd.",
        TextKey::TeletextPublicationReplacePending => "Deze revisie aanvragen vervangt de openstaande aanvraag voor een ander ontwerp. De pagina’s die momenteel voor streams zijn geselecteerd, blijven behouden.",
        TextKey::TeletextPreviewTitle => "Conceptpreview",
        TextKey::TeletextPreviewHelp => "Deze preview gebruikt het gevalideerde model. De ontvangerkop, video en onopgeloste vlakken worden afzonderlijk gemarkeerd.",
        TextKey::TeletextPreviewPage => "Pagina",
        TextKey::TeletextPreviewSubpage => "Subpagina",
        TextKey::TeletextPreviewLoad => "Preview tonen",
        TextKey::TeletextPreviewDiagnostics => "Validatie- en weergaveproblemen",
        TextKey::TeletextPreviewNoDiagnostics => "Geen onopgeloste weergaveproblemen voor deze pagina.",
        TextKey::TeletextPreviewReceiverHeader => "Ontvangerkop",
        TextKey::TeletextPreviewVideo => "Video",
        TextKey::TeletextPreviewUnresolved => "Onopgelost",
        TextKey::TeletextDraftMissing => "Het gevraagde concept of de gevraagde versie is niet beschikbaar voor dit kanaal.",
        TextKey::TeletextWorkBusy => "Teletekstverwerking is bezig. Probeer het opnieuw.",
        TextKey::TeletextWorkExpired => "De oorspronkelijke tijdslimiet is verstreken. Een concept dat al is opgeslagen, blijft bewaard.",
        TextKey::TeletextAuthoringRejected => "De geselecteerde pagina-invoer kon niet worden gevalideerd. De lopende uitzending blijft ongewijzigd.",
        TextKey::TeletextPreviewPosition => "Positie",
        TextKey::TeletextPreviewSource => "Oorspronkelijke bron",
        TextKey::TeletextPreviewReload => "Opgeslagen concept opnieuw openen",
        TextKey::TeletextPreviewEditorialTitle => "Bewerkingsgrenzen",
        TextKey::TeletextPreviewEditorialHelp => "Deze grenzen komen uit dit opgeslagen origineel. Cellen buiten de opgegeven gebieden blijven beschermd; het importeren van een concept wijzigt het sjabloon niet.",
        TextKey::TeletextPreviewRegion => "Gebied",
        TextKey::TeletextPreviewArea => "Paginagebied",
        TextKey::TeletextPreviewExtent => "Rijen en kolommen (geteld vanaf nul)",
        TextKey::TeletextPreviewPermission => "Bewerking",
        TextKey::TeletextPreviewFree => "Vrije inhoud",
        TextKey::TeletextPreviewZone => "Invulbaar gebied",
        TextKey::TeletextPreviewGenerated => "Automatisch gegenereerd",
        TextKey::TeletextPreviewDynamicField => "Dynamisch veld",
        TextKey::TeletextPreviewBinding => "Koppeling",
        TextKey::TeletextPreviewNoEditorialMetadata => "Voor deze opgeslagen pagina zijn geen bewerkingsgebieden opgegeven.",
        TextKey::TeletextPreviewPanelLeft => "Linkerpaneel",
        TextKey::TeletextPreviewPanelMain => "Hoofdpagina",
        TextKey::TeletextPreviewPanelRight => "Rechterpaneel",
        TextKey::TeletextNavIndex => "Index",
        TextKey::TeletextNavRed => "Rood",
        TextKey::TeletextNavGreen => "Groen",
        TextKey::TeletextNavYellow => "Geel",
        TextKey::TeletextNavCyan => "Cyaan",
        TextKey::TeletextAuthoringHealthTitle => "Configuratie van teletekstontwerpen",
        TextKey::TeletextAuthoringHealthAbsentBrief => "Teletekstontwerpen zijn niet geconfigureerd",
        TextKey::TeletextAuthoringHealthAbsentDetail => "Er is geen beleid voor teletekstontwerpen geconfigureerd. Conceptvoorbereiding en voorvertoning zijn niet beschikbaar; gewone weergave wordt niet beïnvloed.",
        TextKey::TeletextAuthoringHealthValidBrief => "Instellingen voor teletekstontwerpen zijn geldig",
        TextKey::TeletextAuthoringHealthValidDetail => "De instellingen voor teletekstontwerpen zijn geldig. Elk concept en elke voorvertoning vereisen nog begrensde validatie. Deze controle verifieert geen ontworpen pagina’s, publicatie, volledige Level 2.5-ondersteuning of geheugenlimieten.",
        TextKey::TeletextAuthoringHealthInvalidBrief => "Instellingen voor teletekstontwerpen zijn ongeldig",
        TextKey::TeletextAuthoringHealthInvalidDetail => "De instellingen voor teletekstontwerpen zijn ongeldig. Conceptvoorbereiding en voorvertoning zijn niet beschikbaar; gewone weergave wordt niet beïnvloed. Corrigeer de instellingen voor teletekstontwerpen.",
        TextKey::TeletextAuthoringHealthUnreadableBrief => "Instellingen voor teletekstontwerpen konden niet worden gelezen",
        TextKey::TeletextAuthoringHealthUnreadableDetail => "De instellingen voor teletekstontwerpen konden niet worden gelezen. Conceptvoorbereiding en voorvertoning zijn niet beschikbaar; gewone weergave wordt niet beïnvloed. Controleer de beschikbaarheid en toegankelijkheid van de database.",
        TextKey::TeletextWorkTitle => "Instellingen voor teleteksttaken",
        TextKey::TeletextWorkDefault => "Teleteksttaken worden standaard één voor één uitgevoerd. Dit is geen geheugenlimiet en bevestigt niet dat zelfgemaakte pagina’s klaar zijn voor uitzending.",
        TextKey::TeletextWorkConfigured => "De ingestelde limiet is {count} gelijktijdige teleteksttaken. Dit is geen geheugenlimiet en bevestigt niet dat zelfgemaakte pagina’s klaar zijn voor uitzending.",
        TextKey::TeletextWorkInvalid => "Het aantal gelijktijdige teleteksttaken moet een positief geheel getal zijn. Teleteksttaken kunnen pas worden uitgevoerd nadat deze instelling is hersteld.",
        TextKey::TeletextOutputTitle => "Instellingen voor teletekstuitvoer",
        TextKey::TeletextOutputAbsentBrief => "Teletekstuitvoer is niet ingesteld",
        TextKey::TeletextOutputAbsentHelp => "Er is geen beleid voor teletekstuitvoer ingesteld. Kanalen blijven normaal afspelen zonder toegevoegde teletekst.",
        TextKey::TeletextOutputConfiguredBrief => "De instellingen voor teletekstuitvoer zijn geldig",
        TextKey::TeletextOutputConfiguredHelp => "De instellingen voor teletekstuitvoer zijn geldig. Bij het afspelen worden per kanaal nog steeds de geaccepteerde pagina’s en de geschiktheid van de stream gecontroleerd. Deze controle bewijst geen teletekstoverdracht, volledige Level 2.5-ondersteuning of geheugenlimieten.",
        TextKey::TeletextOutputInvalidBrief => "De instellingen voor teletekstuitvoer zijn ongeldig",
        TextKey::TeletextOutputInvalidHelp => "De instellingen voor teletekstuitvoer zijn ongeldig. Normaal afspelen gaat door zonder toegevoegde teletekst. Herstel de instellingen om teletekstuitvoer in te schakelen.",
        TextKey::TeletextOutputUnreadableBrief => "De instellingen voor teletekstuitvoer konden niet worden gelezen",
        TextKey::TeletextOutputUnreadableHelp => "De instellingen voor teletekstuitvoer konden niet worden gelezen. Normaal afspelen gaat door zonder toegevoegde teletekst. Controleer de beschikbaarheid en toegang tot de database.",
        TextKey::TeletextDecoderTitle => "Teletekstdecodering",
        TextKey::TeletextDecoderReady => "De geselecteerde FFmpeg-runtime heeft de controles voor teletekstpaginaselectie, ondertiteltekst en kleurenbitmapdecodering doorstaan.",
        TextKey::TeletextDecoderMissing => "Teletekstdecodering is niet beschikbaar. Selecteer een FFmpeg 9-runtime met libzvbi-teletekstondersteuning.",
        TextKey::TeletextDecoderOptions => "De teletekstdecoder mist vereiste opties voor paginaselectie of weergave. Selecteer een compatibele FFmpeg 9-runtime.",
        TextKey::TeletextDecoderHelpFailed => "De teletekstdecodercontrole kon niet worden voltooid. Controleer de beschikbaarheid en toegangsrechten van de FFmpeg-runtime.",
        TextKey::TeletextDecoderTextFailed => "Het decoderen van teletekstpagina’s en ondertiteltekst is mislukt. Selecteer een compatibele FFmpeg 9-runtime.",
        TextKey::TeletextDecoderBitmapFailed => "Het decoderen van teletekstkleurenbitmaps is mislukt. Selecteer een compatibele FFmpeg 9-runtime.",
        TextKey::NextRuntimeAvailability => "Beschikbaarheid van Next-runtime",
        TextKey::NextOverlayInvalid => "Next-configuratieoverlay is niet beschikbaar of ongeldig",
        TextKey::NextOverlaySchema => "Next-configuratieoverlays vereisen een ondersteunde versie-URI (0.1.0 of 0.1.1). Controleer de kopieerinstellingen voordat je een oudere overlay bijwerkt.",
        TextKey::NextOverlayIncomplete => "Validatie van de Next-configuratieoverlay is onvolledig",
        TextKey::JavascriptAudioSelectionTitle => "JavaScript-audioselectie",
        TextKey::JavascriptAudioSelectionHealthy => "Sinds het opstarten zijn geen selectorfouten geregistreerd. Nog niet uitgevoerde scripts zijn niet gevalideerd.",
        TextKey::JavascriptAudioSelectionFailures => "{count} selectorfouten sinds het opstarten; de ingebouwde audioselectie is gebruikt. Controleer scripts/audio-stream-selector en de limieten van ADR 0096. De teller wordt bij een herstart gewist.",
        TextKey::AddonListDiskUsageTitle => "Schijfgebruik van add-on-lijsten",
        TextKey::AddonListDiskUsageBrief => "Waarschuwing voor schijfgebruik van add-on-lijsten",
        TextKey::AddonListDiskUsageUnavailable => "Het schijfgebruik van add-on-lijsten kon niet volledig worden gemeten. Controleer de toegang tot de opslag en voer de gezondheidscontrole opnieuw uit.",
        TextKey::AddonListDiskUsageUnavailableBrief => "Het schijfgebruik van add-on-lijsten kon niet worden gemeten.",
        TextKey::AddonListDiskUsageLine => "Lijst #{id}: {used} GB gebruikt; waarschuwing vanaf {limit} GB.",
        TextKey::AddonListsCombinedDiskUsageLine => "Samen: {used} GB gebruikt; waarschuwing vanaf {limit} GB.",
        TextKey::AddonMediaListUrl => "URL van add-on-lijst",
        TextKey::AddonMediaListFolder => "Opslagmap",
        TextKey::AddonMediaListFolderHelp => "Optioneel; anders afgeleid van de lijstnaam",
        TextKey::DestructiveActions => "Destructieve acties",
        TextKey::ReconcileEntireSource => "Volledige bron vergelijken",
        TextKey::ReconcileEntireSourceHelp => {
            "Vergelijk een volledige momentopname van de bron zonder de huidige publicatie of iteminstellingen te wijzigen."
        }
        TextKey::ReconcileQueued => "Volledige bronvergelijking staat in de wachtrij.",
        TextKey::ReconcileFailed => "Volledige bronvergelijking is mislukt.",
        TextKey::ResetAndReload => "Opnieuw instellen en laden",
        TextKey::ResetAddonMediaListHelp => {
            "Verwijder alle gegenereerde resultaten voor deze lijst en zet daarna een volledige import in de wachtrij."
        }
        TextKey::SourceHost => "Bronhost",
        TextKey::ResetRemoves => "Deze reset verwijdert",
        TextKey::ResetRemovesItems => {
            "Geïmporteerde items, providermetadata, aanpassingen van de beheerder, afleveringsnummering en vervangingen."
        }
        TextKey::ResetRemovesArtifacts => {
            "Gegenereerde media, NFO-bestanden, illustraties, ondertitels, caches en archieven."
        }
        TextKey::ResetRemovesRuntime => {
            "Taken, controlepunten, revisieartefacten, items in de gegenereerde afspeellijst en geprojecteerde media."
        }
        TextKey::ResetRetains => "Deze reset behoudt",
        TextKey::ResetRetainsSettings => {
            "De lijstnaam, koppelingen met add-on, bron en bibliotheek, identiteit van de gegenereerde afspeellijst, vernieuwings- en publicatiebeleid, artefactquotum en overige beheerdersinstellingen."
        }
        TextKey::ResetReloadFailureWarning => {
            "Als het opnieuw laden mislukt, blijven kanalen en de gegenereerde afspeellijst leeg."
        }
        TextKey::TypeListNameToConfirm => "Typ de huidige lijstnaam om te bevestigen",
        TextKey::ResetQueued => "Opnieuw instellen en laden staat in de wachtrij.",
        TextKey::ResetAddonMediaListFailed => "Opnieuw instellen en laden is mislukt.",
        TextKey::ChapterInput => "Hoofdstukken",
        TextKey::ChapterInputHelp => "Optioneel, één per regel: 0:00 Introductie",
        TextKey::RestoreProviderChapters => "Hoofdstukken van provider herstellen",
        TextKey::ManagedByAddon => "Beheerd door een add-on",
        TextKey::Trash => "Prullenbak",
        TextKey::TrashAlreadyEmpty => "De prullenbak is al leeg.",
        TextKey::TrashEmpty => "De prullenbak is leeg.",
        TextKey::TrashEmptyConfirm => {
            "Alle items waarvan het bestand ontbreekt uit de database verwijderen?"
        }
        TextKey::TrashEmptyConfirmHelp => {
            "Dit verwijdert de momenteel ontbrekende media-items en hun afhankelijke metadatarijen permanent."
        }
        TextKey::TrashSubtitle => {
            "Items waarvan de bronbestanden momenteel ontbreken op schijf of bij de provider."
        }
        TextKey::Troubleshooting => "Probleemoplossing",
        TextKey::TunerCount => "Aantal tuners",
        TextKey::HdhrDisabledByJwt => "HDHomeRun-eindpunten zijn uitgeschakeld omdat JWT-beveiliging voor IPTV is ingeschakeld. Deze instellingen hebben geen effect.",
        TextKey::TvShows => "Series",
        TextKey::Ui => "Interface",
        TextKey::UnavailableLocalMediaMessage => "Er zijn {count} items niet beschikbaar omdat ErsatzRS ze niet op schijf kan vinden, waaronder: {items}",
        TextKey::UnavailableRemoteStreamsMessage => "Er zijn {count} Remote Streams niet beschikbaar omdat de provider meldt dat ze niet kunnen worden afgespeeld, waaronder: {items}",
        TextKey::Unavailable => "Niet beschikbaar",
        TextKey::UseActualTimes => "Werkelijke tijden gebruiken",
        TextKey::UseCustomPlaybackOrder => "Aangepaste afspeelvolgorde gebruiken",
        TextKey::UseChaptersAsMediaItems => "Hoofdstukken als media-items gebruiken",
        TextKey::UseChaptersAsMediaItemsHelp => {
            "Plan afzonderlijke hoofdstukken in plaats van volledige bestanden"
        }
        TextKey::UseEmbeddedSubtitles => "Ingesloten ondertitels gebruiken",
        TextKey::Uuid => "UUID",
        TextKey::UntitledItem => "Item zonder titel",
        TextKey::WindowsStartup => "Opstarten met Windows",
        TextKey::WindowsStartupNeedsRepair => {
            "De opgeslagen opstartopdracht verwijst naar een andere locatie. Pas deze instelling toe om dit te herstellen."
        }
        TextKey::WindowsStartupUnavailable => {
            "Opstarten met Windows is niet beschikbaar omdat de meegeleverde launcher niet is gevonden of de registratie niet kon worden gelezen."
        }
        TextKey::ActiveTranscodeLimit => "Maximaal gelijktijdige streamingtranscodes",
        TextKey::ActiveTranscodeLimitHelp => {
            "Harde limiet voor FFmpeg-streamingtranscodes. Wanneer alle slots bezet zijn, krijgt een nieuwe kijker een melding dat alle streams in gebruik zijn totdat er een slot vrijkomt; actieve streams omzeilen de limiet niet."
        }
        TextKey::ActiveTranscodeSuggestionHardware => {
            "Hardware-encoder gevonden; dit is een voorgestelde waarde, geen automatische wijziging."
        }
        TextKey::ActiveTranscodeSuggestionSoftware => {
            "Alleen software-encoding; dit is een voorgestelde waarde, geen automatische wijziging."
        }
        TextKey::Peak => "piek",
        TextKey::StreamsInUse => "Streams in gebruik",
        TextKey::SuggestedTranscodeLimit => "Voorgestelde waarde",
        TextKey::Watermarks => "Zenderlogo's",
        TextKey::Width => "Breedte",
        TextKey::WorkAheadSegmenterLimit => "Vooruitwerkende HLS-transcodes",
        TextKey::WorkAheadSegmenterLimitHelp => {
            "Hoeveel HLS-transcodeslots vooruit mogen werken op maximale snelheid om buffer op te bouwen. Vooruitwerken wijkt altijd voor bekeken streams."
        }
        TextKey::AccelerationKind => "Acceleratietype",
        TextKey::AddFfmpegProfile => "FFmpeg-profiel toevoegen",
        TextKey::AddProfile => "Profiel toevoegen",
        TextKey::AllowBFrames => "B-frames toestaan",
        TextKey::Audio => "Audio",
        TextKey::AudioBitrate => "Audiobitrate (kBit/s)",
        TextKey::AudioBufferSize => "Audiobuffergrootte (kBit)",
        TextKey::AudioChannels => "Kanalen",
        TextKey::AudioFormat => "Audioformaat",
        TextKey::BitDepth => "Bitdiepte",
        TextKey::Copy => "Kopiëren",
        TextKey::CopyBlock => "Blok kopiëren",
        TextKey::CopyFailed => "Kopiëren mislukt",
        TextKey::CopyNameSuffix => "Kopie",
        TextKey::CopyProfile => "Profiel kopiëren",
        TextKey::CopyTemplate => "Sjabloon kopiëren",
        TextKey::Crop => "Bijsnijden",
        TextKey::AllowWatermarks => "Zenderlogo's toestaan",
        TextKey::DeinterlaceVideo => "Video de-interlacen",
        TextKey::Edit => "Bewerken",
        TextKey::EditBlock => "Blok bewerken",
        TextKey::EditCollection => "Collectie bewerken",
        TextKey::EditFfmpegProfile => "FFmpeg-profiel bewerken",
        TextKey::EditMultiCollection => "Multi-collectie bewerken",
        TextKey::EditRerunCollection => "Herhalingscollectie bewerken",
        TextKey::EditTemplate => "Sjabloon bewerken",
        TextKey::Expression => "Expressie",
        TextKey::Fallback => "Fallback",
        TextKey::FillerKind => "Opvultype",
        TextKey::General => "Algemeen",
        TextKey::HardwareAcceleration => "Hardwareacceleratie",
        TextKey::HardwareIfPossible => "Hardware indien mogelijk",
        TextKey::ProfileDeleteDefault => "Het standaard-FFmpeg-profiel kan niet worden verwijderd.",
        TextKey::ProfileDeleteInUse => "Dit FFmpeg-profiel wordt door een kanaal gebruikt en kan niet worden verwijderd.",
        TextKey::ProfileDeleteFailed => "Het FFmpeg-profiel kon niet worden verwijderd.",
        TextKey::NewProfileName => "Nieuwe profielnaam",
        TextKey::NoneOption => "(Geen)",
        TextKey::NumHelp => "het mid-roll-puntnummer, beginnend bij 1",
        TextKey::OptionalDisplayTitle => "Optionele weergavetitel...",
        TextKey::TranscodeAudio => "Audio transcoderen",
        TextKey::NormalizeColors => "Kleuren normaliseren",
        TextKey::NormalizeFramerate => "Framerate normaliseren",
        TextKey::NormalizeLoudness => "Loudness normaliseren",
        TextKey::TranscodeVideo => "Video transcoderen",
        TextKey::Off => "Uit",
        TextKey::Pad => "Opvullen",
        TextKey::PadNearestFifteenMinutes => "15 (:00, :15, :30, :45)",
        TextKey::PadNearestFiveMinutes => "5 (:00, :05, :10, :15, :20 enz.)",
        TextKey::PadNearestTenMinutes => "10 (:00, :10, :20, :30, :40, :50)",
        TextKey::PadNearestThirtyMinutes => "30 (:00, :30)",
        TextKey::PadMode => "Opvulmodus",
        TextKey::PadToNearestMinute => "Opvullen tot dichtstbijzijnde minuut",
        TextKey::PointHelp => "de positie van het mid-roll-punt, in seconden",
        TextKey::ProfileCopied => "Profiel gekopieerd.",
        TextKey::ProfileNotFound => "Profiel niet gevonden.",
        TextKey::QsvExtraHardwareFrames => "Extra QSV-hardwareframes",
        TextKey::Resolution => "Resolutie",
        TextKey::SampleRateKhz => "Samplefrequentie (kHz)",
        TextKey::SaveDecoTemplate => "Decoratiesjabloon opslaan",
        TextKey::SaveProfile => "Profiel opslaan",
        TextKey::SaveFillerPreset => "Opvulvoorinstelling opslaan",
        TextKey::SaveRerunCollection => "Herhalingscollectie opslaan",
        TextKey::SaveSequentialSchedule => "Sequentieel schema opslaan",
        TextKey::SaveTemplate => "Sjabloon opslaan",
        TextKey::SaveBlock => "Blok opslaan",
        TextKey::ScaleAndPad => "Schalen en opvullen",
        TextKey::ScalingBehavior => "Schaalgedrag",
        TextKey::Software => "Software",
        TextKey::Stretch => "Uitrekken",
        TextKey::TargetLoudnessLufs => "Doelloudness (LUFS)",
        TextKey::ThreadCountAuto => "Aantal threads (0 = auto)",
        TextKey::NativeEngineOnlyHelp => "Alleen gebruikt door de native engine van ErsatzRS. ErsatzTV Next negeert deze instelling.",
        TextKey::OutputDynamicRange => "Dynamisch bereik van uitvoer",
        TextKey::OutputDynamicRangeCopyHint => {
            "Video wordt gekopieerd, dus de uitvoer behoudt het dynamisch bereik van de bron."
        }
        TextKey::HdrStaticMetadataHint => {
            "Laat leeg om de gedocumenteerde HDR10-standaardwaarden te gebruiken. Ze gelden alleen wanneer de bron zelf geen HDR-metadata bevat; het masteringscherm en de lichtniveaus van de bron winnen altijd."
        }
        TextKey::HdrMasteringDisplay => "Masteringscherm",
        TextKey::HdrMaxCll => "MaxCLL (cd/m²)",
        TextKey::HdrMaxFall => "MaxFALL (cd/m²)",
        TextKey::TonemapAlgorithm => "Tonemap-algoritme",
        TextKey::VaapiDevicePath => "VAAPI-apparaatpad",
        TextKey::VaapiDisplay => "VAAPI-display",
        TextKey::VaapiDriver => "VAAPI-driver",
        TextKey::Video => "Video",
        TextKey::VideoBitrate => "Videobitrate (kBit/s)",
        TextKey::VideoBufferSize => "Videobuffergrootte (kBit)",
        TextKey::VideoFormat => "Videoformaat",
        TextKey::VideoPreset => "Videopreset",
        TextKey::VideoTune => "Video-afstemming",
        TextKey::VideoProfile => "Videoprofiel",
        TextKey::ViewCollection => "Collectie bekijken",
        TextKey::ViewPlaylist => "Afspeellijst bekijken",
        TextKey::AddAll => "Alles toevoegen",
        TextKey::AddAllToCollection => "Alles toevoegen aan collectie",
        TextKey::AddAllToPlaylist => "Alles toevoegen aan afspeellijst",
        TextKey::AddedItemToPlaylist => "Item toegevoegd aan afspeellijst.",
        TextKey::AddedArtistToSchedule => "Artiest toegevoegd aan schema",
        TextKey::AddedMovieToSchedule => "Film toegevoegd aan schema",
        TextKey::AddedSeasonToSchedule => "Seizoen toegevoegd aan schema",
        TextKey::AddedShowToSchedule => "Serie toegevoegd aan schema",
        TextKey::AddingItem => "Item toevoegen...",
        TextKey::AddingItemToPlaylist => "Item toevoegen aan afspeellijst...",
        TextKey::Artists => "Artiesten",
        TextKey::ClearSelection => "Selectie wissen",
        TextKey::Collection => "Collectie",
        TextKey::CollectionNameRequired => "Collectienaam is verplicht",
        TextKey::CollectionNotFound => "Collectie niet gevonden",
        TextKey::CollectionRenamed => "Collectie hernoemd.",
        TextKey::EditAddonMediaListProperties => "Eigenschappen van add-on-lijst bewerken",
        TextKey::EnterAddonMediaListUrl => "Voer URL van add-on-lijst in",
        TextKey::EnterSearchQuery => "Voer een zoekopdracht in.",
        TextKey::Episodes => "Afleveringen",
        TextKey::LoadingCollections => "Collecties laden...",
        TextKey::LoadingPlaylistGroups => "Afspeellijstgroepen laden...",
        TextKey::LoadingPlaylists => "Afspeellijsten laden...",
        TextKey::GeneratePlaylist => "Afspeellijst in oorspronkelijke volgorde maken",
        TextKey::GeneratePlaylistFromSortedAddonMediaListItems => {
            "Behoud na elke publicatie de oorspronkelijke volgorde van de provider"
        }
        TextKey::MusicVideos => "Muziekvideo's",
        TextKey::NewCollectionName => "Nieuwe collectienaam",
        TextKey::NoMediaMatched => "Geen media gevonden.",
        TextKey::NoItemsToAdd => "Geen items om toe te voegen.",
        TextKey::NoItemsToPreview => "Geen items voor voorbeeld.",
        TextKey::NoPlaylistsInGroup => "Geen afspeellijsten in deze groep.",
        TextKey::PleaseChoosePlaylist => "Kies een afspeellijst.",
        TextKey::RequestFailed => "Aanvraag mislukt",
        TextKey::Released => "Uitgebracht",
        TextKey::SaveAs => "Opslaan als",
        TextKey::Seasons => "Seizoenen",
        TextKey::SeeAll => "Alles weergeven >>",
        TextKey::SelectAll => "Alles selecteren",
        TextKey::SelectGroup => "(Selecteer een groep)",
        TextKey::SelectPlaylist => "(Selecteer een afspeellijst)",
        TextKey::SelectSearchCollectionHelp => {
            "Selecteer de collectie waaraan de zoekresultaten worden toegevoegd."
        }
        TextKey::SelectSearchPlaylistHelp => {
            "Selecteer de afspeellijst waaraan de zoekresultaten worden toegevoegd."
        }
        TextKey::Selected => "Geselecteerd",
        TextKey::SelectCollectionToAddThisMovie => {
            "Selecteer een collectie om deze film toe te voegen."
        }
        TextKey::SelectPlaylistToAddThisMovie => {
            "Selecteer een afspeellijst om deze film toe te voegen."
        }
        TextKey::Add => "Toevoegen",
        TextKey::AddLibrary => "Bibliotheek toevoegen",
        TextKey::Address => "Adres",
        TextKey::AddressRequired => "Adres is verplicht.",
        TextKey::ApiKey => "API-sleutel",
        TextKey::ApiKeyRequired => "API-sleutel is verplicht.",
        TextKey::AuthorizedRedirecting => "Geautoriseerd - doorsturen...",
        TextKey::CheckingEveryTwoSeconds => "Elke 2 seconden controleren...",
        TextKey::ClickAuthorizePlex => "Klik hier om op Plex.tv te autoriseren",
        TextKey::ClickPlayoutItemHelp => {
            "Klik op een uitzenditem om titel, tijden en planningscontext te bekijken."
        }
        TextKey::ConfigureConnection => "Verbinding configureren",
        TextKey::Connection => "Verbinding",
        TextKey::DeepScanCollections => "Collecties diep scannen",
        TextKey::DeepScanLibrary => "Bibliotheek diep scannen",
        TextKey::DeepScanConfirmationQuestion => "Wil je {name} echt diep scannen?",
        TextKey::DeepScanConfirmationWarning => "Een diepe scan vernieuwt alles, ook items die ongewijzigd lijken, en kan veel langer duren dan een normale scan.",
        TextKey::LibraryScanJavascriptRequired => "JavaScript is vereist om bibliotheken en externe collecties te scannen. Schakel JavaScript in om een diepe scan te bekijken en te bevestigen.",
        TextKey::DeepScan => "Diepe scan",
        TextKey::EditLibrary => "Bibliotheek bewerken",
        TextKey::EditPlaylist => "Afspeellijst bewerken",
        TextKey::EnterOnePathPerLine => "Voer een pad per regel in.",
        TextKey::Error => "Fout",
        TextKey::ExternalCollections => "Externe collecties",
        TextKey::Information => "Informatie",
        TextKey::Kind => "Soort",
        TextKey::Library => "Bibliotheek",
        TextKey::LibraryKind => "Bibliotheeksoort",
        TextKey::LibraryPaths => "Bibliotheekpaden",
        TextKey::FolderPreview => "Tv-mappen bekijken",
        TextKey::FolderPreviewHelp => "Voor tv-series: bekijk maximaal acht absolute mappaden voordat je opslaat. De preview leest maximaal 10.000 items, 32 mapniveaus en twee seconden aan bestandssysteemwerk; bestanden en bibliotheekinstellingen blijven ongewijzigd.",
        TextKey::FolderPreviewCandidates => "Dit zijn gevonden kandidaten, geen gedecodeerde media. Per map worden maximaal 40 diagnosevoorbeelden getoond. Verwachte indeling: bibliotheek/serie/seizoen/bestanden; pakketmappen worden niet automatisch doorzocht.",
        TextKey::FolderPreviewPartial => "Onvolledige preview: een limiet is bereikt. De aantallen dekken niet de hele map.",
        TextKey::FolderPreviewUnavailable => "Preview niet beschikbaar. Kies tv-series en voer één tot acht absolute paden in (maximaal 4.096 bytes per pad). Controleer of de mappen bereikbaar zijn en probeer opnieuw; een andere preview kan nog bezig zijn.",
        TextKey::FolderIgnoredExtension => "Genegeerde bestandsextensie",
        TextKey::FolderUnsupportedNesting => "Niet-ondersteunde nesting of verkeerd geplaatst videobestand",
        TextKey::FolderEmptyShow => "Series zonder gevonden media",
        TextKey::FolderUnreadable => "Onleesbaar item",
        TextKey::FolderRepeated => "Herhaalde map of kringverwijzing",
        TextKey::FolderDepthLimit => "Limiet voor mapdiepte",
        TextKey::FolderInspected => "Bekeken items",
        TextKey::FolderExtras => "Gevonden extra video’s",
        TextKey::FolderHidden => "Verborgen items",
        TextKey::LocalLibraryPathAbsolute => "Pad [{path}] moet een absoluut pad zijn.",
        TextKey::LocalLibraryPathExists => "Pad [{path}] moet een bestaande map zijn.",
        TextKey::LocalLibraryPathConflict => "Pad [{path}] overlapt een pad van een andere bibliotheek.",
        TextKey::LocalLibraries => "Lokale bibliotheken",
        TextKey::LocalPath => "Lokaal pad",
        TextKey::MediaKind => "Mediasoort",
        TextKey::NoLibrariesConfigured => {
            "Geen bibliotheken geconfigureerd. Voeg eerst een mediabron toe."
        }
        TextKey::NoSchedulingContext => "Geen planningscontext",
        TextKey::NoPlayoutItemsInWindow => "Geen uitzenditems in dit 24-uursvenster",
        TextKey::NoPlayoutsFound => "Geen uitzendingen gevonden.",
        TextKey::NoPathReplacements => "Geen padvervangingen.",
        TextKey::Paths => "Paden",
        TextKey::PathReplacements => "Padvervangingen",
        TextKey::PlexSignInHelp => "Meld u aan bij Plex om een mediabron toe te voegen.",
        TextKey::Remote => "Extern",
        TextKey::RemotePath => "Extern pad",
        TextKey::Remove => "Verwijderen",
        TextKey::RemoveBlockFromTemplate => "Blok uit sjabloon verwijderen",
        TextKey::RemoveContent => "Inhoud verwijderen",
        TextKey::Retry => "Opnieuw proberen",
        TextKey::Saved => "Opgeslagen.",
        TextKey::Saving => "Opslaan...",
        TextKey::QuickScan => "Snelle scan",
        TextKey::ScanCollections => "Collecties scannen",
        TextKey::ScanFailed => "Scan mislukt",
        TextKey::ScanLibrary => "Bibliotheek scannen",
        TextKey::ScanQueued => "Scan in wachtrij geplaatst",
        TextKey::Scanning => "Scannen...",
        TextKey::Server => "Server",
        TextKey::ServerAddress => "Serveradres",
        TextKey::PlexRefreshFailed => "Plex-bibliotheken konden niet worden vernieuwd. Controleer de serververbinding en probeer het opnieuw.",
        TextKey::ReauthenticatePlex => "Opnieuw aanmelden bij Plex",
        TextKey::PlexMissingServer => "Server ontbreekt bij Plex",
        TextKey::PlexMissingHelp => "Deze server staat niet meer in je Plex-account. Claim hem opnieuw in Plex of verwijder hem hier. Inhoud blijft behouden en scans zijn gepauzeerd.",
        TextKey::PlexSignedOutHelp => "Meld je opnieuw aan om Plex-toegang te herstellen. Je mediabronnen en gesynchroniseerde inhoud blijven behouden.",
        TextKey::PlexRemoveServerHelp => "Als je deze server verwijdert, worden zijn bibliotheken en gesynchroniseerde inhoud uit ErsatzRS verwijderd. Bestanden op de Plex-server worden niet verwijderd.",
        TextKey::PlexSignInFailed => "Aanmelden bij Plex is mislukt of verlopen. Probeer het opnieuw.",
        TextKey::SignInToPlex => "Aanmelden bij Plex",
        TextKey::SignOutOfPlex => "Afmelden bij Plex",
        TextKey::Sync => "Synchroniseren",
        TextKey::WaitingForPlexAuthorization => "Wachten op Plex-autorisatie.",
        TextKey::AddChannel => "Kanaal toevoegen",
        TextKey::Any => "Elke",
        TextKey::BlockPlayoutTroubleshooting => "Probleemoplossing voor blokuitzending",
        TextKey::BlockPlayoutHistory => "Geschiedenis van blokuitzending",
        TextKey::Categories => "Categorieën",
        TextKey::CategoriesHelp => "Komma-gescheiden lijst met categorieën",
        TextKey::Channel => "Kanaal",
        TextKey::ChannelCouldNotBeSaved => {
            "Kanaal kon niet worden opgeslagen. Controleer de formulierwaarden en probeer het opnieuw."
        }
        TextKey::ChannelDisabled => "Kanaal uitgeschakeld",
        TextKey::ChannelHiddenFromEpg => "Kanaal verborgen in EPG",
        TextKey::ChannelLogo => "Kanaallogo",
        TextKey::ChannelLogoPreview => "Voorbeeld van kanaallogo",
        TextKey::ChannelLogoUploadFailed => {
            "Uploaden van kanaallogo mislukt. Controleer het bestand en probeer het opnieuw."
        }
        TextKey::Check => "Controle",
        TextKey::Continuous => "Continu",
        TextKey::DateAndTime => "Datum en tijd",
        TextKey::DefaultOption => "Standaard",
        TextKey::DefaultSchedule => "Standaardschema",
        TextKey::DeleteChannel => "Kanaal verwijderen",
        TextKey::DownloadMediaSample => "Mediavoorbeeld downloaden",
        TextKey::DownloadResults => "Resultaten downloaden",
        TextKey::DisableWatermarks => "Zenderlogo's uitschakelen",
        TextKey::EditChannel => "Kanaal bewerken",
        TextKey::EditChannelNumbers => "Kanaalnummers bewerken",
        TextKey::EditNumbers => "Nummers bewerken",
        TextKey::ExternalLogoUrl => "Externe logo-URL",
        TextKey::Fail => "Mislukt",
        TextKey::FallbackFiller => "Fallback-opvulling",
        TextKey::FfmpegProfile => "FFmpeg-profiel",
        TextKey::FirstPage => "Eerste pagina",
        TextKey::FirstRunPlaybackOrder => "Afspeelvolgorde voor eerste uitzending",
        TextKey::Forced => "Geforceerd",
        TextKey::GenerateSubtitles => "Ondertitels genereren",
        TextKey::Generated => "Gegenereerd",
        TextKey::Details => "Details",
        TextKey::Full => "Volledig",
        TextKey::Group => "Groep",
        TextKey::FfmpegRuntime => "FFmpeg-runtime",
        TextKey::FfmpegRuntimeDownloaded => "FFmpeg-runtime gedownload",
        TextKey::FfmpegRuntimeLatestRelease => "Nieuwste release",
        TextKey::FfmpegRuntimeRelease => "Geinstalleerde release",
        TextKey::FfmpegRuntimeSource => "Bron",
        TextKey::FfmpegRuntimeSourceDownloaded => "Gedownload",
        TextKey::FfmpegRuntimeSourceManaged => "Beheerd",
        TextKey::FfmpegRuntimeSourceOverride => "Override",
        TextKey::FfmpegRuntimeSourcePath => "PATH",
        TextKey::FfmpegRuntimeSourceUnknown => "Onbekend",
        TextKey::FfmpegRuntimeUpdateAvailable => "Update beschikbaar",
        TextKey::FfmpegRuntimeUpdateCheckFailed => "Updatecontrole mislukt",
        TextKey::FfmpegRuntimeUpdateFailed => "FFmpeg-runtime-update mislukt",
        TextKey::FfmpegRuntimeUpToDate => "Actueel",
        TextKey::FfmpegRuntimeUpdated => "FFmpeg-runtime bijgewerkt",
        TextKey::FfmpegRuntimeVersion => "Versie",
        TextKey::HealthNoPersistedRun => "Er zijn nu geen toepasselijke gezondheidscontroles.",
        TextKey::HealthPersistedRunHelp => {
            "Resultaten worden vijf minuten bewaard. Kies Vernieuwen om direct opnieuw te controleren. Deze pagina wordt ook vernieuwd wanneer mediascans de itemstatus bijwerken."
        }
        TextKey::HistoryDetails => "Geschiedenisdetails",
        TextKey::HlsDirect => "HLS direct",
        TextKey::HlsSegmenter => "HLS-segmenter",
        TextKey::Hours => "uur",
        TextKey::IdleBehavior => "Gedrag bij inactiviteit",
        TextKey::IdleBehaviorHelp => {
            "Bepaalt hoe transcodering zich gedraagt nadat alle clients zijn losgekoppeld"
        }
        TextKey::Info => "Info",
        TextKey::Import => "Import",
        TextKey::IsEnabled => "Ingeschakeld",
        TextKey::Json => "JSON",
        TextKey::KeepRunning => "Actief houden",
        TextKey::Key => "Sleutel",
        TextKey::LastMidFillerHelp => "seconden sinds de laatste mid-roll filler",
        TextKey::LastPage => "Laatste pagina",
        TextKey::Level => "Niveau",
        TextKey::LoadingMediaInfo => "Media-info laden...",
        TextKey::Logs => "Logboek",
        TextKey::PlaybackSpeed => "Snelheid",
        TextKey::Logo => "Logo",
        TextKey::MediaItem => "Media-item",
        TextKey::Message => "Bericht",
        TextKey::MissingMediaItemQueryParameter => "Queryparameter mediaItem ontbreekt",
        TextKey::Mirror => "Spiegelen",
        TextKey::MirrorOptionsHelp => {
            "Alleen beschikbaar wanneer Uitzendbron is ingesteld op Spiegelen."
        }
        TextKey::MirrorSourceChannel => "Bronkanaal voor spiegeling",
        TextKey::MusicVideoCreditsMode => "Aftitelingmodus voor muziekvideo's",
        TextKey::MusicVideoCreditsTemplate => "Aftitelingssjabloon voor muziekvideo's",
        TextKey::NoMatchingRecordsFound => "Geen overeenkomende records gevonden",
        TextKey::NoFfmpegProfiles => "Geen FFmpeg-profielen gevonden. Maak er eerst een aan.",
        TextKey::NoBlockPlayoutSelected => {
            "Selecteer een blokuitzending om de blokken te bekijken."
        }
        TextKey::NoBlocks => "Geen blokken gevonden.",
        TextKey::NoHistory => "Geen geschiedenis gevonden.",
        TextKey::NoMessages => "Geen berichten.",
        TextKey::NvidiaCapabilitiesNotCollected => "NVIDIA-mogelijkheden zijn nog niet verzameld.",
        TextKey::Number => "Nummer",
        TextKey::OnDemand => "Op aanvraag",
        TextKey::MatchedPointsHelp => "aantal mid-roll-punten dat al overeenkwam met de expressie",
        TextKey::Pass => "Geslaagd",
        TextKey::ParityDebt => "ParityDebt",
        TextKey::PlaybackTroubleshooting => "Afspeelprobleemoplossing",
        TextKey::Play => "Afspelen",
        TextKey::PlayoutTimeline => "Uitzendtijdlijn",
        TextKey::PlayoutMode => "Uitzendmodus",
        TextKey::PlayoutModeHelp => "Bepaalt de voortgang van de uitzending van het kanaal",
        TextKey::PlayoutOffset => "Uitzendverschuiving",
        TextKey::PlayoutSource => "Uitzendbron",
        TextKey::PlayoutSourceHelp => {
            "Bepaalt de bron van de kanaalinhoud. Dit kan niet worden gewijzigd wanneer het kanaal al een uitzending heeft."
        }
        TextKey::Preview => "Voorbeeld",
        TextKey::PreviewBlockPlayout => "Voorbeeld van blokuitzending",
        TextKey::PreviewChannel => "Kanaalvoorbeeld",
        TextKey::ChannelPreview => "Kanaalvoorbeeld",
        TextKey::ReloadStream => "Stream herladen",
        TextKey::StopPreview => "Voorbeeld stoppen",
        TextKey::OpenFullPlayer => "Volledige speler openen",
        TextKey::BackToChannels => "Terug naar kanalen",
        TextKey::NowPlaying => "Nu bezig",
        TextKey::UpNext => "Hierna",
        TextKey::ProgrammeGuide => "Programmagids",
        TextKey::EpgPreview => "EPG-voorbeeld",
        TextKey::EpgPreviewDescription => {
            "Bekijk de gepubliceerde gids met twee uur historie en 24 uur vooruit."
        }
        TextKey::EpgZoom => "Zoom",
        TextKey::EpgNow => "Nu",
        TextKey::NoEpgChannels => "Er zijn geen ingeschakelde kanalen zichtbaar in de EPG.",
        TextKey::ScheduleGap => "Gat in planning",
        TextKey::Filler => "Opvulling",
        TextKey::NoProgramming => "Er is geen programmering in dit tijdvenster.",
        TextKey::ChannelDisabledPreview => "Dit kanaal is uitgeschakeld.",
        TextKey::ChannelNotAiring => "Dit kanaal zendt momenteel niet uit.",
        TextKey::ChannelNoPlayout => "Dit kanaal heeft geen uitzending.",
        TextKey::PreviewIdle => "Het voorbeeld start automatisch zonder geluid.",
        TextKey::PreviewConnecting => "Verbinding maken met de livestream…",
        TextKey::PreviewErrorSlate => "Foutbeeld: het programma kan niet worden afgespeeld.",
        TextKey::PreviewOutputUnknown => "De uitvoerstatus van de server is niet beschikbaar.",
        TextKey::PreviewLive => "De livestream wordt afgespeeld.",
        TextKey::PreviewPaused => "Voorbeeld gepauzeerd.",
        TextKey::PreviewRecovering => "Afspelen wordt hersteld…",
        TextKey::PreviewStopped => "Voorbeeld gestopt.",
        TextKey::PreviewUnsupported => "Deze browser kan geen HLS-video afspelen.",
        TextKey::PreviewAuthorizationFailed => "Autorisatie voor het voorbeeld is mislukt.",
        TextKey::PreviewQueued => "Voorbeeld staat in de wachtrij",
        TextKey::PreviewRunning => "Voorbeeld wordt gerenderd",
        TextKey::PreviewCompleted => "Renderen voltooid",
        TextKey::PreviewCancelling => "Voorbeeld stoppen; wachten op opruimen",
        TextKey::PreviewProgrammeSeek => "Seconden vanaf het programmabegin",
        TextKey::PreviewProgrammeSeekHelp => "Laat leeg om bij het gekozen UTC-tijdstip te beginnen. Zoeken blijft binnen het gekozen programma.",
        TextKey::PreviewTimestamp => "Voorbeeldtijdstip (UTC):",
        TextKey::PreviewAlreadyRunning => "Er loopt al een diagnostisch voorbeeld. Wacht tot dit klaar is voordat je een nieuw voorbeeld start.",
        TextKey::PreviewCleanupUnconfirmed => "Het opruimen van het voorbeeld kon niet worden bevestigd. Herstart de server voordat je het opnieuw probeert.",
        TextKey::PreviewConfigurationInvalid => "De streamingmodus van het kanaal is ongeldig. Sla een ondersteunde modus op in de kanaaleditor en probeer opnieuw.",
        TextKey::PreviewStartupFailed => "Het voorbeeld kon niet starten. Controleer het serverlog op de opstartfout en controleer de media en het FFmpeg-profiel.",
        TextKey::PreviewUnavailable => "De livestream is niet beschikbaar.",
        TextKey::PreviewCapacityReached => "Alle transcodeerplaatsen zijn momenteel bezet.",
        TextKey::PreviewNetworkError => "De netwerkverbinding van het voorbeeld is verbroken.",
        TextKey::PreviewMediaError => {
            "De browser kan de ingestelde uitvoer van dit kanaal niet decoderen."
        }
        TextKey::PreviewDerivativeNote => {
            "Het beheervoorbeeld gebruikt een browsercompatibele H.264/AAC-afgeleide; de ingestelde kanaaluitvoer blijft ongewijzigd."
        }
        TextKey::JavascriptRequiredPlayback => "JavaScript is vereist voor interactief afspelen.",
        TextKey::ConfirmStopTranscodeSession => {
            "De gedeelde transcodeersessie voor dit kanaal stoppen?"
        }
        TextKey::PreviewPlaylistPlayout => "Voorbeeld van afspeellijstuitzending",
        TextKey::PreferredAudioTitle => "Voorkeurstitel voor audio",
        TextKey::PreferredSubtitleLanguage => "Voorkeurstaal voor ondertitels",
        TextKey::QsvCapabilitiesNotCollected => "QSV-mogelijkheden zijn nog niet verzameld.",
        TextKey::RemoveStoredLogo => "Opgeslagen logo verwijderen",
        TextKey::SaveChannel => "Kanaal opslaan",
        TextKey::SaveChannels => "Kanalen opslaan",
        TextKey::Second => "seconde",
        TextKey::Seconds => "seconden",
        TextKey::SelectProfile => "- Selecteer profiel -",
        TextKey::SequentialSchedule => "Sequentieel schema",
        TextKey::SequentialScheduleValidation => "Validatie van sequentieel schema",
        TextKey::SeekSeconds => "Zoekseconden",
        TextKey::Schema => "Schema",
        TextKey::ShowDisabled => "Uitgeschakelde tonen",
        TextKey::ShowInEpg => "Weergeven in EPG",
        TextKey::SlugSeconds => "Tussenpauze in seconden",
        TextKey::SlugSecondsHelp => "Seconden zwarte video en stille audio tussen elk uitzenditem",
        TextKey::Song => "Nummer",
        TextKey::SongVideoMode => "Nummervideomodus",
        TextKey::SortById => "Sorteren op ID",
        TextKey::SortByLevel => "Sorteren op niveau",
        TextKey::SortByName => "Sorteren op naam",
        TextKey::SortByTimestamp => "Sorteren op tijdstip",
        TextKey::StopOnDisconnect => "Stoppen bij verbreken",
        TextKey::StopScheduling => "Planning stoppen",
        TextKey::StopSchedulingBlockItems => "Planning van blokitems stoppen",
        TextKey::StopTranscodeSession => "Transcodeersessie stoppen",
        TextKey::StartFromBeginning => "Vanaf begin starten",
        TextKey::StreamSelector => "Streamselector",
        TextKey::StreamSelectorMode => "Streamselectormodus",
        TextKey::InvalidNextEngineTextSubtitleMode => "Ongeldige tekstondertitelmodus voor Next-engine",
        TextKey::NextEngineTextSubtitleModeHelp => "Geconverteerde tekstondertitels zijn alleen beschikbaar via HLS. Kies Inbranden voor ondertitels in MPEG-TS.",
        TextKey::NextEngineTextSubtitleMode => "Tekstondertitelmodus voor Next-engine",
        TextKey::SubtitleBurn => "Inbranden",
        TextKey::SubtitleConvertIfPossible => "Converteren indien mogelijk",
        TextKey::PreviewCaptions => "Ondertiteling",
        TextKey::StreamingEngine => "Streamingengine",
        TextKey::InvalidStreamingEngine => "Ongeldige streamingengine",
        TextKey::StreamingMode => "Streamingmodus",
        TextKey::Styles => "Stijlen",
        TextKey::Subtitle => "Ondertitel",
        TextKey::SubtitleMode => "Ondertitelmodus",
        TextKey::SubtitleModeHelp => {
            "Geforceerd toont alleen geforceerde tekstsporen (forced narrative). Kies Elke om \
             ondertitels in je voorkeurstaal altijd te tonen."
        }
        TextKey::Timestamp => "Tijdstip",
        TextKey::Timeline => "Tijdlijn",
        TextKey::Target => "Doel",
        TextKey::Tags => "Tags",
        TextKey::Tools => "Hulpmiddelen",
        TextKey::TranscodeMode => "Transcodeermodus",
        TextKey::TranscodeModeHelp => "Bepaalt wanneer het transcodeerproces actief is",
        TextKey::TroubleshootPlayback => "Afspeelproblemen oplossen",
        TextKey::TroubleshootChannel => "Kanaalprobleemoplossing",
        TextKey::TroubleshootingReports => "Probleemoplossingsrapporten",
        TextKey::MpegTsTroubleshooting => "MPEG-TS-probleemoplossing",
        TextKey::InternalMpegTs => "Interne MPEG-TS",
        TextKey::PublicMpegTs => "Publieke MPEG-TS",
        TextKey::RetainRawSample => "Ruw sample bewaren",
        TextKey::TsdDuckVersion => "TSDuck-versie",
        TextKey::Report => "Rapport",
        TextKey::RunId => "Run-ID",
        TextKey::Status => "Status",
        TextKey::Summary => "Samenvatting",
        TextKey::SampleBytes => "Samplebytes",
        TextKey::RawSampleArchived => "Ruw sample gearchiveerd",
        TextKey::ArchivePath => "Archiefpad",
        TextKey::SampleUrl => "Sample-URL",
        TextKey::StabilityMeter => "Stabiliteitsmeter",
        TextKey::Stable => "Stabiel",
        TextKey::Packets => "pakketten",
        TextKey::Services => "Services",
        TextKey::Pids => "PID's",
        TextKey::ContinuityFindings => "Continuiteitsbevindingen",
        TextKey::PcrFindings => "PCR-bevindingen",
        TextKey::Bitrate => "Bitrate",
        TextKey::AnalysisJson => "Analyse-JSON",
        TextKey::StandardOutput => "Standaarduitvoer",
        TextKey::StandardError => "Standaardfout",
        TextKey::NotAvailable => "Niet beschikbaar",
        TextKey::NotApplicable => "Niet van toepassing",
        TextKey::Completed => "Voltooid",
        TextKey::Failed => "Mislukt",
        TextKey::TroubleshootingPlaybackStartDebt => {
            "Afspeelvoorbeeld voor media-items en kanalen schrijft nu FFmpeg-rapporten, logs, media_info.json, troubleshooting_info.json, geselecteerde stream-selector-YAML en capability-bestanden, voert stream-selectorregels uit en verwijst door naar /iptv/session/.troubleshooting/live.m3u8."
        }
        TextKey::UnableToLoadMediaInfo => "Kan media-informatie niet laden",
        TextKey::UnableToLoadPlaybackTroubleshooting => {
            "Kan context voor afspeelprobleemoplossing niet laden"
        }
        TextKey::UploadLogo => "Logo uploaden",
        TextKey::UploadLogoHelp => {
            "Kies een lokale afbeelding om het opgeslagen kanaallogo bij opslaan te vervangen"
        }
        TextKey::Warning => "Waarschuwing",
        TextKey::Watermark => "Zenderlogo",
        TextKey::WatermarkIds => "Zenderlogo-ID's",
        TextKey::Window => "Venster",
        TextKey::WithProgress => "Met voortgang",
        TextKey::Writers => "Schrijvers",
        TextKey::Validate => "Valideren",
        TextKey::Yaml => "YAML",
        TextKey::YamlFile => "YAML-bestand",
        TextKey::Messages => "Berichten",
        TextKey::CollectionName => "Collectienaam",
        TextKey::MediaItemType => "Media-itemtype",
        TextKey::MediaItemTitle => "Media-itemtitel",
        TextKey::ActualSize => "Werkelijke grootte",
        TextKey::AddWatermark => "Zenderlogo toevoegen",
        TextKey::AspectRatioGuides => "Beeldverhoudingshulplijnen",
        TextKey::BottomLeft => "Linksonder",
        TextKey::BottomMiddle => "Middenonder",
        TextKey::BottomRight => "Rechtsonder",
        TextKey::CopyWatermark => "Zenderlogo kopiëren",
        TextKey::CopyOf => "Kopie van",
        TextKey::CurrentImage => "Huidige afbeelding",
        TextKey::Custom => "Aangepast",
        TextKey::ChannelLogoPreviewUnavailable => {
            "Voor een voorbeeld van het kanaallogo is een kanaalcontext nodig. Kies hier een aangepaste afbeelding."
        }
        TextKey::DeleteWatermark => "Zenderlogo verwijderen",
        TextKey::DurationSeconds => "Duur (seconden)",
        TextKey::EditWatermark => "Zenderlogo bewerken",
        TextKey::EnableWatermarkToPreview => {
            "Schakel het zenderlogo in om de plaatsing te bekijken."
        }
        TextKey::Frequency => "Frequentie",
        TextKey::GeneratingPreview => "Voorbeeld genereren...",
        TextKey::HorizontalMarginPercent => "Horizontale marge (%)",
        TextKey::GuideFourThree => "Veilig 4:3-kader",
        TextKey::Identity => "Identiteit",
        TextKey::Image => "Afbeelding",
        TextKey::ImageSource => "Afbeeldingsbron",
        TextKey::Intermittent => "Periodiek",
        TextKey::IntermittentTiming => "Periodieke timing",
        TextKey::LeftMiddle => "Midden links",
        TextKey::Location => "Locatie",
        TextKey::MiddleCenter => "Midden",
        TextKey::Mode => "Modus",
        TextKey::Minutes => "minuten",
        TextKey::Moods => "Stemmingen",
        TextKey::NewName => "Nieuwe naam",
        TextKey::NativeAssetDimensions => "Oorspronkelijke en weergegeven grootte",
        TextKey::NextWatermark => "Volgend zenderlogo",
        TextKey::Opacity => "Dekking",
        TextKey::OpacityExpression => "Dekkingsuitdrukking",
        TextKey::OpacityOneToHundred => "Dekking (1-100)",
        TextKey::Permanent => "Permanent",
        TextKey::PerPass => "per doorgang",
        TextKey::PlaceWithinSourceContent => "Binnen broninhoud plaatsen",
        TextKey::Placement => "Plaatsing",
        TextKey::PlacementPreview => "Plaatsingsvoorbeeld",
        TextKey::PreviewFailed => "Voorbeeld mislukt",
        TextKey::PreviewReferenceFullHd => "Full-HD-referentiekader · 1920×1080",
        TextKey::PreviousWatermark => "Vorig zenderlogo",
        TextKey::RightMiddle => "Midden rechts",
        TextKey::SaveWatermark => "Zenderlogo opslaan",
        TextKey::Scaled => "Geschaald",
        TextKey::SelectCustomImageToPreview => {
            "Selecteer of upload een aangepaste afbeelding voor het voorbeeld."
        }
        TextKey::Size => "Grootte",
        TextKey::TopLeft => "Linksboven",
        TextKey::TopMiddle => "Middenboven",
        TextKey::TopRight => "Rechtsboven",
        TextKey::UploadFailed => "Uploaden mislukt.",
        TextKey::UploadImage => "Afbeelding uploaden",
        TextKey::UnsavedChanges => "Niet-opgeslagen wijzigingen",
        TextKey::UnsavedWatermarkChangesMessage => {
            "Uw wijzigingen aan het zenderlogo weggooien en het geselecteerde zenderlogo openen?"
        }
        TextKey::VerticalMarginPercent => "Verticale marge (%)",
        TextKey::WatermarkCopied => "Zenderlogo gekopieerd.",
        TextKey::WatermarkNotFound => "Zenderlogo niet gevonden.",
        TextKey::WatermarkPlacementPreview => "voorbeeld van zenderlogoplaatsing",
        TextKey::WatermarkPreview => "zenderlogovoorbeeld",
        TextKey::WatermarkNavigation => "Navigatie tussen zenderlogo's",
        TextKey::WidthPercent => "Breedte (%)",
        TextKey::ZIndex => "Z-index",
        TextKey::DiscardChanges => "Wijzigingen weggooien",
        TextKey::XmltvBlockBehavior => "XMLTV-blokgedrag",
        TextKey::XmltvTimeZone => "XMLTV-tijdzone",
        TextKey::Xmltv => "XMLTV",
        TextKey::Badge => "Badge",
        TextKey::Buttons => "Knoppen",
        TextKey::CheckValue => "Vinkje",
        TextKey::ChannelName => "Kanaalnaam",
        TextKey::ChannelNumber => "Kanaalnummer",
        TextKey::ComponentGallery => "ErsatzRS-componentengalerij",
        TextKey::DangerButton => "Gevaar",
        TextKey::DeleteDialog => "Verwijderdialoog",
        TextKey::Decrement => "Verlagen",
        TextKey::Dialogs => "Dialogen",
        TextKey::DisabledButton => "Uitgeschakeld",
        TextKey::EnterName => "Voer een naam in",
        TextKey::ErrorOccurred => "Er is een fout opgetreden.",
        TextKey::ErrorToast => "Foutmelding",
        TextKey::ExampleModal => "Voorbeeldvenster",
        TextKey::ExampleModalBody => "Dit is een voorbeeldtekst voor een modaal venster.",
        TextKey::FormInputs => "Formuliervelden",
        TextKey::Icons => "Pictogrammen",
        TextKey::IncrementBadge => "Badge verhogen",
        TextKey::InfoToast => "Infomelding",
        TextKey::JustFyi => "Ter informatie.",
        TextKey::LinkButton => "Linkknop",
        TextKey::ModalCancelled => "Geannuleerd.",
        TextKey::ModalConfirmed => "Bevestigd!",
        TextKey::NumValue => "Getal",
        TextKey::OpenModal => "Venster openen",
        TextKey::OperationSucceeded => "Bewerking geslaagd.",
        TextKey::PagedTableDemo => "PagedTable (5 rijen per pagina, 7 totaal)",
        TextKey::PrimaryButton => "Primair",
        TextKey::SecondaryButton => "Secundair",
        TextKey::SelectValue => "Selectie",
        TextKey::SomethingMightBeWrong => "Er is mogelijk iets mis.",
        TextKey::SuccessToast => "Succesmelding",
        TextKey::TextButton => "Tekst",
        TextKey::TextValue => "Tekst",
        TextKey::Toasts => "Meldingen",
        TextKey::WarningToast => "Waarschuwing",
        TextKey::YamlScript => "YAML-script",
        TextKey::Yes => "Ja",
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
        assert_eq!(STRINGS.season_display_name(2), "Seizoen 2");
    }

    #[test]
    fn episode_display_subtitles() {
        assert_eq!(
            episode_display_subtitle_text(UiLanguage::Dutch, Some("Serie"), Some(1), Some(3))
                .as_deref(),
            Some("Serie - S01E03")
        );
        assert_eq!(
            episode_display_subtitle_text(UiLanguage::Dutch, Some("Serie"), None, None).as_deref(),
            Some("Serie - Ongenummerd")
        );
    }

    #[test]
    fn fallback_item_labels() {
        assert_eq!(STRINGS.schedule_item_fallback_label(7), "Schema-item #7");
    }

    #[test]
    fn log_level_labels() {
        assert_eq!(log_level_text(UiLanguage::Dutch, "Fatal"), "Fataal");
        assert_eq!(log_level_text(UiLanguage::Dutch, "TRACE"), "TRACE");
    }

    #[test]
    fn channel_labels() {
        assert_eq!(STRINGS.channel_language_label("nld"), "Nederlands");
        assert_eq!(STRINGS.channel_streaming_mode_label(99), "Onbekend (99)");
        assert_eq!(STRINGS.channel_language_label("tlh"), "tlh");
    }

    #[test]
    fn media_and_library_kind_labels() {
        assert_eq!(media_kind_text(UiLanguage::Dutch, 1), "Films");
        assert_eq!(media_kind_text(UiLanguage::Dutch, 99), "Onbekend");
        assert_eq!(library_kind_text(UiLanguage::Dutch, "Local"), "Lokaal");
    }

    #[test]
    fn playout_schedule_kind_labels() {
        assert_eq!(playout_schedule_kind_text(UiLanguage::Dutch, 1), "Klassiek");
        assert_eq!(
            playout_schedule_kind_text(UiLanguage::Dutch, 20),
            "JSON (dizqueTV)"
        );
        assert_eq!(playout_schedule_kind_text(UiLanguage::Dutch, 99), "");
    }

    #[test]
    fn hardware_acceleration_labels() {
        assert_eq!(
            hardware_acceleration_option_text(UiLanguage::Dutch, "NVENC", false),
            "NVENC (niet beschikbaar op dit systeem)"
        );
        assert_eq!(
            hardware_acceleration_option_text(UiLanguage::Dutch, "VAAPI", true),
            "VAAPI"
        );
    }

    #[test]
    fn ffmpeg_option_labels() {
        assert_eq!(
            ffmpeg_tonemap_algorithm_text(UiLanguage::Dutch, 99),
            "Onbekend (99)"
        );
        assert_eq!(ffmpeg_loudness_mode_text(UiLanguage::Dutch, 0), "Uit");
        assert_eq!(
            ffmpeg_profile_video_format_text(UiLanguage::Dutch, 42, 0),
            "Onbekend (42)"
        );
        assert_eq!(
            ffmpeg_profile_audio_format_text(UiLanguage::Dutch, 99),
            "Kopiëren"
        );
    }

    #[test]
    fn deco_template_action_labels() {
        assert_eq!(
            STRINGS.text(TextKey::AddDecoToTemplate),
            "Decoratie aan sjabloon toevoegen"
        );
    }

    #[test]
    fn server_errors_follow_language_without_changing_unknown_values() {
        assert_eq!(
            ffmpeg_profile_error_text(UiLanguage::Dutch, "Source profile not found."),
            "Bronprofiel niet gevonden."
        );
        assert_eq!(
            ffmpeg_profile_error_text(UiLanguage::Dutch, "custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            schedule_error_text(
                UiLanguage::Dutch,
                "Fixed schedule items require a start time."
            ),
            "Vaste schema-items vereisen een starttijd."
        );
        assert_eq!(
            schedule_error_text(UiLanguage::Dutch, "custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            scheduling_content_error_text(
                UiLanguage::Dutch,
                "[Collection] is required for collection type 'Collection'"
            ),
            "Collectie is verplicht voor collectietype Collectie."
        );
        assert_eq!(
            scheduling_content_error_text(UiLanguage::Dutch, "Template 12 does not exist."),
            "Sjabloon bestaat niet."
        );
        assert_eq!(
            scheduling_content_error_text(
                UiLanguage::Dutch,
                "Break content playlist item does not exist."
            ),
            "Afspeellijst voor onderbrekingsinhoud bestaat niet."
        );
        assert_eq!(
            scheduling_content_error_text(UiLanguage::Dutch, "custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            watermark_error_text(UiLanguage::Dutch, "Watermark 12 does not exist."),
            "Zenderlogo niet gevonden."
        );
        assert_eq!(
            watermark_error_text(UiLanguage::Dutch, "custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            settings_error_text(UiLanguage::Dutch, "ffmpeg path does not exist"),
            "ffmpeg-pad bestaat niet"
        );
        assert_eq!(
            settings_error_text(UiLanguage::Dutch, "custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            media_action_error_text(UiLanguage::Dutch, "Unable to locate media item"),
            "Kan media-item niet vinden."
        );
        assert_eq!(
            media_action_error_text(UiLanguage::Dutch, "custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            filler_error_text(UiLanguage::Dutch, "FillerPreset 42 does not exist."),
            "Fillerpreset bestaat niet."
        );
        assert_eq!(
            filler_error_text(UiLanguage::Dutch, "custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            media_browser_error_text(
                UiLanguage::Dutch,
                "[MultiCollection] is required for collection type 'MultiCollection'"
            ),
            "Multi-collectie is verplicht."
        );
        assert_eq!(
            media_browser_error_text(UiLanguage::Dutch, "[ProgramScheduleId] does not exist."),
            "Schema bestaat niet."
        );
        assert_eq!(
            media_browser_error_text(UiLanguage::Dutch, "custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            media_source_error_text(UiLanguage::Dutch, "'Address' must be a valid URL"),
            "Adres moet een geldige URL zijn."
        );
        assert_eq!(
            media_source_error_text(UiLanguage::Dutch, "custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            addon_media_list_error_text(
                UiLanguage::Dutch,
                "add-on media list with id 42 not found"
            ),
            "add-on-lijst bestaat niet."
        );
        assert_eq!(
            addon_media_list_error_text(UiLanguage::Dutch, "custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            addon_media_list_error_text(
                UiLanguage::Dutch,
                "error running server function: conflict: another list of this add-on already uses the folder teleac"
            ),
            "Een andere lijst van deze add-on gebruikt de map \"teleac\" al."
        );
        assert_eq!(
            upload_error_text(UiLanguage::Dutch, "Read field: connection reset"),
            "Veld lezen mislukt: connection reset"
        );
        assert_eq!(
            upload_error_text(UiLanguage::Dutch, "custom upload failure"),
            "custom upload failure"
        );
    }
}
