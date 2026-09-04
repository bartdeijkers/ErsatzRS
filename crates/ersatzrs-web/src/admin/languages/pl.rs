use crate::admin::i18n::{LanguageStrings, TextKey};

pub const SORT_ORDER: u16 = 20;
pub const CODE: &str = "pl";
pub const LABEL: &str = "Polski";
pub const ALIASES: &[&str] = &["pl"];

pub struct PolishStrings;

pub static STRINGS: PolishStrings = PolishStrings;

fn episode_display_subtitle(
    show_title: Option<&str>,
    season_number: Option<i64>,
    episode_number: Option<i64>,
) -> Option<String> {
    let show = show_title.unwrap_or("").trim();
    match (season_number, episode_number) {
        (Some(season), Some(episode)) if !show.is_empty() => {
            Some(format!("{show} - sezon {season}, odcinek {episode}"))
        }
        (Some(season), Some(episode)) => Some(format!("Sezon {season}, odcinek {episode}")),
        _ => (!show.is_empty()).then(|| show.to_owned()),
    }
}

impl LanguageStrings for PolishStrings {
    fn text(&self, key: TextKey) -> &'static str {
        text(key)
    }

    fn selected_count(&self, count: usize) -> String {
        format!("Zaznaczono: {count}")
    }

    fn item_count(&self, count: u64) -> String {
        format!("Elementy: {count}")
    }

    fn range_of(&self, start: u32, end: u32, total: u64) -> String {
        format!("{start}-{end} z {total}")
    }

    fn match_status(&self, matched: u64, total: u64) -> String {
        format!("{matched} z {total}")
    }

    fn rows_per_page(&self, page_size: u32) -> String {
        format!("Wierszy na stronę: {page_size}")
    }

    fn page_of(&self, page: u32, total: u32) -> String {
        format!("Strona {page} z {total}")
    }

    fn save_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Zapisywanie nie powiodło się: {error}")
    }

    fn delete_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Usuwanie nie powiodło się: {error}")
    }

    fn empty_trash_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Opróżnianie kosza nie powiodło się: {error}")
    }

    fn search_count(&self, total: u32, label: &'static str) -> String {
        format!("{total} {label}")
    }

    fn selected_items_count(&self, count: usize) -> String {
        if count == 1 {
            "Zaznaczono 1 element".to_owned()
        } else {
            format!("Zaznaczono {count} elementów")
        }
    }

    fn delete_selected_count(&self, count: usize) -> String {
        format!("Usuń zaznaczone ({count})")
    }

    fn deleted_items_from_database(&self, count: u64) -> String {
        if count == 1 {
            "Usunięto 1 element z bazy danych.".to_owned()
        } else {
            format!("Usunięto {count} elementów z bazy danych.")
        }
    }

    fn season_display_name(&self, number: i64) -> String {
        if number == 0 {
            "Odcinki specjalne".to_owned()
        } else {
            format!("Sezon {number}")
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
        format!("Element multimedialny #{id}")
    }

    fn schedule_item_fallback_label(&self, id: i64) -> String {
        format!("Element harmonogramu #{id}")
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
            1 => "MPEG-TS (starszy)".to_owned(),
            2 => "HLS Direct".to_owned(),
            4 => "Segmenter HLS".to_owned(),
            5 => "MPEG-TS".to_owned(),
            _ => format!("Nieznany ({mode})"),
        }
    }

    fn channel_language_label(&self, code: &str) -> String {
        match code.trim().to_ascii_lowercase().as_str() {
            "" => String::new(),
            "eng" | "en" => "Angielski".to_owned(),
            "nld" | "dut" | "nl" => "Niderlandzki".to_owned(),
            "deu" | "ger" | "de" => "Niemiecki".to_owned(),
            "fra" | "fre" | "fr" => "Francuski".to_owned(),
            "spa" | "es" => "Hiszpański".to_owned(),
            "ita" | "it" => "Włoski".to_owned(),
            "jpn" | "ja" => "Japoński".to_owned(),
            "kor" | "ko" => "Koreański".to_owned(),
            "zho" | "chi" | "zh" => "Chiński".to_owned(),
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
            _ => "Nieznane",
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
            format!("{label} (niedostępne w tym systemie)")
        }
    }

    fn ffmpeg_tonemap_algorithm_label(&self, algorithm: i32) -> String {
        match algorithm {
            0 => "Liniowy",
            1 => "Przycięcie",
            2 => "Gamma",
            3 => "Reinhard",
            4 => "Mobius",
            5 => "Hable",
            _ => return format!("Nieznany ({algorithm})"),
        }
        .to_owned()
    }

    fn ffmpeg_output_dynamic_range_label(&self, range: i32) -> String {
        match range {
            0 => "SDR (BT.709)",
            1 => "HDR10 (BT.2020 PQ)",
            _ => return format!("Nieznany ({range})"),
        }
        .to_owned()
    }

    fn ffmpeg_loudness_mode_label(&self, mode: i32) -> String {
        match mode {
            0 => text(TextKey::Off),
            1 => "loudnorm",
            _ => return format!("Nieznany ({mode})"),
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
        format!("Opróżniono kosz: usunięto {count} elementów.")
    }

    fn select_collection_to_add_selected_items(&self, count: usize) -> String {
        format!("Wybierz kolekcję, do której dodać {count} zaznaczonych elementów")
    }

    fn select_playlist_to_add_selected_items(&self, count: usize) -> String {
        format!("Wybierz playlistę, do której dodać {count} zaznaczonych elementów")
    }

    fn added_items_to_collection(&self, count: usize) -> String {
        format!("Dodano {count} elementów do kolekcji")
    }

    fn added_items_to_playlist(&self, count: usize) -> String {
        format!("Dodano {count} elementów do playlisty")
    }

    fn add_to_collection_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Dodawanie do kolekcji nie powiodło się: {error}")
    }

    fn add_to_playlist_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Dodawanie do playlisty nie powiodło się: {error}")
    }

    fn no_media_source_configured(&self, provider: &'static str) -> String {
        format!("Brak skonfigurowanego źródła {provider}. Kliknij Skonfiguruj połączenie, aby je dodać.")
    }

    fn provider_connection_title(&self, provider: &'static str) -> String {
        format!("Połączenie {provider}")
    }

    fn provider_libraries_title(&self, provider: &'static str) -> String {
        format!("Biblioteki {provider}")
    }

    fn provider_path_replacements_title(&self, provider: &'static str) -> String {
        format!("Zamienniki ścieżek {provider}")
    }

    fn provider_path_label(&self, provider: &'static str) -> String {
        format!("Ścieżka {provider}")
    }

    fn health_summary(&self, errors: u32, warnings: u32) -> String {
        format!("Błędy: {errors}, ostrzeżenia: {warnings}")
    }

    fn classic_playout_state_message(&self, playouts: &str) -> String {
        format!(
            "Stan klasycznej emisji nie pozwala na bezpieczne odświeżenie dla: {playouts}. Użyj Resetuj emisję dla starego lub niezgodnego stanu; Odśwież nigdy nie usuwa okna bez pełnego punktu kontrolnego."
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
            "Stan odświeżania po skanowaniu wymaga uwagi: {unresolved_resolutions} nierozwiązanych analiz wpływu, {stale_resolutions} nieaktualnych analiz wpływu, {failed_intents} nieudanych zleceń odświeżenia emisji i {stale_intents} nieaktualnych zleceń odświeżenia emisji. Sprawdź dzienniki skanera i harmonogramowania, a następnie ponów skanowanie lub odświeżenie emisji."
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
        format!("{recent} kanał(y) dostarczały multimedia HLS na żywo wolniej niż w czasie rzeczywistym w ciągu ostatnich {minutes} minut; {active} nie odzyskały prawidłowego tempa podczas ostatniej obserwacji; {fallbacks} kanał(y) korzystają ze zweryfikowanego trybu awaryjnego z kodowaniem programowym i/lub niższą jakością przez maksymalnie {fallback_hours} godziny. Sprawdź dzienniki strumieniowania i uruchom test odtwarzania TSDuck uwzględniający tempo dostarczania przed zmianą ustawień dostawcy.")
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
            "Krotność grafik wymaga uwagi: {show_duplicate_rows} zduplikowanych wierszy grafik serialu, {season_duplicate_rows} zduplikowanych wierszy grafik sezonu i {episode_duplicate_rows} zduplikowanych wierszy grafik odcinka; liczby dla projekcji telewizyjnej to {published_items} opublikowanych pozycji odcinków z list dodatków, {projection_rows} wierszy projekcji, {distinct_episodes} różnych projektowanych odcinków, {complete_hierarchy_rows} projekcji z pełną hierarchią i {hierarchy_episodes} odcinków w hierarchii. Odśwież lub ponownie opublikuj odpowiednie telewizyjne listy dodatków."
        )
    }

    fn unable_to_load_playback_troubleshooting(&self, error: &dyn std::fmt::Display) -> String {
        format!("Nie można wczytać kontekstu diagnostyki odtwarzania: {error}")
    }

    fn playout_reset_prompt(&self, channel: &str, schedule: &str) -> String {
        format!(
            "Podaj godzinę, o której plan emisji na kanale {channel} z harmonogramem {schedule} ma być codziennie odbudowywany"
        )
    }
}

