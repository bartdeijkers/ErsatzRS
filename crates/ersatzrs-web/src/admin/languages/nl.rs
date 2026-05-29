#![allow(clippy::match_same_arms, clippy::too_many_lines)]

use crate::admin::i18n::{LanguageStrings, TextKey};

pub struct DutchStrings;

pub static STRINGS: DutchStrings = DutchStrings;

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

    fn trakt_error_message(&self, error: &str) -> String {
        trakt_error_message(error)
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
        format!("Geen {provider}-bron geconfigureerd. Klik op Verbinding configureren om er een toe te voegen.")
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

    fn unable_to_load_playback_troubleshooting(&self, error: &dyn std::fmt::Display) -> String {
        format!("Kan context voor afspeelprobleemoplossing niet laden: {error}")
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
            "Multicollectie-items kunnen alleen Shuffle of Shuffle in volgorde gebruiken.".to_owned()
        }
        "Playlist Item Size can only be used with Playlist schedule items." => {
            "Playlistitemgrootte kan alleen worden gebruikt met schema-items van het type playlist.".to_owned()
        }
        "Multiple playout mode with Count requires a count expression." => {
            "Meervoudige playoutmodus met Aantal vereist een aantalexpressie.".to_owned()
        }
        "Duration playout mode requires a duration between 1 minute and 24 hours." => {
            "Duur-playoutmodus vereist een duur tussen 1 minuut en 24 uur.".to_owned()
        }
        "Duration playout mode requires discard-to-fill attempts." => {
            "Duur-playoutmodus vereist pogingen voor discard-to-fill.".to_owned()
        }
        "Tail Mode Filler requires a tail filler preset." => {
            "Tailmodus Filler vereist een tail-fillerpreset.".to_owned()
        }
        "Tail filler can only be set when Tail Mode is Filler." => {
            "Tail-filler kan alleen worden ingesteld wanneer Tailmodus Filler is.".to_owned()
        }
        "Collection is required." => "Collectie is verplicht.".to_owned(),
        "Media item is required." => "Media-item is verplicht.".to_owned(),
        "Multi Collection is required." => "Multicollectie is verplicht.".to_owned(),
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
        return "Watermerk bestaat niet.".to_owned();
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
            "Multicollectie is verplicht voor collectietype Multicollectie.".to_owned()
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
            "Multicollectie voor onderbrekingsinhoud moet een geldig item hebben.".to_owned()
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
            "Multicollectie-item voor onderbrekingsinhoud bestaat niet.".to_owned()
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
        return "Watermerk niet gevonden.".to_owned();
    }

    match trimmed {
        "Name is required" => "Naam is verplicht".to_owned(),
        "Name must be 50 characters or fewer" => "Naam mag maximaal 50 tekens bevatten".to_owned(),
        "Watermark image is required" | "Watermark image is required!" => {
            "Afbeelding voor watermerk is verplicht".to_owned()
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
            "Naam van watermerk moet uniek zijn".to_owned()
        }
        "Watermark not found" | "Source watermark not found" | "Watermark does not exist." => {
            "Watermerk niet gevonden.".to_owned()
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

    if let Some((current, limit)) = xmltv_days_error_parts(trimmed) {
        return format!(
            "Aantal XMLTV-dagen om te bouwen ({current}) mag niet groter zijn dan het aantal playout-dagen om te bouwen ({limit})"
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
        "Tuner count must be greater than zero" => "Aantal tuners moet groter zijn dan nul.".to_owned(),
        "Library refresh interval must be 0 (do not refresh) or greater than zero" => {
            "Bibliotheekverversingsinterval moet 0 (niet verversen) of groter dan nul zijn.".to_owned()
        }
        "Library refresh interval must be less than 1,000,000. Use 0 to disable automatic refresh" => {
            "Bibliotheekverversingsinterval moet kleiner zijn dan 1.000.000. Gebruik 0 om automatisch verversen uit te schakelen.".to_owned()
        }
        "Playout days to build must be greater than zero" => "Aantal playout-dagen om te bouwen moet groter zijn dan nul.".to_owned(),
        "Scripted schedule timeout seconds must be at least 10" => {
            "Time-out voor scripted schema moet minstens 10 seconden zijn.".to_owned()
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

fn trakt_error_message(error: &str) -> String {
    let trimmed = error.trim();
    let normalized = trimmed.strip_prefix("conflict: ").unwrap_or(trimmed);
    if let Some(error) = normalized.strip_prefix("failed to decode trakt http: ") {
        return format!("Kan Trakt niet bereiken: {error}");
    }
    if normalized.starts_with("trakt list with id ") && normalized.ends_with(" not found") {
        return "Trakt-lijst bestaat niet.".to_owned();
    }

    match normalized {
        "Invalid Trakt list url" => "Ongeldige Trakt-lijst-URL.".to_owned(),
        "Trakt ClientId is not configured; set TRAKT__CLIENTID" => {
            "Trakt ClientId is niet geconfigureerd; stel TRAKT__CLIENTID in.".to_owned()
        }
        "Trakt API client is not configured for this operation" => {
            "Trakt API-client is niet geconfigureerd voor deze bewerking.".to_owned()
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
        TextKey::AddTraktList => "Trakt-lijst toevoegen",
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
        },
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
        TextKey::Blocks => "Blokken",
        TextKey::BlockToRemove => "Te verwijderen blok",
        TextKey::BlockWillNotFit => "Het blok past niet op of na de gekozen starttijd.",
        TextKey::Cancel => "Annuleren",
        TextKey::Clear => "Wissen",
        TextKey::Channels => "Kanalen",
        TextKey::Close => "Sluiten",
        TextKey::Collections => "Collecties",
        TextKey::CollectionDeleted => "Collectie verwijderd.",
        TextKey::CollectionSaved => "Collectie opgeslagen.",
        TextKey::ComputingPreview => "Voorbeeld berekenen...",
        TextKey::Confirm => "Bevestigen",
        TextKey::ConfirmDelete => "Verwijderen bevestigen",
        TextKey::Content => "Inhoud",
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
        },
        TextKey::BreakContentItem => "Breakinhouditem",
        TextKey::BreakContentMode => "Breakinhoudmodus",
        TextKey::DeadAirFallback => "Dead-air fallback",
        TextKey::DeadAirFallbackBody => {
            "Wanneer er voor de huidige tijd geen playoutitem wordt gevonden, wordt willekeurig een item uit deze collectie gekozen, herhaald en precies passend ingekort tot de start van het volgende playoutitem. Dit vervangt de afbeelding Kanaal is offline die anders zou worden getoond."
        },
        TextKey::DeadAirFallbackCollectionType => "Collectietype voor dead-air fallback",
        TextKey::DeadAirFallbackMode => "Dead-air fallbackmodus",
        TextKey::Deco => "Decoratie",
        TextKey::DecoCannotBeAdded => {
            "Decoratie kan niet worden toegevoegd omdat deze zou overlappen of middernacht zou passeren."
        },
        TextKey::DecoCannotBeMoved => {
            "Decoratie kan daar niet naartoe worden verplaatst omdat deze zou overlappen of middernacht zou passeren."
        },
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
        },
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
        TextKey::UseWatermarkDuringFiller => "Watermerk gebruiken tijdens filler",
        TextKey::WatermarkMode => "Watermerkmodus",
        TextKey::DecoTemplates => "Decoratiesjablonen",
        TextKey::DefaultFfmpegProfile => "Standaard FFmpeg-profiel",
        TextKey::DefaultFfmpegProfileHelp => "Het FFmpeg-profiel dat wordt gebruikt bij het maken van nieuwe kanalen",
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
        TextKey::DeleteTraktList => "Trakt-lijst verwijderen",
        TextKey::DisabledCollectionsAlreadyPresent => {
            "Uitgeschakelde collecties staan al in deze multi-collectie"
        },
        TextKey::EmptyTrash => "Prullenbak legen",
        TextKey::ExtractEmbeddedSubtitles => "Ingesloten (tekst)ondertitels extraheren",
        TextKey::ErasedPlayoutItemsAndHistory => "Uitzenditems en geschiedenis zijn gewist.",
        TextKey::EraseFailed => "Wissen mislukt",
        TextKey::EraseItemsAndHistory => "Items en geschiedenis wissen",
        TextKey::ErasePlayoutItemsAndHistoryConfirm => {
            "Hiermee worden alle uitzenditems en geschiedenis voor deze sequentiële uitzending verwijderd. Doorgaan?"
        },
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
        },
        TextKey::FillerPresetDeleted => "Opvulvoorinstelling verwijderd.",
        TextKey::FillerPresetExpressionHelp => {
            "Voeg bij mid-roll-opvulling alleen opvulling toe wanneer deze expressie waar is voor een bepaald mid-roll-punt."
        },
        TextKey::FillerPresetSaved => "Opvulvoorinstelling opgeslagen.",
        TextKey::FillerPresets => "Opvulvoorinstellingen",
        TextKey::Flags => "Vlaggen",
        TextKey::GlobalFallbackFiller => "Globale fallback-opvulling",
        TextKey::GlobalWatermark => "Globaal watermerk",
        TextKey::GoHome => "Naar startpagina",
        TextKey::HdHomeRun => "HDHomeRun",
        TextKey::HealthChecks => "Gezondheidscontroles",
        TextKey::Height => "Hoogte",
        TextKey::HlsDirectOutputFormat => "Direct HLS-uitvoerformaat",
        TextKey::HlsSegmenterIdleTimeout => "Time-out voor inactieve HLS-segmenter",
        TextKey::HlsSegmenterIdleTimeoutHelp => "Het aantal seconden dat een kanaal blijft transcoderen wanneer geen enkele client nog verzoeken heeft ontvangen",
        TextKey::HlsSegmenterInitialSegmentCount => "Aanvankelijk aantal HLS-segmenten",
        TextKey::HlsSegmenterInitialSegmentCountHelp => "Stelt de start van de stream uit totdat het opgegeven aantal segmenten van 4 seconden is getranscodeerd; een groter aantal betekent een tragere start maar mogelijk minder buffering",
        TextKey::HomeRunning => "IPTV-server is actief.",
        TextKey::HttpMinimumLogLevel => "Minimaal logniveau voor verzoeklogging",
        TextKey::Id => "Id",
        TextKey::Images => "Afbeeldingen",
        TextKey::Item => "Item",
        TextKey::ItemActions => "Itemacties",
        TextKey::ItemType => "Itemtype",
        TextKey::Items => "Items",
        TextKey::Language => "Taal",
        TextKey::Languages => "Talen",
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
        TextKey::MatchTraktListItems => "Items van Trakt-lijst koppelen",
        TextKey::Members => "Leden",
        TextKey::MultiCollection => "Multi-collectie",
        TextKey::MultiCollectionDeleted => "Multi-collectie verwijderd.",
        TextKey::MultiCollectionItems => "Multi-collectie-items",
        TextKey::MultiCollectionNameRequired => "Naam van multi-collectie is verplicht!",
        TextKey::MultiCollectionNameTooLong => {
            "Naam van multi-collectie mag niet langer zijn dan 50 tekens."
        },
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
        TextKey::NoTraktListsYet => "Nog geen Trakt-lijsten.",
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
        TextKey::RowsPerPage => "Rijen per pagina",
        TextKey::Save => "Opslaan",
        TextKey::SaveMultiCollection => "Multi-collectie opslaan",
        TextKey::SavePlaylist => "Afspeellijst opslaan",
        TextKey::SaveTroubleshootingReportsToDisk => "Probleemoplossingsrapporten op schijf opslaan",
        TextKey::SaveAsSmartCollection => "Opslaan als slimme collectie",
        TextKey::SearchQueryRequired => "Zoekopdracht is verplicht",
        TextKey::SaveSettings => "Instellingen opslaan",
        TextKey::Scanner => "Scanner",
        TextKey::ScanningMinimumLogLevel => "Minimaal logniveau voor scannen",
        TextKey::Scheduling => "Planning",
        TextKey::SchedulingMinimumLogLevel => "Minimaal logniveau voor planning",
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
        },
        TextKey::FillWithGroupMode => "Vullen met groepsmodus (serie of artiest)",
        TextKey::GraphicsElements => "Grafische elementen",
        TextKey::GraphicsIds => "Grafische ID's",
        TextKey::Genres => "Genres",
        TextKey::GroupName => "Groepsnaam",
        TextKey::GroupMultiPartEpisodesHelp => "Groepeer meerdelige afleveringen bij schudden",
        TextKey::GuideMode => "Gidsmodus",
        TextKey::KeepMultiPartEpisodesTogether => "Meerdelige afleveringen bij elkaar houden",
        TextKey::Marathon => "Marathon",
        TextKey::MarathonBatchSize => "Marathonbatchgrootte",
        TextKey::MarathonBatchSizeHelp => {
            "Hoeveel items uit elke groep worden afgespeeld voordat wordt doorgegaan; leeg of nul speelt alle items af"
        },
        TextKey::MarathonGroupBy => "Marathon groeperen op",
        TextKey::MarathonShuffleGroups => "Marathongroepen schudden",
        TextKey::MarathonShuffleItems => "Marathonitems schudden",
        TextKey::EditFillerPreset => "Opvulvoorinstelling bewerken",
        TextKey::MidRollFiller => "Mid-roll filler",
        TextKey::MidRoll => "Mid-roll",
        TextKey::MoveDown => "Omlaag verplaatsen",
        TextKey::MoveUp => "Omhoog verplaatsen",
        TextKey::MultiEpisodeGroupSize => "Groepsgrootte voor meerdere afleveringen",
        TextKey::MultiEpisodeShuffle => "Meerdere afleveringen schudden",
        TextKey::Multiple => "Meerdere",
        TextKey::MultipleCount => "Aantal meerdere",
        TextKey::MultipleCountHelp => {
            "Kan een expressie zijn met 'count' (collectiegrootte) en 'random' (0 tot count-1)"
        },
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
        TextKey::RemainingDurationHelp => {
            "duur van de inhoud na dit mid-roll-punt, in seconden"
        },
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
        },
        TextKey::SaveSchedule => "Schema opslaan",
        TextKey::SaveScheduleItems => "Schema-items opslaan",
        TextKey::SaveSmartCollection => "Slimme collectie opslaan",
        TextKey::Schedule => "Schema",
        TextKey::ScheduleActionFailed => "Schema-actie mislukt.",
        TextKey::ScheduleAsGroup => "Als groep plannen",
        TextKey::ScheduleKind => "Schematype",
        TextKey::SchedulingContext => "Planningscontext",
        TextKey::ScheduleItem => "Schema-item",
        TextKey::ScheduleItemActionFailed => "Schema-itemactie mislukt.",
        TextKey::ScheduleItemFiller => "Filler voor schema-item",
        TextKey::ScheduleItemOverrides => "Overschrijvingen voor schema-item",
        TextKey::ScheduleItems => "Schema-items",
        TextKey::ScheduleNotFound => "Schema niet gevonden.",
        TextKey::ScheduleSaveFailed => "Schema opslaan mislukt.",
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
        TextKey::SeasonNotFound => "Seizoen niet gevonden",
        TextKey::Select => "Selecteren",
        TextKey::SelectCollection => "Collectie selecteren",
        TextKey::SelectGroupForNewTemplate => "Selecteer een groep voor het nieuwe sjabloon",
        TextKey::SelectScheduleItemPrompt => "Selecteer een schema-item om te bewerken of voeg een nieuw item toe.",
        TextKey::SelectSchedule => "Selecteer een schema",
        TextKey::SelectSmartCollection => "Slimme collectie selecteren",
        TextKey::Shuffle => "Schudden",
        TextKey::ShuffleInOrder => "Op volgorde schudden",
        TextKey::ShuffleScheduleItems => "Schema-items schudden",
        TextKey::ShuffleScheduleItemsHelp => "Opmerking: dit schakelt vaste starttijden en flood-modus uit",
        TextKey::ShuffledGroups => "Geschudde groepen",
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
        TextKey::SearchMedia => "Media zoeken",
        TextKey::SearchMoviesOrShows => "Films of series zoeken...",
        TextKey::SearchTraktList => "Trakt-lijst zoeken",
        TextKey::SearchingMinimumLogLevel => "Minimaal logniveau voor zoeken",
        TextKey::Settings => "Instellingen",
        TextKey::SettingsSaveFailed => "Instellingen opslaan mislukt",
        TextKey::ShowMediaInfo => "Media-info weergeven",
        TextKey::ScriptedScheduleTimeout => "Time-out voor scriptschema",
        TextKey::SkipMissingItems => "Ontbrekende items overslaan",
        TextKey::SkipMissingItemsHelp => "Bepaalt of bestand-niet-gevonden of niet-beschikbare items in uitzendingen worden opgenomen",
        TextKey::SmartCollections => "Slimme collecties",
        TextKey::SmartCollection => "Slimme collectie",
        TextKey::SmartCollectionDeleted => "Slimme collectie verwijderd.",
        TextKey::SmartCollectionQueryPlaceholder => "bijv. genre:action OR genre:comedy",
        TextKey::SmartCollectionSaved => "Slimme collectie opgeslagen.",
        TextKey::SaveTraktList => "Trakt-lijst opslaan",
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
        TextKey::TraktList => "Trakt-lijst",
        TextKey::TraktListActionFailed => "Actie voor Trakt-lijst mislukt.",
        TextKey::TraktListAdded => "Trakt-lijst toegevoegd.",
        TextKey::TraktListDeleted => "Trakt-lijst verwijderd.",
        TextKey::TraktListMatched => "Trakt-lijst gekoppeld.",
        TextKey::TraktListNotFound => "Trakt-lijst niet gevonden.",
        TextKey::TraktListSaveFailed => "Opslaan van Trakt-lijst mislukt.",
        TextKey::TraktListSaved => "Trakt-lijst opgeslagen.",
        TextKey::TraktLists => "Trakt-lijsten",
        TextKey::TraktListUrl => "URL van Trakt-lijst",
        TextKey::Trash => "Prullenbak",
        TextKey::TrashAlreadyEmpty => "De prullenbak is al leeg.",
        TextKey::TrashEmpty => "De prullenbak is leeg.",
        TextKey::TrashEmptyConfirm => "Alle items waarvan het bestand ontbreekt uit de database verwijderen?",
        TextKey::TrashEmptyConfirmHelp => {
            "Dit verwijdert de momenteel ontbrekende media-items en hun afhankelijke metadatarijen permanent."
        },
        TextKey::TrashSubtitle => {
            "Items waarvan de bronbestanden momenteel ontbreken op schijf of bij de provider."
        },
        TextKey::Troubleshooting => "Probleemoplossing",
        TextKey::TunerCount => "Aantal tuners",
        TextKey::TvShows => "Series",
        TextKey::Ui => "Interface",
        TextKey::Unavailable => "Niet beschikbaar",
        TextKey::UseActualTimes => "Werkelijke tijden gebruiken",
        TextKey::UseCustomPlaybackOrder => "Aangepaste afspeelvolgorde gebruiken",
        TextKey::UseChaptersAsMediaItems => "Hoofdstukken als media-items gebruiken",
        TextKey::UseChaptersAsMediaItemsHelp => {
            "Plan afzonderlijke hoofdstukken in plaats van volledige bestanden"
        },
        TextKey::UseEmbeddedSubtitles => "Ingesloten ondertitels gebruiken",
        TextKey::Uuid => "UUID",
        TextKey::UntitledItem => "Item zonder titel",
        TextKey::Watermarks => "Watermerken",
        TextKey::Width => "Breedte",
        TextKey::WorkAheadSegmenterLimit => "Limiet voor vooruitwerkende HLS-segmenters",
        TextKey::WorkAheadSegmenterLimitHelp => "Het aantal segmenters (kanalen) dat tegelijk vooruitwerkt (transcodeert op maximale snelheid) wanneer meerdere kanalen worden bekeken",
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
        TextKey::AllowWatermarks => "Watermerken toestaan",
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
        TextKey::NewProfileName => "Nieuwe profielnaam",
        TextKey::NoneOption => "(Geen)",
        TextKey::NumHelp => "het mid-roll-puntnummer, beginnend bij 1",
        TextKey::OptionalDisplayTitle => "Optionele weergavetitel...",
        TextKey::NormalizeAudio => "Audio normaliseren",
        TextKey::NormalizeColors => "Kleuren normaliseren",
        TextKey::NormalizeFramerate => "Framerate normaliseren",
        TextKey::NormalizeLoudness => "Loudness normaliseren",
        TextKey::NormalizeVideo => "Video normaliseren",
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
        TextKey::TonemapAlgorithm => "Tonemap-algoritme",
        TextKey::VaapiDevicePath => "VAAPI-apparaatpad",
        TextKey::VaapiDisplay => "VAAPI-display",
        TextKey::VaapiDriver => "VAAPI-driver",
        TextKey::Video => "Video",
        TextKey::VideoBitrate => "Videobitrate (kBit/s)",
        TextKey::VideoBufferSize => "Videobuffergrootte (kBit)",
        TextKey::VideoFormat => "Videoformaat",
        TextKey::VideoPreset => "Videopreset",
        TextKey::VideoProfile => "Videoprofiel",
        TextKey::AddAll => "Alles toevoegen",
        TextKey::AddedItemToPlaylist => "Item toegevoegd aan afspeellijst.",
        TextKey::AddedArtistToSchedule => "Artiest toegevoegd aan schema",
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
        TextKey::EditTraktListProperties => "Eigenschappen van Trakt-lijst bewerken",
        TextKey::EnterTraktListUrl => "Voer URL van Trakt-lijst in",
        TextKey::EnterSearchQuery => "Voer een zoekopdracht in.",
        TextKey::Episodes => "Afleveringen",
        TextKey::LoadingCollections => "Collecties laden...",
        TextKey::LoadingPlaylistGroups => "Afspeellijstgroepen laden...",
        TextKey::LoadingPlaylists => "Afspeellijsten laden...",
        TextKey::GeneratePlaylist => "Afspeellijst genereren",
        TextKey::GeneratePlaylistFromSortedTraktItems => {
            "Genereer een afspeellijst van de gesorteerde Trakt-lijstitems"
        },
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
        TextKey::SelectSearchCollectionHelp => "Selecteer de collectie waaraan de zoekresultaten worden toegevoegd.",
        TextKey::SelectSearchPlaylistHelp => "Selecteer de afspeellijst waaraan de zoekresultaten worden toegevoegd.",
        TextKey::Selected => "Geselecteerd",
        TextKey::SelectCollectionToAddThisMovie => {
            "Selecteer een collectie om deze film toe te voegen."
        },
        TextKey::SelectPlaylistToAddThisMovie => {
            "Selecteer een afspeellijst om deze film toe te voegen."
        },
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
        },
        TextKey::ConfigureConnection => "Verbinding configureren",
        TextKey::Connection => "Verbinding",
        TextKey::CollectionsScanFailed => "Collectiescan mislukt",
        TextKey::CollectionsScanQueued => "Collectiescan in wachtrij geplaatst",
        TextKey::DeepScanCollections => "Collecties diep scannen",
        TextKey::DeepScanLibrary => "Bibliotheek diep scannen",
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
        TextKey::LocalLibraries => "Lokale bibliotheken",
        TextKey::LocalPath => "Lokaal pad",
        TextKey::MediaKind => "Mediasoort",
        TextKey::NoLibrariesConfigured => "Geen bibliotheken geconfigureerd. Voeg eerst een mediabron toe.",
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
        TextKey::SignInToPlex => "Aanmelden bij Plex",
        TextKey::SignOutOfPlex => "Afmelden bij Plex",
        TextKey::Sync => "Synchroniseren",
        TextKey::WaitingForPlexAuthorization => "Wachten op Plex-autorisatie.",
        TextKey::AddChannel => "Kanaal toevoegen",
        TextKey::Any => "Elke",
        TextKey::BlockPlayoutTroubleshooting => "Probleemoplossing voor blokuitzending",
        TextKey::Categories => "Categorieën",
        TextKey::CategoriesHelp => "Komma-gescheiden lijst met categorieën",
        TextKey::Channel => "Kanaal",
        TextKey::ChannelCouldNotBeSaved => "Kanaal kon niet worden opgeslagen. Controleer de formulierwaarden en probeer het opnieuw.",
        TextKey::ChannelDisabled => "Kanaal uitgeschakeld",
        TextKey::ChannelHiddenFromEpg => "Kanaal verborgen in EPG",
        TextKey::ChannelLogo => "Kanaallogo",
        TextKey::ChannelLogoPreview => "Voorbeeld van kanaallogo",
        TextKey::ChannelLogoUploadFailed => "Uploaden van kanaallogo mislukt. Controleer het bestand en probeer het opnieuw.",
        TextKey::Check => "Controle",
        TextKey::Continuous => "Continu",
        TextKey::DefaultOption => "Standaard",
        TextKey::DefaultSchedule => "Standaardschema",
        TextKey::DeleteChannel => "Kanaal verwijderen",
        TextKey::DownloadMediaSample => "Mediavoorbeeld downloaden",
        TextKey::DownloadResults => "Resultaten downloaden",
        TextKey::DisableWatermarks => "Watermerken uitschakelen",
        TextKey::EditChannel => "Kanaal bewerken",
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
        TextKey::Group => "Groep",
        TextKey::HealthNoPersistedRun => "Er is nog geen gezondheidscontrole opgeslagen. De planner vult deze weergave na de volgende onderhoudsronde.",
        TextKey::HealthPersistedRunHelp => "Laatste opgeslagen pariteitsgezondheidscontrole van de onderhoudsplanner.",
        TextKey::HlsDirect => "HLS direct",
        TextKey::HlsSegmenter => "HLS-segmenter",
        TextKey::Hours => "uur",
        TextKey::IdleBehavior => "Gedrag bij inactiviteit",
        TextKey::IdleBehaviorHelp => "Bepaalt hoe transcodering zich gedraagt nadat alle clients zijn losgekoppeld",
        TextKey::Info => "Info",
        TextKey::IsEnabled => "Ingeschakeld",
        TextKey::KeepRunning => "Actief houden",
        TextKey::LastMidFillerHelp => "seconden sinds de laatste mid-roll filler",
        TextKey::LastPage => "Laatste pagina",
        TextKey::Level => "Niveau",
        TextKey::LoadingMediaInfo => "Media-info laden...",
        TextKey::Logs => "Logboek",
        TextKey::Logo => "Logo",
        TextKey::MediaItem => "Media-item",
        TextKey::Message => "Bericht",
        TextKey::MissingMediaItemQueryParameter => "Queryparameter mediaItem ontbreekt",
        TextKey::Mirror => "Spiegelen",
        TextKey::MirrorSourceChannel => "Bronkanaal voor spiegeling",
        TextKey::MusicVideoCreditsMode => "Aftitelingmodus voor muziekvideo's",
        TextKey::MusicVideoCreditsTemplate => "Aftitelingssjabloon voor muziekvideo's",
        TextKey::NoMatchingRecordsFound => "Geen overeenkomende records gevonden",
        TextKey::NoFfmpegProfiles => "Geen FFmpeg-profielen gevonden. Maak er eerst een aan.",
        TextKey::NvidiaCapabilitiesNotCollected => "NVIDIA-mogelijkheden zijn nog niet verzameld.",
        TextKey::Number => "Nummer",
        TextKey::OnDemand => "Op aanvraag",
        TextKey::MatchedPointsHelp => {
            "aantal mid-roll-punten dat al overeenkwam met de expressie"
        },
        TextKey::Pass => "Geslaagd",
        TextKey::PlaybackTroubleshooting => "Afspeelprobleemoplossing",
        TextKey::Play => "Afspelen",
        TextKey::PlayoutTimeline => "Uitzendtijdlijn",
        TextKey::PlayoutMode => "Uitzendmodus",
        TextKey::PlayoutModeHelp => "Bepaalt de voortgang van de uitzending van het kanaal",
        TextKey::PlayoutOffset => "Uitzendverschuiving",
        TextKey::PlayoutSource => "Uitzendbron",
        TextKey::PlayoutSourceHelp => "Bepaalt de bron van de kanaalinhoud. Dit kan niet worden gewijzigd wanneer het kanaal al een uitzending heeft.",
        TextKey::Preview => "Voorbeeld",
        TextKey::PreviewBlockPlayout => "Voorbeeld van blokuitzending",
        TextKey::PreviewChannel => "Kanaalvoorbeeld",
        TextKey::PreviewPlaylistPlayout => "Voorbeeld van afspeellijstuitzending",
        TextKey::PreferredAudioTitle => "Voorkeurstitel voor audio",
        TextKey::PreferredSubtitleLanguage => "Voorkeurstaal voor ondertitels",
        TextKey::QsvCapabilitiesNotCollected => "QSV-mogelijkheden zijn nog niet verzameld.",
        TextKey::RemoveStoredLogo => "Opgeslagen logo verwijderen",
        TextKey::SaveChannel => "Kanaal opslaan",
        TextKey::Second => "seconde",
        TextKey::Seconds => "seconden",
        TextKey::SelectProfile => "- Selecteer profiel -",
        TextKey::SequentialSchedule => "Sequentieel schema",
        TextKey::SequentialScheduleValidation => "Validatie van sequentieel schema",
        TextKey::ShowDisabled => "Uitgeschakelde tonen",
        TextKey::ShowInEpg => "Weergeven in EPG",
        TextKey::SlugSeconds => "Tussenpauze in seconden",
        TextKey::SlugSecondsHelp => "Seconden zwarte video en stille audio tussen elk uitzenditem",
        TextKey::Song => "Nummer",
        TextKey::SongVideoMode => "Nummervideomodus",
        TextKey::SortByLevel => "Sorteren op niveau",
        TextKey::SortByTimestamp => "Sorteren op tijdstip",
        TextKey::StopOnDisconnect => "Stoppen bij verbreken",
        TextKey::StopScheduling => "Planning stoppen",
        TextKey::StopSchedulingBlockItems => "Planning van blokitems stoppen",
        TextKey::StreamSelector => "Streamselector",
        TextKey::StreamSelectorMode => "Streamselectormodus",
        TextKey::StreamingMode => "Streamingmodus",
        TextKey::Styles => "Stijlen",
        TextKey::SubtitleMode => "Ondertitelmodus",
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
        TextKey::UnableToLoadMediaInfo => "Kan media-informatie niet laden",
        TextKey::UnableToLoadPlaybackTroubleshooting => {
            "Kan context voor afspeelprobleemoplossing niet laden"
        },
        TextKey::UpdateListFromTraktDaily => "Lijst eenmaal per dag bijwerken vanaf trakt.tv",
        TextKey::UploadLogo => "Logo uploaden",
        TextKey::UploadLogoHelp => "Kies een lokale afbeelding om het opgeslagen kanaallogo bij opslaan te vervangen",
        TextKey::Warning => "Waarschuwing",
        TextKey::Watermark => "Watermerk",
        TextKey::WatermarkIds => "Watermerk-ID's",
        TextKey::Window => "Venster",
        TextKey::WithProgress => "Met voortgang",
        TextKey::Writers => "Schrijvers",
        TextKey::ActualSize => "Werkelijke grootte",
        TextKey::AddWatermark => "Watermerk toevoegen",
        TextKey::BottomLeft => "Linksonder",
        TextKey::BottomMiddle => "Middenonder",
        TextKey::BottomRight => "Rechtsonder",
        TextKey::CopyWatermark => "Watermerk kopiëren",
        TextKey::CopyOf => "Kopie van",
        TextKey::CurrentImage => "Huidige afbeelding",
        TextKey::Custom => "Aangepast",
        TextKey::DeleteWatermark => "Watermerk verwijderen",
        TextKey::DurationSeconds => "Duur (seconden)",
        TextKey::EditWatermark => "Watermerk bewerken",
        TextKey::Frequency => "Frequentie",
        TextKey::GeneratingPreview => "Voorbeeld genereren...",
        TextKey::HorizontalMarginPercent => "Horizontale marge (%)",
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
        TextKey::Opacity => "Dekking",
        TextKey::OpacityExpression => "Dekkingsuitdrukking",
        TextKey::OpacityOneToHundred => "Dekking (1-100)",
        TextKey::Permanent => "Permanent",
        TextKey::PerPass => "per doorgang",
        TextKey::PlaceWithinSourceContent => "Binnen broninhoud plaatsen",
        TextKey::Placement => "Plaatsing",
        TextKey::PlacementPreview => "Plaatsingsvoorbeeld",
        TextKey::RightMiddle => "Midden rechts",
        TextKey::SaveWatermark => "Watermerk opslaan",
        TextKey::Scaled => "Geschaald",
        TextKey::Size => "Grootte",
        TextKey::TopLeft => "Linksboven",
        TextKey::TopMiddle => "Middenboven",
        TextKey::TopRight => "Rechtsboven",
        TextKey::UploadFailed => "Uploaden mislukt.",
        TextKey::UploadImage => "Afbeelding uploaden",
        TextKey::VerticalMarginPercent => "Verticale marge (%)",
        TextKey::WatermarkCopied => "Watermerk gekopieerd.",
        TextKey::WatermarkNotFound => "Watermerk niet gevonden.",
        TextKey::WatermarkPlacementPreview => "voorbeeld van watermerkplaatsing",
        TextKey::WatermarkPreview => "watermerkvoorbeeld",
        TextKey::WidthPercent => "Breedte (%)",
        TextKey::ZIndex => "Z-index",
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