fn schedule_error_message(error: &str) -> String {
    match error.trim() {
        "Schedule name is required!" => "Nazwa harmonogramu jest wymagana!".to_owned(),
        "New Schedule Name is required." => "Nazwa nowego harmonogramu jest wymagana.".to_owned(),
        "Schedule name must be 50 characters or less." => {
            "Nazwa harmonogramu może mieć maksymalnie 50 znaków.".to_owned()
        }
        "Schedule name must be unique" => "Nazwa harmonogramu musi być unikalna".to_owned(),
        "Schedule does not exist" => "Harmonogram nie istnieje".to_owned(),
        "[ProgramScheduleId] does not exist." => "[ProgramScheduleId] nie istnieje.".to_owned(),
        "A collection must use the same playback order in every schedule item." => {
            "Kolekcja musi używać tej samej kolejności odtwarzania w każdym elemencie harmonogramu.".to_owned()
        }
        "Shuffle In Order cannot be used with Fill With Group Mode." => {
            "Kolejności losowej z zachowaniem porządku nie można używać z trybem wypełniania grupą.".to_owned()
        }
        "Fixed schedule items require a start time." => {
            "Stałe elementy harmonogramu wymagają godziny rozpoczęcia.".to_owned()
        }
        "Multi Collection items can only use Shuffle or Shuffle In Order." => {
            "Elementy multikolekcji mogą używać tylko kolejności losowej albo losowej z zachowaniem porządku.".to_owned()
        }
        "Playlist Item Size can only be used with Playlist schedule items." => {
            "Rozmiaru elementu playlisty można używać tylko z elementami harmonogramu typu playlista.".to_owned()
        }
        "Multiple playout mode with Count requires a count expression." => {
            "Tryb wielu odtworzeń z liczbą wymaga wyrażenia liczby.".to_owned()
        }
        "Duration playout mode requires a duration between 1 minute and 24 hours." => {
            "Tryb odtwarzania według czasu wymaga czasu od 1 minuty do 24 godzin.".to_owned()
        }
        "Duration playout mode requires discard-to-fill attempts." => {
            "Tryb odtwarzania według czasu wymaga liczby prób odrzucania do wypełnienia.".to_owned()
        }
        "Tail Mode Filler requires a tail filler preset." => {
            "Tryb końcówki Filler wymaga presetu fillera końcowego.".to_owned()
        }
        "Tail filler can only be set when Tail Mode is Filler." => {
            "Filler końcowy można ustawić tylko wtedy, gdy tryb końcówki to Filler.".to_owned()
        }
        "Collection is required." => "Kolekcja jest wymagana.".to_owned(),
        "Media item is required." => "Element multimedialny jest wymagany.".to_owned(),
        "Multi Collection is required." => "Multikolekcja jest wymagana.".to_owned(),
        "Smart Collection is required." => "Inteligentna kolekcja jest wymagana.".to_owned(),
        "Playlist is required." => "Playlista jest wymagana.".to_owned(),
        "Rerun Collection is required." => "Kolekcja powtórek jest wymagana.".to_owned(),
        "Search Query is required." => "Zapytanie wyszukiwania jest wymagane.".to_owned(),
        "Only one pre-, mid-, or post-roll filler can pad to the nearest minute." => {
            "Tylko jeden filler przed, w trakcie albo po może dopełniać do najbliższej minuty.".to_owned()
        }
        "Padding filler requires a fallback filler." => {
            "Filler dopełniający wymaga fillera awaryjnego.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn scheduling_content_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("Block group name \"") && trimmed.ends_with("\" is invalid") {
        return "Nazwa grupy bloków może mieć maksymalnie 50 znaków.".to_owned();
    }
    if trimmed.starts_with("Block name \"") && trimmed.ends_with("\" is invalid") {
        return "Nazwa bloku może mieć maksymalnie 50 znaków.".to_owned();
    }
    if trimmed.starts_with("Template name \"") && trimmed.ends_with("\" is invalid") {
        return "Nazwa szablonu może mieć maksymalnie 50 znaków.".to_owned();
    }
    if trimmed.starts_with("A template named \"")
        && trimmed.ends_with("\" already exists in that template group")
    {
        return "Szablon o tej nazwie już istnieje w tej grupie szablonów.".to_owned();
    }
    if trimmed.starts_with("Block from ") && trimmed.contains(" intersects block from ") {
        return "Blok nakłada się na inny blok.".to_owned();
    }
    if trimmed.starts_with("TemplateGroup ") && trimmed.ends_with(" does not exist.") {
        return "Grupa szablonów nie istnieje.".to_owned();
    }
    if trimmed.starts_with("Template ") && trimmed.ends_with(" does not exist.") {
        return "Szablon nie istnieje.".to_owned();
    }
    if trimmed.starts_with("DecoGroup ") && trimmed.ends_with(" does not exist.") {
        return "Grupa dekoracji nie istnieje.".to_owned();
    }
    if trimmed.starts_with("Deco ") && trimmed.ends_with(" does not exist.") {
        return "Dekoracja nie istnieje.".to_owned();
    }
    if trimmed.starts_with("DecoTemplateGroup ") && trimmed.ends_with(" does not exist.") {
        return "Grupa szablonów dekoracji nie istnieje.".to_owned();
    }
    if trimmed.starts_with("DecoTemplate ") && trimmed.ends_with(" does not exist.") {
        return "Szablon dekoracji nie istnieje.".to_owned();
    }
    if trimmed.starts_with("Deco name \"") && trimmed.ends_with("\" is invalid") {
        return "Nazwa dekoracji może mieć maksymalnie 50 znaków.".to_owned();
    }
    if trimmed.starts_with("A deco named \"")
        && trimmed.ends_with("\" already exists in that deco group")
    {
        return "Dekoracja o tej nazwie już istnieje w tej grupie dekoracji.".to_owned();
    }
    if trimmed.starts_with("Deco template name \"") && trimmed.ends_with("\" is invalid") {
        return "Nazwa szablonu dekoracji może mieć maksymalnie 50 znaków.".to_owned();
    }
    if trimmed.starts_with("A deco template named \"")
        && trimmed.ends_with("\" already exists in that deco template group")
    {
        return "Szablon dekoracji o tej nazwie już istnieje w tej grupie szablonów dekoracji."
            .to_owned();
    }
    if trimmed.starts_with("Watermark ") && trimmed.ends_with(" does not exist.") {
        return "Znak wodny nie istnieje.".to_owned();
    }
    if trimmed.starts_with("Graphics element ") && trimmed.ends_with(" does not exist.") {
        return "Element graficzny nie istnieje.".to_owned();
    }
    if trimmed.starts_with("Deco from ") && trimmed.contains(" intersects deco from ") {
        return "Dekoracja nakłada się na inną dekorację.".to_owned();
    }

    match trimmed {
        "Block does not exist." => "Blok nie istnieje.".to_owned(),
        "Block group name is required" => "Nazwa grupy bloków jest wymagana".to_owned(),
        "Block group name must be unique" => "Nazwa grupy bloków musi być unikalna".to_owned(),
        "Block name is required" => "Nazwa bloku jest wymagana".to_owned(),
        "A block named with that value already exists in that block group" => {
            "Blok o tej nazwie już istnieje w tej grupie bloków.".to_owned()
        }
        "Block duration must be between 5 minutes and 24 hours" => {
            "Czas trwania bloku musi wynosić od 5 minut do 24 godzin.".to_owned()
        }
        "[Collection] is required for collection type 'Collection'" => {
            "Kolekcja jest wymagana dla typu kolekcji Kolekcja.".to_owned()
        }
        "[MediaItem] is required for collection type 'TelevisionShow'" => {
            "Element multimedialny jest wymagany dla typu kolekcji Serial.".to_owned()
        }
        "[MediaItem] is required for collection type 'TelevisionSeason'" => {
            "Element multimedialny jest wymagany dla typu kolekcji Sezon serialu.".to_owned()
        }
        "[MultiCollection] is required for collection type 'MultiCollection'" => {
            "Multikolekcja jest wymagana dla typu kolekcji Multikolekcja.".to_owned()
        }
        "[SmartCollection] is required for collection type 'SmartCollection'" => {
            "Inteligentna kolekcja jest wymagana dla typu kolekcji Inteligentna kolekcja."
                .to_owned()
        }
        "[SearchQuery] is required for collection type 'SearchQuery'" => {
            "Zapytanie wyszukiwania jest wymagane dla typu kolekcji Zapytanie wyszukiwania."
                .to_owned()
        }
        "[CollectionType] is invalid" => "Typ kolekcji jest nieprawidłowy.".to_owned(),
        "[BlockId] does not exist." => "Blok nie istnieje.".to_owned(),
        "Template does not exist." => "Szablon nie istnieje.".to_owned(),
        "Template group name is required" => "Nazwa grupy szablonów jest wymagana".to_owned(),
        "Template group name must be 50 characters or fewer" => {
            "Nazwa grupy szablonów może mieć maksymalnie 50 znaków.".to_owned()
        }
        "Template group name must be unique" => {
            "Nazwa grupy szablonów musi być unikalna".to_owned()
        }
        "Template group does not exist." => "Grupa szablonów nie istnieje.".to_owned(),
        "[TemplateId] does not exist." => "Szablon nie istnieje.".to_owned(),
        "Template name is required" => "Nazwa szablonu jest wymagana".to_owned(),
        "Template items cannot cross midnight." => {
            "Elementy szablonu nie mogą przekraczać północy.".to_owned()
        }
        "Deco group name is required" => "Nazwa grupy dekoracji jest wymagana".to_owned(),
        "Deco group name must be 50 characters or fewer" => {
            "Nazwa grupy dekoracji może mieć maksymalnie 50 znaków.".to_owned()
        }
        "Deco group name must be unique" => "Nazwa grupy dekoracji musi być unikalna".to_owned(),
        "Deco group does not exist." => "Grupa dekoracji nie istnieje.".to_owned(),
        "Deco name is required" => "Nazwa dekoracji jest wymagana".to_owned(),
        "Deco does not exist" => "Dekoracja nie istnieje.".to_owned(),
        "Default filler has unsupported collection type." => {
            "Domyślne wypełnienie ma nieobsługiwany typ kolekcji.".to_owned()
        }
        "Dead air fallback has unsupported collection type." => {
            "Awaryjna plansza martwej emisji ma nieobsługiwany typ kolekcji.".to_owned()
        }
        "Break content has unsupported collection type." => {
            "Treść przerwy ma nieobsługiwany typ kolekcji.".to_owned()
        }
        "Default filler must have valid item" => {
            "Domyślne wypełnienie musi mieć prawidłowy element.".to_owned()
        }
        "Dead air fallback must have valid item" => {
            "Awaryjna plansza martwej emisji musi mieć prawidłowy element.".to_owned()
        }
        "Break content collection must have valid item" => {
            "Kolekcja treści przerwy musi mieć prawidłowy element.".to_owned()
        }
        "Break content multi collection must have valid item" => {
            "Multikolekcja treści przerwy musi mieć prawidłowy element.".to_owned()
        }
        "Break content smart collection must have valid item" => {
            "Inteligentna kolekcja treści przerwy musi mieć prawidłowy element.".to_owned()
        }
        "Break content media item must have valid item" => {
            "Element multimedialny treści przerwy musi być prawidłowy.".to_owned()
        }
        "Break content playlist must have valid item" => {
            "Playlista treści przerwy musi mieć prawidłowy element.".to_owned()
        }
        "Default filler item does not exist." => {
            "Element domyślnego wypełnienia nie istnieje.".to_owned()
        }
        "Dead air fallback item does not exist." => {
            "Element awaryjnej planszy martwej emisji nie istnieje.".to_owned()
        }
        "Break content collection item does not exist." => {
            "Element kolekcji treści przerwy nie istnieje.".to_owned()
        }
        "Break content multi collection item does not exist." => {
            "Element multikolekcji treści przerwy nie istnieje.".to_owned()
        }
        "Break content smart collection item does not exist." => {
            "Element inteligentnej kolekcji treści przerwy nie istnieje.".to_owned()
        }
        "Break content media item item does not exist." => {
            "Element multimedialny treści przerwy nie istnieje.".to_owned()
        }
        "Break content playlist item does not exist." => {
            "Playlista treści przerwy nie istnieje.".to_owned()
        }
        "Deco may only contain one [Block Start] break content" => {
            "Dekoracja może zawierać tylko jedną treść przerwy na początku bloku.".to_owned()
        }
        "Deco may only contain one [Between Block Items] break content" => {
            "Dekoracja może zawierać tylko jedną treść przerwy między elementami bloku.".to_owned()
        }
        "Deco may only contain one [At Chapter Markers] break content" => {
            "Dekoracja może zawierać tylko jedną treść przerwy przy znacznikach rozdziałów."
                .to_owned()
        }
        "Deco may only contain one [Block Finish] break content" => {
            "Dekoracja może zawierać tylko jedną treść przerwy na końcu bloku.".to_owned()
        }
        "Deco template group name is required" => {
            "Nazwa grupy szablonów dekoracji jest wymagana".to_owned()
        }
        "Deco template group name must be 50 characters or fewer" => {
            "Nazwa grupy szablonów dekoracji może mieć maksymalnie 50 znaków.".to_owned()
        }
        "Deco template group name must be unique" => {
            "Nazwa grupy szablonów dekoracji musi być unikalna".to_owned()
        }
        "Deco template group does not exist." => {
            "Grupa szablonów dekoracji nie istnieje.".to_owned()
        }
        "[DecoTemplateId] does not exist." => "Szablon dekoracji nie istnieje.".to_owned(),
        "Deco template name is required" => "Nazwa szablonu dekoracji jest wymagana".to_owned(),
        "[DecoId] does not exist." => "Dekoracja nie istnieje.".to_owned(),
        "Deco template items cannot be empty." => {
            "Elementy szablonu dekoracji nie mogą być puste.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn watermark_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("Watermark ") && trimmed.ends_with(" does not exist.") {
        return "Nie znaleziono znaku wodnego.".to_owned();
    }

    match trimmed {
        "Name is required" => "Nazwa jest wymagana".to_owned(),
        "Name must be 50 characters or fewer" => "Nazwa może mieć maksymalnie 50 znaków".to_owned(),
        "Watermark image is required" | "Watermark image is required!" => {
            "Obraz znaku wodnego jest wymagany".to_owned()
        }
        "Width must be between 0 and 100%" => {
            "Szerokość musi być z zakresu od 0 do 100%".to_owned()
        }
        "Horizontal margin must be between 0 and 50%" => {
            "Margines poziomy musi być z zakresu od 0 do 50%".to_owned()
        }
        "Vertical margin must be between 0 and 50%" => {
            "Margines pionowy musi być z zakresu od 0 do 50%".to_owned()
        }
        "Duration must be positive and less than frequency window" => {
            "Czas trwania musi być dodatni i krótszy niż okno częstotliwości".to_owned()
        }
        "Opacity must be between 1 and 100" => {
            "Przezroczystość musi być z zakresu od 1 do 100".to_owned()
        }
        "Watermark name must be unique" | "ChannelWatermark name must be unique" => {
            "Nazwa znaku wodnego musi być unikalna".to_owned()
        }
        "Watermark not found" | "Source watermark not found" | "Watermark does not exist." => {
            "Nie znaleziono znaku wodnego.".to_owned()
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
        _ => return format!("Nieznany ({value})"),
    }
    .to_owned()
}

fn ffmpeg_profile_video_format_label(format: i32) -> String {
    match format {
        0 => "Brak",
        1 => "H264",
        2 => "H265 (HEVC)",
        3 => "MPEG-2",
        4 => "AV1",
        99 => text(TextKey::Copy),
        _ => return format!("Nieznany ({format})"),
    }
    .to_owned()
}

fn ffmpeg_profile_audio_format_label(format: i32) -> String {
    match format {
        0 => "Brak",
        1 => "AAC",
        2 => "AC3",
        3 => "AAC (LATM)",
        99 => text(TextKey::Copy),
        _ => return format!("Nieznany ({format})"),
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
            return format!("Profil FFmpeg o nazwie \"{name}\" już istnieje.");
        }
    }

    match trimmed {
        "Profile name is required." => "Nazwa profilu jest wymagana.".to_owned(),
        "Profile name must be 50 characters or fewer." => {
            "Nazwa profilu może mieć maksymalnie 50 znaków.".to_owned()
        }
        "Thread count must be 0 or greater." => {
            "Liczba wątków musi wynosić 0 lub więcej.".to_owned()
        }
        "QSV supports formats (h264, hevc, mpeg2video)" => {
            "QSV obsługuje formaty (h264, hevc, mpeg2video).".to_owned()
        }
        "NVENC supports formats (h264, hevc, av1)" => {
            "NVENC obsługuje formaty (h264, hevc, av1).".to_owned()
        }
        "VAAPI supports formats (h264, hevc, av1, mpeg2video)" => {
            "VAAPI obsługuje formaty (h264, hevc, av1, mpeg2video).".to_owned()
        }
        "VideoToolbox supports formats (h264, hevc)" => {
            "VideoToolbox obsługuje formaty (h264, hevc).".to_owned()
        }
        "Amf supports formats (h264, hevc)" => "Amf obsługuje formaty (h264, hevc).".to_owned(),
        "V4L2 M2M supports formats (h264, hevc)" => {
            "V4L2 M2M obsługuje formaty (h264, hevc).".to_owned()
        }
        "Rkmpp supports formats (h264, hevc)" => "Rkmpp obsługuje formaty (h264, hevc).".to_owned(),
        "HDR10 output requires hevc or av1; h264 and mpeg2video cannot carry HDR10 static metadata" => {
            "Wyjście HDR10 wymaga hevc lub av1; h264 i mpeg2video nie mogą przenosić statycznych metadanych HDR10.".to_owned()
        }
        "HDR10 output requires 10-bit; 8-bit cannot represent a PQ signal without banding" => {
            "Wyjście HDR10 wymaga 10 bitów; 8 bitów nie odwzoruje sygnału PQ bez pasmowania.".to_owned()
        }
        "Mastering display must look like G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1), with chromaticities up to 50000 and max luminance above min" => {
            "Monitor masteringowy musi mieć postać G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1), z chromatycznością do 50000 i luminancją maksymalną większą od minimalnej.".to_owned()
        }
        "MaxCLL and MaxFALL must both be set (or both left empty), at most 65535, with MaxFALL no greater than MaxCLL" => {
            "MaxCLL i MaxFALL muszą być ustawione oba (albo oba puste), maksymalnie 65535, przy czym MaxFALL nie może przekraczać MaxCLL.".to_owned()
        }
        "Mpeg2Video does not support 10-bit content" => {
            "Mpeg2Video nie obsługuje treści 10-bitowych.".to_owned()
        }
        "VideoProfile must be high10 with 10-bit h264" => {
            "Profil wideo musi być high10 dla 10-bitowego h264.".to_owned()
        }
        "VideoProfile must be high444p with NVIDIA 10-bit h264" => {
            "Profil wideo musi być high444p dla 10-bitowego h264 z NVIDIA.".to_owned()
        }
        "VideoProfile cannot be high10 with 8-bit h264" => {
            "Profil wideo nie może być high10 dla 8-bitowego h264.".to_owned()
        }
        "Source profile not found." => "Nie znaleziono profilu źródłowego.".to_owned(),
        "No FFmpeg resolutions are available; database bootstrap is incomplete" => {
            "Brak dostępnych rozdzielczości FFmpeg; bootstrap bazy danych jest niekompletny"
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
            "Liczba dni XMLTV do zbudowania ({current}) nie może być większa niż liczba dni playoutu do zbudowania ({limit})"
        );
    }
    if let Some(name) = trimmed.strip_suffix(" path does not exist") {
        return format!("Ścieżka {name} nie istnieje");
    }
    if let Some(name) = trimmed
        .strip_prefix("Unable to verify ")
        .and_then(|value| value.strip_suffix(" version"))
    {
        return format!("Nie można zweryfikować wersji {name}");
    }
    if let Some(value) = trimmed.strip_prefix("Invalid option: ") {
        return format!("Nieprawidłowa opcja: {value}");
    }
    if trimmed.starts_with("Resolution ") && trimmed.ends_with(" does not exist.") {
        return "Rozdzielczość nie istnieje.".to_owned();
    }

    match trimmed {
        "Windows startup registration is not supported on this platform" => {
            "Rejestracja uruchamiania z systemem Windows nie jest obsługiwana na tej platformie."
                .to_owned()
        }
        "ErsatzRS.exe was not found next to ersatzrs-web.exe" => {
            "Nie znaleziono ErsatzRS.exe obok ersatzrs-web.exe.".to_owned()
        }
        "Unable to update Windows startup registration" => {
            "Nie można zaktualizować rejestracji uruchamiania z systemem Windows.".to_owned()
        }
        "Tuner count must be greater than zero" => "Liczba tunerów musi być większa od zera.".to_owned(),
        "Library refresh interval must be 0 (do not refresh) or greater than zero" => {
            "Interwał odświeżania biblioteki musi wynosić 0 (bez odświeżania) albo więcej niż zero.".to_owned()
        }
        "Library refresh interval must be less than 1,000,000. Use 0 to disable automatic refresh" => {
            "Interwał odświeżania biblioteki musi być mniejszy niż 1 000 000. Użyj 0, aby wyłączyć automatyczne odświeżanie.".to_owned()
        }
        "Playout days to build must be greater than zero" => "Liczba dni playoutu do zbudowania musi być większa od zera.".to_owned(),
        "Scripted schedule timeout seconds must be at least 10" => {
            "Limit czasu harmonogramu skryptowego musi wynosić co najmniej 10 sekund.".to_owned()
        }
        "XMLTV days to build must be greater than zero" => "Liczba dni XMLTV do zbudowania musi być większa od zera.".to_owned(),
        "Resolution width or height is invalid" => "Szerokość lub wysokość rozdzielczości jest nieprawidłowa.".to_owned(),
        "Resolution width and height must be unique" => "Szerokość i wysokość rozdzielczości muszą być unikalne.".to_owned(),
        "HLS Segmenter idle timeout must be greater than or equal to 30" => {
            "Limit bezczynności segmentera HLS musi być większy lub równy 30.".to_owned()
        }
        "Work-Ahead HLS Segmenter limit must be greater than or equal to 0" => {
            "Limit wyprzedzającego segmentera HLS musi być większy lub równy 0.".to_owned()
        }
        "Active HLS transcode limit must be greater than or equal to 1" => {
            "Limit aktywnych transkodowań HLS musi być większy lub równy 1.".to_owned()
        }
        "HLS Segmenter initial segment count must be greater than or equal to 1" => {
            "Początkowa liczba segmentów segmentera HLS musi być większa lub równa 1.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn media_action_error_message(error: &str) -> String {
    match error.trim() {
        "Collection name is required" => "Nazwa kolekcji jest wymagana.".to_owned(),
        "Collection name must be 50 characters or less" => {
            "Nazwa kolekcji może mieć najwyżej 50 znaków.".to_owned()
        }
        "Collection name must be unique" => "Nazwa kolekcji musi być unikatowa.".to_owned(),
        "Media item does not exist" => "Element multimedialny nie istnieje.".to_owned(),
        "Collection does not exist" | "Collection does not exist." => {
            "Kolekcja nie istnieje.".to_owned()
        }
        "Playlist does not exist" | "Playlist does not exist." => {
            "Playlista nie istnieje.".to_owned()
        }
        "Unable to locate media item" => "Nie można znaleźć elementu multimedialnego.".to_owned(),
        "Media action failed" => "Akcja multimediów nie powiodła się.".to_owned(),
        _ => error.to_owned(),
    }
}

fn filler_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("FillerPreset ") && trimmed.ends_with(" does not exist.") {
        return "Preset wypełniacza nie istnieje.".to_owned();
    }

    match trimmed {
        "Filler preset does not exist" => "Preset wypełniacza nie istnieje.".to_owned(),
        "Filler preset name must be unique" => {
            "Nazwa presetu wypełniacza musi być unikatowa.".to_owned()
        }
        "Name must not be empty" => "Nazwa nie może być pusta.".to_owned(),
        "Name must be 50 characters or fewer" => {
            "Nazwa może mieć najwyżej 50 znaków.".to_owned()
        }
        "Filler kind must not be None" => "Rodzaj wypełniacza nie może być pusty.".to_owned(),
        "Filler mode must not be None" => "Tryb wypełniacza nie może być pusty.".to_owned(),
        "Count must be greater than or equal to 0" => {
            "Liczba musi być większa lub równa 0.".to_owned()
        }
        "Duration must not be empty" => "Czas trwania nie może być pusty.".to_owned(),
        "Pad To Nearest Minute must not be empty" => {
            "Dopełnienie do najbliższej minuty nie może być puste.".to_owned()
        }
        "Use Chapters As Media Items must not be enabled for fallback filler" => {
            "Używanie rozdziałów jako elementów multimedialnych nie może być włączone dla wypełniacza awaryjnego.".to_owned()
        }
        "Collection must not be empty" => "Kolekcja nie może być pusta.".to_owned(),
        "Multi Collection must not be empty" => "Multi-kolekcja nie może być pusta.".to_owned(),
        "Smart Collection must not be empty" => {
            "Kolekcja inteligentna nie może być pusta.".to_owned()
        }
        "Media item must not be empty" => {
            "Element multimedialny nie może być pusty.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn media_browser_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed == "Pinned stream selection is invalid or no longer available." {
        return "Przypięty wybór strumienia jest nieprawidłowy lub niedostępny.".to_owned();
    }
    if trimmed == "Movie was not found." {
        return "Nie znaleziono filmu.".to_owned();
    }
    if trimmed.starts_with("Collection ") && trimmed.ends_with(" is already present.") {
        return "Kolekcja jest już obecna.".to_owned();
    }
    if trimmed.starts_with("Collection ") && trimmed.ends_with(" does not exist.") {
        return "Kolekcja nie istnieje.".to_owned();
    }
    if trimmed.starts_with("Smart collection ") && trimmed.ends_with(" is already present.") {
        return "Kolekcja inteligentna jest już obecna.".to_owned();
    }
    if trimmed.starts_with("Smart collection ") && trimmed.ends_with(" does not exist.") {
        return "Kolekcja inteligentna nie istnieje.".to_owned();
    }
    if trimmed.starts_with("MultiCollection ") && trimmed.ends_with(" does not exist.") {
        return "Multi-kolekcja nie istnieje.".to_owned();
    }
    if trimmed.starts_with("Rerun collection ") && trimmed.ends_with(" does not exist.") {
        return "Kolekcja powtórek nie istnieje.".to_owned();
    }
    if trimmed.starts_with("Referenced item ") && trimmed.ends_with(" does not exist.") {
        return "Element referencyjny nie istnieje.".to_owned();
    }
    if trimmed.starts_with("PlaylistGroup ") && trimmed.ends_with(" does not exist.") {
        return "Grupa playlist nie istnieje.".to_owned();
    }
    if trimmed.starts_with("Playlist ") && trimmed.ends_with(" does not exist.") {
        return "Playlista nie istnieje.".to_owned();
    }
    if trimmed.starts_with("Playlist name \"") && trimmed.ends_with("\" is invalid") {
        return "Nazwa playlisty jest nieprawidłowa.".to_owned();
    }
    if let Some(error) = trimmed.strip_prefix("Playlist preview failed: ") {
        return format!("Podgląd playlisty nie powiódł się: {error}");
    }
    if trimmed.starts_with("Media item ") && trimmed.ends_with(" does not exist.") {
        return "Element multimedialny nie istnieje.".to_owned();
    }

    match trimmed {
        "Collection name is required" => "Nazwa kolekcji jest wymagana.".to_owned(),
        "Collection name must be 50 characters or less" => {
            "Nazwa kolekcji może mieć najwyżej 50 znaków.".to_owned()
        }
        "Collection name must be unique" => "Nazwa kolekcji musi być unikatowa.".to_owned(),
        "Collection does not exist." => "Kolekcja nie istnieje.".to_owned(),
        "Smart collection name is required" => {
            "Nazwa kolekcji inteligentnej jest wymagana.".to_owned()
        }
        "Smart collection query is required" => {
            "Zapytanie kolekcji inteligentnej jest wymagane.".to_owned()
        }
        "SmartCollection name must be unique" => {
            "Nazwa kolekcji inteligentnej musi być unikatowa.".to_owned()
        }
        "MultiCollection does not exist." => "Multi-kolekcja nie istnieje.".to_owned(),
        "Multi-collection name is required!" => "Nazwa multi-kolekcji jest wymagana.".to_owned(),
        "Multi-collection name cannot be longer than 50 characters." => {
            "Nazwa multi-kolekcji może mieć najwyżej 50 znaków.".to_owned()
        }
        "MultiCollection name must be unique" => {
            "Nazwa multi-kolekcji musi być unikatowa.".to_owned()
        }
        "Each multi-collection item must reference exactly one collection." => {
            "Każdy element multi-kolekcji musi wskazywać dokładnie jedną kolekcję.".to_owned()
        }
        "Rerun collection does not exist." => "Kolekcja powtórek nie istnieje.".to_owned(),
        "[Name] is an empty string" | "[Name] must not be empty." => {
            "Nazwa nie może być pusta.".to_owned()
        }
        "[Name] must not be longer than 50" | "[Name] must not be longer than 50." => {
            "Nazwa może mieć najwyżej 50 znaków.".to_owned()
        }
        "Rerun collection name must be unique" => {
            "Nazwa kolekcji powtórek musi być unikatowa.".to_owned()
        }
        "Unsupported collection type." => "Nieobsługiwany typ kolekcji.".to_owned(),
        "[ProgramScheduleId] does not exist." => "Harmonogram nie istnieje.".to_owned(),
        "[MediaItem] does not exist." => "Element multimedialny nie istnieje.".to_owned(),
        "[PlaylistGroupId] does not exist." => "Grupa playlist nie istnieje.".to_owned(),
        "Cannot delete system playlist group" => {
            "Nie można usunąć systemowej grupy playlist.".to_owned()
        }
        "Cannot delete system (generated) playlist" => {
            "Nie można usunąć systemowej playlisty.".to_owned()
        }
        "[PlaylistId] does not exist." => "Playlista nie istnieje.".to_owned(),
        "Cannot save system (generated) playlist" => {
            "Nie można zapisać systemowej playlisty.".to_owned()
        }
        "Playlist playout preview has no items" => {
            "Podgląd emisji playlisty nie ma elementów.".to_owned()
        }
        "Playlist name is required" => "Nazwa playlisty jest wymagana.".to_owned(),
        "[Collection] is required for collection type 'Collection'" => {
            "Kolekcja jest wymagana.".to_owned()
        }
        "[MultiCollection] is required for collection type 'MultiCollection'" => {
            "Multi-kolekcja jest wymagana.".to_owned()
        }
        "[SmartCollection] is required for collection type 'SmartCollection'" => {
            "Kolekcja inteligentna jest wymagana.".to_owned()
        }
        "[CollectionType] is invalid" => "Typ kolekcji jest nieprawidłowy.".to_owned(),
        _ => error.to_owned(),
    }
}

fn media_source_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if let Some(error) = trimmed.strip_prefix("Could not reach Jellyfin: ") {
        return format!("Nie można połączyć się z Jellyfin: {error}");
    }

    match trimmed {
        "Address is required." => "Adres jest wymagany.".to_owned(),
        "API key is required." => "Klucz API jest wymagany.".to_owned(),
        "'Address' must be a valid URL" => "Adres musi być prawidłowym adresem URL.".to_owned(),
        _ => error.to_owned(),
    }
}

fn addon_operation_error_message(code: &str, detail: &str) -> String {
    let summary = match code {
        "missing-setting" => "Skonfiguruj wymagane ustawienie dodatku.",
        "missing-secret" => "Skonfiguruj wymagane odwołanie do sekretu w ustawieniach dodatku.",
        "provider-unreachable" => "Dodatek nie mógł połączyć się z dostawcą.",
        "provider-rejected" => "Dostawca odrzucił żądanie dodatku.",
        "provider-rate-limited" => "Dostawca tymczasowo ograniczył dodatek.",
        "not-found" => "Nie znaleziono żądanego elementu dodatku.",
        "unsupported-url" => "Dodatek nie obsługuje tego adresu URL.",
        _ => "Dodatek zgłosił błąd.",
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
        return format!("Add-on zwrócił nieprawidłowe dane listy: {error}");
    }
    if normalized.starts_with("add-on media list with id ") && normalized.ends_with(" not found") {
        return "Lista add-on nie istnieje.".to_owned();
    }

    if let Some(folder) =
        normalized.strip_prefix("another list of this add-on already uses the folder ")
    {
        return format!("Inna lista tego dodatku już używa folderu \"{folder}\".");
    }
    if let Some(folder) = normalized.strip_suffix(" is not usable as a folder name") {
        return format!("\"{folder}\" nie nadaje się na nazwę folderu.");
    }

    match normalized {
        "Invalid add-on list url" => "Nieprawidłowy URL listy add-on.".to_owned(),
        "add-on API client is not configured for this operation" => {
            "Klient API add-on nie jest skonfigurowany dla tej operacji.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn upload_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if let Some(error) = trimmed.strip_prefix("Cannot create upload directory: ") {
        return format!("Nie można utworzyć katalogu przesyłania: {error}");
    }
    if let Some(error) = trimmed.strip_prefix("Write failed: ") {
        return format!("Zapisywanie nie powiodło się: {error}");
    }
    if let Some(error) = trimmed.strip_prefix("Read field: ") {
        return format!("Odczyt pola nie powiódł się: {error}");
    }

    match trimmed {
        "Upload exceeds maximum allowed size" => {
            "Przesłany plik przekracza maksymalny dozwolony rozmiar.".to_owned()
        }
        "No file field" => "Nie znaleziono pola pliku.".to_owned(),
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
        TextKey::Addon => "Dodatek",
        TextKey::PrimaryFeature => "Film główny",
        TextKey::MovieExtra => "Materiał dodatkowy",
        TextKey::NeedReview => "Wymaga sprawdzenia",
        TextKey::ReviewRecommended => "Zalecane sprawdzenie",
        TextKey::PublicationReadiness => "Gotowość do publikacji",
        TextKey::PublicationBlockers => "Blokady",
        TextKey::PublicationActivity => "Aktywność",
        TextKey::PublicationJobs => "zadania publikacji",
        TextKey::PublicationEvents => "zdarzenia",
        TextKey::NoPublicationActivity => "Nie zapisano jeszcze aktywności publikacji.",
        TextKey::CopyRedactedDiagnostics => "Kopiuj zredagowaną diagnostykę",
        TextKey::DownloadRedactedDiagnostics => "Pobierz zredagowaną diagnostykę",
        TextKey::RedactedDiagnosticsCopied => "Zredagowana diagnostyka została skopiowana.",
        TextKey::PublicationGeneration => "Generacja",
        TextKey::CorrelationId => "Identyfikator korelacji",
        TextKey::TargetPublishedRevision => "Docelowa opublikowana wersja",
        TextKey::RelatedOperation => "Powiązana operacja",
        TextKey::FilesystemTransition => "Zmiana systemu plików",
        TextKey::RetryClass => "Klasa ponowienia",
        TextKey::RecoveryAction => "Działanie odzyskiwania",
        TextKey::OperatingSystemErrorCode => "Kod błędu systemu operacyjnego",
        TextKey::PublicationOutcome => "Wynik",
        TextKey::PublicationExpired => "Wygasła",
        TextKey::EventExistingJobAdopted => "Przejęto istniejącą publikację",
        TextKey::EventEnqueued => "Publikacja dodana do kolejki",
        TextKey::EventLeaseWaitStarted => "Oczekiwanie na dostęp do biblioteki",
        TextKey::EventLeaseAcquired => "Uzyskano dostęp do biblioteki",
        TextKey::EventLeaseReclaimed => "Publikację wznowiono po przerwaniu lub wygaśnięciu dzierżawy",
        TextKey::EventPhaseAdvanced => "Przejście do kolejnej fazy publikacji",
        TextKey::EventChildScanStarted => "Rozpoczęto skanowanie weryfikacyjne",
        TextKey::EventChildScanCompleted => "Zakończono skanowanie weryfikacyjne",
        TextKey::EventChildScanFailed => "Skanowanie weryfikacyjne nie powiodło się",
        TextKey::EventOriginalFailure => "Pierwotny błąd publikacji",
        TextKey::EventRecoveryFailure => "Błąd odzyskiwania",
        TextKey::EventRetryScheduled => "Zaplanowano ponowienie odzyskiwania",
        TextKey::EventRecoveryRequested => "Zażądano odzyskiwania",
        TextKey::EventCompleted => "Publikacja zakończona",
        TextKey::EventHistoryTruncated => "Starsza aktywność została obcięta",
        TextKey::TriggerOperator => "Zażądane przez operatora",
        TextKey::TriggerRestoreRevision => "Zażądane przez przywrócenie wersji",
        TextKey::TestSource => "Testuj źródło",
        TextKey::TestSourceHelp => "Sprawdź zapisane źródło bez importowania ani zmiany listy.",
        TextKey::SourceTestQueued => "Test źródła w kolejce",
        TextKey::SourceTestRunning => "Trwa test źródła…",
        TextKey::SourceTestStale => "Test jest nieaktualny, ponieważ zapisane dane wejściowe nie pasują już do bieżącej listy, pakietu dodatku lub wartości domyślnych.",
        TextKey::SourceAvailable => "Źródło jest dostępne",
        TextKey::SourceUnsupported => "Źródło nie jest obsługiwane",
        TextKey::SourceTestFailed => "Test źródła nie powiódł się",
        TextKey::SourceRevision => "Wersja źródła",
        TextKey::FirstPageComplete => "Pierwsza strona kompletna",
        TextKey::ProviderIdentity => "Tożsamość dostawcy",
        TextKey::ReturnedFirstPageItems => "Elementy zwrócone na pierwszej stronie",
        TextKey::ProviderTotalItemCount => "Łączna liczba elementów według dostawcy",
        TextKey::DraftChanges => "Zmiany zapisanego szkicu",
        TextKey::ExactDraftDiffHelp => "Dokładne różnice między zapisanym szkicem a bieżącą opublikowaną wersją.",
        TextKey::DraftDiffUnavailable => "Dokładne porównanie będzie dostępne po opublikowaniu pełnej wersji.",
        TextKey::MetadataChanges => "Zmiany metadanych",
        TextKey::ExcludedItems => "Wykluczone elementy",
        TextKey::Conflicts => "Konflikty",
        TextKey::RemovalCandidates => "Kandydaci do usunięcia",
        TextKey::RevisionHistory => "Historia wersji",
        TextKey::RevisionHistoryHelp => "Przywrócenie zastępuje bieżący szkic i publikuje wybraną migawkę jako nową wersję.",
        TextKey::NoRevisionHistory => "Nie zachowano jeszcze opublikowanych wersji.",
        TextKey::Revision => "Wersja",
        TextKey::CompareRevisions => "Porównaj wersje",
        TextKey::RestoreRevision => "Przywróć wersję",
        TextKey::RestoreRevisionWarning => "To zastępuje bieżący szkic i publikuje wersję",
        TextKey::RestoreRevisionQueued => "Przywracanie wersji w kolejce",
        TextKey::RestoreRevisionFailed => "Nie udało się przywrócić wersji.",
        TextKey::PublishedItems => "Opublikowane elementy",
        TextKey::GeneratedPlaylists => "Wygenerowane listy odtwarzania",
        TextKey::ActiveOrFuturePlayouts => "Aktywne lub przyszłe emisje",
        TextKey::EffectiveOptions => "Obowiązujące opcje",
        TextKey::Files => "Pliki",
        TextKey::Artifacts => "Artefakty",
        TextKey::Added => "Dodano",
        TextKey::Removed => "Usunięto",
        TextKey::Changed => "Zmieniono",
        TextKey::FilesystemPreviousBackedUp => "Utworzono kopię poprzedniej wersji",
        TextKey::FilesystemCandidateCommitted => "Zatwierdzono wersję kandydującą",
        TextKey::FilesystemPreviousRestored => "Przywrócono poprzednią wersję",
        TextKey::FilesystemProjectionRestored => "Przywrócono projekcję bazy danych",
        TextKey::FilesystemRevisionVerified => "Wersja zweryfikowana",
        TextKey::FilesystemCleanupCompleted => "Zakończono czyszczenie publikacji",
        TextKey::RetryTransientRecovery => "Przejściowe odzyskiwanie",
        TextKey::ErrorFilesystemIo => "Wejście/wyjście systemu plików",
        TextKey::ErrorPermissionDenied => "Odmowa dostępu",
        TextKey::ErrorSharingViolation => "Naruszenie współdzielenia pliku",
        TextKey::ErrorUnsafeFilesystemState => "Niebezpieczny stan systemu plików",
        TextKey::ErrorInvalidPublicationMarker => "Nieprawidłowy znacznik publikacji",
        TextKey::ErrorPublicationIntegrity => "Kontrola integralności publikacji",
        TextKey::OutcomePublished => "Opublikowano",
        TextKey::OutcomeRestoredAfterFailure => "Po błędzie przywrócono poprzednią wersję",
        TextKey::ReviewRecommendations => "Zalecenia do sprawdzenia",
        TextKey::OptionalDetails => "Opcjonalne szczegóły",
        TextKey::DraftSaved => "Wersja robocza zapisana",
        TextKey::SavedRevision => "Zapisana rewizja",
        TextKey::PublishList => "Opublikuj listę",
        TextKey::PreparePublication => "Przygotuj publikację",
        TextKey::PreparingPublication => "Przygotowywanie publikacji",
        TextKey::Publishing => "Publikowanie…",
        TextKey::PublicationQueued => "Publikacja w kolejce",
        TextKey::RestoringPublication => "Przywracanie poprzedniej wersji…",
        TextKey::Published => "Opublikowano",
        TextKey::PublishSavedRevisionHelp => "Publikowana jest dokładnie zapisana rewizja. Najpierw zapisz zmiany listy.",
        TextKey::ChangesSavedImmediately => "Zmiany elementów i zmiany zbiorcze są zapisywane od razu i można je cofnąć.",
        TextKey::ReviewItem => "Sprawdź element",
        TextKey::ShowFullDescription => "Pokaż pełny opis",
        TextKey::ShowLess => "Pokaż mniej",
        TextKey::ImportAList => "Importuj listę",
        TextKey::ImportOverview => "Importuj przegląd",
        TextKey::AddonContextSelected => "Wybrany dodatek",
        TextKey::CompletedIn => "Ukończono w",
        TextKey::NoPublicationBlockers => "Brak blokad publikacji.",
        TextKey::BlockerOverviewIncomplete => "Pełny przegląd musi zostać zweryfikowany.",
        TextKey::BlockerPublicationInProgress => "Publikacja już trwa.",
        TextKey::BlockerRecoveryRequired => "Poprzednia publikacja nadal wymaga odzyskania.",
        TextKey::BlockerCollectionTitleMissing => "Kolekcja wymaga tytułu.",
        TextKey::BlockerInvalidArtwork => "Grafika musi używać prawidłowego, ograniczonego adresu HTTPS.",
        TextKey::BlockerNoIncludedItems => "Uwzględnij co najmniej jeden element.",
        TextKey::BlockerMoviePrimaryCount => "Publikacja filmów wymaga dokładnie jednego filmu głównego.",
        TextKey::BlockerPlaybackDecisionRequired => "Wybierz odtwarzanie skończone lub na żywo.",
        TextKey::BlockerPlaybackDetailsUnavailable => "Ten dodatek nie udostępnia wymaganych szczegółów odtwarzania.",
        TextKey::BlockerPlaybackDetailsFailed => "Szczegóły dostawcy nie zawierają dodatniego czasu trwania.",
        TextKey::BlockerUnavailableWithoutReplacement => "Niedostępny element wymaga prawidłowego lokalnego zamiennika.",
        TextKey::BlockerItemTitleMissing => "Element wymaga tytułu.",
        TextKey::BlockerSeasonEpisodeMissing => "Podaj prawidłowy numer sezonu i odcinka.",
        TextKey::BlockerArtistMissing => "Podaj co najmniej jednego wykonawcę.",
        TextKey::PublishUnsavedChanges => "Zapisz zmiany przed opublikowaniem tej rewizji.",
        TextKey::PublicationBlocked => "Publikacja jest zablokowana.",
        TextKey::PublishBlocked => "Publikowanie zablokowane",
        TextKey::RetryRecovery => "Ponów odzyskiwanie",
        TextKey::RestorePreviousRevision => "Przywróć poprzednią wersję",
        TextKey::PublicationPhase => "Faza publikacji",
        TextKey::PublicationAttempts => "Próby",
        TextKey::LastProgress => "Ostatni postęp",
        TextKey::FailureReason => "Pierwotna przyczyna",
        TextKey::RestrictedItemsExcluded => "Elementy z ograniczeniami wykluczone z tej wersji",
        TextKey::RecoveryQueued => "Odzyskiwanie w kolejce",
        TextKey::RecoveryFailed => "Nie udało się dodać odzyskiwania do kolejki.",
        TextKey::PhaseAwaitingLease => "Oczekiwanie na dostęp do biblioteki",
        TextKey::PhaseStaging => "Przygotowywanie wersji",
        TextKey::PhaseScanning => "Skanowanie przygotowanej wersji",
        TextKey::PhaseCommitting => "Zatwierdzanie wersji",
        TextKey::PhaseRestoringFiles => "Przywracanie plików",
        TextKey::PhaseRestoringProjection => "Przywracanie projekcji bazy danych",
        TextKey::PhaseReconciling => "Uzgadnianie przywróconej wersji",
        TextKey::PhaseVerifying => "Weryfikowanie wersji",
        TextKey::PhaseCleaningUp => "Czyszczenie plików publikacji",
        TextKey::PhaseComplete => "Ukończona",
        TextKey::InstallLeavesDisabled => "Po instalacji dodatek pozostaje wyłączony, dopóki go nie włączysz.",
        TextKey::EnableActivatesPermissions => "Włączenie aktywuje zadeklarowane uprawnienia dodatku.",
        TextKey::FirstAired => "Pierwsza emisja",
        TextKey::LoadItemDetailsAfterPublishing => "Automatycznie pobieraj szczegóły",
        TextKey::LoadItemDetailsHelp => "Najpierw zaimportuj przegląd. Szczegóły techniczne wymagane do odtwarzania są pobierane przed pierwszą publikacją, a pozostałe rozszerzone metadane później w kontrolowanym tempie.",
        TextKey::OverviewOnlyManualDetailsHelp => "Zaimportuj tylko przegląd i ręcznie żądaj szczegółów opisowych. Szczegóły techniczne wymagane do odtwarzania są automatycznie przygotowywane przed publikacją.",
        TextKey::PlaybackDetailsQueued => "Szczegóły odtwarzania w kolejce",
        TextKey::PublishAgainAfterDetails => "Sprawdź ukończone szczegóły, a następnie opublikuj ponownie.",
        TextKey::FetchDetails => "Pobierz szczegóły",
        TextKey::DetailsQueued => "Szczegóły w kolejce",
        TextKey::DetailsLoading => "Pobieranie szczegółów",
        TextKey::RetryDetails => "Ponów szczegóły",
        TextKey::DetailsComplete => "Szczegóły kompletne",
        TextKey::DetailsSkipped => "Szczegóły pominięte",
        TextKey::FindingProgrammes => "Wyszukiwanie programów",
        TextKey::CheckingOverview => "Sprawdzanie przeglądu",
        TextKey::ReadyToReview => "Gotowe do sprawdzenia",
        TextKey::LoadingItemDetails => "Pobieranie szczegółów elementów",
        TextKey::WaitingForProvider => "Oczekiwanie na dostawcę",
        TextKey::OverviewOnly => "Tylko przegląd",
        TextKey::PauseImport => "Wstrzymaj import",
        TextKey::ResumeImport => "Wznów import",
        TextKey::RetryImportNow => "Ponów import teraz",
        TextKey::EmptyImportNeedsRetry => "Pusty import wymaga ponowienia",
        TextKey::EmptyImportRetryHelp => "Ten zakończony import nie zawiera elementów. Zaktualizuj dodatek i ponów import; ustawienia oraz zmiany listy zostaną zachowane.",
        TextKey::CancelImport => "Anuluj import",
        TextKey::OtherPageItemsAffected => "Dodatkowe zmieniane elementy są na innych stronach",
        TextKey::ElapsedTime => "Czas trwania",
        TextKey::LastSuccessfulProgress => "Ostatni udany postęp",
        TextKey::EstimatedRemainingTime => "Szacowany pozostały czas",
        TextKey::PlaybackBehavior => "Sposób odtwarzania",
        TextKey::UseProviderValue => "Użyj wartości dostawcy",
        TextKey::FiniteMedia => "Materiał skończony",
        TextKey::LiveSource => "Źródło na żywo",
        TextKey::ProviderValueChanged => "Wartość dostawcy uległa zmianie",
        TextKey::ApplyToSelected => "Zastosuj do zaznaczonych",
        TextKey::ApplyToAllRemoteStreams => "Zastosuj do wszystkich strumieni zdalnych",
        TextKey::SelectItem => "Zaznacz element",
        TextKey::NumberEpisodes => "Numeruj odcinki",
        TextKey::NumberEpisodesHelp => {
            "Przypisz jeden sezon i kolejne numery odcinków w pierwotnej kolejności dodatku. Wykluczone elementy i fragmenty rozdziałów są pomijane."
        }
        TextKey::FirstEpisode => "Pierwszy odcinek",
        TextKey::NumberToAssign => "Liczba do przypisania",
        TextKey::AllRemaining => "Wszystkie pozostałe",
        TextKey::ReplaceExistingEpisodeNumbers => "Zastąp istniejące numery odcinków",
        TextKey::AutoDetectSeasons => "Automatycznie wykryj sezony z dat emisji",
        TextKey::AutoDetectSeasonsHelp => "Nowy sezon zaczyna się, gdy odstęp między dwiema znanymi datami emisji osiąga ustawione minimum. Pozycje bez daty pozostają w bieżącym sezonie.",
        TextKey::MinimumSeasonGapDays => "Minimalny odstęp między sezonami (dni)",
        TextKey::NumberingStartsAt => "Numerowanie zaczyna się od",
        TextKey::FirstIncludedItem => "Pierwsza uwzględniona pozycja",
        TextKey::StartWithFirstItem => "Zacznij od pierwszej pozycji",
        TextKey::StartHere => "Zacznij tutaj",
        TextKey::SaveSeasonEpisode => "Zapisz sezon i odcinek",
        TextKey::BulkEdit => "Edycja zbiorcza",
        TextKey::Scope => "Zakres",
        TextKey::AllItems => "Wszystkie pozycje",
        TextKey::CurrentPage => "Bieżąca strona",
        TextKey::ItemsNeedingReview => "Pozycje wymagające sprawdzenia",
        TextKey::KeepCurrentValue => "Zachowaj bieżącą wartość",
        TextKey::UndoLastChange => "Cofnij ostatnią zmianę",
        TextKey::Undo => "Cofnij",
        TextKey::ChangeUndone => "Zmiana została cofnięta",
        TextKey::MetadataConflicts => "Konflikty metadanych",
        TextKey::MetadataConflictsHelp => {
            "Dostawca zmienił pola zastąpione przez operatora. Sprawdź wartości przed publikacją."
        }
        TextKey::PreviousProviderValue => "Poprzednia wartość dostawcy",
        TextKey::NewProviderValue => "Nowa wartość dostawcy",
        TextKey::OperatorValue => "Wartość operatora",
        TextKey::Discovered => "Wykryto",
        TextKey::EditMetadata => "Edytuj metadane",
        TextKey::OneTagPerLine => "Jeden tag w wierszu",
        TextKey::OriginalTitle => "Tytuł oryginalny",
        TextKey::ReleaseDate => "Data wydania",
        TextKey::Year => "Rok",
        TextKey::SortTitle => "Tytuł sortowania",
        TextKey::MovieFilesAndExtras => "Pliki filmu i dodatki",
        TextKey::MovieFilesAndExtrasHelp => "Przypisz dokładnie jeden film główny. Kolejność źródłowa nie określa roli, więc zwiastun może być pierwszy.",
        TextKey::EpisodesMetadataHelp => "Sprawdź tożsamość i numerację odcinków; kolejność dostawcy pozostaje bez zmian.",
        TextKey::MusicVideosMetadataHelp => "Wykonawca, album i utwór należą do każdego wideo; listy wielu wykonawców pozostają prawidłowe.",
        TextKey::OtherVideosMetadataHelp => "Użyj tego typu dla niezależnych wideo, a nie jednego filmu z dodatkami.",
        TextKey::SongsMetadataHelp => "Utwór, płyta, wykonawca i album są edytowane dla każdej piosenki; kolejność listy nie zastępuje numeru utworu.",
        TextKey::ImagesMetadataHelp => "Podpis i autor są edytowalne; format, wymiary i tożsamość źródła pozostają danymi faktycznymi.",
        TextKey::RemoteStreamsMetadataHelp => "Edytuj tu metadane prezentacji; łączność i protokół pozostają faktami źródła dostawcy.",
        TextKey::CastAndCredits => "Obsada i twórcy",
        TextKey::OneCreditPerLine => "Jedna osoba w wierszu: nazwa | rola",
        TextKey::Poster => "Plakat",
        TextKey::Artwork => "Grafiki",
        TextKey::PublishMetadata => "Opublikuj",
        TextKey::IncludeItem => "Uwzględnij",
        TextKey::IncludeInPublication => "Uwzględnij w publikacji",
        TextKey::MetadataDraftNotice => "Sprawdź metadane i uwzględnione pozycje przed opublikowaniem tej listy.",
        TextKey::ApplyToAllItemsPrompt => "Każda pozycja otrzyma wybrany rodzaj multimediów, zastępując wartości ustalone przez dodatek lub wprowadzone ręcznie.",
        TextKey::TelevisionEpisode => "Odcinek telewizyjny",
        TextKey::CollectionTypeAuto => "Ustal na podstawie pozycji",
        TextKey::CurrentLibraryType => "Bieżący typ biblioteki",
        TextKey::CollectionTypeChangeHelp => "Możesz to zmienić po imporcie. Zapisz i opublikuj, aby przeklasyfikować istniejące pozycje bez ponownego importowania dodatku; kolejność, wybór pozycji i zmiany metadanych zostaną zachowane.",
        TextKey::NonPlayableItems => "Pozycje nieodtwarzalne",
        TextKey::UnknownItems => "Pozycje nieznane",
        TextKey::PlayableItems => "Pozycje odtwarzalne",
        TextKey::ApplyToAllItems => "Zastosuj do wszystkich pozycji",
        TextKey::ChangeAddon => "Zmień dodatek",
        TextKey::ChangeAddonHelp => "Wybierz inny dodatek, aby ponownie zaimportować listę przy jego użyciu.",
        TextKey::ChangeAddonPrompt => "Lista zostanie ponownie zaimportowana przy użyciu wybranego dodatku. Metadane pozycji, zamienniki i poprawki rozdziałów zostaną ponownie ustalone przez nowego dostawcę.",
        TextKey::AddonAutoDetect => "Ustal na podstawie adresu URL",
        TextKey::AddonAutoDetected => "Wybrany automatycznie na podstawie adresu URL.",
        TextKey::AddonMediaListNoAddonForUrl => "Żaden zainstalowany dodatek nie obsługuje tego adresu URL. Wybierz dodatek lub ustaw domyślny procesor strumieni zdalnych.",
        TextKey::DefaultRemoteStreamAddon => "Domyślny procesor strumieni zdalnych",
        TextKey::DefaultRemoteStreamAddonHelp => "Używany dla źródeł HTTP i HTTPS, do których nie zgłasza się żaden zainstalowany dodatek. Dodatek nadal musi mieć prawo łączenia się z tym hostem.",
        TextKey::AddonDetailWait => "Tempo pobierania szczegółów elementów",
        TextKey::MinimumDetailWait => "Minimalny czas oczekiwania na szczegóły",
        TextKey::MaximumDetailWait => "Maksymalny czas oczekiwania na szczegóły",
        TextKey::AddonDetailWaitHelp => "Po każdej próbie pobrania szczegółów elementu ErsatzRS czeka nowo wylosowaną liczbę sekund z tego globalnego zakresu, zanim jakakolwiek lista będzie mogła ponownie użyć tego samego hosta dostawcy.",
        TextKey::AddonDetailWaitInvalid => "Wprowadź dodatnią całkowitą liczbę sekund i upewnij się, że minimum nie przekracza maksimum.",
        TextKey::Addons => "Dodatki",
        TextKey::AddonOverview => "Przegląd",
        TextKey::AddonUpdates => "Aktualizacje",
        TextKey::AddonSources => "Źródła",
        TextKey::AddonNotFound => "Nie znaleziono dodatku",
        TextKey::BackToAddons => "Wróć do dodatków",
        TextKey::AvailableVersion => "Dostępna wersja",
        TextKey::BrowseAddons => "Przeglądaj dodatki",
        TextKey::CheckForUpdates => "Sprawdź aktualizacje",
        TextKey::InstallFromFile => "Zainstaluj z pliku",
        TextKey::InstallRepositoryFromFile => "Zainstaluj repozytorium z pliku",
        TextKey::License => "Licencja",
        TextKey::UnknownSources => "Nieznane źródła",
        TextKey::UnknownSourcesHelp => "Zezwala na dodatki z lokalnych checkoutów Git i plików ZIP. Dodatki działają z uprawnieniami użytkownika usługi ErsatzRS.",
        TextKey::Verified => "Zweryfikowany",
        TextKey::Unverified => "Niezweryfikowany",
        TextKey::Sideloaded => "Zainstalowany lokalnie",
        TextKey::GitCheckout => "Checkout Git",
        TextKey::OfflineBundle => "Pakiet offline",
        TextKey::LegacyUnmanaged => "Starszy katalog · niezarządzany",
        TextKey::LegacyAdoptionHelp => "Spowoduje to zastąpienie niezarządzanej instalacji katalogowej wybranym źródłem lokalnym z zachowaniem ustawień i danych.",
        TextKey::Enabled => "Włączony",
        TextKey::Disabled => "Wyłączony",
        TextKey::Source => "Źródło",
        TextKey::ViewSourceCode => "Zobacz kod źródłowy",
        TextKey::Version => "Wersja",
        TextKey::NoAddonsMatch => "Żaden dodatek nie pasuje do tych filtrów.",
        TextKey::AddonsUpToDate => "Wszystko jest aktualne.",
        TextKey::AddonRepositories => "Repozytoria dodatków",
        TextKey::AvailableAddons => "Dostępne dodatki",
        TextKey::InstalledAddons => "Zainstalowane dodatki",
        TextKey::AddRepository => "Dodaj repozytorium",
        TextKey::LastRefresh => "Ostatnie skanowanie",
        TextKey::NeverRefreshed => "Nigdy",
        TextKey::ChooseFile => "Wybierz plik…",
        TextKey::NoFileSelected => "Nie wybrano pliku",
        TextKey::EditRepository => "Edytuj repozytorium",
        TextKey::OfficialRepositoryProtected => "Oficjalnego repozytorium nie można usunąć.",
        TextKey::UnknownSourcesRequired => "Najpierw włącz nieznane źródła.",
        TextKey::NoRepositories => "Nie znaleziono lokalnych repozytoriów. Sklonuj repozytorium do powyższego folderu checkoutu, a następnie odśwież.",
        TextKey::AddonPackage => "Pakiet dodatku",
        TextKey::RepositoryPackage => "Pakiet repozytorium",
        TextKey::NoSigningKeyPinned => "Nie przypięto publicznego klucza podpisu.",
        TextKey::RepositoryUrl => "Adres URL repozytorium",
        TextKey::PublicSigningKey => "Publiczny klucz podpisu",
        TextKey::DeploymentManagedRepositories => {
            "Repozytoria są zarządzane przez konfigurację wdrożenia."
        }
        TextKey::Refresh => "Odśwież",
        TextKey::Install => "Zainstaluj",
        TextKey::UpdateAddon => "Zaktualizuj",
        TextKey::Uninstall => "Odinstaluj",
        TextKey::Configuration => "Konfiguracja",
        TextKey::SecretReference => "Odwołanie do sekretu",
        TextKey::EnvironmentVariable => "Zmienna środowiskowa",
        TextKey::SecretFile => "Plik sekretu",
        TextKey::Incompatible => "Niezgodny",
        TextKey::PackageUnavailable => "Pakiet niedostępny",
        TextKey::Official => "Oficjalne",
        TextKey::ThirdParty => "Repozytorium zewnętrzne",
        TextKey::Permissions => "Uprawnienia",
        TextKey::Capabilities => "Możliwości",
        TextKey::Dependencies => "Zależności",
        TextKey::Optional => "Opcjonalne",
        TextKey::MediaReadAccess => "Dostęp do odczytu multimediów",
        TextKey::ExternalCommands => "Polecenia zewnętrzne",
        TextKey::ThirdPartyRepositoryWarning => {
            "Wczytuj tylko zaufane lokalne repozytoria i pliki ZIP; dodatki działają jako konto usługi ErsatzRS."
        }
        TextKey::AddCustomResolution => "Dodaj niestandardową rozdzielczość",
        TextKey::AddBlock => "Dodaj blok",
        TextKey::AddBlockGroup => "Dodaj grupę bloków",
        TextKey::AddBlockItem => "Dodaj element bloku",
        TextKey::AddBlockToTemplate => "Dodaj blok do szablonu",
        TextKey::AddCollection => "Dodaj kolekcję",
        TextKey::AddContent => "Dodaj zawartość",
        TextKey::AddFailed => "Dodawanie nie powiodło się",
        TextKey::AddMultiCollection => "Dodaj multi-kolekcję",
        TextKey::AddPlaylist => "Dodaj playlistę",
        TextKey::AddPlaylistGroup => "Dodaj grupę playlist",
        TextKey::AddPlaylistItem => "Dodaj element playlisty",
        TextKey::AddRerunCollection => "Dodaj kolekcję powtórek",
        TextKey::AddTemplate => "Dodaj szablon",
        TextKey::AddTemplateGroup => "Dodaj grupę szablonów",
        TextKey::AddAddonMediaList => "Dodaj listę add-on",
        TextKey::AddToCollection => "Dodaj do kolekcji",
        TextKey::AddToEllipsis => "Dodaj do ...",
        TextKey::AddToPlaylist => "Dodaj do playlisty",
        TextKey::AddToSchedule => "Dodaj do harmonogramu",
        TextKey::Actions => "Akcje",
        TextKey::All => "Wszystkie",
        TextKey::AreYouSureDelete => "Czy na pewno usunąć",
        TextKey::Actors => "Obsada",
        TextKey::Artist => "Artysta",
        TextKey::ArtistNotFound => "Nie znaleziono artysty",
        TextKey::AfterDurationEnd => "Po końcu czasu trwania",
        TextKey::AutomaticRefresh => "Automatyczne odświeżanie",
        TextKey::AdaptiveRefreshUnavailable => "Odświeżanie adaptacyjne jest niedostępne, ponieważ ten dodatek nie obsługuje już sprawdzania źródła. Wybierz inną częstotliwość, aby wznowić automatyczne odświeżanie.",
        TextKey::RefreshCadenceAdaptive => "Adaptacyjnie",
        TextKey::RefreshCadenceDaily => "Codziennie",
        TextKey::RefreshCadenceWeekly => "Co tydzień",
        TextKey::RefreshCadenceMonthly => "Co miesiąc",
        TextKey::RefreshCadenceYearly => "Co rok",
        TextKey::RefreshCadenceNever => "Nigdy",
        TextKey::LastSuccessfulRefresh => "Ostatnie udane odświeżenie",
        TextKey::NextScheduledRefresh => "Następne zaplanowane odświeżenie",
        TextKey::RefreshRetryAt => "Ponowna próba możliwa od",
        TextKey::Back => "Wstecz",
        TextKey::BackToSettings => "Wróć do ustawień",
        TextKey::BackToArtists => "Wróć do artystów",
        TextKey::BackToCollections => "Wróć do kolekcji",
        TextKey::BackToMovies => "Wróć do filmów",
        TextKey::BackToOtherVideos => "Wróć do innych wideo",
        TextKey::BackToTvShows => "Wróć do seriali",
        TextKey::BeforeDurationEnd => "Przed końcem czasu trwania",
        TextKey::BackToTemplates => "Wróć do szablonów",
        TextKey::Block => "Blok",
        TextKey::BlockGroup => "Grupa bloków",
        TextKey::BlockCannotBeMoved => {
            "Nie można przenieść bloku w to miejsce, ponieważ nakładałby się albo przekraczał północ."
        },
        TextKey::BlockCopied => "Blok został skopiowany.",
        TextKey::BlockCreated => "Blok został utworzony.",
        TextKey::BlockDeleted => "Blok został usunięty.",
        TextKey::BlockGroupCreated => "Grupa bloków została utworzona.",
        TextKey::BlockGroupDeleted => "Grupa bloków została usunięta.",
        TextKey::BlockGroupName => "Nazwa grupy bloków",
        TextKey::BlockGroups => "Grupy bloków",
        TextKey::BlockItem => "Element bloku",
        TextKey::BlockItems => "Elementy bloku",
        TextKey::BlockName => "Nazwa bloku",
        TextKey::BlockNotFound => "Nie znaleziono bloku.",
        TextKey::BlockPreviewFailed => "Nie udało się wygenerować podglądu bloku. Sprawdź elementy bloku i spróbuj ponownie.",
        TextKey::Blocks => "Bloki",
        TextKey::BlockToRemove => "Blok do usunięcia",
        TextKey::BlockWillNotFit => {
            "Blok nie zmieści się o wybranym czasie rozpoczęcia ani później."
        },
        TextKey::Cancel => "Anuluj",
        TextKey::Clear => "Wyczyść",
        TextKey::Available => "Dostępne",
        TextKey::UnknownAvailability => "Nieznane",
        TextKey::LocalReplacement => "Lokalny zamiennik",
        TextKey::NoReplacement => "Brak zamiennika",
        TextKey::InvalidReplacement => "Nieprawidłowy zamiennik",
        TextKey::Channels => "Kanały",
        TextKey::Close => "Zamknij",
        TextKey::Collections => "Kolekcje",
        TextKey::CollectionDeleted => "Kolekcja została usunięta.",
        TextKey::CollectionSaved => "Kolekcja została zapisana.",
        TextKey::SaveCollection => "Zapisz kolekcję",
        TextKey::ComputingPreview => "Obliczanie podglądu...",
        TextKey::Confirm => "Potwierdź",
        TextKey::ConfirmDelete => "Potwierdź usunięcie",
        TextKey::CommentaryTracks => "Ścieżki komentarza",
        TextKey::Content => "Zawartość",
        TextKey::ContentVariantAcoustic => "Akustyczne",
        TextKey::ContentVariantAlternateVersion => "Wersja",
        TextKey::ContentVariantBehindTheScenes => "Za kulisami",
        TextKey::ContentVariantDeletedScene => "Scena usunięta",
        TextKey::ContentVariantDirectorsCut => "Wersja reżyserska",
        TextKey::ContentVariantExtended => "Rozszerzone",
        TextKey::ContentVariantExtra => "Dodatek",
        TextKey::ContentVariantFeaturette => "Featurette",
        TextKey::ContentVariantInterview => "Wywiad",
        TextKey::ContentVariantLive => "Na żywo",
        TextKey::ContentVariantOfficialVideo => "Oficjalne wideo",
        TextKey::ContentVariantOther => "Inne",
        TextKey::ContentVariantRemaster => "Remaster",
        TextKey::ContentVariantRemix => "Remiks",
        TextKey::ContentVariantScene => "Scena",
        TextKey::ContentVariantSearchHelp => {
            "Tokeny wariantów: content_kind:extra, content_kind:alternate_version, sub_kind:featurette, sub_kind:unplugged, parent_title:\"Tytuł filmu\", audio_title:commentary, subtitle_title:commentary, has_commentary:true"
        }
        TextKey::ContentVariantShort => "Krótki materiał",
        TextKey::ContentVariantSpecialFeatures => "Dodatki / materiały specjalne",
        TextKey::ContentVariantTrailer => "Zwiastun",
        TextKey::ContentVariantUnplugged => "Unplugged",
        TextKey::ContentVariantVersions => "Wersje",
        TextKey::ContentRatings => "Klasyfikacje treści",
        TextKey::CustomResolutionHeight => "Wysokość niestandardowej rozdzielczości",
        TextKey::CustomResolutionWidth => "Szerokość niestandardowej rozdzielczości",
        TextKey::CustomResolutions => "Niestandardowe rozdzielczości",
        TextKey::Dark => "Ciemny",
        TextKey::DaysToBuild => "Dni do zbudowania",
        TextKey::AddBreakContent => "Dodaj zawartość przerwy",
        TextKey::AddDeco => "Dodaj dekorację",
        TextKey::AddDecoGroup => "Dodaj grupę dekoracji",
        TextKey::AddDecoTemplate => "Dodaj szablon dekoracji",
        TextKey::AddDecoTemplateGroup => "Dodaj grupę szablonów dekoracji",
        TextKey::AtChapterMarkers => "Przy znacznikach rozdziałów",
        TextKey::BackToDecos => "Wróć do dekoracji",
        TextKey::BackToDecoTemplates => "Wróć do szablonów dekoracji",
        TextKey::BetweenBlockItems => "Między elementami bloku",
        TextKey::BlockFinish => "Koniec bloku",
        TextKey::BlockStart => "Początek bloku",
        TextKey::BreakContent => "Zawartość przerwy",
        TextKey::BreakContentBody => {
            "Zawartość przerwy może odtwarzać określone elementy (za pomocą playlist) i dopełniać bloki do pełnego skonfigurowanego czasu trwania."
        },
        TextKey::BreakContentItem => "Element zawartości przerwy",
        TextKey::BreakContentMode => "Tryb zawartości przerwy",
        TextKey::DeadAirFallback => "Awaryjne wypełnienie ciszy",
        TextKey::DeadAirFallbackBody => {
            "Gdy nie znaleziono elementu emisji dla bieżącego czasu, jeden element zostanie losowo wybrany z tej kolekcji, zapętlony i przycięty dokładnie do początku następnego elementu emisji. Zastępuje to obraz Kanał jest offline, który w innym przypadku byłby wyświetlany."
        },
        TextKey::DeadAirFallbackCollectionType => "Typ kolekcji awaryjnego wypełnienia ciszy",
        TextKey::DeadAirFallbackMode => "Tryb awaryjnego wypełnienia ciszy",
        TextKey::Deco => "Dekoracja",
        TextKey::DecoCannotBeAdded => {
            "Nie można dodać dekoracji, ponieważ nakładałaby się lub przekroczyłaby północ."
        },
        TextKey::DecoCannotBeMoved => {
            "Nie można przenieść dekoracji w to miejsce, ponieważ nakładałaby się lub przekroczyłaby północ."
        },
        TextKey::DecoCreated => "Dekoracja została utworzona.",
        TextKey::DecoDeleted => "Dekoracja została usunięta.",
        TextKey::DecoGroup => "Grupa dekoracji",
        TextKey::DecoGroupCreated => "Grupa dekoracji została utworzona.",
        TextKey::DecoGroupDeleted => "Grupa dekoracji została usunięta.",
        TextKey::DecoGroupName => "Nazwa grupy dekoracji",
        TextKey::DecoGroups => "Grupy dekoracji",
        TextKey::DecoName => "Nazwa dekoracji",
        TextKey::DecoNotFound => "Nie znaleziono dekoracji.",
        TextKey::DecoSaved => "Dekoracja została zapisana.",
        TextKey::Decos => "Dekoracje",
        TextKey::DecoTemplate => "Szablon dekoracji",
        TextKey::DecoTemplateCreated => "Szablon dekoracji został utworzony.",
        TextKey::DecoTemplateDeleted => "Szablon dekoracji został usunięty.",
        TextKey::DecoTemplateGroup => "Grupa szablonów dekoracji",
        TextKey::DecoTemplateGroupCreated => "Grupa szablonów dekoracji została utworzona.",
        TextKey::DecoTemplateGroupDeleted => "Grupa szablonów dekoracji została usunięta.",
        TextKey::DecoTemplateGroupName => "Nazwa grupy szablonów dekoracji",
        TextKey::DecoTemplateGroups => "Grupy szablonów dekoracji",
        TextKey::DecoTemplateName => "Nazwa szablonu dekoracji",
        TextKey::DecoTemplateNotFound => "Nie znaleziono szablonu dekoracji.",
        TextKey::DecoTemplateSaved => "Szablon dekoracji został zapisany.",
        TextKey::AddDecoToTemplate => "Dodaj dekorację do szablonu",
        TextKey::DecoToRemove => "Dekoracja do usunięcia",
        TextKey::DefaultFiller => "Domyślne wypełnienie",
        TextKey::DefaultFillerBody => {
            "Po zaplanowaniu wszystkich bloków drugi przebieg wypełni niezaplanowany czas losowymi elementami z tej kolekcji."
        },
        TextKey::DefaultFillerCollectionType => "Typ kolekcji domyślnego wypełnienia",
        TextKey::DefaultFillerMode => "Tryb domyślnego wypełnienia",
        TextKey::DeleteDeco => "Usuń dekorację",
        TextKey::DeleteDecoGroup => "Usuń grupę dekoracji",
        TextKey::DeleteDecoTemplate => "Usuń szablon dekoracji",
        TextKey::DeleteDecoTemplateGroup => "Usuń grupę szablonów dekoracji",
        TextKey::Disable => "Wyłącz",
        TextKey::EditDeco => "Edytuj dekorację",
        TextKey::EditDecoTemplate => "Edytuj szablon dekoracji",
        TextKey::GraphicsElementsMode => "Tryb elementów graficznych",
        TextKey::Inherit => "Dziedzicz",
        TextKey::Merge => "Scal",
        TextKey::NoDecosFound => "Nie znaleziono dekoracji.",
        TextKey::NoDecoTemplatesFound => "Nie znaleziono szablonów dekoracji.",
        TextKey::Override => "Nadpisz",
        TextKey::RemoveBreakContent => "Usuń zawartość przerwy",
        TextKey::RemoveDecoFromTemplate => "Usuń dekorację z szablonu",
        TextKey::Replace => "Zastąp",
        TextKey::SavedDecos => "Zapisane dekoracje",
        TextKey::SavedDecoTemplates => "Zapisane szablony dekoracji",
        TextKey::SearchForDecos => "Szukaj dekoracji",
        TextKey::SearchForDecoTemplates => "Szukaj szablonów dekoracji",
        TextKey::TrimToFit => "Przytnij do dopasowania",
        TextKey::UseGraphicsElementsDuringFiller => "Używaj elementów graficznych podczas wypełnienia",
        TextKey::UseWatermarkDuringFiller => "Używaj znaku wodnego podczas wypełnienia",
        TextKey::WatermarkMode => "Tryb znaku wodnego",
        TextKey::DecoTemplates => "Szablony dekoracji",
        TextKey::DefaultFfmpegProfile => "Domyślny profil FFmpeg",
        TextKey::DefaultFfmpegProfileHelp => "Profil FFmpeg używany podczas tworzenia nowych kanałów",
        TextKey::DefaultMinimumLogLevel => "Domyślny minimalny poziom logów",
        TextKey::DefaultMpegTsScript => "Domyślny skrypt MPEG-TS",
        TextKey::DefaultMpegTsScriptHelp => "Skrypt MPEG-TS używany podczas strumieniowania",
        TextKey::Decorations => "Dekoracje",
        TextKey::Debug => "Debug",
        TextKey::Delete => "Usuń",
        TextKey::DeleteAll => "Usuń wszystko",
        TextKey::DeleteBlock => "Usuń blok",
        TextKey::DeleteBlockGroup => "Usuń grupę bloków",
        TextKey::AddFillerPreset => "Dodaj szablon wypełniacza",
        TextKey::DeleteCustomResolution => "Usuń niestandardową rozdzielczość",
        TextKey::DeleteCollection => "Usuń kolekcję",
        TextKey::DeleteFillerPreset => "Usuń szablon wypełniacza",
        TextKey::DeleteMultiCollection => "Usuń multi-kolekcję",
        TextKey::DeletePlaylist => "Usuń playlistę",
        TextKey::DeletePlaylistGroup => "Usuń grupę playlist",
        TextKey::DeleteRerunCollection => "Usuń kolekcję powtórek",
        TextKey::DeleteTemplate => "Usuń szablon",
        TextKey::DeleteTemplateGroup => "Usuń grupę szablonów",
        TextKey::DeleteAddonMediaList => "Usuń listę add-on",
        TextKey::DisabledCollectionsAlreadyPresent => {
            "Wyłączone kolekcje są już obecne w tej multi-kolekcji"
        },
        TextKey::EmptyTrash => "Opróżnij kosz",
        TextKey::ExtractEmbeddedSubtitles => "Wyodrębnij osadzone napisy tekstowe",
        TextKey::ErasedPlayoutItemsAndHistory => "Usunięto elementy emisji i historię.",
        TextKey::EraseFailed => "Usuwanie nie powiodło się",
        TextKey::EraseItemsAndHistory => "Usuń elementy i historię",
        TextKey::ErasePlayoutItemsAndHistoryConfirm => {
            "Spowoduje to usunięcie wszystkich elementów emisji i historii dla tej emisji sekwencyjnej. Kontynuować?"
        },
        TextKey::Episode => "Odcinek",
        TextKey::FanArt => "fanart",
        TextKey::Ffmpeg => "FFmpeg",
        TextKey::FileNotFound => "Nie znaleziono pliku",
        TextKey::Finish => "Koniec",
        TextKey::Fatal => "Krytyczny",
        TextKey::FfmpegPath => "Ścieżka FFmpeg",
        TextKey::FfmpegPathHelp => "Pełna ścieżka do pliku wykonywalnego ffmpeg",
        TextKey::FfmpegProfiles => "Profile FFmpeg",
        TextKey::FfprobePath => "Ścieżka FFprobe",
        TextKey::FfprobePathHelp => "Pełna ścieżka do pliku wykonywalnego ffprobe",
        TextKey::FillerPreset => "Szablon wypełniacza",
        TextKey::FillerPresetActionFailed => "Akcja szablonu wypełniacza nie powiodła się.",
        TextKey::FillerPresetCouldNotBeSaved => {
            "Nie można zapisać szablonu wypełniacza. Sprawdź wartości formularza i spróbuj ponownie."
        },
        TextKey::FillerPresetDeleted => "Szablon wypełniacza został usunięty.",
        TextKey::FillerPresetExpressionHelp => {
            "Dla wypełniacza śródemisji dodawaj wypełniacz tylko wtedy, gdy to wyrażenie jest prawdziwe dla danego punktu śródemisji."
        },
        TextKey::FillerPresetSaved => "Szablon wypełniacza został zapisany.",
        TextKey::FillerPresets => "Szablony wypełniaczy",
        TextKey::Flags => "Flagi",
        TextKey::GlobalFallbackFiller => "Globalny wypełniacz awaryjny",
        TextKey::GlobalWatermark => "Globalny znak wodny",
        TextKey::GoHome => "Strona główna",
        TextKey::HdHomeRun => "HDHomeRun",
        TextKey::HealthChecks => "Diagnostyka",
        TextKey::Height => "Wysokość",
        TextKey::HlsDirectOutputFormat => "Bezpośredni format wyjściowy HLS",
        TextKey::HlsSegmenterIdleTimeout => "Limit bezczynności segmentera HLS",
        TextKey::HlsSegmenterIdleTimeoutHelp => "Liczba sekund dalszego transkodowania kanału, gdy żaden klient nie wysyła żądań",
        TextKey::HlsSegmenterInitialSegmentCount => "Początkowa liczba segmentów HLS",
        TextKey::HlsSegmenterInitialSegmentCountHelp => "Opóźnia start strumienia do czasu transkodowania podanej liczby 4-sekundowych segmentów; większa liczba oznacza wolniejszy start, ale może ograniczyć buforowanie",
        TextKey::HomeRunning => "Serwer IPTV działa.",
        TextKey::HttpMinimumLogLevel => "Minimalny poziom logów żądań",
        TextKey::Id => "Id",
        TextKey::Images => "Obrazy",
        TextKey::Item => "Element",
        TextKey::ItemActions => "Akcje elementu",
        TextKey::ItemType => "Typ elementu",
        TextKey::Items => "Elementy",
        TextKey::Language => "Język",
        TextKey::Languages => "Języki",
            TextKey::MusicLabels => "Wytwórnie muzyczne",
        TextKey::LeaveEmptyToAutoDetect => "Pozostaw puste, aby wykryć automatycznie",
        TextKey::Libraries => "Biblioteki",
        TextKey::LibraryRefreshInterval => "Interwał odświeżania biblioteki",
        TextKey::Light => "Jasny",
        TextKey::Lists => "Listy",
        TextKey::Loading => "Ładowanie...",
        TextKey::LoadingMembers => "Ładowanie elementów...",
        TextKey::Local => "Lokalne",
        TextKey::Logging => "Dzienniki zdarzeń",
        TextKey::M3u => "M3U",
        TextKey::Media => "Multimedia",
        TextKey::MediaInfo => "Informacje o multimediach",
        TextKey::MediaSources => "Źródła multimediów",
        TextKey::Movie => "Film",
        TextKey::MovieNotFound => "Nie znaleziono filmu",
        TextKey::Movies => "Filmy",
        TextKey::MusicVideo => "Teledysk",
        TextKey::MultiCollections => "Multikolekcje",
        TextKey::ManualCollections => "Kolekcje ręczne",
        TextKey::MatchFailed => "Dopasowanie nie powiodło się",
        TextKey::MatchStatus => "Status dopasowania",
        TextKey::MatchAddonMediaListItems => "Dopasuj elementy listy add-on",
        TextKey::Members => "Elementy",
        TextKey::MultiCollection => "Multi-kolekcja",
        TextKey::MultiCollectionDeleted => "Multi-kolekcja została usunięta.",
        TextKey::MultiCollectionItems => "Elementy multi-kolekcji",
        TextKey::MultiCollectionNameRequired => "Nazwa multi-kolekcji jest wymagana!",
        TextKey::MultiCollectionNameTooLong => {
            "Nazwa multi-kolekcji nie może być dłuższa niż 50 znaków."
        },
        TextKey::MultiCollectionSaved => "Multi-kolekcja została zapisana.",
        TextKey::Music => "Muzyka",
        TextKey::Name => "Nazwa",
        TextKey::NameRequired => "Nazwa jest wymagana",
        TextKey::Networks => "Sieci",
        TextKey::NewCollection => "(Nowa kolekcja)",
        TextKey::NewCollectionTitle => "Nowa kolekcja",
        TextKey::NewFillerPreset => "Nowy szablon wypełniacza",
        TextKey::NewMultiCollection => "Nowa multi-kolekcja",
        TextKey::NewTemplateGroup => "Nowa grupa szablonów",
        TextKey::NewTemplateName => "Nowa nazwa szablonu",
        TextKey::Next24Hours => "Następne 24 godz.",
        TextKey::NextPage => "Następna strona",
        TextKey::NoFillerPresets => "Brak szablonów wypełniaczy.",
        TextKey::NoBlockItems => "Brak elementów bloku.",
        TextKey::NoItems => "Brak elementów.",
        TextKey::NoItemsToDisplay => "Brak elementów do wyświetlenia.",
        TextKey::NoAddonMediaListsYet => "Brak list add-on.",
        TextKey::No => "Nie",
        TextKey::NoCollections => "Brak kolekcji.",
        TextKey::NoMembers => "Brak elementów.",
        TextKey::NoMultiCollections => "Brak multi-kolekcji.",
        TextKey::NoTemplatesFound => "Nie znaleziono szablonów.",
        TextKey::NotFound => "Nie znaleziono strony.",
        TextKey::OtherVideo => "Inne wideo",
        TextKey::OtherVideoNotFound => "Nie znaleziono innego wideo",
        TextKey::OtherVideos => "Inne wideo",
        TextKey::OpenBrowser => "Otwórz przeglądarkę",
        TextKey::Playout => "Emisja",
        TextKey::PlayoutItemsAndHistory => "Elementy emisji i historia",
        TextKey::PlayoutNotFound => "Nie znaleziono emisji.",
        TextKey::Playouts => "Plany emisji",
        TextKey::AddPlayout => "Dodaj plan emisji",
        TextKey::ClassicScheduleMenu => "Harmonogram klasyczny",
        TextKey::BlockScheduleMenu => "Harmonogram blokowy",
        TextKey::SequentialScheduleMenu => "Harmonogram sekwencyjny",
        TextKey::ScriptedScheduleMenu => "Harmonogram skryptowy",
        TextKey::JsonDizqueTvScheduleMenu => "Harmonogram JSON (dizqueTV)",
        TextKey::ResetAllPlayouts => "Odbuduj wszystkie plany emisji",
        TextKey::PlayoutBuildQueued => "Odbudowę planu emisji dodano do kolejki",
        TextKey::PlayoutBuildsQueued => "odbudów planów emisji dodano do kolejki",
        TextKey::SearchForPlayouts => "Szukaj planów emisji",
        TextKey::PlayoutDetail => "Szczegóły planu emisji",
        TextKey::ShowFiller => "Pokaż wypełniacze",
        TextKey::EditPlayout => "Edytuj plan emisji",
        TextKey::RefreshPlayout => "Odśwież plan emisji",
        TextKey::ResetPlayout => "Odbuduj plan emisji",
        TextKey::ScheduleReset => "Zaplanuj odbudowę",
        TextKey::SchedulePlayoutReset => "Zaplanuj odbudowę planu emisji",
        TextKey::EditExternalJsonFile => "Edytuj zewnętrzny plik JSON",
        TextKey::DeletePlayout => "Usuń plan emisji",
        TextKey::Troubleshoot => "Diagnostyka",
        TextKey::DailyResetTime => "Godzina codziennej odbudowy",
        TextKey::DoNotAutomaticallyReset => "Nie odbudowuj automatycznie",
        TextKey::SaveChanges => "Zapisz zmiany",
        TextKey::ExternalJsonFileLabel => "Zewnętrzny plik JSON",
        TextKey::EditExternalJsonFilePrompt => "Edytuj zewnętrzny plik JSON planu emisji",
        TextKey::UnscheduledPlayoutItem => "NIEZAPLANOWANE",
        TextKey::DisabledChannelsAlreadyHavePlayout => {
            "Wyłączone kanały mają już plan emisji"
        }
        TextKey::ProgramSchedule => "Harmonogram programu",
        TextKey::SequentialScheduleFileHelp => {
            "Pełna ścieżka do pliku harmonogramu sekwencyjnego (YAML)"
        }
        TextKey::BlockTemplatesAddedLater => "Szablony bloków zostaną dodane później",
        TextKey::UnsupportedPlayoutKind => "Nieobsługiwany typ planu emisji.",
        TextKey::ClassicPlayout => "Klasyczna emisja",
        TextKey::ClassicPlayoutState => "Stan klasycznej emisji",
        TextKey::ScanRefreshState => "Stan odświeżania po skanowaniu",
        TextKey::ArtworkCardinality => "Krotność grafik",
        TextKey::HlsLiveDelivery => "Dostarczanie HLS na żywo",
        TextKey::BlockPlayout => "Emisja blokowa",
        TextKey::ScriptedPlayout => "Harmonogram skryptowy",
        TextKey::AlternateSchedules => "Harmonogramy alternatywne",
        TextKey::EditAlternateSchedules => "Edytuj harmonogramy alternatywne",
        TextKey::PlayoutTemplates => "Szablony emisji",
        TextKey::EditTemplates => "Edytuj szablony",
        TextKey::DefaultDeco => "Domyślne deco",
        TextKey::EnableDefaultDeco => "Włącz domyślne deco",
        TextKey::SaveDefaultDeco => "Zapisz domyślne deco",
        TextKey::SaveScriptedSchedule => "Zapisz harmonogram skryptowy",
        TextKey::EraseItems => "Usuń elementy",
        TextKey::ErasedPlayoutItems => "Usunięto elementy emisji.",
        TextKey::SavedDefaultDeco => "Zapisano domyślne deco.",
        TextKey::SavedScriptedSchedule => "Zapisano harmonogram skryptowy.",
        TextKey::Maintenance => "Konserwacja",
        TextKey::SaveAlternateSchedules => "Zapisz harmonogramy alternatywne",
        TextKey::AddAlternateSchedule => "Dodaj harmonogram alternatywny",
        TextKey::SaveTemplates => "Zapisz szablony",
        TextKey::PreviewCalendar => "Podgląd kalendarza",
        TextKey::EditContent => "Edytuj zawartość",
        TextKey::PlayoutNestedCatchAllHelp => {
            "W kolejności priorytetu od góry do dołu. Dolny wpis zawsze pasuje do wszystkich dni i miesięcy jako wpis domyślny."
        }
        TextKey::PlayoutNestedPriorityHelp => "W kolejności priorytetu od góry do dołu.",
        TextKey::DaysOfWeek => "Dni tygodnia",
        TextKey::DaysOfMonth => "Dni miesiąca",
        TextKey::Months => "Miesiące",
        TextKey::LimitToDateRange => "Ogranicz do zakresu dat",
        TextKey::StartDay => "Dzień początkowy",
        TextKey::EndDay => "Dzień końcowy",
        TextKey::Enable => "Włącz",
        TextKey::Up => "W górę",
        TextKey::Down => "W dół",
        TextKey::Weekdays => "Dni robocze",
        TextKey::Weekends => "Weekendy",
        TextKey::Sunday => "Niedziela",
        TextKey::Monday => "Poniedziałek",
        TextKey::Tuesday => "Wtorek",
        TextKey::Wednesday => "Środa",
        TextKey::Thursday => "Czwartek",
        TextKey::Friday => "Piątek",
        TextKey::Saturday => "Sobota",
        TextKey::January => "Styczeń",
        TextKey::February => "Luty",
        TextKey::March => "Marzec",
        TextKey::April => "Kwiecień",
        TextKey::May => "Maj",
        TextKey::June => "Czerwiec",
        TextKey::July => "Lipiec",
        TextKey::August => "Sierpień",
        TextKey::September => "Wrzesień",
        TextKey::October => "Październik",
        TextKey::November => "Listopad",
        TextKey::December => "Grudzień",
        TextKey::JanuaryShort => "Sty",
        TextKey::FebruaryShort => "Lut",
        TextKey::MarchShort => "Mar",
        TextKey::AprilShort => "Kwi",
        TextKey::MayShort => "Maj",
        TextKey::JuneShort => "Cze",
        TextKey::JulyShort => "Lip",
        TextKey::AugustShort => "Sie",
        TextKey::SeptemberShort => "Wrz",
        TextKey::OctoberShort => "Paź",
        TextKey::NovemberShort => "Lis",
        TextKey::DecemberShort => "Gru",
        TextKey::PlayAll => "Odtwórz wszystko",
        TextKey::Playlist => "Playlista",
        TextKey::PlaylistAdded => "Playlista została dodana.",
        TextKey::PlaylistDeleted => "Playlista została usunięta.",
        TextKey::PlaylistGroup => "Grupa playlist",
        TextKey::PlaylistGroupAdded => "Grupa playlist została dodana.",
        TextKey::PlaylistGroupDeleted => "Grupa playlist została usunięta.",
        TextKey::PlaylistGroupName => "Nazwa grupy playlist",
        TextKey::PlaylistGroups => "Grupy playlist",
        TextKey::PlaylistItem => "Element playlisty",
        TextKey::PlaylistItems => "Elementy playlisty",
        TextKey::PlaylistName => "Nazwa playlisty",
        TextKey::PlaylistNotFound => "Nie znaleziono playlisty.",
        TextKey::PlaylistPlayoutPreview => "Podgląd emisji playlisty",
        TextKey::PlaylistPreviewEmpty => "Podgląd emisji playlisty nie ma elementów",
        TextKey::Playlists => "Playlisty",
        TextKey::ProbeForInterlacedFrames => "Sprawdzaj klatki z przeplotem",
        TextKey::PreferredAudioLanguage => "Preferowany język audio",
        TextKey::DefaultSubtitleLanguage => "Domyślny język napisów",
        TextKey::FallbackSubtitleLanguage => "Zapasowy język napisów",
        TextKey::OriginalLanguageOption => "Oryginalny (und)",
        TextKey::UndefinedLanguageOption => "Niezdefiniowany (und)",
        TextKey::Previous24Hours => "Poprzednie 24 godz.",
        TextKey::PreviousPage => "Poprzednia strona",
        TextKey::RerunCollections => "Kolekcje powtórek",
        TextKey::RerunCollection => "Kolekcja powtórek",
        TextKey::RerunCollectionActionFailed => "Akcja kolekcji powtórek nie powiodła się.",
        TextKey::RerunCollectionDeleted => "Kolekcja powtórek została usunięta.",
        TextKey::RerunCollectionNameRequired => "Nazwa kolekcji powtórek jest wymagana!",
        TextKey::RerunCollectionSaved => "Kolekcja powtórek została zapisana.",
        TextKey::RerunPlaybackOrder => "Kolejność odtwarzania powtórek",
        TextKey::RemoteStreams => "Zdalne strumienie",
        TextKey::RemoteStream => "Zdalny strumień",
        TextKey::PublishedOptions => "Opublikowane opcje",
        TextKey::PlaybackHealth => "Stan odtwarzania",
        TextKey::Configure => "Konfiguruj",
        TextKey::Watch => "Oglądaj",
        TextKey::RemoteStreamPlaylists => "Playlisty zdalnych strumieni",
        TextKey::PlaylistUrl => "Adres URL playlisty",
        TextKey::Extractor => "Eksadd-onor",
        TextKey::DestinationFolder => "Folder docelowy",
        TextKey::SyncInterval => "Interwał synchronizacji",
        TextKey::MaxItems => "Maksymalna liczba elementów",
        TextKey::RemovedEntries => "Usunięte elementy",
        TextKey::CreateOrderedPlaylist => "Utwórz uporządkowaną playlistę",
        TextKey::SyncNow => "Synchronizuj teraz",
        TextKey::LastSync => "Ostatnia synchronizacja",
        TextKey::NeverSynced => "Nigdy nie synchronizowano",
        TextKey::ManagedManifestReadOnly => "Ten ręcznie utworzony manifest jest tutaj tylko do odczytu.",
        TextKey::Truncated => "Skrócono",
        TextKey::MissingDuration => "Brak czasu trwania",
        TextKey::Orphaned => "Osierocony",
        TextKey::Materialized => "Utworzony",
        TextKey::State => "Stan",
        TextKey::Upstream => "Kolejność źródłowa",
        TextKey::NewestFirst => "Najnowsze najpierw",
        TextKey::Keep => "Zachowaj",
        TextKey::ResetChannels => "Resetuj kanały",
        TextKey::RowsPerPage => "Wierszy na stronę",
        TextKey::Save => "Zapisz",
        TextKey::SaveMultiCollection => "Zapisz multi-kolekcję",
        TextKey::SavePlaylist => "Zapisz playlistę",
        TextKey::SaveTroubleshootingReportsToDisk => "Zapisuj raporty diagnostyczne na dysku",
        TextKey::SaveAsSmartCollection => "Zapisz jako inteligentną kolekcję",
        TextKey::SearchQueryRequired => "Zapytanie wyszukiwania jest wymagane",
        TextKey::SaveSettings => "Zapisz ustawienia",
        TextKey::Scanner => "Skaner",
        TextKey::ScanningMinimumLogLevel => "Minimalny poziom logów skanowania",
        TextKey::Scheduling => "Planowanie",
        TextKey::SchedulingMinimumLogLevel => "Minimalny poziom logów planowania",
        TextKey::StartWithWindows => "Uruchamiaj ErsatzRS po zalogowaniu do systemu Windows",
        TextKey::StartWithWindowsHelp => {
            "Automatycznie uruchamiaj aplikację zasobnika dla bieżącego użytkownika systemu Windows."
        }
        TextKey::Schedules => "Harmonogramy",
        TextKey::AddSchedule => "Dodaj harmonogram",
        TextKey::AddScheduleItem => "Dodaj element harmonogramu",
        TextKey::AddSmartCollection => "Dodaj inteligentną kolekcję",
        TextKey::Album => "Album",
        TextKey::Chronological => "Chronologicznie",
        TextKey::CollectionSize => "Rozmiar kolekcji",
        TextKey::CollectionType => "Typ kolekcji",
        TextKey::Count => "Liczba",
        TextKey::CopySchedule => "Kopiuj harmonogram",
        TextKey::CopyScheduleItem => "Kopiuj element harmonogramu",
        TextKey::CustomTitle => "Tytuł niestandardowy",
        TextKey::DeleteSchedule => "Usuń harmonogram",
        TextKey::DeleteScheduleItem => "Usuń element harmonogramu",
        TextKey::DeleteSmartCollection => "Usuń inteligentną kolekcję",
        TextKey::DetailRowsPerPage => "Wiersze szczegółów na stronę",
        TextKey::Director => "Reżyser",
        TextKey::Directors => "Reżyserzy",
        TextKey::DiscardToFillAttempts => "Próby odrzucenia do wypełnienia",
        TextKey::Duration => "Czas trwania",
        TextKey::Dynamic => "Dynamiczny",
        TextKey::EditProperties => "Edytuj właściwości",
        TextKey::EditScheduleItems => "Edytuj elementy harmonogramu",
        TextKey::EditSmartCollection => "Edytuj inteligentną kolekcję",
        TextKey::EnterNameForNewSchedule => "Wprowadź nazwę nowego harmonogramu",
        TextKey::Fixed => "Stały",
        TextKey::FixedStartTimeBehavior => "Zachowanie stałego czasu rozpoczęcia",
        TextKey::Flood => "Flood",
        TextKey::Flexible => "Elastyczny",
        TextKey::FlexibleStartSoonAsPossible => "Elastycznie - rozpocznij jak najszybciej",
        TextKey::FlexibleStartSoonAsPossibleAfterStartTime => {
            "Elastycznie - rozpocznij jak najszybciej po czasie startu"
        },
        TextKey::FillWithGroupMode => "Tryb wypełniania grupami (serial lub wykonawca)",
        TextKey::GraphicsElements => "Elementy graficzne",
        TextKey::AddGraphicsElement => "Dodaj element graficzny",
        TextKey::DrawtextRendererOption => "Drawtext — dynamiczny/oparty na pliku",
        TextKey::DrawtextSettings => "Ustawienia Drawtext",
        TextKey::DrawvgRendererOption => "Drawvg — statyczny/wektorowy",
        TextKey::EditGraphicsElement => "Edytuj element graficzny",
        TextKey::FontFile => "Plik czcionki",
        TextKey::FontFileOptionalHelp => "Opcjonalne. Pozostaw puste, aby użyć dołączonej czcionki Roboto.",
        TextKey::GraphicsElement => "Element graficzny",
        TextKey::GraphicsElementConfigInvalid => "Ten tekstowy element graficzny ma nieprawidłową konfigurację i nie został zmieniony.",
        TextKey::GraphicsElementKindImage => "Obraz",
        TextKey::GraphicsElementKindMotion => "Ruch",
        TextKey::GraphicsElementKindScript => "Skrypt",
        TextKey::GraphicsElementKindSubtitle => "Napisy",
        TextKey::GraphicsElementKindText => "Tekst",
        TextKey::GraphicsElementNameTooLong => "Nazwa może mieć najwyżej 50 znaków.",
        TextKey::GraphicsElementNotEditable => "Nie można edytować tutaj",
        TextKey::GraphicsElementNotFound => "Nie znaleziono elementu graficznego.",
        TextKey::GraphicsElementNotText => "Tutaj można edytować tylko tekstowe elementy graficzne.",
        TextKey::GraphicsElementSaved => "Element graficzny został zapisany.",
        TextKey::GraphicsElementSaveFailed => "Nie udało się zapisać elementu graficznego.",
        TextKey::GraphicsElementType => "Typ",
        TextKey::GraphicsExistingDrawvgHelp => "Istniejące i nowe elementy domyślnie używają Drawvg.",
        TextKey::GraphicsStreamRestartHelp => "Zmiany dotyczą nowo uruchomionych strumieni i nie uruchamiają ponownie aktywnych strumieni.",
        TextKey::NoGraphicsElements => "Nie znaleziono elementów graficznych.",
        TextKey::RefreshPeriodBounds => "Okres odświeżania musi wynosić od 250 do 60000 milisekund.",
        TextKey::RefreshPeriodMilliseconds => "Okres odświeżania (milisekundy)",
        TextKey::ReloadFrameInterval => "Interwał klatek ponownego wczytania",
        TextKey::ReloadFrameIntervalBounds => "Interwał klatek ponownego wczytania musi wynosić co najmniej 1.",
        TextKey::Renderer => "Mechanizm renderujący",
        TextKey::SaveGraphicsElement => "Zapisz element graficzny",
        TextKey::TextTemplate => "Szablon tekstu",
        TextKey::TextTemplateRequired => "Szablon tekstu jest wymagany.",
        TextKey::TextTemplateTooLong => "Szablon musi być prawidłowym tekstem UTF-8 o rozmiarze do 64 KiB.",
        TextKey::GraphicsIds => "Identyfikatory grafiki",
        TextKey::Genres => "Gatunki",
        TextKey::GroupName => "Nazwa grupy",
        TextKey::GroupMultiPartEpisodesHelp => "Grupuj odcinki wieloczęściowe podczas tasowania",
        TextKey::GuideMode => "Tryb przewodnika",
        TextKey::KeepMultiPartEpisodesTogether => "Trzymaj odcinki wieloczęściowe razem",
        TextKey::Marathon => "Maraton",
        TextKey::MarathonBatchSize => "Rozmiar partii maratonu",
        TextKey::MarathonBatchSizeHelp => {
            "Ile elementów odtworzyć z każdej grupy przed przejściem dalej; puste lub zero odtworzy wszystkie elementy"
        },
        TextKey::MarathonGroupBy => "Grupuj maraton według",
        TextKey::MarathonShuffleGroups => "Tasuj grupy maratonu",
        TextKey::MarathonShuffleItems => "Tasuj elementy maratonu",
        TextKey::EditFillerPreset => "Edytuj szablon wypełniacza",
        TextKey::MidRollFiller => "Wypełniacz śródemisji",
        TextKey::MidRoll => "Śródemisja",
        TextKey::MoveDown => "Przenieś w dół",
        TextKey::MoveUp => "Przenieś w górę",
        TextKey::ReorderPlaylistItem => "Przeciągnij, aby zmienić kolejność elementu playlisty",
        TextKey::MultiEpisodeGroupSize => "Rozmiar grupy wielu odcinków",
        TextKey::MultiEpisodeShuffle => "Tasowanie wielu odcinków",
        TextKey::Multiple => "Wiele",
        TextKey::MultipleCount => "Liczba elementów",
        TextKey::MultipleCountHelp => {
            "Może być wyrażeniem z 'count' (rozmiar kolekcji) i 'random' (0 do count-1)"
        },
        TextKey::MultipleMode => "Tryb wielu elementów",
        TextKey::NewScheduleName => "Nazwa nowego harmonogramu",
        TextKey::NewSmartCollection => "Nowa inteligentna kolekcja",
        TextKey::NoScheduleItems => "Brak elementów harmonogramu.",
        TextKey::NoSchedules => "Brak harmonogramów.",
        TextKey::Normal => "Normalny",
        TextKey::Offline => "Offline",
        TextKey::One => "Jeden",
        TextKey::OrderedGroups => "Uporządkowane grupy",
        TextKey::Playback => "Odtwarzanie",
        TextKey::PlaybackOrder => "Kolejność odtwarzania",
        TextKey::PlaybackOrderUpdateFailed => "Aktualizacja kolejności odtwarzania nie powiodła się",
        TextKey::PlaybackOrderUpdated => "Zaktualizowano kolejność odtwarzania.",
        TextKey::PlaylistItemSize => "Rozmiar elementu playlisty",
        TextKey::PlayoutDurationHours => "Czas emisji w godzinach",
        TextKey::PlayoutDurationMinutes => "Czas emisji w minutach",
        TextKey::PostRollFiller => "Wypełniacz po emisji",
        TextKey::PostRoll => "Po emisji",
        TextKey::PreRollFiller => "Wypełniacz przed emisją",
        TextKey::PreRoll => "Przed emisją",
        TextKey::Random => "Losowo",
        TextKey::RandomCount => "Losowa liczba",
        TextKey::RandomRotation => "Losowa rotacja",
        TextKey::RemainingDurationHelp => {
            "czas trwania treści po tym punkcie śródemisji, w sekundach"
        },
        TextKey::RandomStartPoint => "Losowy punkt startu",
        TextKey::RemoveFailed => "Usuwanie nie powiodło się",
        TextKey::RemovedFromCollection => "Usunięto z kolekcji.",
        TextKey::RemoveFromCollection => "Usuń z kolekcji",
        TextKey::Rename => "Zmień nazwę",
        TextKey::RenameFailed => "Zmiana nazwy nie powiodła się",
        TextKey::ReorderFailed => "Zmiana kolejności nie powiodła się",
        TextKey::RerunFirstRun => "Powtórka - pierwsza emisja",
        TextKey::RerunRerun => "Powtórka - reemisja",
        TextKey::ReadOnlyTwentyFourHourPlayoutView => {
            "Tylko do odczytu: 24-godzinny widok zaplanowanych elementów emisji."
        },
        TextKey::SaveSchedule => "Zapisz harmonogram",
        TextKey::SaveScheduleItems => "Zapisz elementy harmonogramu",
        TextKey::SaveSmartCollection => "Zapisz inteligentną kolekcję",
        TextKey::Schedule => "Harmonogram",
        TextKey::ScheduleActionFailed => "Akcja harmonogramu nie powiodła się.",
        TextKey::ScheduleAsGroup => "Planuj jako grupę",
        TextKey::ScheduleKind => "Typ harmonogramu",
        TextKey::ScheduleKindBlock => "Blokowy",
        TextKey::ScheduleKindClassic => "Klasyczny",
        TextKey::ScheduleKindExternalJson => "JSON (dizqueTV)",
        TextKey::ScheduleKindScripted => "Skryptowy",
        TextKey::ScheduleKindSequential => "Sekwencyjny",
        TextKey::SchedulingContext => "Kontekst planowania",
        TextKey::ScheduleItem => "Element harmonogramu",
        TextKey::ScheduleItemActionFailed => "Akcja elementu harmonogramu nie powiodła się.",
        TextKey::ScheduleItemFiller => "Wypełniacz elementu harmonogramu",
        TextKey::ScheduleItemFillerHelp => {
            "Wypełniacz przed, w trakcie i po emisji jest stosowany do każdego medium wybranego przez ten element harmonogramu. Aby odtworzyć zawartość raz po bloku wielu elementów, dodaj ją jako następny element harmonogramu."
        },
        TextKey::ScheduleItemOverrides => "Nadpisania elementu harmonogramu",
        TextKey::ScheduleItems => "Elementy harmonogramu",
        TextKey::ScheduleNotFound => "Nie znaleziono harmonogramu.",
        TextKey::ScheduleSaveFailed => "Zapis harmonogramu nie powiódł się.",
        TextKey::SmartShuffle => "Inteligentne losowanie",
        TextKey::SmartShufflePreset => "Ustawienie odstępu",
        TextKey::Balanced => "Zrównoważone",
        TextKey::SmartShuffleScope => "Zakres odstępu",
        TextKey::SmartShuffleScopePlayout => "Emisja",
        TextKey::SmartShuffleScopeScheduleItem => "Element harmonogramu",
        TextKey::SmartShuffleValidation => "Wybierz próg odstępu i co najmniej jedną regułę podobieństwa.",
        TextKey::SmartShuffleScopeContentSource => "Źródło treści",
        TextKey::SmartShuffleAfterItems => "Filmy między podobnymi elementami",
        TextKey::SmartShuffleAfterDurationMinutes => "Czas odstępu (minuty)",
        TextKey::SmartShuffleIncludeFiller => "Uwzględnij wypełniacz",
        TextKey::SmartShuffleInheritedBlock => {
            "Ten element nie ma nadpisania Smart Shuffle i zachowuje ewentualną starszą zasadę emisji."
        }
        TextKey::SmartShuffleInactive => "Inteligentne losowanie jest nieaktywne, ponieważ ta kolejność odtwarzania nie jest losowa.",
        TextKey::SmartShuffleOverrideBlock => "Nadpisz dla tego elementu bloku",
        TextKey::SimilarityRules => "Reguły podobieństwa",
        TextKey::AlbumArtist => "Wykonawca albumu",
        TextKey::Apply => "Zastosuj",
        TextKey::ContentParent => "Element nadrzędny lub wariant",
        TextKey::SmartShuffleFilterRules => "Reguły filtrów",
        TextKey::SearchForSchedules => "Szukaj harmonogramów",
        TextKey::SearchForManualCollections => "Szukaj kolekcji ręcznych",
        TextKey::SearchForRerunCollections => "Szukaj kolekcji powtórek",
        TextKey::SearchForSmartCollections => "Szukaj inteligentnych kolekcji",
        TextKey::SearchForTemplates => "Szukaj szablonów",
        TextKey::SearchForBlocks => "Szukaj bloków",
        TextKey::SearchForMultiCollections => "Szukaj multi-kolekcji",
        TextKey::SearchQuery => "Zapytanie wyszukiwania",
        TextKey::BlockSearchQueryPlaceholder => "np. genre:action",
        TextKey::SearchTitle => "Tytuł wyszukiwania",
        TextKey::SearchTitleDisplay => "Tytuł wyszukiwania (wyświetlany)",
        TextKey::SearchSmartCollection => "Szukaj inteligentnej kolekcji",
        TextKey::Season => "Sezon",
        TextKey::SeasonEpisode => "Sezon, odcinek",
        TextKey::Unnumbered => "Nienumerowany",
        TextKey::SeasonNotFound => "Nie znaleziono sezonu",
        TextKey::Select => "Zaznacz",
        TextKey::SelectCollection => "Wybierz kolekcję",
        TextKey::SelectGroupForNewTemplate => "Wybierz grupę dla nowego szablonu",
        TextKey::SelectScheduleItemPrompt => "Wybierz element harmonogramu do edycji albo dodaj nowy element.",
        TextKey::SelectSchedule => "Wybierz harmonogram",
        TextKey::SelectSmartCollection => "Wybierz inteligentną kolekcję",
        TextKey::Shuffle => "Tasuj",
        TextKey::ShuffleInOrder => "Tasuj w kolejności",
        TextKey::ShuffleScheduleItems => "Tasuj elementy harmonogramu",
        TextKey::ShuffleScheduleItemsHelp => "Uwaga: wyłącza stałe czasy startu i tryb flood",
        TextKey::ShuffledGroups => "Tasowane grupy",
        TextKey::Show => "Serial",
        TextKey::ShowNotFound => "Nie znaleziono serialu",
        TextKey::Shows => "Seriale",
        TextKey::Start => "Start",
        TextKey::StartTime => "Czas rozpoczęcia",
        TextKey::StartTimeOnOrAfter => "Czas rozpoczęcia o lub po",
        TextKey::StartType => "Typ startu",
        TextKey::Strict => "Ścisły",
        TextKey::StrictAlwaysWaitExactStartTime => "Ściśle - zawsze czekaj na dokładny czas startu",
        TextKey::StrictWaitExactStartTime => "Ściśle - czekaj na dokładny czas startu",
        TextKey::TailFiller => "Wypełniacz końcowy",
        TextKey::TailMode => "Tryb końcówki",
        TextKey::Tail => "Końcówka",
        TextKey::TelevisionSeason => "Sezon TV",
        TextKey::TelevisionShow => "Serial TV",
        TextKey::TreatCollectionsAsShows => "add-onuj kolekcje jak seriale",
        TextKey::TreatCollectionsAsShowsHelp => "Przydatne dla wieloczęściowych odcinków crossover",
        TextKey::Type => "Typ",
        TextKey::TypeToSearch => "Wpisz, aby wyszukać...",
        TextKey::Search => "Szukaj",
        TextKey::Filters => "Filtry",
        TextKey::ActiveFilters => "aktywne filtry",
        TextKey::ClearFilters => "Wyczyść filtry",
        TextKey::CreateSmartCollection => "Utwórz inteligentną kolekcję",
        TextKey::FromYear => "Od roku",
        TextKey::ToYear => "Do roku",
        TextKey::CollectionComposer => "Edytor inteligentnej kolekcji",
        TextKey::CollectionDetails => "Szczegóły kolekcji",
        TextKey::Rules => "Reguły",
        TextKey::Overview => "Przegląd",
        TextKey::MatchAll => "Spełnij wszystkie",
        TextKey::MatchAllHelp => "Każda reguła w tej grupie musi być spełniona.",
        TextKey::MatchAny => "Spełnij dowolną",
        TextKey::MatchAnyHelp => "Co najmniej jedna reguła w tej grupie musi być spełniona.",
        TextKey::ExcludeAny => "Wyklucz dopasowania",
        TextKey::ExcludeAnyHelp => "Elementy spełniające dowolną regułę z tej grupy są wykluczane.",
        TextKey::AddRule => "Dodaj regułę",
        TextKey::EditRule => "Edytuj regułę",
        TextKey::RemoveRule => "Usuń regułę",
        TextKey::RuleField => "Pole",
        TextKey::Genre => "Gatunek",
        TextKey::Studio => "Studio",
        TextKey::Network => "Sieć",
        TextKey::Actor => "Aktor",
        TextKey::Writer => "Scenarzysta",
        TextKey::SubtitleLanguage => "Język napisów",
        TextKey::ShowTitle => "Tytuł serialu",
        TextKey::ContentKindLabel => "Typ treści",
        TextKey::SubKindLabel => "Podtyp",
        TextKey::HasCommentary => "Z komentarzem",
        TextKey::AllFields => "Wszystkie pola",
        TextKey::MatchType => "Typ dopasowania",
        TextKey::MatchValue => "Wartość",
        TextKey::TextContains => "Tekst zawiera",
        TextKey::ExactPhrase => "Dokładna fraza",
        TextKey::Wildcard => "Symbol wieloznaczny",
        TextKey::Fuzzy => "Przybliżone",
        TextKey::Range => "Zakres",
        TextKey::RangeStart => "Początek zakresu",
        TextKey::RangeEnd => "Koniec zakresu",
        TextKey::NestedGroup => "Grupa zagnieżdżona",
        TextKey::NestedGroupHelp => "Edytuj tę grupę w trybie skupienia, aby widzieć jeden poziom logiczny naraz.",
        TextKey::AddNestedGroup => "Dodaj grupę zagnieżdżoną",
        TextKey::EditNestedGroup => "Edytuj grupę zagnieżdżoną",
        TextKey::BackToRules => "Wróć do reguł",
        TextKey::BackToSearch => "Wróć do wyszukiwania",
        TextKey::CanonicalQuery => "Zapytanie kanoniczne",
        TextKey::CanonicalQueryHelp => "To zapytanie jest używane przez wyszukiwanie, planowanie i zapisaną kolekcję.",
        TextKey::LivePreview => "Podgląd na żywo",
        TextKey::MatchingItems => "pasujących elementów",
        TextKey::QueryValid => "Zapytanie prawidłowe",
        TextKey::QueryInvalid => "Zapytanie wymaga uwagi",
        TextKey::SearchRuleValueRequired => "Każda reguła wyszukiwania wymaga wartości.",
        TextKey::SearchGroupRequired => "Dodaj co najmniej jedną regułę do tej grupy.",
        TextKey::SearchWildcardRequired => "Reguła z symbolem wieloznacznym musi zawierać * lub ?.",
        TextKey::SavedSearches => "Zapisane wyszukiwania",
        TextKey::SavedSearchName => "Nazwa zapisanego wyszukiwania",
        TextKey::SavedSearchHelp => "Zapisz to złożone wyszukiwanie, aby później je wczytać i zmienić.",
        TextKey::ChooseSavedSearch => "Wybierz zapisane wyszukiwanie",
        TextKey::LoadSavedSearch => "Wczytaj zapisane wyszukiwanie",
        TextKey::SaveSavedSearch => "Zapisz wyszukiwanie",
        TextKey::UpdateSavedSearch => "Aktualizuj zapisane wyszukiwanie",
        TextKey::NewSavedSearch => "Nowe zapisane wyszukiwanie",
        TextKey::DeleteSavedSearch => "Usuń zapisane wyszukiwanie",
        TextKey::SavedSearchSaved => "Wyszukiwanie zapisane.",
        TextKey::SavedSearchDeleted => "Zapisane wyszukiwanie usunięte.",
        TextKey::SavedSearchLoaded => "Zapisane wyszukiwanie wczytane.",
        TextKey::SavedSearchNameRequired => "Nazwa zapisanego wyszukiwania jest wymagana.",
        TextKey::SavedSearchNameTooLong => "Nazwa może mieć najwyżej 80 znaków.",
        TextKey::SavedSearchNameUnique => "Nazwa zapisanego wyszukiwania musi być unikalna.",
        TextKey::NoSavedSearches => "Brak zapisanych wyszukiwań.",
        TextKey::RefreshPreview => "Odśwież podgląd",
        TextKey::NoRules => "Brak reguł w tej grupie.",
        TextKey::SearchFiltersHelp => "Tylko typowe filtry. Logikę zagnieżdżoną utwórz w edytorze kolekcji.",
        TextKey::SearchMedia => "Szukaj multimediów",
        TextKey::SearchMoviesOrShows => "Szukaj filmów lub seriali...",
        TextKey::SearchAddonMediaList => "Szukaj listy add-on",
        TextKey::SearchingMinimumLogLevel => "Minimalny poziom logów wyszukiwania",
        TextKey::Settings => "Ustawienia",
        TextKey::SettingsSaveFailed => "Nie udało się zapisać ustawień",
        TextKey::ShowMediaInfo => "Pokaż informacje o mediach",
        TextKey::ScriptedScheduleTimeout => "Limit czasu harmonogramu skryptowego",
        TextKey::SkipMissingItems => "Pomijaj brakujące elementy",
        TextKey::SkipMissingItemsHelp => "Określa, czy elementy z brakującym plikiem lub niedostępne mają być uwzględniane w emisjach",
        TextKey::SmartCollections => "Inteligentne kolekcje",
        TextKey::SmartCollection => "Inteligentna kolekcja",
        TextKey::SmartCollectionDeleted => "Inteligentna kolekcja została usunięta.",
        TextKey::SmartCollectionQueryPlaceholder => "np. genre:action OR genre:comedy",
        TextKey::SmartCollectionSaved => "Inteligentna kolekcja została zapisana.",
        TextKey::SaveAddonMediaList => "Zapisz listę add-on",
        TextKey::Songs => "Utwory",
        TextKey::SplitTimeEvenly => "Podziel czas równomiernie",
        TextKey::StreamingMinimumLogLevel => "Minimalny poziom logów strumieniowania",
        TextKey::Studios => "Studia",
        TextKey::Support => "Pomoc",
        TextKey::Template => "Szablon",
        TextKey::TemplateCopied => "Szablon został skopiowany.",
        TextKey::TemplateCreated => "Szablon został utworzony.",
        TextKey::TemplateDeleted => "Szablon został usunięty.",
        TextKey::TemplateGroup => "Grupa szablonów",
        TextKey::TemplateGroupCreated => "Grupa szablonów została utworzona.",
        TextKey::TemplateGroupDeleted => "Grupa szablonów została usunięta.",
        TextKey::TemplateGroupName => "Nazwa grupy szablonów",
        TextKey::TemplateGroups => "Grupy szablonów",
        TextKey::TemplateName => "Nazwa szablonu",
        TextKey::TemplateNotFound => "Nie znaleziono szablonu.",
        TextKey::Templates => "Szablony",
        TextKey::Theme => "Motyw",
        TextKey::ThisActionCannotBeUndone => "Tej czynności nie można cofnąć.",
        TextKey::ThisCollectionHasNoItemsYet => "Ta kolekcja nie ma jeszcze elementów.",
        TextKey::Thumbnail => "miniatura",
        TextKey::Title => "Tytuł",
        TextKey::ToggleNavigation => "Przełącz nawigację",
        TextKey::TotalDurationHelp => "łączny czas trwania treści, w sekundach",
        TextKey::TotalPointsHelp => "łączna liczba możliwych punktów śródemisji",
        TextKey::TotalProgressHelp => "znormalizowana pozycja od 0 do 1",
        TextKey::AddonMediaList => "Lista add-on",
        TextKey::AddonMediaListActionFailed => "Akcja listy add-on nie powiodła się.",
        TextKey::AddonMediaListAdded => "Lista add-on została dodana.",
        TextKey::AddonMediaListDeleted => "Lista add-on została usunięta.",
        TextKey::AddonMediaListMatched => "Lista add-on została dopasowana.",
        TextKey::AddonMediaListNotFound => "Nie znaleziono listy add-on.",
        TextKey::AddonMediaListSaveFailed => "Zapis listy add-on nie powiódł się.",
        TextKey::AddonMediaListSaved => "Lista add-on została zapisana.",
        TextKey::AddonMediaLists => "Listy add-on",
        TextKey::AddonMediaListUrl => "URL listy add-on",
        TextKey::AddonMediaListFolder => "Folder magazynu",
        TextKey::AddonMediaListFolderHelp => "Opcjonalne; w przeciwnym razie wywodzi się z nazwy listy",
        TextKey::DestructiveActions => "Działania destrukcyjne",
        TextKey::ResetAndReload => "Resetuj i wczytaj ponownie",
        TextKey::ResetAddonMediaListHelp => "Usuwa wszystkie wygenerowane wyniki tej listy, a następnie kolejkuje pełny import.",
        TextKey::SourceHost => "Host źródłowy",
        TextKey::ResetRemoves => "Ten reset usuwa",
        TextKey::ResetRemovesItems => "Zaimportowane elementy, metadane dostawcy, zmiany operatora, numerację odcinków i zamienniki.",
        TextKey::ResetRemovesArtifacts => "Wygenerowane multimedia, pliki NFO, grafiki, napisy, pamięci podręczne i archiwa.",
        TextKey::ResetRemovesRuntime => "Zadania, punkty kontrolne, artefakty wersji, elementy wygenerowanej listy odtwarzania i media projekcji.",
        TextKey::ResetRetains => "Ten reset zachowuje",
        TextKey::ResetRetainsSettings => "Nazwę listy, powiązania z dodatkiem, źródłem i biblioteką, tożsamość wygenerowanej listy odtwarzania, zasady odświeżania i publikacji, limit artefaktów oraz inne ustawienia operatora.",
        TextKey::ResetReloadFailureWarning => "Jeśli ponowne wczytanie się nie powiedzie, kanały i wygenerowana lista odtwarzania pozostaną puste.",
        TextKey::TypeListNameToConfirm => "Wpisz bieżącą nazwę listy, aby potwierdzić",
        TextKey::ResetQueued => "Resetowanie i ponowne wczytanie dodano do kolejki.",
        TextKey::ResetAddonMediaListFailed => "Resetowanie i ponowne wczytanie nie powiodło się.",
        TextKey::ChapterInput => "Rozdziały",
        TextKey::ChapterInputHelp => "Opcjonalnie, jeden na wiersz: 0:00 Wprowadzenie",
        TextKey::RestoreProviderChapters => "Przywróć rozdziały dostawcy",
        TextKey::ManagedByAddon => "Zarządzane przez dodatek",
        TextKey::Trash => "Kosz",
        TextKey::TrashAlreadyEmpty => "Kosz jest już pusty.",
        TextKey::TrashEmpty => "Kosz jest pusty.",
        TextKey::TrashEmptyConfirm => "Usunąć z bazy danych wszystkie elementy, których plików nie znaleziono?",
        TextKey::TrashEmptyConfirmHelp => {
            "To trwale usuwa obecnie brakujące elementy multimedialne i zależne wiersze metadanych."
        },
        TextKey::TrashSubtitle => {
            "Elementy, których plików źródłowych obecnie brakuje na dysku lub u dostawcy."
        },
        TextKey::Troubleshooting => "Rozwiązywanie problemów",
        TextKey::TunerCount => "Liczba tunerów",
        TextKey::TvShows => "Seriale",
        TextKey::Ui => "Interfejs",
        TextKey::Unavailable => "Niedostępne",
        TextKey::UseActualTimes => "Użyj rzeczywistych czasów",
        TextKey::UseCustomPlaybackOrder => "Użyj własnej kolejności odtwarzania",
        TextKey::UseChaptersAsMediaItems => "Używaj rozdziałów jako elementów multimedialnych",
        TextKey::UseChaptersAsMediaItemsHelp => {
            "Planuj pojedyncze rozdziały zamiast całych plików"
        },
        TextKey::UseEmbeddedSubtitles => "Używaj osadzonych napisów",
        TextKey::Uuid => "UUID",
        TextKey::UntitledItem => "Element bez tytułu",
        TextKey::WindowsStartup => "Uruchamianie z systemem Windows",
        TextKey::WindowsStartupNeedsRepair => {
            "Zapisane polecenie uruchamiania wskazuje inną lokalizację. Zastosuj ustawienie, aby je naprawić."
        }
        TextKey::WindowsStartupUnavailable => {
            "Uruchamianie z systemem Windows jest niedostępne, ponieważ nie znaleziono aplikacji uruchamiającej lub nie można odczytać rejestracji."
        }
        TextKey::ActiveTranscodeLimit => "Maksymalna liczba jednoczesnych transkodowań strumieni",
        TextKey::ActiveTranscodeLimitHelp => {
            "Twardy limit strumieniowych transkodowań FFmpeg. Gdy wszystkie miejsca są zajęte, nowy widz otrzymuje komunikat, że wszystkie strumienie są w użyciu, dopóki miejsce się nie zwolni; aktywne strumienie nie omijają limitu."
        },
        TextKey::ActiveTranscodeSuggestionHardware => {
            "Wykryto enkoder sprzętowy; to sugerowana wartość, a nie automatyczna zmiana."
        },
        TextKey::ActiveTranscodeSuggestionSoftware => {
            "Tylko kodowanie programowe; to sugerowana wartość, a nie automatyczna zmiana."
        },
        TextKey::Peak => "szczyt",
        TextKey::StreamsInUse => "Strumienie w użyciu",
        TextKey::SuggestedTranscodeLimit => "Sugerowana wartość",
        TextKey::Watermarks => "Znaki wodne",
        TextKey::Width => "Szerokość",
        TextKey::WorkAheadSegmenterLimit => "Transkodowania HLS z wyprzedzeniem",
        TextKey::WorkAheadSegmenterLimitHelp => "Ile miejsc transkodowania HLS może pracować z wyprzedzeniem z maksymalną szybkością, aby zbudować bufor. Praca z wyprzedzeniem zawsze ustępuje oglądanym strumieniom.",
        TextKey::AccelerationKind => "Rodzaj akceleracji",
        TextKey::AddFfmpegProfile => "Dodaj profil FFmpeg",
        TextKey::AddProfile => "Dodaj profil",
        TextKey::AllowBFrames => "Zezwalaj na klatki B",
        TextKey::Audio => "Audio",
        TextKey::AudioBitrate => "Bitrate audio (kBit/s)",
        TextKey::AudioBufferSize => "Rozmiar bufora audio (kBit)",
        TextKey::AudioChannels => "Kanały",
        TextKey::AudioFormat => "Format audio",
        TextKey::BitDepth => "Głębia bitowa",
        TextKey::Copy => "Kopiuj",
        TextKey::CopyBlock => "Kopiuj blok",
        TextKey::CopyFailed => "Kopiowanie nie powiodło się",
        TextKey::CopyNameSuffix => "Kopia",
        TextKey::CopyProfile => "Kopiuj profil",
        TextKey::CopyTemplate => "Kopiuj szablon",
        TextKey::Crop => "Przytnij",
        TextKey::AllowWatermarks => "Zezwalaj na znaki wodne",
        TextKey::DeinterlaceVideo => "Usuwaj przeplot wideo",
        TextKey::Edit => "Edytuj",
        TextKey::EditBlock => "Edytuj blok",
        TextKey::EditCollection => "Edytuj kolekcję",
        TextKey::EditFfmpegProfile => "Edytuj profil FFmpeg",
        TextKey::EditMultiCollection => "Edytuj multi-kolekcję",
        TextKey::EditRerunCollection => "Edytuj kolekcję powtórek",
        TextKey::EditTemplate => "Edytuj szablon",
        TextKey::Expression => "Wyrażenie",
        TextKey::Fallback => "Awaryjny",
        TextKey::FillerKind => "Typ wypełniacza",
        TextKey::General => "Ogólne",
        TextKey::HardwareAcceleration => "Akceleracja sprzętowa",
        TextKey::HardwareIfPossible => "Sprzętowo, jeśli możliwe",
        TextKey::NewProfileName => "Nowa nazwa profilu",
        TextKey::NoneOption => "(Brak)",
        TextKey::NumHelp => "numer punktu śródemisji, zaczynając od 1",
        TextKey::OptionalDisplayTitle => "Opcjonalny tytuł wyświetlany...",
        TextKey::TranscodeAudio => "Transkoduj audio",
        TextKey::NormalizeColors => "Normalizuj kolory",
        TextKey::NormalizeFramerate => "Normalizuj liczbę klatek",
        TextKey::NormalizeLoudness => "Normalizuj głośność",
        TextKey::TranscodeVideo => "Transkoduj wideo",
        TextKey::Off => "Wyłączone",
        TextKey::Pad => "Dopełnij",
        TextKey::PadNearestFifteenMinutes => "15 (:00, :15, :30, :45)",
        TextKey::PadNearestFiveMinutes => "5 (:00, :05, :10, :15, :20 itd.)",
        TextKey::PadNearestTenMinutes => "10 (:00, :10, :20, :30, :40, :50)",
        TextKey::PadNearestThirtyMinutes => "30 (:00, :30)",
        TextKey::PadMode => "Tryb wypełniania",
        TextKey::PadToNearestMinute => "Dopełnij do najbliższej minuty",
        TextKey::PointHelp => "pozycja punktu śródemisji, w sekundach",
        TextKey::ProfileCopied => "Profil został skopiowany.",
        TextKey::ProfileNotFound => "Nie znaleziono profilu.",
        TextKey::QsvExtraHardwareFrames => "Dodatkowe klatki sprzętowe QSV",
        TextKey::Resolution => "Rozdzielczość",
        TextKey::SampleRateKhz => "Częstotliwość próbkowania (kHz)",
        TextKey::SaveDecoTemplate => "Zapisz szablon dekoracji",
        TextKey::SaveProfile => "Zapisz profil",
        TextKey::SaveFillerPreset => "Zapisz szablon wypełniacza",
        TextKey::SaveRerunCollection => "Zapisz kolekcję powtórek",
        TextKey::SaveSequentialSchedule => "Zapisz harmonogram sekwencyjny",
        TextKey::SaveTemplate => "Zapisz szablon",
        TextKey::SaveBlock => "Zapisz blok",
        TextKey::ScaleAndPad => "Skaluj i wypełnij",
        TextKey::ScalingBehavior => "Zachowanie skalowania",
        TextKey::Software => "Programowo",
        TextKey::Stretch => "Rozciągnij",
        TextKey::TargetLoudnessLufs => "Docelowa głośność (LUFS)",
        TextKey::ThreadCountAuto => "Liczba wątków (0 = auto)",
        TextKey::OutputDynamicRange => "Zakres dynamiki wyjścia",
        TextKey::OutputDynamicRangeCopyHint => "Wideo jest kopiowane, więc wyjście zachowuje zakres dynamiki źródła.",
        TextKey::HdrStaticMetadataHint => "Pozostaw puste, aby użyć udokumentowanych wartości domyślnych HDR10. Mają zastosowanie tylko wtedy, gdy źródło nie zawiera własnych metadanych HDR; monitor masteringowy i poziomy jasności źródła zawsze mają pierwszeństwo.",
        TextKey::HdrMasteringDisplay => "Monitor masteringowy",
        TextKey::HdrMaxCll => "MaxCLL (cd/m²)",
        TextKey::HdrMaxFall => "MaxFALL (cd/m²)",
        TextKey::TonemapAlgorithm => "Algorytm tonemappingu",
        TextKey::VaapiDevicePath => "Ścieżka urządzenia VAAPI",
        TextKey::VaapiDisplay => "Ekran VAAPI",
        TextKey::VaapiDriver => "Sterownik VAAPI",
        TextKey::Video => "Wideo",
        TextKey::VideoBitrate => "Bitrate wideo (kBit/s)",
        TextKey::VideoBufferSize => "Rozmiar bufora wideo (kBit)",
        TextKey::VideoFormat => "Format wideo",
        TextKey::VideoPreset => "Preset wideo",
        TextKey::VideoTune => "Dostrajanie wideo",
        TextKey::VideoProfile => "Profil wideo",
        TextKey::ViewCollection => "Zobacz kolekcję",
        TextKey::ViewPlaylist => "Zobacz playlistę",
        TextKey::AddAll => "Dodaj wszystko",
        TextKey::AddAllToCollection => "Dodaj wszystko do kolekcji",
        TextKey::AddAllToPlaylist => "Dodaj wszystko do playlisty",
        TextKey::AddedItemToPlaylist => "Dodano element do playlisty.",
        TextKey::AddedArtistToSchedule => "Dodano artystę do harmonogramu",
        TextKey::AddedMovieToSchedule => "Dodano film do harmonogramu",
        TextKey::AddedSeasonToSchedule => "Dodano sezon do harmonogramu",
        TextKey::AddedShowToSchedule => "Dodano serial do harmonogramu",
        TextKey::AddingItem => "Dodawanie elementu...",
        TextKey::AddingItemToPlaylist => "Dodawanie elementu do playlisty...",
        TextKey::Artists => "Artyści",
        TextKey::ClearSelection => "Wyczyść zaznaczenie",
        TextKey::Collection => "Kolekcja",
        TextKey::CollectionNameRequired => "Nazwa kolekcji jest wymagana",
        TextKey::CollectionNotFound => "Nie znaleziono kolekcji",
        TextKey::CollectionRenamed => "Zmieniono nazwę kolekcji.",
        TextKey::EditAddonMediaListProperties => "Edytuj właściwości listy add-on",
        TextKey::EnterAddonMediaListUrl => "Wpisz URL listy add-on",
        TextKey::EnterSearchQuery => "Wpisz zapytanie wyszukiwania.",
        TextKey::Episodes => "Odcinki",
        TextKey::LoadingCollections => "Ładowanie kolekcji...",
        TextKey::LoadingPlaylistGroups => "Ładowanie grup playlist...",
        TextKey::LoadingPlaylists => "Ładowanie playlist...",
        TextKey::GeneratePlaylist => "Utwórz playlistę w kolejności źródłowej",
        TextKey::GeneratePlaylistFromSortedAddonMediaListItems => {
            "Zachowaj oryginalną kolejność dostawcy po każdej publikacji"
        },
        TextKey::MusicVideos => "Teledyski",
        TextKey::NewCollectionName => "Nazwa nowej kolekcji",
        TextKey::NoMediaMatched => "Nie znaleziono pasujących multimediów.",
        TextKey::NoItemsToAdd => "Brak elementów do dodania.",
        TextKey::NoItemsToPreview => "Brak elementów do podglądu.",
        TextKey::NoPlaylistsInGroup => "Brak playlist w tej grupie.",
        TextKey::PleaseChoosePlaylist => "Wybierz playlistę.",
        TextKey::RequestFailed => "Żądanie nie powiodło się",
        TextKey::Released => "Premiera",
        TextKey::SaveAs => "Zapisz jako",
        TextKey::Seasons => "Sezony",
        TextKey::SeeAll => "Zobacz wszystko >>",
        TextKey::SelectAll => "Zaznacz wszystko",
        TextKey::SelectGroup => "(Wybierz grupę)",
        TextKey::SelectPlaylist => "(Wybierz playlistę)",
        TextKey::SelectSearchCollectionHelp => "Wybierz kolekcję, do której dodać wyniki wyszukiwania.",
        TextKey::SelectSearchPlaylistHelp => "Wybierz playlistę, do której dodać wyniki wyszukiwania.",
        TextKey::Selected => "Wybrano",
        TextKey::SelectCollectionToAddThisMovie => {
            "Wybierz kolekcję, do której dodać ten film."
        },
        TextKey::SelectPlaylistToAddThisMovie => {
            "Wybierz playlistę, do której dodać ten film."
        },
        TextKey::Add => "Dodaj",
        TextKey::AddLibrary => "Dodaj bibliotekę",
        TextKey::Address => "Adres",
        TextKey::AddressRequired => "Adres jest wymagany.",
        TextKey::ApiKey => "Klucz API",
        TextKey::ApiKeyRequired => "Klucz API jest wymagany.",
        TextKey::AuthorizedRedirecting => "Autoryzowano - przekierowywanie...",
        TextKey::CheckingEveryTwoSeconds => "Sprawdzanie co 2 sekundy...",
        TextKey::ClickAuthorizePlex => "Kliknij tutaj, aby autoryzować na Plex.tv",
        TextKey::ClickPlayoutItemHelp => {
            "Kliknij element emisji, aby sprawdzić jego tytuł, czasy i kontekst planowania."
        },
        TextKey::ConfigureConnection => "Skonfiguruj połączenie",
        TextKey::Connection => "Połączenie",
        TextKey::DeepScanCollections => "Głębokie skanowanie kolekcji",
        TextKey::DeepScanLibrary => "Głębokie skanowanie biblioteki",
        TextKey::DeepScan => "Głębokie skanowanie",
        TextKey::EditLibrary => "Edytuj bibliotekę",
        TextKey::EditPlaylist => "Edytuj playlistę",
        TextKey::EnterOnePathPerLine => "Wpisz jedną ścieżkę w każdym wierszu.",
        TextKey::Error => "Błąd",
        TextKey::ExternalCollections => "Kolekcje zewnętrzne",
        TextKey::Information => "Informacja",
        TextKey::Kind => "Rodzaj",
        TextKey::Library => "Biblioteka",
        TextKey::LibraryKind => "Rodzaj biblioteki",
        TextKey::LibraryPaths => "Ścieżki biblioteki",
        TextKey::LocalLibraries => "Biblioteki lokalne",
        TextKey::LocalPath => "Ścieżka lokalna",
        TextKey::MediaKind => "Rodzaj multimediów",
        TextKey::NoLibrariesConfigured => "Brak skonfigurowanych bibliotek. Najpierw dodaj źródło multimediów.",
        TextKey::NoSchedulingContext => "Brak kontekstu planowania",
        TextKey::NoPlayoutItemsInWindow => "Brak elementów emisji w tym 24-godzinnym oknie",
        TextKey::NoPlayoutsFound => "Nie znaleziono emisji.",
        TextKey::NoPathReplacements => "Brak zamienników ścieżek.",
        TextKey::Paths => "Ścieżki",
        TextKey::PathReplacements => "Zamienniki ścieżek",
        TextKey::PlexSignInHelp => "Zaloguj się do Plex, aby dodać źródło multimediów.",
        TextKey::Remote => "Zdalne",
        TextKey::RemotePath => "Ścieżka zdalna",
        TextKey::Remove => "Usuń",
        TextKey::RemoveBlockFromTemplate => "Usuń blok z szablonu",
        TextKey::RemoveContent => "Usuń zawartość",
        TextKey::Retry => "Ponów",
        TextKey::Saved => "Zapisano.",
        TextKey::Saving => "Zapisywanie...",
        TextKey::QuickScan => "Szybkie skanowanie",
        TextKey::ScanCollections => "Skanuj kolekcje",
        TextKey::ScanFailed => "Skanowanie nie powiodło się",
        TextKey::ScanLibrary => "Skanuj bibliotekę",
        TextKey::ScanQueued => "Skanowanie dodano do kolejki",
        TextKey::Scanning => "Skanowanie...",
        TextKey::Server => "Serwer",
        TextKey::ServerAddress => "Adres serwera",
        TextKey::SignInToPlex => "Zaloguj się do Plex",
        TextKey::SignOutOfPlex => "Wyloguj z Plex",
        TextKey::Sync => "Synchronizuj",
        TextKey::WaitingForPlexAuthorization => "Oczekiwanie na autoryzację Plex.",
        TextKey::AddChannel => "Dodaj kanał",
        TextKey::Any => "Dowolne",
        TextKey::BlockPlayoutTroubleshooting => "Diagnostyka emisji blokowej",
        TextKey::BlockPlayoutHistory => "Historia emisji blokowej",
        TextKey::Categories => "Kategorie",
        TextKey::CategoriesHelp => "Lista kategorii rozdzielona przecinkami",
        TextKey::Channel => "Kanał",
        TextKey::ChannelCouldNotBeSaved => "Nie można zapisać kanału. Sprawdź wartości formularza i spróbuj ponownie.",
        TextKey::ChannelDisabled => "Kanał wyłączony",
        TextKey::ChannelHiddenFromEpg => "Kanał ukryty w EPG",
        TextKey::ChannelLogo => "Logo kanału",
        TextKey::ChannelLogoPreview => "Podgląd logo kanału",
        TextKey::ChannelLogoUploadFailed => "Przesłanie logo kanału nie powiodło się. Sprawdź plik i spróbuj ponownie.",
        TextKey::Check => "Kontrola",
        TextKey::Continuous => "Ciągły",
        TextKey::DateAndTime => "Data i godzina",
        TextKey::DefaultOption => "Domyślne",
        TextKey::DefaultSchedule => "Domyślny harmonogram",
        TextKey::DeleteChannel => "Usuń kanał",
        TextKey::DownloadMediaSample => "Pobierz próbkę multimediów",
        TextKey::DownloadResults => "Pobierz wyniki",
        TextKey::DisableWatermarks => "Wyłącz znaki wodne",
        TextKey::EditChannel => "Edytuj kanał",
        TextKey::EditChannelNumbers => "Edytuj numery kanałów",
        TextKey::EditNumbers => "Edytuj numery",
        TextKey::ExternalLogoUrl => "Zewnętrzny URL logo",
        TextKey::Fail => "Błąd",
        TextKey::FallbackFiller => "Wypełniacz awaryjny",
        TextKey::FfmpegProfile => "Profil FFmpeg",
        TextKey::FirstPage => "Pierwsza strona",
        TextKey::FirstRunPlaybackOrder => "Kolejność odtwarzania pierwszej emisji",
        TextKey::Forced => "Wymuszone",
        TextKey::GenerateSubtitles => "Generuj napisy",
        TextKey::Generated => "Generowane",
        TextKey::Details => "Szczegóły",
        TextKey::Full => "Pełny",
        TextKey::Group => "Grupa",
        TextKey::FfmpegRuntime => "Runtime FFmpeg",
        TextKey::FfmpegRuntimeDownloaded => "Pobrano runtime FFmpeg",
        TextKey::FfmpegRuntimeLatestRelease => "Najnowsze wydanie",
        TextKey::FfmpegRuntimeRelease => "Zainstalowane wydanie",
        TextKey::FfmpegRuntimeSource => "Źródło",
        TextKey::FfmpegRuntimeSourceDownloaded => "Pobrano",
        TextKey::FfmpegRuntimeSourceManaged => "Zarządzane",
        TextKey::FfmpegRuntimeSourceOverride => "Nadpisanie",
        TextKey::FfmpegRuntimeSourcePath => "PATH",
        TextKey::FfmpegRuntimeSourceUnknown => "Nieznane",
        TextKey::FfmpegRuntimeUpdateAvailable => "Dostępna aktualizacja",
        TextKey::FfmpegRuntimeUpdateCheckFailed => "Sprawdzenie aktualizacji nie powiodło się",
        TextKey::FfmpegRuntimeUpdateFailed => "Aktualizacja runtime FFmpeg nie powiodła się",
        TextKey::FfmpegRuntimeUpToDate => "Aktualne",
        TextKey::FfmpegRuntimeUpdated => "Zaktualizowano runtime FFmpeg",
        TextKey::FfmpegRuntimeVersion => "Wersja",
        TextKey::HealthNoPersistedRun => "Brak obecnie odpowiednich kontroli diagnostycznych.",
        TextKey::HealthPersistedRunHelp => "Bieżące wyniki kontroli diagnostycznych parytetu. Ta strona odświeża się po zmianie stanu elementów przez skanowanie mediów.",
        TextKey::HistoryDetails => "Szczegóły historii",
        TextKey::HlsDirect => "HLS Direct",
        TextKey::HlsSegmenter => "Segmenter HLS",
        TextKey::Hours => "godz.",
        TextKey::IdleBehavior => "Zachowanie bezczynności",
        TextKey::IdleBehaviorHelp => "Określa zachowanie transkodowania po rozłączeniu wszystkich klientów",
        TextKey::Info => "Informacja",
        TextKey::Import => "Import",
        TextKey::IsEnabled => "Włączony",
        TextKey::Json => "JSON",
        TextKey::KeepRunning => "Nie zatrzymuj",
        TextKey::Key => "Klucz",
        TextKey::LastMidFillerHelp => "sekundy od ostatniego wypełniacza śródemisji",
        TextKey::LastPage => "Ostatnia strona",
        TextKey::Level => "Poziom",
        TextKey::LoadingMediaInfo => "Ładowanie informacji o multimediach...",
        TextKey::Logs => "Dzienniki",
        TextKey::Logo => "Logo",
        TextKey::MediaItem => "Element multimedialny",
        TextKey::Message => "Komunikat",
        TextKey::MissingMediaItemQueryParameter => "Brak parametru zapytania mediaItem",
        TextKey::Mirror => "Odbicie",
        TextKey::MirrorOptionsHelp => {
            "Dostępne tylko wtedy, gdy źródło emisji jest ustawione na Odbicie."
        }
        TextKey::MirrorSourceChannel => "Kanał źródłowy odbicia",
        TextKey::MusicVideoCreditsMode => "Tryb napisów końcowych teledysków",
        TextKey::MusicVideoCreditsTemplate => "Szablon napisów końcowych teledysków",
        TextKey::NoMatchingRecordsFound => "Nie znaleziono pasujących rekordów",
        TextKey::NoFfmpegProfiles => "Nie znaleziono profili FFmpeg. Najpierw utwórz profil.",
        TextKey::NoBlockPlayoutSelected => "Wybierz emisję blokową, aby sprawdzić jej bloki.",
        TextKey::NoBlocks => "Nie znaleziono bloków.",
        TextKey::NoHistory => "Nie znaleziono historii.",
        TextKey::NoMessages => "Brak komunikatów.",
        TextKey::NvidiaCapabilitiesNotCollected => "Możliwości NVIDIA nie zostały jeszcze zebrane.",
        TextKey::Number => "Numer",
        TextKey::OnDemand => "Na żądanie",
        TextKey::MatchedPointsHelp => {
            "liczba punktów śródemisji, które już pasowały do wyrażenia"
        },
        TextKey::Pass => "Sukces",
        TextKey::ParityDebt => "ParityDebt",
        TextKey::PlaybackTroubleshooting => "Diagnostyka odtwarzania",
        TextKey::Play => "Odtwórz",
        TextKey::PlayoutTimeline => "Oś czasu emisji",
        TextKey::PlayoutMode => "Tryb emisji",
        TextKey::PlayoutModeHelp => "Określa postęp emisji kanału",
        TextKey::PlayoutOffset => "Przesunięcie emisji",
        TextKey::PlayoutSource => "Źródło emisji",
        TextKey::PlayoutSourceHelp => "Określa źródło treści kanału. Nie można tego zmienić, gdy kanał ma już emisję.",
        TextKey::Preview => "Podgląd",
        TextKey::PreviewBlockPlayout => "Podgląd emisji bloku",
        TextKey::PreviewChannel => "Podgląd kanału",
        TextKey::ChannelPreview => "Podgląd kanału",
        TextKey::ReloadStream => "Wczytaj transmisję ponownie",
        TextKey::StopPreview => "Zatrzymaj podgląd",
        TextKey::OpenFullPlayer => "Otwórz pełny odtwarzacz",
        TextKey::BackToChannels => "Wróć do kanałów",
        TextKey::NowPlaying => "Teraz",
        TextKey::UpNext => "Następnie",
        TextKey::ProgrammeGuide => "Program",
        TextKey::EpgPreview => "Podgląd EPG",
        TextKey::EpgPreviewDescription => {
            "Przeglądaj opublikowany program z dwiema godzinami historii i 24 godzinami do przodu."
        }
        TextKey::EpgZoom => "Powiększenie",
        TextKey::EpgNow => "Teraz",
        TextKey::NoEpgChannels => "Brak włączonych kanałów widocznych w EPG.",
        TextKey::ScheduleGap => "Przerwa w harmonogramie",
        TextKey::Filler => "Wypełnienie",
        TextKey::NoProgramming => "W tym przedziale nie zaplanowano programu.",
        TextKey::ChannelDisabledPreview => "Ten kanał jest wyłączony.",
        TextKey::ChannelNotAiring => "Ten kanał obecnie nie nadaje.",
        TextKey::ChannelNoPlayout => "Ten kanał nie ma emisji.",
        TextKey::PreviewIdle => "Podgląd uruchamia się automatycznie z wyciszonym dźwiękiem.",
        TextKey::PreviewConnecting => "Łączenie z transmisją na żywo…",
        TextKey::PreviewLive => "Transmisja na żywo jest odtwarzana.",
        TextKey::PreviewPaused => "Podgląd wstrzymany.",
        TextKey::PreviewRecovering => "Odzyskiwanie odtwarzania…",
        TextKey::PreviewStopped => "Podgląd zatrzymany.",
        TextKey::PreviewUnsupported => "Ta przeglądarka nie odtwarza wideo HLS.",
        TextKey::PreviewAuthorizationFailed => "Autoryzacja podglądu nie powiodła się.",
        TextKey::PreviewUnavailable => "Transmisja na żywo jest niedostępna.",
        TextKey::PreviewCapacityReached => "Wszystkie miejsca transkodowania są zajęte.",
        TextKey::PreviewNetworkError => "Podgląd utracił połączenie sieciowe.",
        TextKey::PreviewMediaError => "Przeglądarka nie może zdekodować skonfigurowanego wyjścia kanału.",
        TextKey::PreviewDerivativeNote => {
            "Podgląd administracyjny używa zgodnej z przeglądarką wersji H.264/AAC; skonfigurowane wyjście kanału pozostaje bez zmian."
        }
        TextKey::JavascriptRequiredPlayback => "JavaScript jest wymagany do interaktywnego odtwarzania.",
        TextKey::ConfirmStopTranscodeSession => "Zatrzymać współdzieloną sesję transkodowania tego kanału?",
        TextKey::PreviewPlaylistPlayout => "Podgląd emisji playlisty",
        TextKey::PreferredAudioTitle => "Preferowany tytuł audio",
        TextKey::PreferredSubtitleLanguage => "Preferowany język napisów",
        TextKey::QsvCapabilitiesNotCollected => "Możliwości QSV nie zostały jeszcze zebrane.",
        TextKey::RemoveStoredLogo => "Usuń zapisane logo",
        TextKey::SaveChannel => "Zapisz kanał",
        TextKey::SaveChannels => "Zapisz kanały",
        TextKey::Second => "sekunda",
        TextKey::Seconds => "sekundy",
        TextKey::SelectProfile => "- Wybierz profil -",
        TextKey::SequentialSchedule => "Harmonogram sekwencyjny",
        TextKey::SequentialScheduleValidation => "Walidacja harmonogramu sekwencyjnego",
        TextKey::SeekSeconds => "Sekundy przewijania",
        TextKey::Schema => "Schemat",
        TextKey::ShowDisabled => "Pokaż wyłączone",
        TextKey::ShowInEpg => "Pokaż w EPG",
        TextKey::SlugSeconds => "Sekundy przerwy",
        TextKey::SlugSecondsHelp => "Sekundy czarnego obrazu i ciszy dodawane między każdym elementem emisji",
        TextKey::Song => "Utwór",
        TextKey::SongVideoMode => "Tryb wideo utworu",
        TextKey::SortById => "Sortuj według identyfikatora",
        TextKey::SortByLevel => "Sortuj według poziomu",
        TextKey::SortByName => "Sortuj według nazwy",
        TextKey::SortByTimestamp => "Sortuj według czasu",
        TextKey::StopOnDisconnect => "Zatrzymaj po rozłączeniu",
        TextKey::StopScheduling => "Zatrzymaj harmonogramowanie",
        TextKey::StopSchedulingBlockItems => "Zatrzymaj harmonogramowanie elementów bloku",
        TextKey::StopTranscodeSession => "Zatrzymaj transkodowanie",
        TextKey::StartFromBeginning => "Rozpocznij od początku",
        TextKey::StreamSelector => "Selektor strumienia",
        TextKey::StreamSelectorMode => "Tryb selektora strumienia",
        TextKey::StreamingMode => "Tryb strumieniowania",
        TextKey::Styles => "Style",
        TextKey::Subtitle => "Napisy",
        TextKey::SubtitleMode => "Tryb napisów",
        TextKey::SubtitleModeHelp => {
            "Wymuszone pokazuje tylko wymuszone ścieżki narracyjne (forced narrative). Wybierz \
             Dowolne, aby zawsze wyświetlać napisy w preferowanym języku."
        }
        TextKey::Timestamp => "Czas",
        TextKey::Timeline => "Oś czasu",
        TextKey::Target => "Cel",
        TextKey::Tags => "Tagi",
        TextKey::Tools => "Narzędzia",
        TextKey::TranscodeMode => "Tryb transkodowania",
        TextKey::TranscodeModeHelp => "Określa, kiedy proces transkodowania jest aktywny",
        TextKey::TroubleshootPlayback => "Diagnozuj odtwarzanie",
        TextKey::TroubleshootChannel => "Diagnozuj kanał",
        TextKey::TroubleshootingReports => "Raporty diagnostyczne",
        TextKey::MpegTsTroubleshooting => "Diagnostyka MPEG-TS",
        TextKey::InternalMpegTs => "Wewnętrzny MPEG-TS",
        TextKey::PublicMpegTs => "Publiczny MPEG-TS",
        TextKey::RetainRawSample => "Zachowaj surową próbkę",
        TextKey::TsdDuckVersion => "Wersja TSDuck",
        TextKey::Report => "Raport",
        TextKey::RunId => "Identyfikator uruchomienia",
        TextKey::Status => "Status",
        TextKey::Summary => "Podsumowanie",
        TextKey::SampleBytes => "Bajty próbki",
        TextKey::RawSampleArchived => "Surowa próbka zarchiwizowana",
        TextKey::ArchivePath => "Ścieżka archiwum",
        TextKey::SampleUrl => "URL próbki",
        TextKey::StabilityMeter => "Miernik stabilności",
        TextKey::Stable => "Stabilne",
        TextKey::Packets => "pakiety",
        TextKey::Services => "Usługi",
        TextKey::Pids => "PID-y",
        TextKey::ContinuityFindings => "Wyniki ciągłości",
        TextKey::PcrFindings => "Wyniki PCR",
        TextKey::Bitrate => "Przepływność",
        TextKey::AnalysisJson => "JSON analizy",
        TextKey::StandardOutput => "Wyjście standardowe",
        TextKey::StandardError => "Błąd standardowy",
        TextKey::NotAvailable => "Niedostępne",
        TextKey::NotApplicable => "Nie dotyczy",
        TextKey::Completed => "Ukończono",
        TextKey::Failed => "Niepowodzenie",
        TextKey::TroubleshootingPlaybackStartDebt => {
            "Podgląd odtwarzania elementu multimedialnego i kanału zapisuje teraz raporty FFmpeg, logi, media_info.json, troubleshooting_info.json, wybrany YAML selektora strumienia i pliki możliwości, wykonuje reguły selektora strumienia oraz przekierowuje do /iptv/session/.troubleshooting/live.m3u8."
        },
        TextKey::UnableToLoadMediaInfo => "Nie można wczytać informacji o multimediach",
        TextKey::UnableToLoadPlaybackTroubleshooting => {
            "Nie można wczytać kontekstu diagnostyki odtwarzania"
        },
        TextKey::UploadLogo => "Prześlij logo",
        TextKey::UploadLogoHelp => "Wybierz lokalny obraz, aby zastąpić zapisane logo kanału przy zapisie",
        TextKey::Warning => "Ostrzeżenie",
        TextKey::Watermark => "Znak wodny",
        TextKey::WatermarkIds => "Identyfikatory znaków wodnych",
        TextKey::Window => "Okno",
        TextKey::WithProgress => "Z postępem",
        TextKey::Writers => "Scenarzyści",
        TextKey::Validate => "Waliduj",
        TextKey::Yaml => "YAML",
        TextKey::YamlFile => "Plik YAML",
        TextKey::Messages => "Komunikaty",
        TextKey::CollectionName => "Nazwa kolekcji",
        TextKey::MediaItemType => "Typ elementu multimedialnego",
        TextKey::MediaItemTitle => "Tytuł elementu multimedialnego",
        TextKey::ActualSize => "Rzeczywisty rozmiar",
        TextKey::AddWatermark => "Dodaj znak wodny",
        TextKey::AspectRatioGuides => "Prowadnice proporcji obrazu",
        TextKey::BottomLeft => "Na dole po lewej",
        TextKey::BottomMiddle => "Na dole pośrodku",
        TextKey::BottomRight => "Na dole po prawej",
        TextKey::CopyWatermark => "Kopiuj znak wodny",
        TextKey::CopyOf => "Kopia",
        TextKey::CurrentImage => "Bieżący obraz",
        TextKey::Custom => "Niestandardowy",
        TextKey::ChannelLogoPreviewUnavailable => {
            "Podgląd logo kanału wymaga kontekstu kanału. Wybierz tutaj niestandardowy obraz."
        }
        TextKey::DeleteWatermark => "Usuń znak wodny",
        TextKey::DurationSeconds => "Czas trwania (sekundy)",
        TextKey::EditWatermark => "Edytuj znak wodny",
        TextKey::EnableWatermarkToPreview => "Włącz znak wodny, aby zobaczyć jego położenie.",
        TextKey::Frequency => "Częstotliwość",
        TextKey::GeneratingPreview => "Generowanie podglądu...",
        TextKey::HorizontalMarginPercent => "Margines poziomy (%)",
        TextKey::GuideFourThree => "Bezpieczna ramka 4:3",
        TextKey::Identity => "Tożsamość",
        TextKey::Image => "Obraz",
        TextKey::ImageSource => "Źródło obrazu",
        TextKey::Intermittent => "Przerywany",
        TextKey::IntermittentTiming => "Czas przerywany",
        TextKey::LeftMiddle => "Po lewej pośrodku",
        TextKey::Location => "Położenie",
        TextKey::MiddleCenter => "Środek",
        TextKey::Mode => "Tryb",
        TextKey::Minutes => "minut",
        TextKey::Moods => "Nastroje",
        TextKey::NewName => "Nowa nazwa",
        TextKey::NativeAssetDimensions => "Rozmiar źródłowy i renderowany",
        TextKey::NextWatermark => "Następny znak wodny",
        TextKey::Opacity => "Przezroczystość",
        TextKey::OpacityExpression => "Wyrażenie przezroczystości",
        TextKey::OpacityOneToHundred => "Przezroczystość (1-100)",
        TextKey::Permanent => "Stały",
        TextKey::PerPass => "na przebieg",
        TextKey::PlaceWithinSourceContent => "Umieść w treści źródłowej",
        TextKey::Placement => "Umieszczenie",
        TextKey::PlacementPreview => "Podgląd umieszczenia",
        TextKey::PreviewFailed => "Podgląd nie powiódł się",
        TextKey::PreviewReferenceFullHd => "Ramka referencyjna Full HD · 1920×1080",
        TextKey::PreviousWatermark => "Poprzedni znak wodny",
        TextKey::RightMiddle => "Po prawej pośrodku",
        TextKey::SaveWatermark => "Zapisz znak wodny",
        TextKey::Scaled => "Skalowany",
        TextKey::SelectCustomImageToPreview => {
            "Wybierz lub prześlij niestandardowy obraz do podglądu."
        }
        TextKey::Size => "Rozmiar",
        TextKey::TopLeft => "Na górze po lewej",
        TextKey::TopMiddle => "Na górze pośrodku",
        TextKey::TopRight => "Na górze po prawej",
        TextKey::UploadFailed => "Przesyłanie nie powiodło się.",
        TextKey::UploadImage => "Prześlij obraz",
        TextKey::UnsavedChanges => "Niezapisane zmiany",
        TextKey::UnsavedWatermarkChangesMessage => {
            "Odrzucić zmiany znaku wodnego i otworzyć wybrany znak wodny?"
        }
        TextKey::VerticalMarginPercent => "Margines pionowy (%)",
        TextKey::WatermarkCopied => "Znak wodny został skopiowany.",
        TextKey::WatermarkNotFound => "Nie znaleziono znaku wodnego.",
        TextKey::WatermarkPlacementPreview => "podgląd położenia znaku wodnego",
        TextKey::WatermarkPreview => "podgląd znaku wodnego",
        TextKey::WatermarkNavigation => "Nawigacja po znakach wodnych",
        TextKey::WidthPercent => "Szerokość (%)",
        TextKey::ZIndex => "Indeks Z",
        TextKey::DiscardChanges => "Odrzuć zmiany",
        TextKey::XmltvBlockBehavior => "Zachowanie bloków XMLTV",
        TextKey::XmltvTimeZone => "Strefa czasowa XMLTV",
        TextKey::Xmltv => "XMLTV",
        TextKey::Badge => "Odznaka",
        TextKey::Buttons => "Przyciski",
        TextKey::CheckValue => "Zaznaczenie",
        TextKey::ChannelName => "Nazwa kanału",
        TextKey::ChannelNumber => "Numer kanału",
        TextKey::ComponentGallery => "Galeria komponentów ErsatzRS",
        TextKey::DangerButton => "Niebezpieczny",
        TextKey::DeleteDialog => "Okno usuwania",
        TextKey::Decrement => "Zmniejsz",
        TextKey::Dialogs => "Okna dialogowe",
        TextKey::DisabledButton => "Wyłączony",
        TextKey::EnterName => "Wpisz nazwę",
        TextKey::ErrorOccurred => "Wystąpił błąd.",
        TextKey::ErrorToast => "Komunikat błędu",
        TextKey::ExampleModal => "Przykładowe okno",
        TextKey::ExampleModalBody => "To jest przykładowa treść okna modalnego.",
        TextKey::FormInputs => "Pola formularza",
        TextKey::Icons => "Ikony",
        TextKey::IncrementBadge => "Zwiększ odznakę",
        TextKey::InfoToast => "Komunikat informacyjny",
        TextKey::JustFyi => "Tylko informacyjnie.",
        TextKey::LinkButton => "Przycisk linku",
        TextKey::ModalCancelled => "Anulowano.",
        TextKey::ModalConfirmed => "Potwierdzono!",
        TextKey::NumValue => "Liczba",
        TextKey::OpenModal => "Otwórz okno",
        TextKey::OperationSucceeded => "Operacja zakończona powodzeniem.",
        TextKey::PagedTableDemo => "PagedTable (5 wierszy na stronę, 7 łącznie)",
        TextKey::PrimaryButton => "Podstawowy",
        TextKey::SecondaryButton => "Drugorzędny",
        TextKey::SelectValue => "Wybór",
        TextKey::SomethingMightBeWrong => "Coś może być nie tak.",
        TextKey::SuccessToast => "Komunikat sukcesu",
        TextKey::TextButton => "Tekst",
        TextKey::TextValue => "Tekst",
        TextKey::Toasts => "Powiadomienia",
        TextKey::WarningToast => "Ostrzeżenie",
        TextKey::YamlScript => "Skrypt YAML",
        TextKey::Yes => "Tak",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::i18n::{
        addon_media_list_error_text, episode_display_subtitle_text, ffmpeg_profile_error_text,
        ffmpeg_profile_video_format_text, ffmpeg_tonemap_algorithm_text, filler_error_text,
        hardware_acceleration_option_text, library_kind_text, log_level_text,
        media_action_error_text, media_browser_error_text, media_kind_text,
        media_source_error_text, playout_schedule_kind_text, schedule_error_text,
        scheduling_content_error_text, settings_error_text, upload_error_text,
        watermark_error_text, UiLanguage,
    };

    #[test]
    fn season_display_names() {
        assert_eq!(STRINGS.season_display_name(0), "Odcinki specjalne");
    }

    #[test]
    fn episode_display_subtitles() {
        assert_eq!(
            episode_display_subtitle_text(UiLanguage::Polish, Some("Show"), Some(2), Some(5))
                .as_deref(),
            Some("Show - sezon 2, odcinek 5")
        );
    }

    #[test]
    fn fallback_item_labels() {
        assert_eq!(
            STRINGS.schedule_item_fallback_label(7),
            "Element harmonogramu #7"
        );
    }

    #[test]
    fn log_level_labels() {
        assert_eq!(log_level_text(UiLanguage::Polish, "Warning"), "Ostrzeżenie");
    }

    #[test]
    fn channel_labels() {
        assert_eq!(STRINGS.channel_streaming_mode_label(1), "MPEG-TS (starszy)");
    }

    #[test]
    fn media_and_library_kind_labels() {
        assert_eq!(media_kind_text(UiLanguage::Polish, 7), "Zdalne strumienie");
        assert_eq!(library_kind_text(UiLanguage::Polish, "Plex"), "Plex");
    }

    #[test]
    fn playout_schedule_kind_labels() {
        assert_eq!(playout_schedule_kind_text(UiLanguage::Polish, 2), "Blokowy");
    }

    #[test]
    fn hardware_acceleration_labels() {
        assert_eq!(
            hardware_acceleration_option_text(UiLanguage::Polish, "NVENC", false),
            "NVENC (niedostępne w tym systemie)"
        );
    }

    #[test]
    fn ffmpeg_option_labels() {
        assert_eq!(
            ffmpeg_tonemap_algorithm_text(UiLanguage::Polish, 0),
            "Liniowy"
        );
        assert_eq!(
            ffmpeg_profile_video_format_text(UiLanguage::Polish, 99, 0),
            "Kopiuj"
        );
    }

    #[test]
    fn deco_template_action_labels() {
        assert_eq!(
            STRINGS.text(TextKey::SaveDecoTemplate),
            "Zapisz szablon dekoracji"
        );
    }

    #[test]
    fn server_errors_follow_language() {
        assert_eq!(
            ffmpeg_profile_error_text(UiLanguage::Polish, "Thread count must be 0 or greater."),
            "Liczba wątków musi wynosić 0 lub więcej."
        );
        assert_eq!(
            schedule_error_text(UiLanguage::Polish, "Collection is required."),
            "Kolekcja jest wymagana."
        );
        assert_eq!(
            scheduling_content_error_text(UiLanguage::Polish, "Template group name must be unique"),
            "Nazwa grupy szablonów musi być unikalna"
        );
        assert_eq!(
            scheduling_content_error_text(
                UiLanguage::Polish,
                "Deco may only contain one [Block Start] break content"
            ),
            "Dekoracja może zawierać tylko jedną treść przerwy na początku bloku."
        );
        assert_eq!(
            watermark_error_text(UiLanguage::Polish, "Watermark name must be unique"),
            "Nazwa znaku wodnego musi być unikalna"
        );
        assert_eq!(
            settings_error_text(
                UiLanguage::Polish,
                "Library refresh interval must be 0 (do not refresh) or greater than zero"
            ),
            "Interwał odświeżania biblioteki musi wynosić 0 (bez odświeżania) albo więcej niż zero."
        );
        assert_eq!(
            media_action_error_text(UiLanguage::Polish, "Collection name is required"),
            "Nazwa kolekcji jest wymagana."
        );
        assert_eq!(
            filler_error_text(UiLanguage::Polish, "Duration must not be empty"),
            "Czas trwania nie może być pusty."
        );
        assert_eq!(
            media_browser_error_text(UiLanguage::Polish, "Smart collection query is required"),
            "Zapytanie kolekcji inteligentnej jest wymagane."
        );
        assert_eq!(
            media_browser_error_text(UiLanguage::Polish, "[MediaItem] does not exist."),
            "Element multimedialny nie istnieje."
        );
        assert_eq!(
            media_source_error_text(UiLanguage::Polish, "API key is required."),
            "Klucz API jest wymagany."
        );
        assert_eq!(
            addon_media_list_error_text(
                UiLanguage::Polish,
                &format!(
                    "invoke media-list add-on is unavailable: {}",
                    ersatzrs_addon_contract::AddonOperationError::new(
                        "missing-secret",
                        "Provider detail."
                    )
                )
            ),
            "Skonfiguruj wymagane odwołanie do sekretu w ustawieniach dodatku. Provider detail."
        );
        assert_eq!(
            upload_error_text(UiLanguage::Polish, "No file field"),
            "Nie znaleziono pola pliku."
        );
    }
}
