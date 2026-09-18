use crate::admin::i18n::{LanguageStrings, TextKey};

pub const SORT_ORDER: u16 = 90;
pub const CODE: &str = "it-it";
pub const LABEL: &str = "Italiano (Italia)";
pub const ALIASES: &[&str] = &["it", "it-it", "it_it"];

pub struct ItalianStrings;

pub static STRINGS: ItalianStrings = ItalianStrings;

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

impl LanguageStrings for ItalianStrings {
    fn text(&self, key: TextKey) -> &'static str {
        text(key)
    }

    fn selected_count(&self, count: usize) -> String {
        format!("{count} selezionato")
    }

    fn item_count(&self, count: u64) -> String {
        if count == 1 {
            "1 elemento".to_owned()
        } else {
            format!("{count} elementi")
        }
    }

    fn range_of(&self, start: u32, end: u32, total: u64) -> String {
        format!("{start}-{end} di {total}")
    }

    fn match_status(&self, matched: u64, total: u64) -> String {
        format!("{matched} di {total}")
    }

    fn rows_per_page(&self, page_size: u32) -> String {
        format!("Righe per pagina: {page_size}")
    }

    fn page_of(&self, page: u32, total: u32) -> String {
        format!("Pagina {page} di {total}")
    }

    fn save_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Salvataggio fallito: {error}")
    }

    fn delete_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Eliminazione fallita: {error}")
    }

    fn empty_trash_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Svuatura cestino fallita: {error}")
    }

    fn search_count(&self, total: u32, label: &'static str) -> String {
        format!("{total} {label}")
    }

    fn selected_items_count(&self, count: usize) -> String {
        if count == 1 {
            "1 elemento selezionato".to_owned()
        } else {
            format!("{count} elementi selezionati")
        }
    }

    fn delete_selected_count(&self, count: usize) -> String {
        format!("Elimina selezionati ({count})")
    }

    fn deleted_items_from_database(&self, count: u64) -> String {
        if count == 1 {
            "Eliminato 1 elemento dal database.".to_owned()
        } else {
            format!("Eliminati {count} elementi dal database.")
        }
    }

    fn season_display_name(&self, number: i64) -> String {
        if number == 0 {
            "Speciali".to_owned()
        } else {
            format!("Stagione {number}")
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
        format!("Elemento media #{id}")
    }

    fn schedule_item_fallback_label(&self, id: i64) -> String {
        format!("Elemento programma #{id}")
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
            2 => "HLS diretto".to_owned(),
            4 => "HLS Segmenter".to_owned(),
            5 => "MPEG-TS".to_owned(),
            _ => format!("Sconosciuto ({mode})"),
        }
    }

    fn channel_language_label(&self, code: &str) -> String {
        match code.trim().to_ascii_lowercase().as_str() {
            "" => String::new(),
            "eng" | "en" => "Inglese".to_owned(),
            "nld" | "dut" | "nl" => "Olandese".to_owned(),
            "deu" | "ger" | "de" => "Tedesco".to_owned(),
            "fra" | "fre" | "fr" => "Francese".to_owned(),
            "spa" | "es" => "Spagnolo".to_owned(),
            "ita" | "it" => "Italiano".to_owned(),
            "jpn" | "ja" => "Giapponese".to_owned(),
            "kor" | "ko" => "Coreano".to_owned(),
            "zho" | "chi" | "zh" => "Cinese".to_owned(),
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
            _ => "Sconosciuto",
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
            format!("{label} (non disponibile su questo sistema)")
        }
    }

    fn ffmpeg_tonemap_algorithm_label(&self, algorithm: i32) -> String {
        match algorithm {
            0 => "Lineare",
            1 => "Clip",
            2 => "Gamma",
            3 => "Reinhard",
            4 => "Mobius",
            5 => "Hable",
            _ => return format!("Sconosciuto ({algorithm})"),
        }
        .to_owned()
    }

    fn ffmpeg_output_dynamic_range_label(&self, range: i32) -> String {
        match range {
            0 => "SDR (BT.709)",
            1 => "HDR10 (BT.2020 PQ)",
            _ => return format!("Sconosciuto ({range})"),
        }
        .to_owned()
    }

    fn ffmpeg_loudness_mode_label(&self, mode: i32) -> String {
        match mode {
            0 => text(TextKey::Off),
            1 => "loudnorm",
            _ => return format!("Sconosciuto ({mode})"),
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
        format!("Cestino svuato: eliminati {count} elementi.")
    }

    fn select_collection_to_add_selected_items(&self, count: usize) -> String {
        format!("Seleziona la collezione per aggiungere gli {count} elementi selezionati")
    }

    fn select_playlist_to_add_selected_items(&self, count: usize) -> String {
        format!("Seleziona la playlist per aggiungere gli {count} elementi selezionati")
    }

    fn added_items_to_collection(&self, count: usize) -> String {
        format!("Aggiunti {count} elementi alla collezione")
    }

    fn added_items_to_playlist(&self, count: usize) -> String {
        format!("Aggiunti {count} elementi alla playlist")
    }

    fn add_to_collection_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Aggiunta alla collezione fallita: {error}")
    }

    fn add_to_playlist_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Aggiunta alla playlist fallita: {error}")
    }

    fn no_media_source_configured(&self, provider: &'static str) -> String {
        format!(
            "Nessuna sorgente {provider} configurata. Clicca su Configura connessione per aggiungerne una."
        )
    }

    fn provider_connection_title(&self, provider: &'static str) -> String {
        format!("Connessione {provider}")
    }

    fn provider_libraries_title(&self, provider: &'static str) -> String {
        format!("Biblioteche {provider}")
    }

    fn provider_path_replacements_title(&self, provider: &'static str) -> String {
        format!("Sostituzioni percorso {provider}")
    }

    fn provider_path_label(&self, provider: &'static str) -> String {
        format!("Percorso {provider}")
    }

    fn health_summary(&self, errors: u32, warnings: u32) -> String {
        format!(
            "{} {}, {} {}",
            errors,
            if errors == 1 { "errore" } else { "errori" },
            warnings,
            if warnings == 1 { "avviso" } else { "avvisi" },
        )
    }

    fn classic_playout_state_message(&self, playouts: &str) -> String {
        format!(
            "Lo stato della riproduzione classica non consente un aggiornamento sicuro per: {playouts}. Usa Reimposta riproduzione per stati precedenti o incompatibili; Aggiorna non cancella mai una finestra senza un checkpoint completo."
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
            "Lo stato dell'aggiornamento dopo la scansione richiede attenzione: {unresolved_resolutions} risoluzioni dell'impatto non completate, {stale_resolutions} risoluzioni dell'impatto bloccate, {failed_intents} richieste di aggiornamento del playout non riuscite e {stale_intents} richieste di aggiornamento del playout bloccate. Controlla i log dello scanner e della pianificazione, quindi riprova la scansione o l'aggiornamento del playout interessato."
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
            "{recent} canale/i hanno distribuito contenuti HLS in diretta più lentamente del tempo reale negli ultimi {minutes} minuti; {active} non avevano ancora recuperato all'ultima osservazione; {fallbacks} canale/i usano un ripiego verificato alla codifica software e/o a una qualità inferiore per un massimo di {fallback_hours} ore. Controlla i log di streaming ed esegui il test di riproduzione TSDuck sensibile alla velocità di consegna prima di modificare le impostazioni del provider."
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
            "La cardinalità delle immagini richiede attenzione: {show_duplicate_rows} righe duplicate di immagini della serie, {season_duplicate_rows} righe duplicate di immagini della stagione e {episode_duplicate_rows} righe duplicate di immagini dell'episodio; i conteggi della proiezione televisiva sono {published_items} elementi episodio pubblicati da elenchi di add-on, {projection_rows} righe di proiezione, {distinct_episodes} episodi proiettati distinti, {complete_hierarchy_rows} proiezioni con gerarchia completa e {hierarchy_episodes} episodi nella gerarchia. Aggiorna o ripubblica gli elenchi televisivi degli add-on interessati."
        )
    }

    fn unable_to_load_playback_troubleshooting(&self, error: &dyn std::fmt::Display) -> String {
        format!(
            "Impossibile caricare il contesto di risoluzione dei problemi della riproduzione: {error}"
        )
    }

    fn playout_reset_prompt(&self, channel: &str, schedule: &str) -> String {
        format!(
            "Inserisci l'orario in cui la riproduzione sul canale {channel} con programma {schedule} dovrebbe resettarsi ogni giorno"
        )
    }
}

pub fn text(key: TextKey) -> &'static str {
    match key {
        TextKey::ItemDetailsNotRequested => "Dettagli non recuperati",
        TextKey::ItemDetailsFailed => "Recupero dei dettagli non riuscito",
        TextKey::ItemDetailsRetry => "Dettagli in attesa di un nuovo tentativo",
        TextKey::ItemProviderPlayback => "Riproduzione del fornitore",
        TextKey::ItemPosition => "Posizione",
        TextKey::ItemPrevious => "Precedente",
        TextKey::ItemNext => "Successivo",
        TextKey::ItemBack => "Torna agli elementi",
        TextKey::ItemTools => "Strumenti dell’elemento",
        TextKey::ItemEpisodePosition => "Posizione dell’episodio",
        TextKey::ItemNumbered => "Stagione ed episodio",
        TextKey::ItemUnnumberedHelp => "Questo elemento non avrà un numero di stagione o episodio dopo il salvataggio.",
        TextKey::ItemNumberingHelp => "Un elemento numerato richiede entrambi i valori. La stagione 0 è per gli speciali.",
        TextKey::ItemMetadata => "Riassunto, generi e tag",
        TextKey::ItemMetadataHelp => "Altri titoli, lingue e crediti",
        TextKey::ItemSaveHelp => "Salva le modifiche per applicarle automaticamente.",
        TextKey::ItemEditNumbering => "Modifica numerazione nella lista",
        TextKey::ItemReplacementHelp => "Scegliere o rimuovere una sostituzione modifica questa bozza. Usa Salva elemento per conservarla.",
        TextKey::ItemReplacementPending => "La modifica della sostituzione non è salvata.",
        TextKey::ItemSearchReplacement => "Cerca media locali",
        TextKey::ItemRemoveReplacement => "Rimuovi sostituzione",
        TextKey::ItemInformation => "Informazioni e provenienza del media",
        TextKey::ItemInformationHelp => "Audio, sottotitoli, dettagli del flusso e opzioni del fornitore",
        TextKey::ItemSavedToolsHelp => "Queste azioni usano l’elemento salvato. Le modifiche non salvate sono escluse.",
        TextKey::ItemViewMedia => "Visualizza dettagli del media",
        TextKey::ItemSavedValue => "Valore salvato",
        TextKey::AddonWorkspaceSelectFirst => "Seleziona prima gli elementi da modificare.",
        TextKey::AddonsNoneInstalled => "Nessun componente aggiuntivo è ancora installato.",
        TextKey::AddonsNoneAvailable => "Le tue fonti non offrono ancora componenti aggiuntivi.",
        TextKey::AddonsSetupHelp => "Apri Fonti per aggiungere un repository o installare da un file.",
        TextKey::AddonPackageHelp => "Usa questa riga per uno ZIP contenente un solo componente aggiuntivo.",
        TextKey::AddonRepositoryPackageHelp => "Usa questa riga per uno ZIP di repository, come ErsatzRS-addons.zip, contenente un catalogo di componenti aggiuntivi.",
        TextKey::AddonPackageUploadHelp => "Impossibile installare questo componente aggiuntivo. Per uno ZIP di repository, usa la riga Pacchetto repository qui sotto.",
        TextKey::AddonRepositoryUploadHelp => "Impossibile aggiungere questo repository. Se lo ZIP contiene un solo componente aggiuntivo, usa la riga Pacchetto componente aggiuntivo qui sopra.",
        TextKey::AddonConfigurationSavedRecheck => "Salvato. Il componente aggiuntivo è disattivato finché non lo controlli e riattivi.",
        TextKey::AddonCheckRequired => "Configurazione modificata. Controlla di nuovo il componente aggiuntivo prima di attivarlo.",
        TextKey::AddonCheckPending => "Controllo del componente aggiuntivo…",
        TextKey::AddonWorkspaceIncludeSelected => "Includi gli elementi selezionati",
        TextKey::AddonWorkspaceExcludeSelected => "Escludi gli elementi selezionati",
        TextKey::AddonWorkspaceInclusionHelp => "Salva le modifiche alla selezione per applicarle automaticamente.",
        TextKey::AddonWorkspacePublicationFailed => "Pubblicazione non riuscita",
        TextKey::AddonWorkspacePublicationRestoredHelp => "La versione precedente è stata ripristinata. Le modifiche salvate vengono conservate e ritentate automaticamente. Attività mostra gli errori tecnici.",

        TextKey::AddonWorkspaceParentMetadataPreserved => "Elaborazione interrotta per preservare i metadati NFO principali modificati o non validi. Correggi il file prima di riprovare.",

        TextKey::AddonWorkspaceImporting => "Importazione della lista…",
        TextKey::AddonWorkspaceImportingHelp => "La lista si aprirà automaticamente al termine della ricerca. Senza JavaScript, usa Aggiorna.",
        TextKey::AddonWorkspaceImportSteps => "Importa la lista. I metadati e i contenuti necessari vengono elaborati e pubblicati automaticamente.",
        TextKey::AddonWorkspaceChooseProvider => "Scegli un componente aggiuntivo qui sotto se non ne viene rilevato uno per questo URL.",
        TextKey::AddonWorkspaceExcluded => "Escluso dalla pubblicazione",
        TextKey::ItemFromList => "Dalla lista",
        TextKey::ItemUseListSetting => "Usa impostazione della lista",
        TextKey::ItemOverride => "Valore dell’elemento",
        TextKey::ItemPackageDefault => "Valore del pacchetto; non ancora attivato",
        TextKey::ItemPreviewOptions => "Anteprima delle modifiche alle opzioni",
        TextKey::ItemAccessActive => "È già in corso una verifica dell’accesso.",
        TextKey::OpenItemInNewTab => "Apri in una nuova scheda",
        TextKey::BulkItemsToChange => "Elementi da modificare",
        TextKey::BulkScopeHelp => "La numerazione, il rilevamento delle stagioni e le modifiche del tipo di media usano questi elementi. Il filtro di durata vale per la numerazione e il rilevamento delle stagioni.",
        TextKey::BulkDetectSeasons => "Rileva stagioni",
        TextKey::BulkFirstSeason => "Prima stagione",
        TextKey::BulkReviewChanges => "Controlla le modifiche",
        TextKey::BulkPreviewSaves => "Applica salva questi valori; le modifiche salvate vengono pubblicate automaticamente.",
        TextKey::BulkPreviewNothing => "Questa proposta non modifica nulla.",
        TextKey::BulkApplyNumbering => "Applica numerazione",
        TextKey::BulkApplyChanges => "Applica modifiche",
        TextKey::BulkCancelPreview => "Annulla anteprima",
        TextKey::AddonWorkspaceLengthFilter => "Filtra per durata",
        TextKey::AddonWorkspaceMinimumLength => "Durata minima",
        TextKey::AddonWorkspaceMaximumLength => "Durata massima",
        TextKey::AddonWorkspaceUnknownLength => "Includi durate sconosciute",
        TextKey::AddonWorkspaceLengthHelp => "Minuti:secondi. Lascia vuoto per non impostare un limite.",
        TextKey::AddonWorkspaceReviewLatest => "Controlla l’ultima versione salvata",
        TextKey::AddonWorkspaceReviewRetained => "I dati inseriti sono conservati. Confrontali con gli ultimi valori salvati prima di salvare. Controlla separatamente le altre attività aperte.",
        TextKey::AddonWorkspaceStreamHelp => "Riproduci direttamente dalla sorgente.",
        TextKey::AddonWorkspaceDownloadHelp => "Prepara ogni video prima della riproduzione.",
        TextKey::AddonWorkspaceNoOptions => "Questo componente aggiuntivo non offre impostazioni di accesso o segmenti.",
        TextKey::AddonWorkspaceNewHelp => "Scarica i nuovi video importati; trasmetti quelli esistenti.",
        TextKey::AddonWorkspaceSizeLimit => "Questo modulo è troppo grande. Torna ai dati inseriti e salva una parte più piccola.",
        TextKey::AddonWorkspaceItems => "Elementi",
        TextKey::AddonWorkspaceSettings => "Impostazioni elenco",
        TextKey::AddonWorkspacePlayback => "Riproduzione e accesso",
        TextKey::AddonWorkspaceActivity => "Attività e ripristino",
        TextKey::AddonWorkspaceSections => "Sezioni elenco",
        TextKey::AddonWorkspaceSaveSettings => "Salva impostazioni",
        TextKey::AddonWorkspaceSaveItem => "Salva elemento",
        TextKey::AddonWorkspaceSaveNumbering => "Salva numerazione",
        TextKey::AddonWorkspacePublish => "Pubblica modifiche salvate",
        TextKey::AddonWorkspacePublished => "Elenco pubblicato aggiornato",
        TextKey::AddonWorkspacePending => "Le modifiche salvate vengono applicate automaticamente",
        TextKey::AddonWorkspacePreparationRequired => "I contenuti necessari vengono preparati automaticamente",
        TextKey::AddonWorkspacePreparing => "Preparazione dei contenuti multimediali per la pubblicazione",
        TextKey::AddonWorkspaceAddIncomplete => "Elenco aggiunto, ma alcune impostazioni non sono state salvate. Apri Impostazioni elenco, controllale e salva di nuovo.",
        TextKey::AddonWorkspacePreparationFailed => "Preparazione dei contenuti multimediali non riuscita",
        TextKey::RetryPreparation => "Riprova preparazione",
        TextKey::PreparationFailedItems => "Attività di preparazione",
        TextKey::PreparationFailureFailed => "Il componente aggiuntivo non è riuscito a preparare i contenuti multimediali. Riprova la preparazione e controlla il componente aggiuntivo se continua a non riuscire.",
        TextKey::PreparationFailureInputChanged => "La richiesta di preparazione non corrispondeva più all’elenco salvato o al componente aggiuntivo installato. Riprova la preparazione.",
        TextKey::PreparationFailureInvalidRequest => "Le opzioni salvate non corrispondono al componente aggiuntivo installato. Controlla Riproduzione e accesso, salva e riprova la preparazione.",
        TextKey::PreparationFailureCancelled => "La preparazione è stata annullata. Riprova la preparazione per ricominciare.",
        TextKey::PreparationFailureAttemptsExhausted => "La preparazione si è interrotta dopo ripetuti tentativi non riusciti. Riprova la preparazione e controlla il componente aggiuntivo se continua a non riuscire.",
        TextKey::PreparationFailureStorageFull => "Lo spazio di archiviazione non era sufficiente per preparare i contenuti multimediali. Libera spazio e riprova la preparazione.",
        TextKey::PreparationFailureUnsupported => "Il componente aggiuntivo non può preparare questi contenuti multimediali.",
        TextKey::PreviousSavedRevision => "Revisione salvata precedente",
        TextKey::AddonWorkspacePreparationActivity => "Preparazione dei contenuti multimediali",
        TextKey::AddonWorkspaceBulk => "Modifica più elementi",
        TextKey::AddonWorkspaceSelection => "Selezionati per la modifica",
        TextKey::AddonWorkspaceOffPage => "Su altre pagine",
        TextKey::AddonWorkspaceDiscard => "Scarta modifiche",
        TextKey::AddonWorkspaceSelected => "Elementi selezionati",
        TextKey::AddonWorkspaceMatching => "Tutti gli elementi corrispondenti",
        TextKey::AddonWorkspacePage => "Pagina corrente",
        TextKey::AddonWorkspaceAccess => "Accesso e segmenti",
        TextKey::AddonWorkspaceSaveConflict => "Le impostazioni salvate sono cambiate. Controlla la versione attuale prima di salvare.",
        TextKey::AddonWorkspaceSaveInvalid => "Impossibile convalidare queste impostazioni. Controlla i dati e riprova.",
        TextKey::AddonWorkspaceSaveStorage => "Impossibile salvare le impostazioni. I dati inseriti sono conservati; riprova.",
        TextKey::AddonWorkspaceLoadIncomplete => "Alcune informazioni dell’editor non sono state caricate in modo coerente. I dati inseriti sono conservati. Controlla la versione attuale per riprovare.",
        TextKey::TechnicalDetails => "Dettagli tecnici",
        TextKey::AddonPermissionNetworkHelp => "Connettersi agli host di rete elencati",
        TextKey::AddonPermissionMediaHelp => "Leggere i contenuti multimediali disponibili per ErsatzRS",
        TextKey::AddonPermissionFfmpegHelp => "Eseguire la versione gestita di FFmpeg",
        TextKey::AddonPermissionCommandHelp => "Eseguire i comandi esterni elencati",
        TextKey::PreparationTasks => "Attività di preparazione",
        TextKey::PreparationAffectedItems => "Elementi interessati",
        TextKey::PreparationIntervals => "Attività intervalli / SponsorBlock",
        TextKey::PreparationDownloads => "Attività di download",
        TextKey::PreparationSubtitles => "Attività sottotitoli",
        TextKey::PreparationQueued => "In coda",
        TextKey::PreparationRunning => "In esecuzione",
        TextKey::PreparationWaiting => "In attesa",
        TextKey::PreparationNotQueued => "Non in coda",
        TextKey::LastActivity => "Ultima attività",
        TextKey::AddonAccessUnknownFailure => "La causa di questo errore di accesso è sconosciuta. Controlla le impostazioni di accesso salvate, prova la fonte nel browser e ripeti la verifica.",
        TextKey::AddonWorkspaceRetained => "Impossibile salvare le modifiche. I dati inseriti sono conservati; controlla gli errori o ricarica la revisione corrente.",
        TextKey::AddonWorkspaceNavigation => "La navigazione mantiene le modifiche non salvate. Le modifiche salvate vengono applicate automaticamente.",
        TextKey::AddonWorkspaceSeasonOnly => "Modifica solo stagione",
        TextKey::AddonWorkspaceSeasonPerYear => "Nuova stagione per anno solare",
        TextKey::AddonWorkspacePageSize => "Elementi per pagina",

        TextKey::AddonAccessTest => "Verifica accesso",
        TextKey::AddonAccessHelp => {
            "Verifica questo elemento con le opzioni salvate della bozza. Le modifiche non salvate sono escluse; la riproduzione pubblicata resta invariata."
        }
        TextKey::AddonAccessSaveFirst => {
            "Salva prima un’opzione per attivare le impostazioni della bozza prima di verificare l’accesso."
        }
        TextKey::AddonAccessError => {
            "Impossibile caricare o modificare la verifica dell’accesso. Aggiorna l’elemento e riprova."
        }
        TextKey::AddonAccessNotChecked => "Non verificato",
        TextKey::AddonAccessQueued => "Verifica dell’accesso in coda",
        TextKey::AddonAccessRunning => "Verifica dell’accesso in corso",
        TextKey::AddonAccessWaiting => "In attesa di riprovare la verifica",
        TextKey::AddonAccessCancelled => "Verifica dell’accesso annullata",
        TextKey::AddonAccessStale => "Risultato dell’accesso scaduto o impostazioni modificate",
        TextKey::AddonAccessAccessible => "Accessibile",
        TextKey::AddonAccessProfileUnavailable => "Il profilo del browser selezionato non è disponibile per il server. Verifica il profilo salvato e ripeti il controllo di accesso.",
        TextKey::AddonAccessProfileLocked => "Il profilo del browser è bloccato. Chiudi il browser che usa questo profilo e ripeti il controllo di accesso.",
        TextKey::AddonAccessDecryptionFailed => "Il server non ha potuto decifrare le credenziali del browser. Verifica il profilo e l’account che esegue il server, poi riprova.",
        TextKey::AddonDerivedMetadataLimited => "I metadati della serie hanno raggiunto il limite di dimensione. Quelli degli episodi rimangono completi.",
        TextKey::AddonLocalPlaybackAvailable => "Copia locale riproducibile",
        TextKey::AddonAccessRestricted => "Il fornitore ha limitato l’accesso. Apri la fonte nel browser per verificare le restrizioni dell’account o della regione, poi ripeti il controllo di accesso.",
        TextKey::AddonAccessNextAttempt => "Prossimo tentativo:",
        TextKey::AddonAccessExpires => "Scadenza del risultato:",
        TextKey::AddonAccessRefresh => "Aggiorna stato",
        TextKey::AddonOptionsCurrentFilter => "Filtro corrente",
        TextKey::AddonOptions => "Opzioni del fornitore",
        TextKey::AddonOptionsDraftHelp => "Le opzioni salvate vengono applicate automaticamente. La riproduzione attuale resta disponibile durante l’elaborazione.",
        TextKey::AddonOptionsProvisional => {
            "Questi valori predefiniti del pacchetto sono provvisori. Salvare un’opzione li attiva per questa bozza."
        }
        TextKey::AddonOptionsListDefault => "Valore predefinito dell’elenco",
        TextKey::AddonOptionsInherited => "Ereditato",
        TextKey::AddonOptionsOverride => "Personalizzato",
        TextKey::AddonOptionsFiniteOnly => {
            "Non disponibile per elementi in diretta o di durata sconosciuta: questa opzione richiede contenuti di durata finita. La riproduzione diretta rimane disponibile."
        }
        TextKey::AddonOptionsResetPackage => "Ripristina valore del pacchetto",
        TextKey::AddonOptionsResetList => "Ripristina valore dell’elenco",
        TextKey::AddonOptionsMixed => "Valori diversi — scegli un valore",
        TextKey::AddonOptionsPreviewHelp => {
            "Controlla questi elementi specifici prima di applicare. Una modifica intermedia alla bozza o allo schema richiede una nuova anteprima."
        }
        TextKey::AddonOptionsBefore => "Prima",
        TextKey::AddonOptionsAfter => "Dopo",
        TextKey::AddonOptionsConflict => {
            "Impossibile salvare o caricare le opzioni. La bozza, la selezione o lo schema del pacchetto potrebbe essere cambiato. Ricarica l’editor e riprova."
        }
        TextKey::AddonOptionsNone => "Questo pacchetto non definisce opzioni.",
        TextKey::Addon => "Componente aggiuntivo",
        TextKey::PrimaryFeature => "Film principale",
        TextKey::MovieExtra => "Contenuto extra",
        TextKey::NeedReview => "Avvisi",

        TextKey::ReviewRecommended => "Revisione consigliata",
        TextKey::PublicationReadiness => "Preparazione alla pubblicazione",
        TextKey::PublicationBlockers => "Blocchi",
        TextKey::PublicationActivity => "Attività",
        TextKey::PublicationJobs => "attività di pubblicazione",
        TextKey::PublicationEvents => "eventi",
        TextKey::NoPublicationActivity => {
            "Non è stata ancora registrata alcuna attività di pubblicazione."
        }
        TextKey::CopyRedactedDiagnostics => "Copia diagnostica oscurata",
        TextKey::DownloadRedactedDiagnostics => "Scarica diagnostica oscurata",
        TextKey::RedactedDiagnosticsCopied => "Diagnostica oscurata copiata.",
        TextKey::PublicationGeneration => "Generazione",
        TextKey::CorrelationId => "ID di correlazione",
        TextKey::TargetPublishedRevision => "Revisione pubblicata di destinazione",
        TextKey::RelatedOperation => "Operazione correlata",
        TextKey::FilesystemTransition => "Transizione del file system",
        TextKey::RetryClass => "Classe di nuovo tentativo",
        TextKey::RecoveryAction => "Azione di ripristino",
        TextKey::OperatingSystemErrorCode => "Codice errore del sistema operativo",
        TextKey::PublicationOutcome => "Esito",
        TextKey::PublicationExpired => "Scaduta",
        TextKey::EventExistingJobAdopted => "Pubblicazione esistente acquisita",
        TextKey::EventEnqueued => "Pubblicazione accodata",
        TextKey::EventLeaseWaitStarted => "In attesa di accesso alla libreria",
        TextKey::EventLeaseAcquired => "Accesso alla libreria ottenuto",
        TextKey::EventLeaseReclaimed => {
            "Pubblicazione ripresa dopo un'interruzione o una concessione scaduta"
        }
        TextKey::EventPhaseAdvanced => "Fase di pubblicazione avanzata",
        TextKey::EventChildScanStarted => "Scansione di verifica avviata",
        TextKey::EventChildScanCompleted => "Scansione di verifica completata",
        TextKey::EventChildScanFailed => "Scansione di verifica non riuscita",
        TextKey::EventOriginalFailure => "Errore di pubblicazione originale",
        TextKey::EventRecoveryFailure => "Errore di ripristino",
        TextKey::EventRetryScheduled => "Nuovo tentativo di ripristino pianificato",
        TextKey::EventRecoveryRequested => "Ripristino richiesto",
        TextKey::EventCompleted => "Pubblicazione completata",
        TextKey::EventHistoryTruncated => "L'attività precedente è stata troncata",
        TextKey::TriggerAutomaticRefresh => "Aggiornamento pianificato",
        TextKey::TriggerOperator => "Richiesta da un operatore",
        TextKey::TriggerRestoreRevision => "Richiesta dal ripristino di una revisione",
        TextKey::TestSource => "Verifica origine",
        TextKey::TestSourceHelp => {
            "Verifica l’origine salvata senza importare o modificare l’elenco."
        }
        TextKey::SourceTestQueued => "Verifica origine in coda",
        TextKey::SourceTestRunning => "Verifica origine in corso…",
        TextKey::SourceTestStale => {
            "La verifica è obsoleta perché gli input salvati non corrispondono più all’elenco, al pacchetto add-on o ai valori predefiniti correnti."
        }
        TextKey::SourceAvailable => "L’origine è disponibile",
        TextKey::SourceUnsupported => "L’origine non è supportata",
        TextKey::SourceTestFailed => "Verifica origine non riuscita",
        TextKey::SourceRevision => "Revisione origine",
        TextKey::FirstPageComplete => "Prima pagina completa",
        TextKey::ProviderIdentity => "Identità del provider",
        TextKey::ReturnedFirstPageItems => "Elementi restituiti nella prima pagina",
        TextKey::ProviderTotalItemCount => "Numero totale di elementi del provider",
        TextKey::DraftChanges => "Modifiche alla bozza salvata",
        TextKey::ExactDraftDiffHelp => {
            "Differenze esatte tra la bozza salvata e la revisione pubblicata corrente."
        }
        TextKey::DraftDiffUnavailable => {
            "Un confronto esatto sarà disponibile dopo la pubblicazione di una revisione completa."
        }
        TextKey::MetadataChanges => "Modifiche ai metadati",
        TextKey::ExcludedItems => "Elementi esclusi",
        TextKey::Conflicts => "Conflitti",
        TextKey::PendingRemovals => "Rimozioni in sospeso",
        TextKey::PendingRemovalHelp => {
            "Assente una volta in una riconciliazione completa. Conferma ora la rimozione oppure attendi una seconda osservazione completa dopo almeno 24 ore."
        }
        TextKey::Reappearances => "Ricomparse",
        TextKey::RemovalCandidates => "Candidati alla rimozione",
        TextKey::ConfirmRemoval => "Conferma rimozione",
        TextKey::RemovalConfirmed => "Rimozione confermata.",
        TextKey::RemovalConfirmationFailed => "Conferma della rimozione non riuscita.",
        TextKey::RevisionHistory => "Cronologia revisioni",
        TextKey::RevisionHistoryHelp => {
            "Il ripristino sostituisce la bozza corrente e pubblica l’istantanea scelta come nuova revisione."
        }
        TextKey::NoRevisionHistory => "Non sono ancora conservate revisioni pubblicate.",
        TextKey::Revision => "Revisione",
        TextKey::CompareRevisions => "Confronta revisioni",
        TextKey::RestoreRevision => "Ripristina revisione",
        TextKey::RestoreRevisionWarning => {
            "Questo sostituisce la bozza corrente e pubblica la revisione"
        }
        TextKey::RestoreRevisionQueued => "Ripristino revisione in coda",
        TextKey::RestoreRevisionFailed => "Impossibile ripristinare la revisione.",
        TextKey::PublishedItems => "Elementi pubblicati",
        TextKey::GeneratedPlaylists => "Playlist generate",
        TextKey::ActiveOrFuturePlayouts => "Riproduzioni attive o future",
        TextKey::EffectiveOptions => "Opzioni effettive",
        TextKey::Files => "File",
        TextKey::Artifacts => "Artefatti",
        TextKey::Added => "Aggiunto",
        TextKey::Removed => "Rimosso",
        TextKey::Changed => "Modificato",
        TextKey::FilesystemPreviousBackedUp => "Revisione precedente salvata",
        TextKey::FilesystemCandidateCommitted => "Revisione candidata confermata",
        TextKey::FilesystemPreviousRestored => "Revisione precedente ripristinata",
        TextKey::FilesystemProjectionRestored => "Proiezione del database ripristinata",
        TextKey::FilesystemRevisionVerified => "Revisione verificata",
        TextKey::FilesystemCleanupCompleted => "Pulizia della pubblicazione completata",
        TextKey::RetryTransientRecovery => "Ripristino transitorio",
        TextKey::ErrorFilesystemIo => "I/O del file system",
        TextKey::ErrorPermissionDenied => "Autorizzazione negata",
        TextKey::ErrorSharingViolation => "Violazione di condivisione file",
        TextKey::ErrorUnsafeFilesystemState => "Stato del file system non sicuro",
        TextKey::ErrorInvalidPublicationMarker => "Indicatore di pubblicazione non valido",
        TextKey::ErrorPublicationIntegrity => "Controllo di integrità della pubblicazione",
        TextKey::OutcomePublished => "Pubblicata",
        TextKey::OutcomeRestoredAfterFailure => "Revisione precedente ripristinata dopo l'errore",
        TextKey::ReviewRecommendations => "Consigli di revisione",
        TextKey::OptionalDetails => "Dettagli facoltativi",
        TextKey::DraftSaved => "Bozza salvata",
        TextKey::SavedRevision => "Revisione salvata",
        TextKey::PublishList => "Pubblica elenco",
        TextKey::PreparePublication => "Prepara pubblicazione",
        TextKey::ProcessedMediaRequired => "Attività di preparazione",
        TextKey::ProcessedMediaQueued => "Preparazione dei contenuti multimediali in coda",
        TextKey::PreparingProcessedMedia => "Preparazione dei file multimediali",
        TextKey::PreparingPublication => "Preparazione della pubblicazione",
        TextKey::Publishing => "Pubblicazione…",
        TextKey::PublicationQueued => "Pubblicazione in coda",
        TextKey::RestoringPublication => "Ripristino della revisione precedente…",
        TextKey::Published => "Pubblicato",
        TextKey::PublishSavedRevisionHelp => {
            "La pubblicazione usa esattamente la revisione salvata. Salva prima le modifiche dell’elenco."
        }
        TextKey::ChangesSavedImmediately => {
            "Le modifiche agli elementi e in blocco vengono salvate subito e possono essere annullate."
        }
        TextKey::ReviewItem => "Rivedi elemento",
        TextKey::ShowFullDescription => "Mostra descrizione completa",
        TextKey::ShowLess => "Mostra meno",
        TextKey::ImportAList => "Importa un elenco",
        TextKey::ImportOverview => "Importa panoramica",
        TextKey::AddonContextSelected => "Add-on selezionato",
        TextKey::CompletedIn => "Completato in",
        TextKey::NoPublicationBlockers => "Nessun blocco alla pubblicazione.",
        TextKey::BlockerOverviewIncomplete => "La panoramica completa deve essere verificata.",
        TextKey::BlockerPublicationInProgress => "La pubblicazione è già in corso.",
        TextKey::BlockerRecoveryRequired => {
            "Una pubblicazione precedente richiede ancora il ripristino."
        }
        TextKey::BlockerCollectionTitleMissing => "La raccolta richiede un titolo.",
        TextKey::BlockerInvalidArtwork => {
            "Le immagini devono usare un URL HTTPS valido e limitato."
        }
        TextKey::BlockerNoIncludedItems => "Includi almeno un elemento.",
        TextKey::BlockerMoviePrimaryCount => {
            "La pubblicazione di film richiede esattamente un film principale."
        }
        TextKey::BlockerPlaybackDecisionRequired => "Scegli la riproduzione finita o dal vivo.",
        TextKey::BlockerPlaybackDetailsUnavailable => {
            "Questo add-on non fornisce i dettagli di riproduzione richiesti."
        }
        TextKey::BlockerPlaybackDetailsFailed => {
            "I dettagli del provider non contengono una durata positiva."
        }
        TextKey::BlockerUnavailableWithoutReplacement => {
            "L’elemento non disponibile richiede una sostituzione locale valida."
        }
        TextKey::BlockerItemTitleMissing => "L’elemento richiede un titolo.",
        TextKey::BlockerSeasonEpisodeMissing => "Inserisci una stagione e un episodio validi.",
        TextKey::BlockerArtistMissing => "Inserisci almeno un artista.",
        TextKey::PublishUnsavedChanges => {
            "Salva le modifiche prima di pubblicare questa revisione."
        }
        TextKey::PublicationBlocked => "La pubblicazione è bloccata.",
        TextKey::PublishBlocked => "Pubblicazione bloccata",
        TextKey::RetryRecovery => "Riprova il recupero",
        TextKey::RestorePreviousRevision => "Ripristina revisione precedente",
        TextKey::PublicationPhase => "Fase di pubblicazione",
        TextKey::PublicationAttempts => "Tentativi",
        TextKey::LastProgress => "Ultimo avanzamento",
        TextKey::FailureReason => "Motivo originale",
        TextKey::RestrictedItemsExcluded => "Elementi con restrizioni esclusi da questa revisione",
        TextKey::RecoveryQueued => "Recupero in coda",
        TextKey::RecoveryFailed => "Impossibile mettere in coda il recupero.",
        TextKey::PhaseAwaitingLease => "In attesa dell’accesso alla libreria",
        TextKey::PhaseStaging => "Preparazione della revisione",
        TextKey::PhaseScanning => "Scansione della revisione preparata",
        TextKey::PhaseCommitting => "Conferma della revisione",
        TextKey::PhaseRestoringFiles => "Ripristino dei file",
        TextKey::PhaseRestoringProjection => "Ripristino della proiezione del database",
        TextKey::PhaseReconciling => "Riconciliazione della revisione ripristinata",
        TextKey::PhaseVerifying => "Verifica della revisione",
        TextKey::PhaseCleaningUp => "Pulizia dei file di pubblicazione",
        TextKey::PhaseComplete => "Completata",
        TextKey::InstallLeavesDisabled => {
            "Dopo l’installazione, l’add-on resta disattivato finché non viene abilitato."
        }
        TextKey::EnableActivatesPermissions => {
            "L’abilitazione attiva le autorizzazioni dichiarate dell’add-on."
        }
        TextKey::FirstAired => "Prima messa in onda",
        TextKey::LoadItemDetailsAfterPublishing => "Carica automaticamente i dettagli",
        TextKey::LoadItemDetailsHelp => {
            "Importa prima la panoramica. I dettagli tecnici necessari per la riproduzione vengono caricati prima della prima pubblicazione; gli altri metadati estesi seguono a un ritmo controllato."
        }
        TextKey::OverviewOnlyManualDetailsHelp => {
            "Importa solo la panoramica e richiedi manualmente i dettagli descrittivi. I dettagli tecnici necessari per la riproduzione vengono preparati automaticamente prima della pubblicazione."
        }
        TextKey::PlaybackDetailsQueued => "Dettagli di riproduzione in coda",
        TextKey::PublishAgainAfterDetails => "I dettagli completati vengono applicati automaticamente.",
TextKey::FetchDetails => "Recupera dettagli",
        TextKey::DetailsQueued => "Dettagli in coda",
        TextKey::DetailsLoading => "Caricamento dettagli",
        TextKey::RetryDetails => "Riprova dettagli",
        TextKey::DetailsComplete => "Dettagli completi",
        TextKey::DetailsSkipped => "Dettagli ignorati",
        TextKey::FindingProgrammes => "Ricerca programmi",
        TextKey::CheckingOverview => "Verifica panoramica",
        TextKey::ReadyToReview => "Elaborazione automatica",

        TextKey::LoadingItemDetails => "Caricamento dettagli elementi",
        TextKey::WaitingForProvider => "In attesa del provider",
        TextKey::OverviewOnly => "Solo panoramica",
        TextKey::PauseImport => "Sospendi importazione",
        TextKey::ResumeImport => "Riprendi importazione",
        TextKey::RetryImportNow => "Riprova ora l'importazione",
        TextKey::EmptyImportNeedsRetry => "L'importazione vuota deve essere ripetuta",
        TextKey::EmptyImportRetryHelp => {
            "Questa importazione completata non contiene elementi. Aggiorna l'add-on e riprova; le impostazioni e le modifiche dell'elenco verranno mantenute."
        }
        TextKey::CancelImport => "Annulla importazione",
        TextKey::OtherPageItemsAffected => "Altri elementi interessati si trovano in altre pagine",
        TextKey::ElapsedTime => "Tempo trascorso",
        TextKey::LastSuccessfulProgress => "Ultimo avanzamento riuscito",
        TextKey::EstimatedRemainingTime => "Tempo rimanente stimato",
        TextKey::PlaybackBehavior => "Comportamento di riproduzione",
        TextKey::UseProviderValue => "Usa valore del provider",
        TextKey::FiniteMedia => "Contenuto finito",
        TextKey::LiveSource => "Sorgente live",
        TextKey::ProviderValueChanged => "Valore del provider modificato",
        TextKey::ApplyToSelected => "Applica alla selezione",
        TextKey::ApplyToAllRemoteStreams => "Applica a tutti gli stream remoti",
        TextKey::SelectItem => "Seleziona elemento",
        TextKey::NumberEpisodes => "Numerare gli episodi",
        TextKey::NumberEpisodesHelp => {
            "Assegna una stagione e numeri di episodio consecutivi nell’ordine originale dell’add-on. Gli elementi esclusi e i frammenti di capitolo vengono ignorati."
        }
        TextKey::FirstEpisode => "Primo episodio",
        TextKey::NumberToAssign => "Numero da assegnare",
        TextKey::AllRemaining => "Tutti i rimanenti",
        TextKey::ReplaceExistingEpisodeNumbers => "Sostituisci i numeri di episodio esistenti",
        TextKey::AutoDetectSeasons => "Rileva le stagioni dalle date di prima messa in onda",
        TextKey::AutoDetectSeasonsHelp => {
            "Una nuova stagione inizia quando l’intervallo tra due date note raggiunge il minimo configurato. Gli elementi senza data continuano nella stagione corrente."
        }
        TextKey::MinimumSeasonGapDays => "Intervallo minimo tra stagioni (giorni)",
        TextKey::NumberingStartsAt => "La numerazione inizia da",
        TextKey::FirstIncludedItem => "Primo elemento incluso",
        TextKey::StartWithFirstItem => "Inizia dal primo elemento",
        TextKey::StartHere => "Inizia qui",
        TextKey::SaveSeasonEpisode => "Salva stagione ed episodio",
        TextKey::BulkEdit => "Modifica in blocco",
        TextKey::Scope => "Ambito",
        TextKey::AllItems => "Tutti gli elementi",
        TextKey::CurrentPage => "Pagina corrente",
        TextKey::ItemsNeedingReview => "Elementi con avvisi",

        TextKey::KeepCurrentValue => "Mantieni il valore corrente",
        TextKey::UndoLastChange => "Annulla l’ultima modifica",
        TextKey::Undo => "Annulla",
        TextKey::ChangeUndone => "Modifica annullata",
        TextKey::MetadataConflicts => "Conflitti nei metadati",
        TextKey::MetadataConflictsHelp => "Il fornitore ha modificato campi personalizzati. I valori salvati rimangono in uso.",
TextKey::PreviousProviderValue => "Valore precedente del fornitore",
        TextKey::NewProviderValue => "Nuovo valore del fornitore",
        TextKey::OperatorValue => "Valore dell’operatore",
        TextKey::Discovered => "Rilevati",
        TextKey::Examined => "Esaminati",
        TextKey::ArchivedSkipped => "Saltati perché archiviati",
        TextKey::Emitted => "Emessi",
        TextKey::EditMetadata => "Modifica metadati",
        TextKey::OneTagPerLine => "Un tag per riga",
        TextKey::OriginalTitle => "Titolo originale",
        TextKey::ReleaseDate => "Data di uscita",
        TextKey::Year => "Anno",
        TextKey::SortTitle => "Titolo di ordinamento",
        TextKey::MovieFilesAndExtras => "File del film ed extra",
        TextKey::MovieFilesAndExtrasHelp => {
            "Assegna esattamente un film principale. L’ordine delle fonti non decide il ruolo, quindi un trailer può comparire per primo."
        }
        TextKey::EpisodesMetadataHelp => {
            "Controlla identità e numerazione degli episodi; l’ordine del provider resta invariato."
        }
        TextKey::MusicVideosMetadataHelp => {
            "Artista, album e traccia appartengono a ogni video; gli elenchi con più artisti restano validi."
        }
        TextKey::OtherVideosMetadataHelp => {
            "Usa questo tipo per video indipendenti invece che per un film con extra."
        }
        TextKey::SongsMetadataHelp => {
            "Traccia, disco, artista e album si modificano per brano; l’ordine dell’elenco non sostituisce il numero di traccia."
        }
        TextKey::ImagesMetadataHelp => {
            "Didascalia e autore sono modificabili; formato, dimensioni e identità della fonte restano dati oggettivi."
        }
        TextKey::RemoteStreamsMetadataHelp => {
            "Modifica qui i metadati di presentazione; connessione e protocollo restano dati della fonte provider."
        }
        TextKey::CastAndCredits => "Cast e collaboratori",
        TextKey::OneCreditPerLine => "Una persona per riga: nome | ruolo",
        TextKey::Poster => "Poster",
        TextKey::Artwork => "Immagini",
        TextKey::PublishMetadata => "Pubblica",
        TextKey::IncludeItem => "Includi",
        TextKey::IncludeInPublication => "Includi nella pubblicazione",
        TextKey::MetadataDraftNotice => "Metadati e selezioni salvati vengono applicati automaticamente.",
TextKey::ApplyToAllItemsPrompt => {
            "Ogni elemento viene riclassificato con il tipo di contenuto selezionato, sostituendo i valori dedotti dal componente aggiuntivo o impostati manualmente."
        }
        TextKey::TelevisionEpisode => "Episodio televisivo",
        TextKey::CollectionTypeAuto => "Determina dagli elementi",
        TextKey::CurrentLibraryType => "Tipo di libreria attuale",
        TextKey::CollectionTypeChangeHelp => "Salva per riorganizzare automaticamente gli elementi esistenti. Ordine, selezione e modifiche ai metadati vengono conservati.",
TextKey::NonPlayableItems => "Elementi non riproducibili",
        TextKey::UnknownItems => "Elementi sconosciuti",
        TextKey::PlayableItems => "Elementi riproducibili",
        TextKey::ApplyToAllItems => "Applica a tutti gli elementi",
        TextKey::ChangeAddon => "Cambia componente aggiuntivo",
        TextKey::ChangeAddonHelp => {
            "Seleziona un altro componente aggiuntivo per reimportare l'elenco tramite esso."
        }
        TextKey::ChangeAddonPrompt => {
            "L'elenco viene reimportato tramite il componente aggiuntivo selezionato. Metadati degli elementi, sostituzioni e correzioni dei capitoli vengono ricavati di nuovo dal nuovo fornitore."
        }
        TextKey::AddonAutoDetect => "Determina dall'URL",
        TextKey::AddonAutoDetected => "Selezionato automaticamente in base all'URL.",
        TextKey::AddonMediaListNoAddonForUrl => {
            "Nessun componente aggiuntivo installato gestisce questo URL. Scegline uno o imposta un elaboratore predefinito per i flussi remoti."
        }
        TextKey::DefaultRemoteStreamAddon => "Elaboratore predefinito per i flussi remoti",
        TextKey::DefaultRemoteStreamAddonHelp => {
            "Usato per le origini HTTP e HTTPS che nessun componente aggiuntivo installato rivendica. Il componente deve comunque essere autorizzato a contattare l'host."
        }
        TextKey::AddonDetailWait => "Ritmo dei dettagli degli elementi",
        TextKey::MinimumDetailWait => "Attesa minima per i dettagli",
        TextKey::MaximumDetailWait => "Attesa massima per i dettagli",
        TextKey::AddonDetailWaitHelp => {
            "Dopo ogni tentativo di caricare i dettagli di un elemento, ErsatzRS attende un nuovo numero casuale di secondi entro questo intervallo globale prima che un elenco possa riutilizzare lo stesso host del provider."
        }
        TextKey::AddonDetailWaitInvalid => {
            "Inserisci secondi interi positivi e assicurati che il minimo non superi il massimo."
        }
        TextKey::Addons => "Componenti aggiuntivi",
        TextKey::AddonOverview => "Panoramica",
        TextKey::AddonUpdates => "Aggiornamenti",
        TextKey::AddonSources => "Origini",
        TextKey::AddonNotFound => "Componente aggiuntivo non trovato",
        TextKey::BackToAddons => "Torna ai componenti aggiuntivi",
        TextKey::AvailableVersion => "Versione disponibile",
        TextKey::BrowseAddons => "Sfoglia componenti aggiuntivi",
        TextKey::CheckForUpdates => "Controlla aggiornamenti",
        TextKey::InstallFromFile => "Installa da file",
        TextKey::InstallRepositoryFromFile => "Installa repository da file",
        TextKey::License => "Licenza",
        TextKey::UnknownSources => "Origini sconosciute",
        TextKey::UnknownSourcesHelp => {
            "Consente add-on da checkout Git locali e file ZIP. Gli add-on vengono eseguiti con l’accesso dell’utente del servizio ErsatzRS."
        }
        TextKey::Verified => "Verificato",
        TextKey::Unverified => "Non verificato",
        TextKey::Sideloaded => "Installato localmente",
        TextKey::GitCheckout => "Checkout Git",
        TextKey::OfflineBundle => "Pacchetto offline",
        TextKey::LegacyUnmanaged => "Catalogo precedente · non gestito",
        TextKey::LegacyAdoptionHelp => {
            "Questa operazione sostituisce l’installazione del catalogo non gestita con la fonte locale selezionata, mantenendo impostazioni e dati."
        }
        TextKey::Enabled => "Abilitato",
        TextKey::Disabled => "Disabilitato",
        TextKey::Source => "Origine",
        TextKey::ViewSourceCode => "Visualizza il codice sorgente",
        TextKey::Version => "Versione",
        TextKey::NoAddonsMatch => "Nessun componente aggiuntivo corrisponde a questi filtri.",
        TextKey::AddonsUpToDate => "Tutto è aggiornato.",
        TextKey::AddonRepositories => "Repository dei componenti aggiuntivi",
        TextKey::AvailableAddons => "Add-on disponibili",
        TextKey::InstalledAddons => "Add-on installati",
        TextKey::AddRepository => "Aggiungi repository",
        TextKey::LastRefresh => "Ultima scansione",
        TextKey::NeverRefreshed => "Mai",
        TextKey::ChooseFile => "Scegli file…",
        TextKey::NoFileSelected => "Nessun file selezionato",
        TextKey::EditRepository => "Modifica repository",
        TextKey::OfficialRepositoryProtected => "Il repository ufficiale non può essere rimosso.",
        TextKey::UnknownSourcesRequired => "Abilita prima le origini sconosciute.",
        TextKey::NoRepositories => {
            "Nessun repository locale trovato. Clonane uno nella cartella di checkout indicata sopra, quindi aggiorna."
        }
        TextKey::AddonPackage => "Pacchetto del componente aggiuntivo",
        TextKey::RepositoryPackage => "Pacchetto repository",
        TextKey::NoSigningKeyPinned => "Nessuna chiave di firma pubblica è associata.",
        TextKey::RepositoryUrl => "URL del repository",
        TextKey::PublicSigningKey => "Chiave di firma pubblica",
        TextKey::DeploymentManagedRepositories => {
            "I repository sono gestiti dalla configurazione di distribuzione."
        }
        TextKey::Refresh => "Aggiorna",
        TextKey::Install => "Installa",
        TextKey::UpdateAddon => "Aggiorna",
        TextKey::Uninstall => "Disinstalla",
        TextKey::Configuration => "Configurazione",
        TextKey::SecretReference => "Riferimento segreto",
        TextKey::EnvironmentVariable => "Variabile d'ambiente",
        TextKey::SecretFile => "File segreto",
        TextKey::Incompatible => "Incompatibile",
        TextKey::PackageUnavailable => "Pacchetto non disponibile",
        TextKey::Official => "Ufficiale",
        TextKey::ThirdParty => "Repository di terze parti",
        TextKey::Permissions => "Permessi",
        TextKey::Capabilities => "Capacità",
        TextKey::Dependencies => "Dipendenze",
        TextKey::Optional => "Opzionale",
        TextKey::MediaReadAccess => "Accesso alla lettura dei media",
        TextKey::ExternalCommands => "Comandi esterni",
        TextKey::ThirdPartyRepositoryWarning => {
            "Carica solo repository locali e file ZIP attendibili; gli add-on vengono eseguiti come utente di servizio ErsatzRS."
        }
        TextKey::AddCustomResolution => "Aggiungi Risoluzione Personalizzata",
        TextKey::AddBlock => "Aggiungi Blocco",
        TextKey::AddBlockGroup => "Aggiungi Gruppo di Blocchi",
        TextKey::AddBlockItem => "Aggiungi Elemento di Blocco",
        TextKey::AddBlockToTemplate => "Aggiungi Blocco al Modello",
        TextKey::AddCollection => "Aggiungi raccolta",
        TextKey::AddContent => "Aggiungi Contenuto",
        TextKey::AddFailed => "Aggiunta fallita",
        TextKey::AddFillerPreset => "Aggiungi Preset Riempitivo",
        TextKey::AddMultiCollection => "Aggiungi Multi-Colezione",
        TextKey::AddPlaylist => "Aggiungi playlist",
        TextKey::AddPlaylistGroup => "Aggiungi gruppo di playlist",
        TextKey::AddPlaylistItem => "Aggiungi elemento alla playlist",
        TextKey::AddRerunCollection => "Aggiungi raccolta di ripetizioni",
        TextKey::AddTemplate => "Aggiungi Modello",
        TextKey::AddTemplateGroup => "Aggiungi Gruppo di Modelli",
        TextKey::AddAddonMediaList => "Aggiungi Lista add-on",
        TextKey::AddToCollection => "Aggiungi alla raccolta",
        TextKey::AddToEllipsis => "Aggiungi a...",
        TextKey::AddToPlaylist => "Aggiungi alla playlist",
        TextKey::AddToSchedule => "Aggiungi alla Programmazione",
        TextKey::Actions => "Azioni",
        TextKey::All => "Tutti",
        TextKey::AreYouSureDelete => "Sei sicuro di voler eliminare",
        TextKey::Actors => "Attori",
        TextKey::Artist => "Artista",
        TextKey::ArtistNotFound => "Artista non trovato",
        TextKey::AfterDurationEnd => "Dopo la fine della durata",
        TextKey::AutomaticRefresh => "Aggiornamento Automatico",
        TextKey::AddonDeliveryMode => "Distribuzione",
        TextKey::AddonDeliveryStream => "Streaming",
        TextKey::AddonDeliveryDownloadAhead => "Scarica in anticipo",
        TextKey::AddonDeliveryNewVideos => "Scarica in anticipo — solo nuovi video",
        TextKey::AddonDeliveryHelp => "Le modifiche hanno effetto dopo la prossima pubblicazione. Solo nuovi video scarica i video importati per la prima volta dopo la data limite salvata; quelli già noti vengono riprodotti in streaming. Riattivare la modalità imposta una nuova data limite. Solo i contenuti di durata finita possono essere scaricati in anticipo. I contenuti dal vivo e quelli di durata sconosciuta continuano a essere riprodotti in streaming.",
        TextKey::AddonDeliveryUnavailable => "Il download richiede un componente aggiuntivo compatibile e attivo e contenuti di durata finita.",
        TextKey::AddonDeliveryCutoff => "Data limite dei nuovi video",
        TextKey::AddonDeliveryFreshCutoff => "Al salvataggio viene registrata una nuova data limite.",
        TextKey::AdaptiveRefreshUnavailable => {
            "L'aggiornamento adattivo non è disponibile perché questo add-on non supporta più i controlli della sorgente. Scegli un'altra frequenza per riprendere l'aggiornamento automatico."
        }
        TextKey::RefreshPublicationPolicy => "Pubblicazione dopo aggiornamento",
        TextKey::RefreshPublicationReview => "Verifica ogni aggiornamento",
        TextKey::RefreshPublicationAutomatic => "Applica automaticamente le modifiche salvate",
        TextKey::RefreshPublicationPolicyHelp => "Importazioni, aggiornamenti, metadati e modifiche salvate vengono applicati automaticamente.",
        TextKey::AutoPublicationHeld => "L’elaborazione automatica richiede attenzione",
        TextKey::PreparationWorkerUnavailable => {
            "Il processo di preparazione dei contenuti multimediali non è disponibile. Riavvia il server prima di riprovare la preparazione."
        }
        TextKey::PreparationProcessRecoveryRequired => {
            "La preparazione dei contenuti richiede il ripristino dei processi. Verifica che i processi precedenti del provider siano terminati prima di recuperare lo spazio riservato e riprovare la preparazione."
        }
        TextKey::AutoPublicationBaselineUnavailable => "Nessuna base pubblicata disponibile.",
        TextKey::AutoPublicationExistingChanges => "Gli elementi esistenti sono cambiati.",
        TextKey::AutoPublicationConflicts => {
            "Verifica i conflitti nei metadati o nelle sostituzioni."
        }
        TextKey::AutoPublicationRemovals => "Alcuni elementi sono stati rimossi.",
        TextKey::AutoPublicationRestricted => "Alcune aggiunte hanno restrizioni di accesso.",
        TextKey::AutoPublicationUnavailable => "Alcune aggiunte non sono disponibili.",
        TextKey::AutoPublicationOptionsChanged => "Le opzioni di pubblicazione sono cambiate.",
        TextKey::AutoPublicationPreparationFailed => {
            "La preparazione della pubblicazione non è riuscita."
        }
        TextKey::AutoPublicationConcurrentChange => "La lista è cambiata durante la preparazione.",
        TextKey::RefreshCadenceAdaptive => "Adattivo",
        TextKey::RefreshCadenceDaily => "Giornaliero",
        TextKey::RefreshCadenceWeekly => "Settimanale",
        TextKey::RefreshCadenceMonthly => "Mensile",
        TextKey::RefreshCadenceYearly => "Annuale",
        TextKey::RefreshCadenceNever => "Mai",
        TextKey::LastSuccessfulRefresh => "Ultimo aggiornamento riuscito",
        TextKey::NextScheduledRefresh => "Prossimo aggiornamento pianificato",
        TextKey::RefreshRetryAt => "Nuovo tentativo disponibile dalle",
        TextKey::Back => "Indietro",
        TextKey::BackToSettings => "Torna alle impostazioni",
        TextKey::BackToArtists => "Torna agli artisti",
        TextKey::BackToCollections => "Torna alle raccolte",
        TextKey::BackToMovies => "Torna ai film",
        TextKey::BackToOtherVideos => "Torna ad altri video",
        TextKey::BackToTvShows => "Torna alle serie TV",
        TextKey::BeforeDurationEnd => "Prima della fine della durata",
        TextKey::BackToTemplates => "Torna ai modelli",
        TextKey::Block => "Blocco",
        TextKey::BlockGroup => "Gruppo di Blocchi",
        TextKey::BlockCannotBeMoved => {
            "Il blocco non può essere spostato lì perché sovrapporrebbe o attraverserebbe la mezzanotte."
        }
        TextKey::BlockCopied => "Blocco copiato.",
        TextKey::BlockCreated => "Blocco creato.",
        TextKey::BlockDeleted => "Blocco eliminato.",
        TextKey::BlockGroupCreated => "Gruppo di blocchi creato.",
        TextKey::BlockGroupDeleted => "Gruppo di blocchi eliminato.",
        TextKey::BlockGroupName => "Nome del Blocco",
        TextKey::BlockGroups => "Gruppi di Blocchi",
        TextKey::BlockItem => "Elemento di Blocco",
        TextKey::BlockItems => "Elementi di Blocco",
        TextKey::BlockName => "Nome del Blocco",
        TextKey::BlockNotFound => "Blocco non trovato.",
        TextKey::BlockPreviewFailed => {
            "Impossibile generare l'anteprima del blocco. Controlla gli elementi del blocco e riprova."
        }
        TextKey::Blocks => "Blocchi",
        TextKey::BlockToRemove => "Blocco da rimuovere",
        TextKey::BlockWillNotFit => {
            "Il blocco non si adatterà al o dopo l'orario di inizio selezionato."
        }
        TextKey::Cancel => "Annulla",
        TextKey::Clear => "Pulisci",
        TextKey::Available => "Disponibile",
        TextKey::UnknownAvailability => "Sconosciuto",
        TextKey::LocalReplacement => "Sostituzione locale",
        TextKey::NoReplacement => "Nessuna sostituzione",
        TextKey::InvalidReplacement => "Sostituzione non valida",
        TextKey::Channels => "Canali",
        TextKey::Close => "Chiudi",
        TextKey::Collections => "Raccolte",
        TextKey::CollectionDeleted => "Raccolta eliminata.",
        TextKey::CollectionSaved => "Raccolta salvata.",
        TextKey::SaveCollection => "Salva raccolta",
        TextKey::ComputingPreview => "Elaborazione anteprima...",
        TextKey::Confirm => "Conferma",
        TextKey::ConfirmDelete => "Conferma Eliminazione",
        TextKey::CommentaryTracks => "Tracce di Commento",
        TextKey::Content => "Contenuto",
        TextKey::ContentVariantAcoustic => "Acustico",
        TextKey::ContentVariantAlternateVersion => "Versione",
        TextKey::ContentVariantBehindTheScenes => "Dietro le quinte",
        TextKey::ContentVariantDeletedScene => "Scena eliminata",
        TextKey::ContentVariantDirectorsCut => "Taglio del regista",
        TextKey::ContentVariantExtended => "Esteso",
        TextKey::ContentVariantExtra => "Extra",
        TextKey::ContentVariantFeaturette => "Featurette",
        TextKey::ContentVariantInterview => "Intervista",
        TextKey::ContentVariantLive => "In diretta",
        TextKey::ContentVariantOfficialVideo => "Video Ufficiale",
        TextKey::ContentVariantOther => "Altro",
        TextKey::ContentVariantRemaster => "Remaster",
        TextKey::ContentVariantRemix => "Remix",
        TextKey::ContentVariantScene => "Scena",
        TextKey::ContentVariantSearchHelp => {
            "Token di variante: content_kind:extra, content_kind:alternate_version, sub_kind:featurette, sub_kind:unplugged, parent_title:\"Titolo film\", audio_title:commentary, subtitle_title:commentary, has_commentary:true"
        }
        TextKey::ContentVariantShort => "Breve",
        TextKey::ContentVariantSpecialFeatures => "Extra / Caratteristiche speciali",
        TextKey::ContentVariantTrailer => "Trailer",
        TextKey::ContentVariantUnplugged => "Non collegato",
        TextKey::ContentVariantVersions => "Versioni",
        TextKey::ContentRatings => "Classificazioni dei contenuti",
        TextKey::CustomResolutionHeight => "Altezza risoluzione personalizzata",
        TextKey::CustomResolutionWidth => "Larghezza risoluzione personalizzata",
        TextKey::CustomResolutions => "Risoluzioni personalizzate",
        TextKey::Dark => "Scuro",
        TextKey::DaysToBuild => "Giorni per costruire",
        TextKey::AddBreakContent => "Aggiungi contenuto di pausa",
        TextKey::AddDeco => "Aggiungi decorazione",
        TextKey::AddDecoGroup => "Aggiungi gruppo decorazioni",
        TextKey::AddDecoTemplate => "Aggiungi modello decorazione",
        TextKey::AddDecoTemplateGroup => "Aggiungi gruppo modelli decorazione",
        TextKey::AtChapterMarkers => "Ai marcatori di capitolo",
        TextKey::BackToDecos => "Torna alle decorazioni",
        TextKey::BackToDecoTemplates => "Torna ai modelli decorazione",
        TextKey::BetweenBlockItems => "Tra gli elementi di blocco",
        TextKey::BlockFinish => "Fine blocco",
        TextKey::BlockStart => "Inizio blocco",
        TextKey::BreakContent => "Contenuto di pausa",
        TextKey::BreakContentBody => {
            "Il contenuto di pausa può essere utilizzato per riprodurre elementi specifici (utilizzando playlist) e per riempire i blocchi fino alla loro durata configurata completa."
        }
        TextKey::BreakContentItem => "Elemento contenuto di pausa",
        TextKey::BreakContentMode => "Modalità contenuto di pausa",
        TextKey::DeadAirFallback => "Fallback per interruzione",
        TextKey::DeadAirFallbackBody => {
            "Quando non viene trovato un elemento del playout per l'orario corrente, verrà selezionato casualmente un elemento da questa raccolta e ripetuto e ritagliato per adattarsi esattamente fino all'inizio del prossimo elemento del playout. Questo sostituisce l'immagine Canale offline che verrebbe altrimenti visualizzata."
        }
        TextKey::DeadAirFallbackCollectionType => "Tipo di raccolta fallback per interruzione",
        TextKey::DeadAirFallbackMode => "Modalità fallback per interruzione",
        TextKey::Deco => "Decorazione",
        TextKey::DecoCannotBeAdded => {
            "La decorazione non può essere aggiunta perché sovrapporrebbe o attraverserebbe la mezzanotte."
        }
        TextKey::DecoCannotBeMoved => {
            "La decorazione non può essere spostata lì perché sovrapporrebbe o attraverserebbe la mezzanotte."
        }
        TextKey::DecoCreated => "Decorazione creata.",
        TextKey::DecoDeleted => "Decorazione eliminata.",
        TextKey::DecoGroup => "Gruppo decorazioni",
        TextKey::DecoGroupCreated => "Gruppo decorazioni creato.",
        TextKey::DecoGroupDeleted => "Gruppo decorazioni eliminato.",
        TextKey::DecoGroupName => "Nome gruppo decorazioni",
        TextKey::DecoGroups => "Gruppi decorazioni",
        TextKey::DecoName => "Nome decorazione",
        TextKey::DecoNotFound => "Decorazione non trovata.",
        TextKey::DecoSaved => "Decorazione salvata.",
        TextKey::Decos => "Decorazioni",
        TextKey::DecoTemplate => "Modello decorazione",
        TextKey::DecoTemplateCreated => "Modello decorazione creato.",
        TextKey::DecoTemplateDeleted => "Modello decorazione eliminato.",
        TextKey::DecoTemplateGroup => "Gruppo modelli decorazione",
        TextKey::DecoTemplateGroupCreated => "Gruppo modelli decorazione creato.",
        TextKey::DecoTemplateGroupDeleted => "Gruppo modelli decorazione eliminato.",
        TextKey::DecoTemplateGroupName => "Nome gruppo modelli decorazione",
        TextKey::DecoTemplateGroups => "Gruppi modelli decorazione",
        TextKey::DecoTemplateName => "Nome modello decorazione",
        TextKey::DecoTemplateNotFound => "Modello decorazione non trovato.",
        TextKey::DecoTemplateSaved => "Modello decorazione salvato.",
        TextKey::AddDecoToTemplate => "Aggiungi decorazione al modello",
        TextKey::DecoToRemove => "Decorazione da rimuovere",
        TextKey::DefaultFiller => "Riempimento predefinito",
        TextKey::DefaultFillerBody => {
            "Dopo aver programmato tutti i blocchi, verrà effettuata una seconda passata per riempire il tempo non programmato utilizzando elementi casuali da questa raccolta."
        }
        TextKey::DefaultFillerCollectionType => "Tipo di raccolta riempimento predefinito",
        TextKey::DefaultFillerMode => "Modalità riempimento predefinito",
        TextKey::DeleteDeco => "Elimina decorazione",
        TextKey::DeleteDecoGroup => "Elimina gruppo decorazioni",
        TextKey::DeleteDecoTemplate => "Elimina modello decorazione",
        TextKey::DeleteDecoTemplateGroup => "Elimina gruppo modelli decorazione",
        TextKey::Disable => "Disabilita",
        TextKey::EditDeco => "Modifica decorazione",
        TextKey::EditDecoTemplate => "Modifica modello decorazione",
        TextKey::GraphicsElementsMode => "Modalità elementi grafici",
        TextKey::Inherit => "Eredità",
        TextKey::Merge => "Unisci",
        TextKey::NoDecosFound => "Nessuna decorazione trovata.",
        TextKey::NoDecoTemplatesFound => "Nessun modello decorazione trovato.",
        TextKey::Override => "Sovrascrivi",
        TextKey::RemoveBreakContent => "Rimuovi contenuto di pausa",
        TextKey::RemoveDecoFromTemplate => "Rimuovi Deco dal Modello",
        TextKey::Replace => "Sostituisci",
        TextKey::SavedDecos => "Deco Salvati",
        TextKey::SavedDecoTemplates => "Modelli Deco Salvati",
        TextKey::SearchForDecos => "Cerca decos",
        TextKey::SearchForDecoTemplates => "Cerca modelli deco",
        TextKey::TrimToFit => "Ridimensiona per Adattare",
        TextKey::UseGraphicsElementsDuringFiller => "Utilizza Elementi Grafici durante il Filler",
        TextKey::UseWatermarkDuringFiller => "Utilizza Filigrana durante il Filler",
        TextKey::WatermarkMode => "Modalità Filigrana",
        TextKey::DecoTemplates => "Modelli Deco",
        TextKey::DefaultFfmpegProfile => "Profilo FFmpeg Predefinito",
        TextKey::DefaultFfmpegProfileHelp => {
            "Il profilo FFmpeg da utilizzare quando si creano nuovi canali"
        }
        TextKey::DefaultMinimumLogLevel => "Livello di Log Minimo Predefinito",
        TextKey::DefaultMpegTsScript => "Script MPEG-TS Predefinito",
        TextKey::DefaultMpegTsScriptHelp => "Lo script MPEG-TS da utilizzare durante lo streaming",
        TextKey::Decorations => "Decorazioni",
        TextKey::Debug => "Debug",
        TextKey::Delete => "Elimina",
        TextKey::DeleteAll => "Elimina Tutto",
        TextKey::DeleteBlock => "Elimina Blocco",
        TextKey::DeleteBlockGroup => "Elimina Gruppo di Blocchi",
        TextKey::DeleteCustomResolution => "Elimina Risoluzione Personalizzata",
        TextKey::DeleteCollection => "Elimina raccolta",
        TextKey::DeleteFillerPreset => "Elimina Preset Filler",
        TextKey::DeleteMultiCollection => "Elimina raccolta multipla",
        TextKey::DeletePlaylist => "Elimina playlist",
        TextKey::DeletePlaylistGroup => "Elimina gruppo di playlist",
        TextKey::DeleteRerunCollection => "Elimina raccolta di ripetizioni",
        TextKey::DeleteTemplate => "Elimina Modello",
        TextKey::DeleteTemplateGroup => "Elimina Gruppo Modelli",
        TextKey::DeleteAddonMediaList => "Elimina Lista add-on",
        TextKey::DisabledCollectionsAlreadyPresent => {
            "Le raccolte disabilitate sono già presenti in questa raccolta multipla"
        }
        TextKey::EmptyTrash => "Svuota Cestino",
        TextKey::ExtractEmbeddedSubtitles => "Estrai Sottotitoli Incorporati (Testo)",
        TextKey::ErasedPlayoutItemsAndHistory => "Elementi del playout e cronologia cancellati.",
        TextKey::EraseFailed => "Eliminazione fallita",
        TextKey::EraseItemsAndHistory => "Elimina Elementi e Cronologia",
        TextKey::ErasePlayoutItemsAndHistoryConfirm => {
            "Questa operazione eliminerà tutti gli elementi del playout e la cronologia per questo playout sequenziale. Continuare?"
        }
        TextKey::Episode => "Episodio",
        TextKey::FanArt => "Fan Art",
        TextKey::Ffmpeg => "FFmpeg",
        TextKey::FileNotFound => "File non trovato",
        TextKey::Finish => "Fine",
        TextKey::Fatal => "Fatale",
        TextKey::FfmpegPath => "Percorso FFmpeg",
        TextKey::FfmpegPathHelp => "Il percorso completo del file eseguibile ffmpeg",
        TextKey::FfmpegProfiles => "Profili FFmpeg",
        TextKey::FfprobePath => "Percorso FFprobe",
        TextKey::FfprobePathHelp => "Il percorso completo del file eseguibile ffprobe",
        TextKey::FillerPreset => "Preset Filler",
        TextKey::FillerPresetActionFailed => "Azione preset filler non riuscita.",
        TextKey::FillerPresetCouldNotBeSaved => {
            "Il preset filler non può essere salvato. Controlla i valori del modulo e riprova."
        }
        TextKey::FillerPresetDeleted => "Preset filler eliminato.",
        TextKey::FillerPresetExpressionHelp => {
            "Per il filler mid-roll, aggiungi il filler solo quando questa espressione restituisce true per un dato punto mid-roll."
        }
        TextKey::FillerPresetSaved => "Preset filler salvato.",
        TextKey::FillerPresets => "Preset Filler",
        TextKey::Flags => "Bandiere",
        TextKey::GlobalFallbackFiller => "Filler di Fallback Globale",
        TextKey::GlobalWatermark => "Filigrana Globale",
        TextKey::GoHome => "Torna alla Home",
        TextKey::HdHomeRun => "HDHomeRun",
        TextKey::HealthChecks => "Controlli di Salute",
        TextKey::Height => "Altezza",
        TextKey::HlsDirectOutputFormat => "Formato di Output Diretto HLS",
        TextKey::HlsSegmenterIdleTimeout => "Timeout di Inattività Segmentatore HLS",
        TextKey::HlsSegmenterIdleTimeoutHelp => {
            "Il numero di secondi per continuare la transcoding di un canale mentre non vengono ricevute richieste da alcun client"
        }
        TextKey::HlsSegmenterInitialSegmentCount => {
            "Numero Iniziale di Segmenti del Segmentatore HLS"
        }
        TextKey::HlsSegmenterInitialSegmentCountHelp => {
            "Rinvia l'inizio dello streaming fino al numero specificato di segmenti (di 4 secondi) trascodificati; un numero maggiore comporterà una riproduzione iniziale più lenta, ma potenzialmente meno buffering"
        }
        TextKey::HomeRunning => "Il server IPTV è in esecuzione.",
        TextKey::HttpMinimumLogLevel => {
            "Livello Minimo di Log per la Registrazione delle Richieste"
        }
        TextKey::Id => "ID",
        TextKey::Images => "Immagini",
        TextKey::Item => "Elemento",
        TextKey::ItemActions => "Azioni elemento",
        TextKey::ItemType => "Tipo elemento",
        TextKey::Items => "Elementi",
        TextKey::Language => "Lingua",
        TextKey::Languages => "Lingue",
        TextKey::MusicLabels => "Etichette musicali",
        TextKey::LeaveEmptyToAutoDetect => "Lascia vuoto per rilevamento automatico",
        TextKey::Libraries => "Biblioteche",
        TextKey::LibraryRefreshInterval => "Intervallo di aggiornamento della biblioteca",
        TextKey::Light => "Luce",
        TextKey::Lists => "Liste",
        TextKey::Loading => "Caricamento...",
        TextKey::LoadingMembers => "Caricamento membri...",
        TextKey::Local => "Locale",
        TextKey::Logging => "Registrazione",
        TextKey::M3u => "M3U",
        TextKey::Media => "Media",
        TextKey::MediaInfo => "Info media",
        TextKey::MediaSources => "Sorgenti media",
        TextKey::Movie => "Film",
        TextKey::MovieNotFound => "Film non trovato",
        TextKey::Movies => "Film",
        TextKey::MusicVideo => "Video musicale",
        TextKey::MultiCollections => "Raccolte multiple",
        TextKey::ManualCollections => "Raccolte manuali",
        TextKey::MatchFailed => "Abbinamento fallito",
        TextKey::MatchStatus => "Stato abbinamento",
        TextKey::MatchAddonMediaListItems => "Elenco elementi add-on",
        TextKey::Members => "Membri",
        TextKey::MultiCollection => "Raccolta multipla",
        TextKey::MultiCollectionDeleted => "Raccolta multipla eliminata.",
        TextKey::MultiCollectionItems => "Elementi della raccolta multipla",
        TextKey::MultiCollectionNameRequired => "Il nome della raccolta multipla è obbligatorio!",
        TextKey::MultiCollectionNameTooLong => {
            "Il nome della raccolta multipla non può superare i 50 caratteri."
        }
        TextKey::MultiCollectionSaved => "Raccolta multipla salvata.",
        TextKey::Music => "Musica",
        TextKey::Name => "Nome",
        TextKey::NameRequired => "Il nome è obbligatorio",
        TextKey::Networks => "Reti",
        TextKey::NewCollection => "(Nuova raccolta)",
        TextKey::NewCollectionTitle => "Nuova raccolta",
        TextKey::NewFillerPreset => "Nuovo preset riempitivo",
        TextKey::NewMultiCollection => "Nuova raccolta multipla",
        TextKey::NewTemplateGroup => "Nuovo gruppo modelli",
        TextKey::NewTemplateName => "Nuovo nome modello",
        TextKey::Next24Hours => "Prossime 24h",
        TextKey::NextPage => "Pagina successiva",
        TextKey::NoFillerPresets => "Nessun preset riempitivo presente.",
        TextKey::NoBlockItems => "Nessun elemento blocco.",
        TextKey::NoItems => "Nessun elemento.",
        TextKey::NoItemsToDisplay => "Nessun elemento da visualizzare.",
        TextKey::NoAddonMediaListsYet => "Nessun elenco add-on presente.",
        TextKey::No => "Nessuno",
        TextKey::NoCollections => "Nessuna raccolta presente.",
        TextKey::NoMembers => "Nessun membro presente.",
        TextKey::NoMultiCollections => "Nessuna raccolta multipla presente.",
        TextKey::NoTemplatesFound => "Nessun modello trovato.",
        TextKey::NotFound => "Pagina non trovata.",
        TextKey::OtherVideo => "Altro video",
        TextKey::OtherVideoNotFound => "Altro video non trovato",
        TextKey::OtherVideos => "Altri video",
        TextKey::OpenBrowser => "Apri browser",
        TextKey::Playout => "Playout",
        TextKey::PlayoutItemsAndHistory => "Elementi del playout e cronologia",
        TextKey::PlayoutNotFound => "Playout non trovato.",
        TextKey::Playouts => "Playout",
        TextKey::AddPlayout => "Aggiungi playout",
        TextKey::ClassicScheduleMenu => "Programma classico",
        TextKey::BlockScheduleMenu => "Programma a blocchi",
        TextKey::SequentialScheduleMenu => "Programma sequenziale",
        TextKey::ScriptedScheduleMenu => "Programma scriptato",
        TextKey::JsonDizqueTvScheduleMenu => "Programma JSON (dizqueTV)",
        TextKey::ResetAllPlayouts => "Ripristina tutti i playout",
        TextKey::PlayoutBuildQueued => "Ricostruzione del playout in coda",
        TextKey::PlayoutBuildsQueued => "ricostruzioni del playout in coda",
        TextKey::SearchForPlayouts => "Cerca playout",
        TextKey::PlayoutDetail => "Dettagli del playout",
        TextKey::ShowFiller => "Mostra riempitivo",
        TextKey::EditPlayout => "Modifica playout",
        TextKey::RefreshPlayout => "Aggiorna playout",
        TextKey::ResetPlayout => "Ripristina playout",
        TextKey::ScheduleReset => "Ripristino programma",
        TextKey::SchedulePlayoutReset => "Pianifica il ripristino del playout",
        TextKey::EditExternalJsonFile => "Modifica file JSON esterno",
        TextKey::DeletePlayout => "Elimina playout",
        TextKey::Troubleshoot => "Risoluzione problemi",
        TextKey::DailyResetTime => "Ora di ripristino giornaliero",
        TextKey::DoNotAutomaticallyReset => "Non ripristinare automaticamente",
        TextKey::SaveChanges => "Salva modifiche",
        TextKey::ExternalJsonFileLabel => "File JSON esterno",
        TextKey::EditExternalJsonFilePrompt => "Modifica il file JSON esterno del playout",
        TextKey::UnscheduledPlayoutItem => "NON PROGRAMMATO",
        TextKey::DisabledChannelsAlreadyHavePlayout => "I canali disabilitati hanno già un playout",
        TextKey::ProgramSchedule => "Programma",
        TextKey::SequentialScheduleFileHelp => {
            "Il percorso completo del file di programma sequenziale (YAML)"
        }
        TextKey::BlockTemplatesAddedLater => "I modelli di blocco verranno aggiunti in seguito",
        TextKey::UnsupportedPlayoutKind => "Tipo di playout non supportato.",
        TextKey::ClassicPlayout => "Playout classico",
        TextKey::ClassicPlayoutState => "Stato del playout classico",
        TextKey::ScanRefreshState => "Stato dell'aggiornamento dopo la scansione",
        TextKey::ArtworkCardinality => "Cardinalità delle immagini",
        TextKey::HlsLiveDelivery => "Distribuzione HLS in diretta",
        TextKey::BlockPlayout => "Playout a blocchi",
        TextKey::ScriptedPlayout => "Programma scriptato",
        TextKey::AlternateSchedules => "Programmi alternativi",
        TextKey::EditAlternateSchedules => "Modifica programmi alternativi",
        TextKey::PlayoutTemplates => "Modelli di playout",
        TextKey::EditTemplates => "Modifica modelli",
        TextKey::DefaultDeco => "Deco predefinito",
        TextKey::EnableDefaultDeco => "Abilita deco predefinito",
        TextKey::SaveDefaultDeco => "Salva deco predefinito",
        TextKey::SaveScriptedSchedule => "Salva programma scriptato",
        TextKey::EraseItems => "Cancella elementi",
        TextKey::ErasedPlayoutItems => "Elementi del playout cancellati.",
        TextKey::SavedDefaultDeco => "Deco predefinito salvato.",
        TextKey::SavedScriptedSchedule => "Programma scriptato salvato.",
        TextKey::Maintenance => "Manutenzione",
        TextKey::SaveAlternateSchedules => "Salva programmi alternativi",
        TextKey::AddAlternateSchedule => "Aggiungi programma alternativo",
        TextKey::SaveTemplates => "Salva modelli",
        TextKey::PreviewCalendar => "Anteprima calendario",
        TextKey::EditContent => "Modifica contenuti",
        TextKey::PlayoutNestedCatchAllHelp => {
            "In ordine di priorità da alto a basso. L'ultima voce corrisponderà sempre a tutti i giorni e a tutti i mesi come valore predefinito."
        }
        TextKey::PlayoutNestedPriorityHelp => "In ordine di priorità da alto a basso.",
        TextKey::DaysOfWeek => "Giorni della settimana",
        TextKey::DaysOfMonth => "Giorni del mese",
        TextKey::Months => "Mesi",
        TextKey::LimitToDateRange => "Limita all'intervallo di date",
        TextKey::StartDay => "Giorno di inizio",
        TextKey::EndDay => "Giorno di fine",
        TextKey::Enable => "Abilita",
        TextKey::Up => "Su",
        TextKey::Down => "Giù",
        TextKey::Weekdays => "Giorni feriali",
        TextKey::Weekends => "Weekend",
        TextKey::Sunday => "Domenica",
        TextKey::Monday => "Lunedì",
        TextKey::Tuesday => "Martedì",
        TextKey::Wednesday => "Mercoledì",
        TextKey::Thursday => "Giovedì",
        TextKey::Friday => "Venerdì",
        TextKey::Saturday => "Sabato",
        TextKey::January => "Gennaio",
        TextKey::February => "Febbraio",
        TextKey::March => "Marzo",
        TextKey::April => "Aprile",
        TextKey::May => "Maggio",
        TextKey::June => "Giugno",
        TextKey::July => "Luglio",
        TextKey::August => "Agosto",
        TextKey::September => "Settembre",
        TextKey::October => "Ottobre",
        TextKey::November => "Novembre",
        TextKey::December => "Dicembre",
        TextKey::JanuaryShort => "Gen",
        TextKey::FebruaryShort => "Feb",
        TextKey::MarchShort => "Mar",
        TextKey::AprilShort => "Apr",
        TextKey::MayShort => "Maggio",
        TextKey::JuneShort => "Giu",
        TextKey::JulyShort => "Lug",
        TextKey::AugustShort => "Ago",
        TextKey::SeptemberShort => "Set",
        TextKey::OctoberShort => "Ott",
        TextKey::NovemberShort => "Nov",
        TextKey::DecemberShort => "Dic",
        TextKey::PlayAll => "Riproduci tutto",
        TextKey::Playlist => "Playlist",
        TextKey::PlaylistAdded => "Playlist aggiunta.",
        TextKey::PlaylistDeleted => "Playlist eliminata.",
        TextKey::PlaylistGroup => "Gruppo di playlist",
        TextKey::PlaylistGroupAdded => "Gruppo di playlist aggiunto.",
        TextKey::PlaylistGroupDeleted => "Gruppo di playlist eliminato.",
        TextKey::PlaylistGroupName => "Nome del gruppo di playlist",
        TextKey::PlaylistGroups => "Gruppi di playlist",
        TextKey::PlaylistItem => "Elemento della playlist",
        TextKey::PlaylistItems => "Elementi della playlist",
        TextKey::PlaylistName => "Nome della playlist",
        TextKey::PlaylistNotFound => "Playlist non trovata.",
        TextKey::PlaylistPlayoutPreview => "Anteprima del playout della playlist",
        TextKey::PlaylistPreviewEmpty => {
            "L'anteprima del playout della playlist non contiene elementi"
        }
        TextKey::Playlists => "Playlist",
        TextKey::ProbeForInterlacedFrames => "Rileva fotogrammi interlacciati",
        TextKey::PreferredAudioLanguage => "Lingua audio preferita",
        TextKey::DefaultSubtitleLanguage => "Lingua sottotitoli predefinita",
        TextKey::FallbackSubtitleLanguage => "Lingua sottotitoli di riserva",
        TextKey::OriginalLanguageOption => "Originale (und)",
        TextKey::UndefinedLanguageOption => "Non definito (und)",
        TextKey::Previous24Hours => "Ultimi 24h",
        TextKey::PreviousPage => "Pagina precedente",
        TextKey::RerunCollections => "Raccolte di ripetizioni",
        TextKey::RerunCollection => "Raccolta di ripetizioni",
        TextKey::RerunCollectionActionFailed => "Azione sulla raccolta di ripetizioni fallita.",
        TextKey::RerunCollectionDeleted => "Raccolta di ripetizioni eliminata.",
        TextKey::RerunCollectionNameRequired => {
            "Il nome della raccolta di ripetizioni è obbligatorio!"
        }
        TextKey::RerunCollectionSaved => "Raccolta di ripetizioni salvata.",
        TextKey::RerunPlaybackOrder => "Ordine di riproduzione delle ripetizioni",
        TextKey::RemoteStreams => "Flussi remoti",
        TextKey::RemoteStream => "Flusso remoto",
        TextKey::PublishedOptions => "Opzioni pubblicate",
        TextKey::PlaybackHealth => "Stato di riproduzione",
        TextKey::Configure => "Configura",
        TextKey::Watch => "Guarda",
        TextKey::RemoteStreamPlaylists => "Playlist dei flussi remoti",
        TextKey::PlaylistUrl => "URL della playlist",
        TextKey::Extractor => "Estrattore",
        TextKey::DestinationFolder => "Cartella di destinazione",
        TextKey::SyncInterval => "Intervallo di sincronizzazione",
        TextKey::MaxItems => "Numero massimo di elementi",
        TextKey::RemovedEntries => "Entrate rimosse",
        TextKey::CreateOrderedPlaylist => "Crea una playlist ordinata",
        TextKey::SyncNow => "Sincronizza ora",
        TextKey::LastSync => "Ultima sincronizzazione",
        TextKey::NeverSynced => "Mai sincronizzato",
        TextKey::ManagedManifestReadOnly => {
            "Questo manifesto creato manualmente è solo di lettura qui."
        }
        TextKey::Truncated => "Troncato",
        TextKey::MissingDuration => "Durata mancante",
        TextKey::Orphaned => "Orfano",
        TextKey::Materialized => "Materializzato",
        TextKey::State => "Stato",
        TextKey::Upstream => "A monte",
        TextKey::NewestFirst => "Più recenti per primi",
        TextKey::Keep => "Mantieni",
        TextKey::ResetChannels => "Resetta i canali",
        TextKey::RowsPerPage => "Righe per pagina",
        TextKey::Save => "Salva",
        TextKey::SaveMultiCollection => "Salva raccolta multipla",
        TextKey::SavePlaylist => "Salva playlist",
        TextKey::SaveTroubleshootingReportsToDisk => {
            "Salva i rapporti di risoluzione problemi su disco"
        }
        TextKey::SaveAsSmartCollection => "Salva come raccolta intelligente",
        TextKey::SearchQueryRequired => "La query di ricerca è obbligatoria",
        TextKey::SaveSettings => "Salva impostazioni",
        TextKey::Scanner => "Scanner",
        TextKey::ScanningMinimumLogLevel => "Livello minimo di registro per lo scanner",
        TextKey::Scheduling => "Programmazione",
        TextKey::SchedulingMinimumLogLevel => "Livello minimo di registro per la programmazione",
        TextKey::StartWithWindows => "Avvia ErsatzRS quando mi connetto a Windows",
        TextKey::StartWithWindowsHelp => {
            "Avvia automaticamente il launcher della tray per l'utente Windows corrente."
        }
        TextKey::Schedules => "Programmi",
        TextKey::AddSchedule => "Aggiungi programma",
        TextKey::AddScheduleItem => "Aggiungi elemento programma",
        TextKey::AddSmartCollection => "Aggiungi raccolta intelligente",
        TextKey::Album => "Album",
        TextKey::Chronological => "Cronologico",
        TextKey::CollectionSize => "Dimensione della raccolta",
        TextKey::CollectionType => "Tipo di raccolta",
        TextKey::Count => "Conteggio",
        TextKey::CopySchedule => "Copia programma",
        TextKey::CopyScheduleItem => "Copia elemento programma",
        TextKey::CustomTitle => "Titolo personalizzato",
        TextKey::DeleteSchedule => "Elimina programma",
        TextKey::DeleteScheduleItem => "Elimina elemento programma",
        TextKey::DeleteSmartCollection => "Elimina raccolta intelligente",
        TextKey::DetailRowsPerPage => "Righe dettagli per pagina",
        TextKey::Director => "Regista",
        TextKey::Directors => "Registi",
        TextKey::DiscardToFillAttempts => "Abbandona per tentare di riempire",
        TextKey::Duration => "Durata",
        TextKey::Dynamic => "Dinamico",
        TextKey::EditFillerPreset => "Modifica preset riempitivo",
        TextKey::EditProperties => "Modifica proprietà",
        TextKey::EditScheduleItems => "Modifica elementi programma",
        TextKey::EditSmartCollection => "Modifica raccolta intelligente",
        TextKey::EnterNameForNewSchedule => "Inserisci un nome per il nuovo programma",
        TextKey::Fixed => "Fisso",
        TextKey::FixedStartTimeBehavior => "Comportamento dell'orario di inizio fisso",
        TextKey::Flood => "Alluvione",
        TextKey::Flexible => "Flessibile",
        TextKey::FlexibleStartSoonAsPossible => "Flessibile - Inizia il prima possibile",
        TextKey::FlexibleStartSoonAsPossibleAfterStartTime => {
            "Flessibile - Inizia il prima possibile dopo l'orario di inizio"
        }
        TextKey::FillWithGroupMode => "Riempimento con modalità gruppo (Serie o artista)",
        TextKey::GraphicsElements => "Elementi grafici",
        TextKey::AddGraphicsElement => "Aggiungi elemento grafico",
        TextKey::DrawtextRendererOption => "Drawtext — dinamico/basato su file",
        TextKey::DrawtextSettings => "Impostazioni Drawtext",
        TextKey::DrawvgRendererOption => "Drawvg — statico/vettoriale",
        TextKey::EditGraphicsElement => "Modifica elemento grafico",
        TextKey::FontFile => "File del carattere",
        TextKey::FontFileOptionalHelp => {
            "Facoltativo. Lascia vuoto per usare il carattere Roboto incluso."
        }
        TextKey::GraphicsElement => "Elemento grafico",
        TextKey::GraphicsElementConfigInvalid => {
            "Questo elemento grafico di testo ha una configurazione non valida e non è stato modificato."
        }
        TextKey::GraphicsElementKindImage => "Immagine",
        TextKey::GraphicsElementKindMotion => "Movimento",
        TextKey::GraphicsElementKindScript => "Script",
        TextKey::GraphicsElementKindSubtitle => "Sottotitolo",
        TextKey::GraphicsElementKindText => "Testo",
        TextKey::GraphicsElementNameTooLong => "Il nome deve contenere al massimo 50 caratteri.",
        TextKey::GraphicsElementNotEditable => "Non modificabile qui",
        TextKey::GraphicsElementNotFound => "Elemento grafico non trovato.",
        TextKey::GraphicsElementNotText => {
            "Qui è possibile modificare solo elementi grafici di testo."
        }
        TextKey::GraphicsElementSaved => "Elemento grafico salvato.",
        TextKey::GraphicsElementSaveFailed => "Impossibile salvare l’elemento grafico.",
        TextKey::GraphicsElementType => "Tipo",
        TextKey::GraphicsExistingDrawvgHelp => {
            "Gli elementi esistenti e quelli nuovi usano Drawvg per impostazione predefinita."
        }
        TextKey::GraphicsStreamRestartHelp => {
            "Le modifiche interessano i nuovi flussi e non riavviano quelli attivi."
        }
        TextKey::NoGraphicsElements => "Nessun elemento grafico trovato.",
        TextKey::RefreshPeriodBounds => {
            "Il periodo di aggiornamento deve essere compreso tra 250 e 60000 millisecondi."
        }
        TextKey::RefreshPeriodMilliseconds => "Periodo di aggiornamento (millisecondi)",
        TextKey::ReloadFrameInterval => "Intervallo fotogrammi di ricaricamento",
        TextKey::ReloadFrameIntervalBounds => {
            "L’intervallo fotogrammi di ricaricamento deve essere almeno 1."
        }
        TextKey::Renderer => "Renderer",
        TextKey::SaveGraphicsElement => "Salva elemento grafico",
        TextKey::TextTemplate => "Modello di testo",
        TextKey::TextTemplateRequired => "Il modello di testo è obbligatorio.",
        TextKey::TextTemplateTooLong => "Il modello deve essere testo UTF-8 valido fino a 64 KiB.",
        TextKey::GraphicsIds => "ID grafici",
        TextKey::Genres => "Generi",
        TextKey::GroupName => "Nome gruppo",
        TextKey::GroupMultiPartEpisodesHelp => {
            "Raggruppa gli episodi multi-parte durante lo shuffling"
        }
        TextKey::GuideMode => "Modalità guida",
        TextKey::KeepMultiPartEpisodesTogether => "Mantieni gli episodi multi-parte insieme",
        TextKey::Marathon => "Maratona",
        TextKey::MarathonBatchSize => "Dimensione lotto maratona",
        TextKey::MarathonBatchSizeHelp => {
            "Numero di elementi da riprodurre da ciascun gruppo prima di avanzare; vuoto o zero riprodurrà tutti gli elementi"
        }
        TextKey::MarathonGroupBy => "Raggruppa per maratona",
        TextKey::MarathonShuffleGroups => "Mescola gruppi maratona",
        TextKey::MarathonShuffleItems => "Elementi casuali della maratona",
        TextKey::MidRollFiller => "Riempitivo Mid-Roll",
        TextKey::MidRoll => "Mid-Roll",
        TextKey::MoveDown => "Sposta in basso",
        TextKey::MoveUp => "Sposta in alto",
        TextKey::ReorderPlaylistItem => "Trascina per riordinare l'elemento della playlist",
        TextKey::MultiEpisodeGroupSize => "Dimensione gruppo multi-episodio",
        TextKey::MultiEpisodeShuffle => "Scambio multi-episodio",
        TextKey::Multiple => "Multipli",
        TextKey::MultipleCount => "Conteggio multipli",
        TextKey::MultipleCountHelp => {
            "Può essere un'espressione con 'count' (dimensione della raccolta) e 'random' (da 0 a count-1)"
        }
        TextKey::MultipleMode => "Modalità multipli",
        TextKey::NewScheduleName => "Nuovo nome programma",
        TextKey::NewSmartCollection => "Nuova raccolta intelligente",
        TextKey::NoScheduleItems => "Nessun elemento del programma.",
        TextKey::NoSchedules => "Nessun programma.",
        TextKey::Normal => "Normale",
        TextKey::Offline => "Offline",
        TextKey::One => "Uno",
        TextKey::OrderedGroups => "Gruppi ordinati",
        TextKey::Playback => "Riproduzione",
        TextKey::PlaybackOrder => "Ordine di riproduzione",
        TextKey::PlaybackOrderUpdateFailed => "Aggiornamento ordine di riproduzione fallito",
        TextKey::PlaybackOrderUpdated => "Ordine di riproduzione aggiornato.",
        TextKey::PlaylistItemSize => "Dimensione dell'elemento della playlist",
        TextKey::PlayoutDurationHours => "Ore di durata del playout",
        TextKey::PlayoutDurationMinutes => "Minuti di durata del playout",
        TextKey::PostRollFiller => "Riempitivo Post-Roll",
        TextKey::PostRoll => "Post-Roll",
        TextKey::PreRollFiller => "Riempitivo Pre-Roll",
        TextKey::PreRoll => "Pre-Roll",
        TextKey::Random => "Casuale",
        TextKey::RandomCount => "Conteggio casuale",
        TextKey::RandomRotation => "Rotazione casuale",
        TextKey::RemainingDurationHelp => {
            "durata del contenuto dopo questo punto mid-roll, in secondi"
        }
        TextKey::RandomStartPoint => "Punto iniziale casuale",
        TextKey::RemoveFailed => "Rimozione fallita",
        TextKey::RemovedFromCollection => "Rimosso dalla raccolta.",
        TextKey::RemoveFromCollection => "Rimuovi dalla raccolta",
        TextKey::Rename => "Rinomina",
        TextKey::RenameFailed => "Rinomazione fallita",
        TextKey::ReorderFailed => "Riordino fallito",
        TextKey::RerunFirstRun => "Ripeti prima esecuzione",
        TextKey::RerunRerun => "Ripeti ripetizione",
        TextKey::ReadOnlyTwentyFourHourPlayoutView => {
            "Vista di sola lettura di 24 ore degli elementi del playout programmati."
        }
        TextKey::SaveSchedule => "Salva programma",
        TextKey::SaveScheduleItems => "Salva elementi programma",
        TextKey::SaveSmartCollection => "Salva raccolta intelligente",
        TextKey::Schedule => "Programma",
        TextKey::ScheduleActionFailed => "Azione programma fallita.",
        TextKey::ScheduleAsGroup => "Programma come gruppo",
        TextKey::ScheduleKind => "Tipo programma",
        TextKey::ScheduleKindBlock => "Blocco",
        TextKey::ScheduleKindClassic => "Classico",
        TextKey::ScheduleKindExternalJson => "JSON (dizqueTV)",
        TextKey::ScheduleKindScripted => "Scriptato",
        TextKey::ScheduleKindSequential => "Sequenziale",
        TextKey::SchedulingContext => "Contesto programmazione",
        TextKey::ScheduleItem => "Elemento programma",
        TextKey::ScheduleItemActionFailed => "Azione elemento programma fallita.",
        TextKey::ScheduleItemFiller => "Elemento programma riempitivo",
        TextKey::ScheduleItemFillerHelp => {
            "Il riempitivo pre-, mid- e post-roll si applica a ogni elemento multimedia selezionato da questo elemento programma. Per riprodurre il contenuto una volta dopo un blocco Multipli, aggiungerlo come prossimo elemento programma."
        }
        TextKey::ScheduleItemOverrides => "Sovrascritture elemento programma",
        TextKey::ScheduleItems => "Elementi programma",
        TextKey::ScheduleNotFound => "Programma non trovato.",
        TextKey::ScheduleSaveFailed => "Salvataggio programma fallito.",
        TextKey::SmartShuffle => "Scambio intelligente",
        TextKey::SmartShufflePreset => "Preset spaziatura",
        TextKey::Balanced => "Bilanciato",
        TextKey::SmartShuffleScope => "Condividi spaziatura tra",
        TextKey::SmartShuffleScopePlayout => "Playout",
        TextKey::SmartShuffleScopeScheduleItem => "Elemento programma",
        TextKey::SmartShuffleValidation => {
            "Scegli una soglia di spaziatura e almeno una regola di similarità."
        }
        TextKey::SmartShuffleScopeContentSource => "Sorgente contenuto",
        TextKey::SmartShuffleAfterItems => "Video tra elementi simili",
        TextKey::SmartShuffleAfterDurationMinutes => "Minuti tra elementi simili",
        TextKey::SmartShuffleIncludeFiller => "Includi riempitivo",
        TextKey::SmartShuffleInheritedBlock => {
            "Questo elemento non ha sovrascrittura Scambio intelligente e mantiene qualsiasi politica di playout legacy."
        }
        TextKey::SmartShuffleInactive => {
            "Scambio intelligente è inattivo perché questo ordine di riproduzione non è randomizzato."
        }
        TextKey::SmartShuffleOverrideBlock => "Sovrascrittura per questo elemento blocco",
        TextKey::SimilarityRules => "Regole di similarità",
        TextKey::AlbumArtist => "Artista album",
        TextKey::Apply => "Applica",
        TextKey::ContentParent => "Genitore o variante",
        TextKey::SmartShuffleFilterRules => "Regole di filtraggio",
        TextKey::SearchForSchedules => "Cerca gli orari",
        TextKey::SearchForManualCollections => "Cerca raccolte manuali",
        TextKey::SearchForRerunCollections => "Cerca raccolte di ripetizioni",
        TextKey::SearchForSmartCollections => "Cerca raccolte intelligenti",
        TextKey::SearchForTemplates => "Cerca modelli",
        TextKey::SearchForBlocks => "Cerca blocchi",
        TextKey::SearchForMultiCollections => "Cerca raccolte multiple",
        TextKey::SearchQuery => "Ricerca query",
        TextKey::BlockSearchQueryPlaceholder => "ad es. genere:azione",
        TextKey::SearchTitle => "Cerca titolo",
        TextKey::SearchTitleDisplay => "Cerca titolo (visualizzazione)",
        TextKey::SearchSmartCollection => "Cerca raccolta intelligente",
        TextKey::Season => "Stagione",
        TextKey::SeasonEpisode => "Stagione, Episodio",
        TextKey::Unnumbered => "Non numerato",
        TextKey::SeasonNotFound => "Stagione non trovata",
        TextKey::Select => "Seleziona",
        TextKey::SelectCollection => "Seleziona una raccolta",
        TextKey::SelectGroupForNewTemplate => "Seleziona un gruppo per il nuovo modello",
        TextKey::SelectScheduleItemPrompt => {
            "Seleziona un elemento dell'orario da modificare o aggiungi un nuovo elemento."
        }
        TextKey::SelectSchedule => "Seleziona un orario",
        TextKey::SelectSmartCollection => "Seleziona raccolta intelligente",
        TextKey::Shuffle => "Mescola",
        TextKey::ShuffleInOrder => "Mescola in ordine",
        TextKey::ShuffleScheduleItems => "Mescola elementi dell'orario",
        TextKey::ShuffleScheduleItemsHelp => {
            "Nota: questa opzione disabilita gli orari di inizio fissi e la modalità alluvione"
        }
        TextKey::ShuffledGroups => "Gruppi mescolati",
        TextKey::Show => "Serie",
        TextKey::ShowNotFound => "Serie non trovata",
        TextKey::Shows => "Serie",
        TextKey::Start => "Inizio",
        TextKey::StartTime => "Ora di inizio",
        TextKey::StartTimeOnOrAfter => "Inizio da o dopo",
        TextKey::StartType => "Tipo di inizio",
        TextKey::Strict => "Rigido",
        TextKey::StrictAlwaysWaitExactStartTime => {
            "Rigido - Aspetta sempre l'orario di inizio esatto"
        }
        TextKey::StrictWaitExactStartTime => "Rigido - Aspetta l'orario di inizio esatto",
        TextKey::TailFiller => "Riempitivo finale",
        TextKey::TailMode => "Modalità finale",
        TextKey::Tail => "Finale",
        TextKey::TelevisionSeason => "Stagione televisiva",
        TextKey::TelevisionShow => "Serie televisiva",
        TextKey::TreatCollectionsAsShows => "Tratta le raccolte come serie",
        TextKey::TreatCollectionsAsShowsHelp => "Utile per episodi di crossover in più parti",
        TextKey::Type => "Tipo",
        TextKey::TypeToSearch => "Tipo da cercare...",
        TextKey::Search => "Cerca",
        TextKey::Filters => "Filtri",
        TextKey::ActiveFilters => "filtri attivi",
        TextKey::ClearFilters => "Cancella filtri",
        TextKey::CreateSmartCollection => "Crea raccolta intelligente",
        TextKey::FromYear => "Dall’anno",
        TextKey::ToYear => "All’anno",
        TextKey::CollectionComposer => "Editor della raccolta intelligente",
        TextKey::CollectionDetails => "Dettagli della raccolta",
        TextKey::Rules => "Regole",
        TextKey::Overview => "Panoramica",
        TextKey::MatchAll => "Soddisfa tutte",
        TextKey::MatchAllHelp => "Ogni regola di questo gruppo deve corrispondere.",
        TextKey::MatchAny => "Soddisfa almeno una",
        TextKey::MatchAnyHelp => "Almeno una regola di questo gruppo deve corrispondere.",
        TextKey::ExcludeAny => "Escludi corrispondenze",
        TextKey::ExcludeAnyHelp => {
            "Gli elementi che corrispondono a una regola del gruppo vengono esclusi."
        }
        TextKey::AddRule => "Aggiungi regola",
        TextKey::EditRule => "Modifica regola",
        TextKey::RemoveRule => "Rimuovi regola",
        TextKey::RuleField => "Campo",
        TextKey::Genre => "Genere",
        TextKey::Studio => "Studio",
        TextKey::Network => "Rete",
        TextKey::Actor => "Attore",
        TextKey::Writer => "Sceneggiatore",
        TextKey::SubtitleLanguage => "Lingua sottotitoli",
        TextKey::ShowTitle => "Titolo serie",
        TextKey::ContentKindLabel => "Tipo di contenuto",
        TextKey::SubKindLabel => "Sottotipo",
        TextKey::HasCommentary => "Con commento",
        TextKey::AllFields => "Tutti i campi",
        TextKey::MatchType => "Tipo di corrispondenza",
        TextKey::MatchValue => "Valore",
        TextKey::TextContains => "Il testo contiene",
        TextKey::ExactPhrase => "Frase esatta",
        TextKey::Wildcard => "Carattere jolly",
        TextKey::Fuzzy => "Approssimativa",
        TextKey::Range => "Intervallo",
        TextKey::RangeStart => "Inizio intervallo",
        TextKey::RangeEnd => "Fine intervallo",
        TextKey::NestedGroup => "Gruppo annidato",
        TextKey::NestedGroupHelp => {
            "Modifica il gruppo in modalità focus per mostrare un solo livello booleano alla volta."
        }
        TextKey::AddNestedGroup => "Aggiungi gruppo annidato",
        TextKey::EditNestedGroup => "Modifica gruppo annidato",
        TextKey::BackToRules => "Torna alle regole",
        TextKey::BackToSearch => "Torna alla ricerca",
        TextKey::CanonicalQuery => "Query canonica",
        TextKey::CanonicalQueryHelp => {
            "Questa query viene usata dalla ricerca, dalla programmazione e dalla raccolta salvata."
        }
        TextKey::LivePreview => "Anteprima in tempo reale",
        TextKey::MatchingItems => "elementi corrispondenti",
        TextKey::QueryValid => "Query valida",
        TextKey::QueryInvalid => "La query richiede attenzione",
        TextKey::SearchRuleValueRequired => "Ogni regola di ricerca richiede un valore.",
        TextKey::SearchGroupRequired => "Aggiungi almeno una regola a questo gruppo.",
        TextKey::SearchWildcardRequired => "Una regola con caratteri jolly deve contenere * o ?.",
        TextKey::SavedSearches => "Ricerche salvate",
        TextKey::SavedSearchName => "Nome della ricerca salvata",
        TextKey::SavedSearchHelp => {
            "Salva questa ricerca complessa per riprenderla e modificarla in seguito."
        }
        TextKey::ChooseSavedSearch => "Scegli una ricerca salvata",
        TextKey::LoadSavedSearch => "Carica ricerca salvata",
        TextKey::SaveSavedSearch => "Salva ricerca",
        TextKey::UpdateSavedSearch => "Aggiorna ricerca salvata",
        TextKey::NewSavedSearch => "Nuova ricerca salvata",
        TextKey::DeleteSavedSearch => "Elimina ricerca salvata",
        TextKey::SavedSearchSaved => "Ricerca salvata.",
        TextKey::SavedSearchDeleted => "Ricerca salvata eliminata.",
        TextKey::SavedSearchLoaded => "Ricerca salvata caricata.",
        TextKey::SavedSearchNameRequired => "Il nome della ricerca salvata è obbligatorio.",
        TextKey::SavedSearchNameTooLong => "Il nome deve contenere al massimo 80 caratteri.",
        TextKey::SavedSearchNameUnique => "Il nome della ricerca salvata deve essere univoco.",
        TextKey::NoSavedSearches => "Nessuna ricerca salvata.",
        TextKey::RefreshPreview => "Aggiorna anteprima",
        TextKey::NoRules => "Nessuna regola in questo gruppo.",
        TextKey::SearchFiltersHelp => {
            "Solo filtri comuni. Usa l’editor della raccolta per la logica annidata."
        }
        TextKey::SearchMedia => "Cerca media",
        TextKey::SearchMoviesOrShows => "Cerca film o serie...",
        TextKey::SearchAddonMediaList => "Cerca lista add-on",
        TextKey::SearchingMinimumLogLevel => "Livello minimo di registro durante la ricerca",
        TextKey::Settings => "Impostazioni",
        TextKey::SettingsSaveFailed => "Salvataggio impostazioni fallito",
        TextKey::ShowMediaInfo => "Mostra informazioni sul media",
        TextKey::ScriptedScheduleTimeout => "Timeout per gli orari programmati",
        TextKey::SkipMissingItems => "Salta gli elementi mancanti",
        TextKey::SkipMissingItemsHelp => {
            "Controlla se gli elementi non trovati o non disponibili devono essere inclusi nelle trasmissioni"
        }
        TextKey::SmartCollections => "Raccolte intelligenti",
        TextKey::SmartCollection => "Raccolta intelligente",
        TextKey::SmartCollectionDeleted => "Raccolta intelligente eliminata.",
        TextKey::SmartCollectionQueryPlaceholder => "ad es. genere:azione OPPURE genere:commedia",
        TextKey::SmartCollectionSaved => "Raccolta intelligente salvata.",
        TextKey::SaveAddonMediaList => "Salva lista add-on",
        TextKey::Songs => "Canzoni",
        TextKey::SplitTimeEvenly => "Dividi il tempo in modo uniforme",
        TextKey::StreamingMinimumLogLevel => "Livello minimo di registro per lo streaming",
        TextKey::Studios => "Studi",
        TextKey::Support => "Supporto",
        TextKey::Template => "Modello",
        TextKey::TemplateCopied => "Modello copiato.",
        TextKey::TemplateCreated => "Modello creato.",
        TextKey::TemplateDeleted => "Modello eliminato.",
        TextKey::TemplateGroup => "Gruppo di modelli",
        TextKey::TemplateGroupCreated => "Gruppo di modelli creato.",
        TextKey::TemplateGroupDeleted => "Gruppo di modelli eliminato.",
        TextKey::TemplateGroupName => "Nome del gruppo di modelli",
        TextKey::TemplateGroups => "Gruppi di modelli",
        TextKey::TemplateName => "Nome del modello",
        TextKey::TemplateNotFound => "Modello non trovato.",
        TextKey::Templates => "Modelli",
        TextKey::Theme => "Tema",
        TextKey::ThisActionCannotBeUndone => "Questa azione non può essere annullata.",
        TextKey::ThisCollectionHasNoItemsYet => "Questa raccolta non ha ancora elementi.",
        TextKey::Thumbnail => "miniatura",
        TextKey::Title => "Titolo",
        TextKey::ToggleNavigation => "Attiva/disattiva navigazione",
        TextKey::TotalDurationHelp => "durata totale del contenuto, in secondi",
        TextKey::TotalPointsHelp => "numero totale di punti potenziali mid-roll",
        TextKey::TotalProgressHelp => "posizione normalizzata da 0 a 1",
        TextKey::AddonMediaList => "Lista add-on",
        TextKey::AddonMediaListActionFailed => "add-on l'azione della lista non è riuscita.",
        TextKey::AddonMediaListAdded => "Lista add-on aggiunta.",
        TextKey::AddonMediaListDeleted => "Lista add-on eliminata.",
        TextKey::AddonMediaListMatched => "Lista add-on trovata.",
        TextKey::AddonMediaListNotFound => "Lista add-on non trovata.",
        TextKey::AddonMediaListSaveFailed => "add-on il salvataggio della lista non è riuscito.",
        TextKey::AddonMediaListSaved => "Lista add-on salvata.",
        TextKey::AddonMediaLists => "add-on Liste",
        TextKey::AddonToolMaintenance => "Manutenzione degli strumenti",
        TextKey::AddonToolMaintenanceNone => "Nessun tentativo di manutenzione.",
        TextKey::AddonToolUpdatePermission => "Consenti aggiornamenti nativi degli eseguibili configurati",
        TextKey::AddonToolMaintenanceQueued => "In coda",
        TextKey::AddonToolMaintenanceRunning => "In esecuzione",
        TextKey::AddonToolMaintenanceWaiting => "In attesa di un nuovo tentativo",
        TextKey::AddonToolUpdated => "Aggiornato",
        TextKey::AddonToolAlreadyCurrent => "Già aggiornato",
        TextKey::AddonToolManualUpdate => "Aggiornamento manuale necessario",
        TextKey::AddonToolManualHelp => "Aggiorna manualmente lo strumento configurato con il metodo di installazione originale, poi verifica il componente aggiuntivo.",
        TextKey::AddonToolFailedHelp => "L’aggiornamento non è riuscito. Verifica la disponibilità dello strumento e l’accesso alla rete; la manutenzione automatica riproverà più tardi.",
        TextKey::AddonToolUpdateFailedStale => "Non è stato possibile aggiornare il programma ed è disponibile una versione più recente. La manutenzione automatica riproverà più tardi. Per aggiornarlo ora, usa il metodo di installazione originale, quindi verifica il componente aggiuntivo.",
        TextKey::AddonToolUpdateVersions => "{program}: versione installata {installed}, ultima versione {latest}.",
        TextKey::AddonToolVersionHelp => "Impossibile leggere la versione dello strumento. Verifica l’eseguibile configurato e le sue autorizzazioni.",
        TextKey::AddonToolUnavailableHelp => "Lo strumento configurato non è disponibile. Verifica l’impostazione dell’eseguibile e l’installazione.",
        TextKey::AddonToolBusyHelp => "In attesa che lo strumento sia libero; la riproduzione attiva non viene interrotta.",
        TextKey::AddonToolCleanupHelp => "Arresta il server, lo scanner e tutti i processi correlati del provider e dello strumento. Dalla cartella di installazione del server, esegui il comando di ripristino offline per questo strumento con conferma esplicita, sostituendo <absolute-executable-path> con il percorso assoluto configurato. Su Windows usa ersatzrs-web.exe. Per un profilo personalizzato, aggiungi --state-dir con la sua cartella assoluta. Riavvia solo dopo. L’operazione rimuove il blocco di sicurezza dei processi; non ripara l’eseguibile.",
        TextKey::AddonListDiskWarnings => "Avvisi sull’utilizzo del disco",
        TextKey::AddonListDiskWarningGb => "Questa lista: avvisa a partire da (GB)",
        TextKey::AddonListsCombinedDiskWarningGb => "Tutte le liste dei componenti aggiuntivi: avvisa a partire da (GB)",
        TextKey::AddonListDiskWarningHelp => "GB decimali interi (1 GB = 1.000.000.000 byte). 0 disattiva solo questo avviso. Raggiungere o superare la soglia mostra soltanto un avviso sullo stato; non interrompe i download né elimina file.",
        TextKey::AddonListDiskWarningFailed => "Impossibile caricare o salvare l’avviso sull’utilizzo del disco. Ricarica la pagina e riprova.",
        TextKey::AddonListDiskUsageTitle => "Utilizzo del disco delle liste dei componenti aggiuntivi",
        TextKey::AddonListDiskUsageBrief => "Avviso sull’utilizzo del disco delle liste dei componenti aggiuntivi",
        TextKey::AddonListDiskUsageUnavailable => "Impossibile misurare completamente l’utilizzo del disco delle liste dei componenti aggiuntivi. Verifica l’accesso all’archiviazione e ripeti il controllo dello stato.",
        TextKey::AddonListDiskUsageUnavailableBrief => "Impossibile misurare l’utilizzo del disco delle liste dei componenti aggiuntivi.",
        TextKey::AddonListDiskUsageLine => "Lista n. {id}: {used} GB utilizzati; avviso a partire da {limit} GB.",
        TextKey::AddonListsCombinedDiskUsageLine => "Totale: {used} GB utilizzati; avviso a partire da {limit} GB.",
        TextKey::AddonMediaListUrl => "URL della lista add-on",
        TextKey::AddonMediaListFolder => "Cartella di archiviazione",
        TextKey::AddonMediaListFolderHelp => {
            "Facoltativo; altrimenti derivato dal nome della lista"
        }
        TextKey::DestructiveActions => "Azioni distruttive",
        TextKey::ReconcileEntireSource => "Riconcilia l’intera fonte",
        TextKey::ReconcileEntireSourceHelp => {
            "Confronta un’istantanea completa della fonte senza modificare la pubblicazione corrente o le impostazioni degli elementi."
        }
        TextKey::ReconcileQueued => "Riconciliazione completa della fonte accodata.",
        TextKey::ReconcileFailed => "Riconciliazione completa della fonte non riuscita.",
        TextKey::ResetAndReload => "Reimposta e ricarica",
        TextKey::ResetAddonMediaListHelp => {
            "Rimuove tutti i risultati generati per questa lista, quindi accoda un'importazione completa."
        }
        TextKey::SourceHost => "Host di origine",
        TextKey::ResetRemoves => "Questa reimpostazione rimuove",
        TextKey::ResetRemovesItems => {
            "Elementi importati, metadati del provider, modifiche dell'operatore, numerazione degli episodi e sostituzioni."
        }
        TextKey::ResetRemovesArtifacts => {
            "Media generati, file NFO, immagini, sottotitoli, cache e archivi."
        }
        TextKey::ResetRemovesRuntime => {
            "Processi, checkpoint, artefatti di revisione, elementi della playlist generata e media proiettati."
        }
        TextKey::ResetRetains => "Questa reimpostazione conserva",
        TextKey::ResetRetainsSettings => {
            "Il nome della lista, i collegamenti ad add-on, origine e libreria, l'identità della playlist generata, i criteri di aggiornamento e pubblicazione, la quota degli artefatti e le altre impostazioni dell'operatore."
        }
        TextKey::ResetReloadFailureWarning => {
            "Se la ricarica non riesce, i canali e la playlist generata rimangono vuoti."
        }
        TextKey::TypeListNameToConfirm => "Digita il nome corrente della lista per confermare",
        TextKey::ResetQueued => "Reimpostazione e ricarica accodate.",
        TextKey::ResetAddonMediaListFailed => "Reimpostazione e ricarica non riuscite.",
        TextKey::ChapterInput => "Capitoli",
        TextKey::ChapterInputHelp => "Facoltativo, uno per riga: 0:00 Introduzione",
        TextKey::RestoreProviderChapters => "Ripristina i capitoli del provider",
        TextKey::ManagedByAddon => "Gestito da un componente aggiuntivo",
        TextKey::Trash => "Cestino",
        TextKey::TrashAlreadyEmpty => "Il cestino è già vuoto.",
        TextKey::TrashEmpty => "Il cestino è vuoto.",
        TextKey::TrashEmptyConfirm => "Eliminare ogni elemento non trovato nel database?",
        TextKey::TrashEmptyConfirmHelp => {
            "Questo rimuove permanentemente gli elementi multimediali attualmente mancanti e le loro righe di metadati dipendenti."
        }
        TextKey::TrashSubtitle => {
            "Elementi il cui file sorgente è attualmente mancante dal disco o dal provider."
        }
        TextKey::Troubleshooting => "Risoluzione problemi",
        TextKey::TunerCount => "Numero di sintonizzatori",
        TextKey::TvShows => "Programmi TV",
        TextKey::Ui => "Interfaccia utente",
        TextKey::Unavailable => "Non disponibile",
        TextKey::UseActualTimes => "Usa orari reali",
        TextKey::UseCustomPlaybackOrder => "Usa ordine di riproduzione personalizzato",
        TextKey::UseChaptersAsMediaItems => "Usa capitoli come elementi multimediali",
        TextKey::UseChaptersAsMediaItemsHelp => {
            "Programma capitoli individuali invece di file interi"
        }
        TextKey::UseEmbeddedSubtitles => "Usa sottotitoli incorporati",
        TextKey::Uuid => "UUID",
        TextKey::UntitledItem => "Elemento senza titolo",
        TextKey::WindowsStartup => "Avvio di Windows",
        TextKey::WindowsStartupNeedsRepair => {
            "Il comando di avvio salvato punta a un'altra posizione. Applica questa impostazione per ripararlo."
        }
        TextKey::WindowsStartupUnavailable => {
            "L'avvio di Windows non è disponibile perché non è stato trovato il launcher imballato o non è stato possibile leggere la registrazione."
        }
        TextKey::ActiveTranscodeLimit => "Trascode di streaming simultanei massimi",
        TextKey::ActiveTranscodeLimitHelp => {
            "Limite massimo per le trascode FFmpeg di streaming. Quando tutte le slot sono occupate, un nuovo utente riceve una risposta all-streams-in-use fino a quando una slot non si libera; gli stream attivi non vengono bypassati oltre il limite."
        }
        TextKey::ActiveTranscodeSuggestionHardware => {
            "Encoder hardware rilevato; questo è un valore suggerito, non una modifica automatica."
        }
        TextKey::ActiveTranscodeSuggestionSoftware => {
            "Solo codifica software; questo è un valore suggerito, non una modifica automatica."
        }
        TextKey::Peak => "picco",
        TextKey::StreamsInUse => "Stream in uso",
        TextKey::SuggestedTranscodeLimit => "Valore suggerito",
        TextKey::Watermarks => "Filigrane",
        TextKey::Width => "Larghezza",
        TextKey::WorkAheadSegmenterLimit => "Trascode HLS Work-Ahead",
        TextKey::WorkAheadSegmenterLimitHelp => {
            "Quante slot di trascode HLS possono funzionare in anticipo alla massima velocità per costruire la buffer. Work-ahead cede sempre agli stream guardati."
        }
        TextKey::AccelerationKind => "Tipo di accelerazione",
        TextKey::AddFfmpegProfile => "Aggiungi profilo FFmpeg",
        TextKey::AddProfile => "Aggiungi profilo",
        TextKey::AllowBFrames => "Consenti B-Frames",
        TextKey::AllowWatermarks => "Consenti filigrane",
        TextKey::Audio => "Audio",
        TextKey::AudioBitrate => "Bitrate audio (kBit/s)",
        TextKey::AudioBufferSize => "Dimensione buffer audio (kBit)",
        TextKey::AudioChannels => "Canali",
        TextKey::AudioFormat => "Formato audio",
        TextKey::BitDepth => "Profondità bit",
        TextKey::Copy => "Copia",
        TextKey::CopyBlock => "Blocco copia",
        TextKey::CopyFailed => "Copia non riuscita",
        TextKey::CopyNameSuffix => "Copia",
        TextKey::CopyProfile => "Profilo copia",
        TextKey::CopyTemplate => "Modello copia",
        TextKey::Crop => "Ritaglio",
        TextKey::DeinterlaceVideo => "Deinterlaccia video",
        TextKey::Edit => "Modifica",
        TextKey::EditBlock => "Blocco modifica",
        TextKey::EditCollection => "Modifica raccolta",
        TextKey::EditFfmpegProfile => "Profilo modifica FFmpeg",
        TextKey::EditMultiCollection => "Modifica raccolta multipla",
        TextKey::EditRerunCollection => "Modifica raccolta di ripetizioni",
        TextKey::EditTemplate => "Modifica modello",
        TextKey::Expression => "Espressione",
        TextKey::Fallback => "Alternativa",
        TextKey::FillerKind => "Tipo di riempitivo",
        TextKey::General => "Generale",
        TextKey::HardwareAcceleration => "Accelerazione hardware",
        TextKey::HardwareIfPossible => "Se possibile",
        TextKey::NewProfileName => "Nuovo nome profilo",
        TextKey::NoneOption => "(Nessuno)",
        TextKey::NumHelp => "il numero del punto di inserzione, a partire da 1",
        TextKey::OptionalDisplayTitle => "Titolo opzionale da visualizzare...",
        TextKey::TranscodeAudio => "Transcodifica audio",
        TextKey::NormalizeColors => "Normalizza colori",
        TextKey::NormalizeFramerate => "Normalizza fotogrammi al secondo",
        TextKey::NormalizeLoudness => "Normalizza volume",
        TextKey::TranscodeVideo => "Transcodifica video",
        TextKey::Off => "Disattivato",
        TextKey::Pad => "Padding",
        TextKey::PadNearestFifteenMinutes => "15 (:00, :15, :30, :45)",
        TextKey::PadNearestFiveMinutes => "5 (:00, :05, :10, :15, :20, ecc)",
        TextKey::PadNearestTenMinutes => "10 (:00, :10, :20, :30, :40, :50)",
        TextKey::PadNearestThirtyMinutes => "30 (:00, :30)",
        TextKey::PadMode => "Modalità padding",
        TextKey::PadToNearestMinute => "Padding fino al minuto più vicino",
        TextKey::PointHelp => "la posizione del punto di inserzione, in secondi",
        TextKey::ProfileCopied => "Profilo copiato.",
        TextKey::ProfileNotFound => "Profilo non trovato.",
        TextKey::QsvExtraHardwareFrames => "QSV Frame hardware extra",
        TextKey::Resolution => "Risoluzione",
        TextKey::SampleRateKhz => "Frequenza di campionamento (kHz)",
        TextKey::SaveDecoTemplate => "Salva modello Deco",
        TextKey::SaveProfile => "Salva profilo",
        TextKey::SaveFillerPreset => "Salva preset riempitivo",
        TextKey::SaveRerunCollection => "Salva raccolta di ripetizioni",
        TextKey::SaveSequentialSchedule => "Salva programma sequenziale",
        TextKey::SaveTemplate => "Salva modello",
        TextKey::SaveBlock => "Salva blocco",
        TextKey::ScaleAndPad => "Ridimensiona e padding",
        TextKey::ScalingBehavior => "Comportamento ridimensionamento",
        TextKey::Software => "Software",
        TextKey::Stretch => "Stiramento",
        TextKey::TargetLoudnessLufs => "Volume target (LUFS)",
        TextKey::ThreadCountAuto => "Numero thread (0 = automatico)",
        TextKey::OutputDynamicRange => "Gamma dinamica di output",
        TextKey::OutputDynamicRangeCopyHint => {
            "Il video è copiato, quindi l'output mantiene la gamma dinamica della sorgente."
        }
        TextKey::HdrStaticMetadataHint => {
            "Lascia vuoto per utilizzare i valori predefiniti HDR10 documentati. Questi si applicano solo quando la sorgente non porta metadati HDR propri; i livelli del display di mastering e di luminosità della sorgente vincono sempre."
        }
        TextKey::HdrMasteringDisplay => "Display di mastering",
        TextKey::HdrMaxCll => "MaxCLL (cd/m²)",
        TextKey::HdrMaxFall => "MaxFALL (cd/m²)",
        TextKey::TonemapAlgorithm => "Algoritmo di tonemapping",
        TextKey::VaapiDevicePath => "VAAPI Percorso dispositivo",
        TextKey::VaapiDisplay => "Display VAAPI",
        TextKey::VaapiDriver => "Driver VAAPI",
        TextKey::Video => "Video",
        TextKey::VideoBitrate => "Bitrate video (kBit/s)",
        TextKey::VideoBufferSize => "Dimensione buffer video (kBit)",
        TextKey::VideoFormat => "Formato video",
        TextKey::VideoPreset => "Preset video",
        TextKey::VideoTune => "Impostazioni video",
        TextKey::VideoProfile => "Profilo video",
        TextKey::ViewCollection => "Visualizza raccolta",
        TextKey::ViewPlaylist => "Visualizza playlist",
        TextKey::AddAll => "Aggiungi tutto",
        TextKey::AddAllToCollection => "Aggiungi tutto alla raccolta",
        TextKey::AddAllToPlaylist => "Aggiungi tutto alla playlist",
        TextKey::AddedItemToPlaylist => "Elemento aggiunto alla playlist.",
        TextKey::AddedArtistToSchedule => "Artista aggiunto al programma",
        TextKey::AddedMovieToSchedule => "Film aggiunto al programma",
        TextKey::AddedSeasonToSchedule => "Stagione aggiunta al programma",
        TextKey::AddedShowToSchedule => "Serie aggiunta al programma",
        TextKey::AddingItem => "Aggiornamento elemento...",
        TextKey::AddingItemToPlaylist => "Aggiunta elemento alla playlist...",
        TextKey::Artists => "Artisti",
        TextKey::ClearSelection => "Cancella selezione",
        TextKey::Collection => "Raccolta",
        TextKey::CollectionNameRequired => "Il nome della raccolta è obbligatorio",
        TextKey::CollectionNotFound => "Raccolta non trovata",
        TextKey::CollectionRenamed => "Raccolta rinominata.",
        TextKey::EditAddonMediaListProperties => "Modifica proprietà elenco add-on",
        TextKey::EnterAddonMediaListUrl => "Inserisci elenco add-on URL",
        TextKey::EnterSearchQuery => "Inserisci una query di ricerca.",
        TextKey::Episodes => "Episodi",
        TextKey::LoadingCollections => "Caricamento raccolte...",
        TextKey::LoadingPlaylistGroups => "Caricamento gruppi playlist...",
        TextKey::LoadingPlaylists => "Caricamento playlist...",
        TextKey::GeneratePlaylist => "Crea playlist nell’ordine originale",
        TextKey::GeneratePlaylistFromSortedAddonMediaListItems => {
            "Mantieni l’ordine originale del provider dopo ogni pubblicazione"
        }
        TextKey::MusicVideos => "Video musicali",
        TextKey::NewCollectionName => "Nuovo nome della raccolta",
        TextKey::NoMediaMatched => "Nessun media trovato.",
        TextKey::NoItemsToAdd => "Nessun elemento da aggiungere.",
        TextKey::NoItemsToPreview => "Nessun elemento da anteprima.",
        TextKey::NoPlaylistsInGroup => "Nessuna playlist in questo gruppo.",
        TextKey::PleaseChoosePlaylist => "Scegliere una playlist.",
        TextKey::RequestFailed => "Richiesta fallita",
        TextKey::Released => "Pubblicato",
        TextKey::SaveAs => "Salva come",
        TextKey::Seasons => "Stagioni",
        TextKey::SeeAll => "Vedi tutto >>",
        TextKey::SelectAll => "Seleziona tutto",
        TextKey::SelectGroup => "(Seleziona un gruppo)",
        TextKey::SelectPlaylist => "(Seleziona una playlist)",
        TextKey::SelectSearchCollectionHelp => {
            "Seleziona la raccolta a cui aggiungere i risultati della ricerca."
        }
        TextKey::SelectSearchPlaylistHelp => {
            "Seleziona la playlist a cui aggiungere i risultati della ricerca."
        }
        TextKey::Selected => "Selezionato",
        TextKey::SelectCollectionToAddThisMovie => {
            "Seleziona una raccolta a cui aggiungere questo film."
        }
        TextKey::SelectPlaylistToAddThisMovie => {
            "Seleziona una playlist a cui aggiungere questo film."
        }
        TextKey::Add => "Aggiungi",
        TextKey::AddLibrary => "Aggiungi alla libreria",
        TextKey::Address => "Indirizzo",
        TextKey::AddressRequired => "L'indirizzo è obbligatorio.",
        TextKey::ApiKey => "Chiave API",
        TextKey::ApiKeyRequired => "La chiave API è obbligatoria.",
        TextKey::AuthorizedRedirecting => "Autorizzato - reindirizzamento...",
        TextKey::CheckingEveryTwoSeconds => "Controllo ogni 2 secondi...",
        TextKey::ClickAuthorizePlex => "Clicca qui per autorizzarti su Plex.tv",
        TextKey::ClickPlayoutItemHelp => {
            "Clicca su un elemento del playout per ispezionarne il titolo, i tempi e il contesto di programmazione."
        }
        TextKey::ConfigureConnection => "Configura connessione",
        TextKey::Connection => "Connessione",
        TextKey::DeepScanCollections => "Scansione approfondita delle raccolte",
        TextKey::DeepScanLibrary => "Scansione approfondita della libreria",
        TextKey::DeepScan => "Scansione approfondita",
        TextKey::EditLibrary => "Modifica libreria",
        TextKey::EditPlaylist => "Modifica playlist",
        TextKey::EnterOnePathPerLine => "Inserisci un percorso per riga.",
        TextKey::Error => "Errore",
        TextKey::ExternalCollections => "Raccolte esterne",
        TextKey::Information => "Informazioni",
        TextKey::Kind => "Tipo",
        TextKey::Library => "Libreria",
        TextKey::LibraryKind => "Tipo di libreria",
        TextKey::LibraryPaths => "Percorsi della libreria",
        TextKey::LocalLibraries => "Librerie locali",
        TextKey::LocalPath => "Percorso locale",
        TextKey::MediaKind => "Tipo di media",
        TextKey::NoLibrariesConfigured => {
            "Nessuna libreria configurata. Aggiungi prima una fonte media."
        }
        TextKey::NoSchedulingContext => "Nessun contesto di programmazione",
        TextKey::NoPlayoutItemsInWindow => {
            "Nessun elemento del playout in questa finestra di 24 ore"
        }
        TextKey::NoPlayoutsFound => "Nessun playout trovato.",
        TextKey::NoPathReplacements => "Nessun sostituzione di percorso.",
        TextKey::Paths => "Percorsi",
        TextKey::PathReplacements => "Sostituzioni di percorsi",
        TextKey::PlexSignInHelp => "Accedi a Plex per aggiungere una fonte media.",
        TextKey::Remote => "Remoto",
        TextKey::RemotePath => "Percorso remoto",
        TextKey::Remove => "Rimuovi",
        TextKey::RemoveBlockFromTemplate => "Rimuovi blocco dal modello",
        TextKey::RemoveContent => "Rimuovi contenuto",
        TextKey::Retry => "Riprova",
        TextKey::Saved => "Salvato.",
        TextKey::Saving => "Salvataggio...",
        TextKey::QuickScan => "Scansione rapida",
        TextKey::ScanCollections => "Scansione delle raccolte",
        TextKey::ScanFailed => "Scansione fallita",
        TextKey::ScanLibrary => "Scansione libreria",
        TextKey::ScanQueued => "Scansione in coda",
        TextKey::Scanning => "Scansione in corso...",
        TextKey::Server => "Server",
        TextKey::ServerAddress => "Indirizzo server",
        TextKey::SignInToPlex => "Accedi a Plex",
        TextKey::SignOutOfPlex => "Esci da Plex",
        TextKey::Sync => "Sincronizza",
        TextKey::WaitingForPlexAuthorization => "In attesa dell'autorizzazione di Plex.",
        TextKey::AddChannel => "Aggiungi canale",
        TextKey::Any => "Qualsiasi",
        TextKey::BlockPlayoutTroubleshooting => "Risoluzione problemi del playout a blocchi",
        TextKey::BlockPlayoutHistory => "Cronologia del playout a blocchi",
        TextKey::Categories => "Categorie",
        TextKey::CategoriesHelp => "Elenco separato da virgola di categorie",
        TextKey::Channel => "Canale",
        TextKey::ChannelCouldNotBeSaved => {
            "Il canale non può essere salvato. Controlla i valori del modulo e riprova."
        }
        TextKey::ChannelDisabled => "Canale disabilitato",
        TextKey::ChannelHiddenFromEpg => "Canale nascosto dall'EPG",
        TextKey::ChannelLogo => "Logo del canale",
        TextKey::ChannelLogoPreview => "Anteprima logo del canale",
        TextKey::ChannelLogoUploadFailed => {
            "Caricamento logo del canale fallito. Controlla il file e riprova."
        }
        TextKey::Check => "Controlla",
        TextKey::Continuous => "Continuo",
        TextKey::DateAndTime => "Data e ora",
        TextKey::DefaultOption => "Predefinito",
        TextKey::DefaultSchedule => "Programma predefinito",
        TextKey::DeleteChannel => "Elimina canale",
        TextKey::DownloadMediaSample => "Scarica campione multimedia",
        TextKey::DownloadResults => "Risultati scaricamento",
        TextKey::DisableWatermarks => "Disabilita filigrane",
        TextKey::EditChannel => "Modifica canale",
        TextKey::EditChannelNumbers => "Modifica numeri canale",
        TextKey::EditNumbers => "Modifica numeri",
        TextKey::ExternalLogoUrl => "Logo esterno URL",
        TextKey::Fail => "Errore",
        TextKey::FallbackFiller => "Fallback riempitivo",
        TextKey::FfmpegProfile => "Profilo FFmpeg",
        TextKey::FirstPage => "Prima pagina",
        TextKey::FirstRunPlaybackOrder => "Ordine riproduzione prima esecuzione",
        TextKey::Forced => "Forzato",
        TextKey::GenerateSubtitles => "Genera sottotitoli",
        TextKey::Generated => "Generato",
        TextKey::Details => "Dettagli",
        TextKey::Full => "Completo",
        TextKey::Group => "Gruppo",
        TextKey::FfmpegRuntime => "Tempo di esecuzione FFmpeg",
        TextKey::FfmpegRuntimeDownloaded => "Tempo di esecuzione FFmpeg scaricato",
        TextKey::FfmpegRuntimeLatestRelease => "Ultima versione",
        TextKey::FfmpegRuntimeRelease => "Versione installata",
        TextKey::FfmpegRuntimeSource => "Sorgente",
        TextKey::FfmpegRuntimeSourceDownloaded => "Scaricato",
        TextKey::FfmpegRuntimeSourceManaged => "Gestito",
        TextKey::FfmpegRuntimeSourceOverride => "Sovrascrivi",
        TextKey::FfmpegRuntimeSourcePath => "PERCORSO",
        TextKey::FfmpegRuntimeSourceUnknown => "Sconosciuto",
        TextKey::FfmpegRuntimeUpdateAvailable => "Aggiornamento disponibile",
        TextKey::FfmpegRuntimeUpdateCheckFailed => "Controllo aggiornamenti fallito",
        TextKey::FfmpegRuntimeUpdateFailed => "Aggiornamento tempo di esecuzione FFmpeg fallito",
        TextKey::FfmpegRuntimeUpToDate => "Aggiornato",
        TextKey::FfmpegRuntimeUpdated => "Tempo di esecuzione FFmpeg aggiornato",
        TextKey::FfmpegRuntimeVersion => "Versione",
        TextKey::HealthNoPersistedRun => "Al momento non sono applicabili controlli di stato.",
        TextKey::HealthPersistedRunHelp => {
            "Risultati controllo stato parità corrente. Questa pagina si aggiorna dopo l'aggiornamento dello stato degli elementi tramite scansione dei media."
        }
        TextKey::HistoryDetails => "Dettagli cronologia",
        TextKey::HlsDirect => "HLS diretto",
        TextKey::HlsSegmenter => "HLS Segmenter",
        TextKey::Hours => "ore",
        TextKey::IdleBehavior => "Comportamento inattivo",
        TextKey::IdleBehaviorHelp => {
            "Controlla il comportamento del processo di transcoding dopo che tutti i client si sono disconnessi"
        }
        TextKey::Info => "Info",
        TextKey::Import => "Importa",
        TextKey::IsEnabled => "Abilitato",
        TextKey::Json => "JSON",
        TextKey::KeepRunning => "Continua a funzionare",
        TextKey::Key => "Chiave",
        TextKey::LastMidFillerHelp => "secondi dall'ultimo riempitivo mid-roll",
        TextKey::LastPage => "Ultima pagina",
        TextKey::Level => "Livello",
        TextKey::LoadingMediaInfo => "Caricamento informazioni media...",
        TextKey::Logs => "Log",
        TextKey::Logo => "Logo",
        TextKey::MediaItem => "Elemento multimedia",
        TextKey::Message => "Messaggio",
        TextKey::MissingMediaItemQueryParameter => "Parametro di query mediaItem mancante",
        TextKey::Mirror => "Specchio",
        TextKey::MirrorOptionsHelp => {
            "Disponibile solo quando la Sorgente del playout è impostata su Specchio."
        }
        TextKey::MirrorSourceChannel => "Canale sorgente specchio",
        TextKey::MusicVideoCreditsMode => "Modalità crediti video musicale",
        TextKey::MusicVideoCreditsTemplate => "Modello crediti video musicale",
        TextKey::NoMatchingRecordsFound => "Nessun record corrispondente trovato",
        TextKey::NoFfmpegProfiles => "Nessun profilo FFmpeg trovato. Creane uno prima.",
        TextKey::NoBlockPlayoutSelected => {
            "Seleziona un playout a blocchi per ispezionarne i blocchi."
        }
        TextKey::NoBlocks => "Nessun blocco trovato.",
        TextKey::NoHistory => "Nessun cronologia trovata.",
        TextKey::NoMessages => "Nessun messaggio.",
        TextKey::NvidiaCapabilitiesNotCollected => {
            "Le capacità NVIDIA non sono ancora state raccolte."
        }
        TextKey::Number => "Numero",
        TextKey::OnDemand => "Su richiesta",
        TextKey::MatchedPointsHelp => {
            "numero di punti mid-roll che hanno già corrisposto all'espressione"
        }
        TextKey::Pass => "Passa",
        TextKey::ParityDebt => "ParityDebt",
        TextKey::PlaybackTroubleshooting => "Risoluzione problemi riproduzione",
        TextKey::Play => "Riproduci",
        TextKey::PlayoutTimeline => "Timeline del playout",
        TextKey::PlayoutMode => "Modalità del playout",
        TextKey::PlayoutModeHelp => "Controlla la progressione del playout del canale",
        TextKey::PlayoutOffset => "Offset del playout",
        TextKey::PlayoutSource => "Sorgente del playout",
        TextKey::PlayoutSourceHelp => {
            "Controlla la sorgente del contenuto del canale. Non può essere modificato quando il canale ha già un playout."
        }
        TextKey::Preview => "Anteprima",
        TextKey::PreviewBlockPlayout => "Anteprima del playout a blocchi",
        TextKey::PreviewChannel => "Canale di anteprima",
        TextKey::ChannelPreview => "Anteprima del canale",
        TextKey::ReloadStream => "Ricarica flusso",
        TextKey::StopPreview => "Ferma anteprima",
        TextKey::OpenFullPlayer => "Apri lettore completo",
        TextKey::BackToChannels => "Torna ai canali",
        TextKey::NowPlaying => "In riproduzione",
        TextKey::UpNext => "Prossimo",
        TextKey::ProgrammeGuide => "Guida dei programmi",
        TextKey::EpgPreview => "Anteprima EPG",
        TextKey::EpgPreviewDescription => {
            "Naviga nella guida pubblicata con due ore di storia e 24 ore in anticipo."
        }
        TextKey::EpgZoom => "Zoom",
        TextKey::EpgNow => "Ora",
        TextKey::NoEpgChannels => "Non sono visibili canali abilitati nell'EPG.",
        TextKey::ScheduleGap => "Intervallo di programmazione",
        TextKey::Filler => "Riempitivo",
        TextKey::NoProgramming => "Non è programmata alcuna programmazione in questa finestra.",
        TextKey::ChannelDisabledPreview => "Questo canale è disabilitato.",
        TextKey::ChannelNotAiring => "Questo canale non sta attualmente trasmettendo.",
        TextKey::ChannelNoPlayout => "Questo canale non ha un playout.",
        TextKey::PreviewIdle => "L'anteprima inizia in mutato automaticamente.",
        TextKey::PreviewConnecting => "Connessione al flusso live in corso…",
        TextKey::PreviewLive => "La riproduzione live è in esecuzione.",
        TextKey::PreviewPaused => "Anteprima in pausa.",
        TextKey::PreviewRecovering => "Ripristino della riproduzione in corso…",
        TextKey::PreviewStopped => "Anteprima arrestata.",
        TextKey::PreviewUnsupported => "Questo browser non può riprodurre video HLS.",
        TextKey::PreviewAuthorizationFailed => "L'autorizzazione dell'anteprima è fallita.",
        TextKey::PreviewUnavailable => "Il flusso live non è disponibile.",
        TextKey::PreviewCapacityReached => "Tutte le slot di transcoding sono attualmente in uso.",
        TextKey::PreviewNetworkError => "L'anteprima ha perso la connessione di rete.",
        TextKey::PreviewMediaError => {
            "Il browser non è riuscito a decodificare l'output configurato di questo canale."
        }
        TextKey::PreviewDerivativeNote => {
            "L'anteprima di amministrazione utilizza una derivazione H.264/AAC compatibile con i browser; l'output del canale configurato non è modificato."
        }
        TextKey::JavascriptRequiredPlayback => {
            "È richiesto JavaScript per la riproduzione interattiva."
        }
        TextKey::ConfirmStopTranscodeSession => {
            "Arrestare la sessione di transcoding condivisa per questo canale?"
        }
        TextKey::PreviewPlaylistPlayout => "Anteprima del playout della playlist",
        TextKey::PreferredAudioTitle => "Titolo audio preferito",
        TextKey::PreferredSubtitleLanguage => "Lingua dei sottotitoli preferita",
        TextKey::QsvCapabilitiesNotCollected => "Le capacità QSV non sono ancora state raccolte.",
        TextKey::RemoveStoredLogo => "Rimuovi logo memorizzato",
        TextKey::SaveChannel => "Salva canale",
        TextKey::SaveChannels => "Salva canali",
        TextKey::Second => "secondo",
        TextKey::Seconds => "secondi",
        TextKey::SelectProfile => "- Seleziona profilo -",
        TextKey::SequentialSchedule => "Programma sequenziale",
        TextKey::SequentialScheduleValidation => "Validazione sequenziale della programmazione",
        TextKey::SeekSeconds => "Cerca secondi",
        TextKey::Schema => "Schema",
        TextKey::ShowDisabled => "Mostra disabilitata",
        TextKey::ShowInEpg => "Mostra in EPG",
        TextKey::SlugSeconds => "Slug secondi",
        TextKey::SlugSecondsHelp => {
            "Secondi di video nero e audio silenzioso da inserire tra ogni elemento del playout"
        }
        TextKey::Song => "Canzone",
        TextKey::SongVideoMode => "Modalità video canzone",
        TextKey::SortById => "Ordina per ID",
        TextKey::SortByLevel => "Ordina per livello",
        TextKey::SortByName => "Ordina per nome",
        TextKey::SortByTimestamp => "Ordina per timestamp",
        TextKey::StopOnDisconnect => "Ferma allo scollegamento",
        TextKey::StopScheduling => "Ferma la programmazione",
        TextKey::StopSchedulingBlockItems => "Ferma la programmazione degli elementi di blocco",
        TextKey::StopTranscodeSession => "Ferma sessione di transcoding",
        TextKey::StartFromBeginning => "Inizia dall'inizio",
        TextKey::StreamSelector => "Selettore flusso",
        TextKey::StreamSelectorMode => "Modalità selettore flusso",
        TextKey::StreamingMode => "Modalità streaming",
        TextKey::Styles => "Stili",
        TextKey::Subtitle => "Sottotitolo",
        TextKey::SubtitleMode => "Modalità sottotitolo",
        TextKey::SubtitleModeHelp => {
            "Mostra solo le tracce narrative forzate. Per mostrare sempre i sottotitoli nella lingua preferita, scegli Qualsiasi."
        }
        TextKey::Timestamp => "Marca temporale",
        TextKey::Timeline => "Linea temporale",
        TextKey::Target => "Destinazione",
        TextKey::Tags => "Tag",
        TextKey::Tools => "Strumenti",
        TextKey::TranscodeMode => "Modalità di transcoding",
        TextKey::TranscodeModeHelp => "Controlla quando il processo di transcoding è attivo",
        TextKey::TroubleshootPlayback => "Risoluzione problemi di riproduzione",
        TextKey::TroubleshootChannel => "Risoluzione problemi del canale",
        TextKey::TroubleshootingReports => "Report di risoluzione problemi",
        TextKey::MpegTsTroubleshooting => "MPEG-TS Risoluzione problemi",
        TextKey::InternalMpegTs => "Interno MPEG-TS",
        TextKey::PublicMpegTs => "Pubblico MPEG-TS",
        TextKey::RetainRawSample => "Conserva il campione grezzo",
        TextKey::TsdDuckVersion => "Versione TSDuck",
        TextKey::Report => "Rapporto",
        TextKey::RunId => "ID Esecuzione",
        TextKey::Status => "Stato",
        TextKey::Summary => "Riepilogo",
        TextKey::SampleBytes => "Byte del campione",
        TextKey::RawSampleArchived => "Campione grezzo archiviato",
        TextKey::ArchivePath => "Percorso di archiviazione",
        TextKey::SampleUrl => "Campione URL",
        TextKey::StabilityMeter => "Misuratore di stabilità",
        TextKey::Stable => "Stabile",
        TextKey::Packets => "pacchetti",
        TextKey::Services => "Servizi",
        TextKey::Pids => "PIDs",
        TextKey::ContinuityFindings => "Trovamenti di continuità",
        TextKey::PcrFindings => "Trovamenti PCR",
        TextKey::Bitrate => "Bitrate",
        TextKey::AnalysisJson => "Analisi JSON",
        TextKey::StandardOutput => "Output standard",
        TextKey::StandardError => "Errore standard",
        TextKey::NotAvailable => "Non disponibile",
        TextKey::NotApplicable => "Non applicabile",
        TextKey::Completed => "Completato",
        TextKey::Failed => "Fallito",
        TextKey::TroubleshootingPlaybackStartDebt => {
            "Anteprima di riproduzione di elementi multimediali e canali ora scrive report FFmpeg, log, media_info.json, troubleshooting_info.json, selezionatore di flusso YAML, file di capacità, esegue regole del selezionatore di flusso e reindirizza a /iptv/session/.troubleshooting/live.m3u8."
        }
        TextKey::UnableToLoadMediaInfo => "Impossibile caricare le informazioni multimediali",
        TextKey::UnableToLoadPlaybackTroubleshooting => {
            "Impossibile caricare il contesto di risoluzione problemi della riproduzione"
        }
        TextKey::UploadLogo => "Carica Logo",
        TextKey::UploadLogoHelp => {
            "Scegli un'immagine locale per sostituire il logo del canale memorizzato al salvataggio"
        }
        TextKey::Warning => "Avvertenza",
        TextKey::Watermark => "Filigrana",
        TextKey::WatermarkIds => "ID Filigrana",
        TextKey::Window => "Finestra",
        TextKey::WithProgress => "Con Progresso",
        TextKey::Writers => "Sceneggiatori",
        TextKey::Validate => "Validare",
        TextKey::Yaml => "YAML",
        TextKey::YamlFile => "File YAML",
        TextKey::Messages => "Messaggi",
        TextKey::CollectionName => "Nome della raccolta",
        TextKey::MediaItemType => "Tipo di Elemento Multimediale",
        TextKey::MediaItemTitle => "Titolo dell'Elemento Multimediale",
        TextKey::ActualSize => "Dimensione Reale",
        TextKey::AddWatermark => "Aggiungi Filigrana",
        TextKey::AspectRatioGuides => "Guide del rapporto di aspetto",
        TextKey::BottomLeft => "Basso Sinistra",
        TextKey::BottomMiddle => "Basso Centro",
        TextKey::BottomRight => "Basso Destra",
        TextKey::CopyWatermark => "Copia Filigrana",
        TextKey::CopyOf => "Copia di",
        TextKey::CurrentImage => "Immagine corrente",
        TextKey::Custom => "Personalizzato",
        TextKey::ChannelLogoPreviewUnavailable => {
            "Le anteprime del logo del canale richiedono un contesto del canale. Scegli un'immagine personalizzata per l'anteprima qui."
        }
        TextKey::DeleteWatermark => "Elimina filigrana",
        TextKey::DurationSeconds => "Durata (secondi)",
        TextKey::EditWatermark => "Modifica Filigrana",
        TextKey::EnableWatermarkToPreview => "Abilita la filigrana per visualizzarne la posizione.",
        TextKey::Frequency => "Frequenza",
        TextKey::GeneratingPreview => "Generazione anteprima...",
        TextKey::HorizontalMarginPercent => "Margine Orizzontale (%)",
        TextKey::GuideFourThree => "Quadro sicuro 4:3",
        TextKey::Identity => "Identità",
        TextKey::Image => "Immagine",
        TextKey::ImageSource => "Sorgente Immagine",
        TextKey::Intermittent => "Intermittente",
        TextKey::IntermittentTiming => "Temporizzazione intermittente",
        TextKey::LeftMiddle => "Centro Sinistra",
        TextKey::Location => "Posizione",
        TextKey::MiddleCenter => "Centro Centrale",
        TextKey::Mode => "Modalità",
        TextKey::Minutes => "minuti",
        TextKey::Moods => "Umore",
        TextKey::NewName => "Nuovo nome",
        TextKey::NativeAssetDimensions => "Asset nativo e dimensione resa",
        TextKey::NextWatermark => "Filigrana successiva",
        TextKey::Opacity => "Opacità",
        TextKey::OpacityExpression => "Espressione opacità",
        TextKey::OpacityOneToHundred => "Opacità (1-100)",
        TextKey::Permanent => "Permanente",
        TextKey::PerPass => "per passaggio",
        TextKey::PlaceWithinSourceContent => "Posizionamento all'interno del contenuto sorgente",
        TextKey::Placement => "Posizionamento",
        TextKey::PlacementPreview => "Anteprima posizionamento",
        TextKey::PreviewFailed => "Anteprima fallita",
        TextKey::PreviewReferenceFullHd => "Quadro di riferimento Full HD · 1920×1080",
        TextKey::PreviousWatermark => "Filigrana precedente",
        TextKey::RightMiddle => "Centro Destra",
        TextKey::SaveWatermark => "Salva Filigrana",
        TextKey::Scaled => "Ridimensionato",
        TextKey::SelectCustomImageToPreview => {
            "Seleziona o carica un'immagine personalizzata per l'anteprima."
        }
        TextKey::Size => "Dimensione",
        TextKey::TopLeft => "In alto a sinistra",
        TextKey::TopMiddle => "In alto al centro",
        TextKey::TopRight => "In alto a destra",
        TextKey::UploadFailed => "Caricamento fallito.",
        TextKey::UploadImage => "Carica immagine",
        TextKey::UnsavedChanges => "Modifiche non salvate",
        TextKey::UnsavedWatermarkChangesMessage => {
            "Scartare le modifiche alla filigrana e aprire la filigrana selezionata?"
        }
        TextKey::VerticalMarginPercent => "Margine verticale (%)",
        TextKey::WatermarkCopied => "Filigrana copiata.",
        TextKey::WatermarkNotFound => "Filigrana non trovata.",
        TextKey::WatermarkPlacementPreview => "anteprima posizionamento filigrana",
        TextKey::WatermarkPreview => "anteprima filigrana",
        TextKey::WatermarkNavigation => "Navigazione filigrana",
        TextKey::WidthPercent => "Larghezza (%)",
        TextKey::ZIndex => "Z-Index",
        TextKey::DiscardChanges => "Scartare modifiche",
        TextKey::XmltvBlockBehavior => "XMLTV Comportamento blocco",
        TextKey::XmltvTimeZone => "XMLTV Fuso orario",
        TextKey::Xmltv => "XMLTV",
        TextKey::Badge => "Etichetta",
        TextKey::Buttons => "Pulsanti",
        TextKey::CheckValue => "Controlla",
        TextKey::ChannelName => "Nome canale",
        TextKey::ChannelNumber => "Numero canale",
        TextKey::ComponentGallery => "ErsatzRS Galleria componenti",
        TextKey::DangerButton => "Pericolo",
        TextKey::DeleteDialog => "Dialogo eliminazione",
        TextKey::Decrement => "Decrementa",
        TextKey::Dialogs => "Dialoghi",
        TextKey::DisabledButton => "Disabilitato",
        TextKey::EnterName => "Inserisci nome",
        TextKey::ErrorOccurred => "Si è verificato un errore.",
        TextKey::ErrorToast => "Toast errore",
        TextKey::ExampleModal => "Modal di esempio",
        TextKey::ExampleModalBody => "Questo è il corpo del dialogo modale di esempio.",
        TextKey::FormInputs => "Input modulo",
        TextKey::Icons => "Icone",
        TextKey::IncrementBadge => "Incrementa badge",
        TextKey::InfoToast => "Toast info",
        TextKey::JustFyi => "Solo a titolo informativo.",
        TextKey::LinkButton => "Pulsante link",
        TextKey::ModalCancelled => "Annullato.",
        TextKey::ModalConfirmed => "Confermato!",
        TextKey::NumValue => "N.",
        TextKey::OpenModal => "Apri modulo",
        TextKey::OperationSucceeded => "Operazione riuscita.",
        TextKey::PagedTableDemo => "PagedTable (5 righe per pagina, 7 totali)",
        TextKey::PrimaryButton => "Primario",
        TextKey::SecondaryButton => "Secondario",
        TextKey::SelectValue => "Seleziona",
        TextKey::SomethingMightBeWrong => "Qualcosa potrebbe essere sbagliato.",
        TextKey::SuccessToast => "Toast successo",
        TextKey::TextButton => "Testo",
        TextKey::TextValue => "Testo",
        TextKey::Toasts => "Toast",
        TextKey::WarningToast => "Toast avviso",
        TextKey::YamlScript => "Script YAML",
        TextKey::Yes => "Sì",
    }
}

fn schedule_error_message(error: &str) -> String {
    match error.trim() {
        "Schedule name is required!" => "Il nome del programma è obbligatorio!".to_owned(),
        "New Schedule Name is required." => "Il nuovo nome del programma è obbligatorio.".to_owned(),
        "Schedule name must be 50 characters or less." => {
            "Il nome del programma deve contenere 50 caratteri o meno.".to_owned()
        }
        "Schedule name must be unique" => "Il nome del programma deve essere unico".to_owned(),
        "Schedule does not exist" => "Il programma non esiste".to_owned(),
        "[ProgramScheduleId] does not exist." => "[ProgramScheduleId] non esiste.".to_owned(),
        "A collection must use the same playback order in every schedule item." => {
            "Una raccolta deve utilizzare lo stesso ordine di riproduzione in ogni elemento del programma.".to_owned()
        }
        "Shuffle In Order cannot be used with Fill With Group Mode." => {
            "Shuffle In Order non può essere utilizzato con Fill With Group Mode.".to_owned()
        }
        "Fixed schedule items require a start time." => {
            "Gli elementi del programma fissi richiedono un orario di inizio.".to_owned()
        }
        "Multi Collection items can only use Shuffle or Shuffle In Order." => {
            "Gli elementi Multi Collection possono utilizzare solo Shuffle o Shuffle In Order.".to_owned()
        }
        "Playlist Item Size can only be used with Playlist schedule items." => {
            "La dimensione dell'elemento della playlist può essere utilizzata solo con gli elementi di programmazione della playlist.".to_owned()
        }
        "Multiple playout mode with Count requires a count expression." => {
            "La modalità di riproduzione multipla con Conteggio richiede un'espressione di conteggio.".to_owned()
        }
        "Duration playout mode requires a duration between 1 minute and 24 hours." => {
            "La modalità di riproduzione Durata richiede una durata compresa tra 1 minuto e 24 ore.".to_owned()
        }
        "Duration playout mode requires discard-to-fill attempts." => {
            "La modalità di riproduzione Durata richiede tentativi di eliminazione per riempimento.".to_owned()
        }
        "Tail Mode Filler requires a tail filler preset." => {
            "Il riempitore di coda richiede un preset di riempitore di coda.".to_owned()
        }
        "Tail filler can only be set when Tail Mode is Filler." => {
            "Il riempitore di coda può essere impostato solo quando la modalità di coda è Riempitore.".to_owned()
        }
        "Collection is required." => "È richiesta una Raccolta.".to_owned(),
        "Media item is required." => "È richiesto un elemento multimediale.".to_owned(),
        "Multi Collection is required." => "È richiesta una Multi-Raccolta.".to_owned(),
        "Smart Collection is required." => "È richiesta una Raccolta Intelligente.".to_owned(),
        "Playlist is required." => "È richiesta una Playlist.".to_owned(),
        "Rerun Collection is required." => "È richiesta una Raccolta di Ripetizione.".to_owned(),
        "Search Query is required." => "È richiesta una Query di ricerca.".to_owned(),
        "Only one pre-, mid-, or post-roll filler can pad to the nearest minute." => {
            "Solo un riempitore pre-, mid- o post-roll può essere utilizzato per arrotondare al minuto più vicino.".to_owned()
        }
        "Padding filler requires a fallback filler." => {
            "Il riempitore di riempimento richiede un riempitore di riserva.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn scheduling_content_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("Block group name \"") && trimmed.ends_with("\" is invalid") {
        return "Il nome del gruppo di blocchi deve essere di 50 caratteri o meno.".to_owned();
    }
    if trimmed.starts_with("Block name \"") && trimmed.ends_with("\" is invalid") {
        return "Il nome del blocco deve essere di 50 caratteri o meno.".to_owned();
    }
    if trimmed.starts_with("Template name \"") && trimmed.ends_with("\" is invalid") {
        return "Il nome del modello deve essere di 50 caratteri o meno.".to_owned();
    }
    if trimmed.starts_with("A template named \"")
        && trimmed.ends_with("\" already exists in that template group")
    {
        return "Un modello con quel nome esiste già in quel gruppo di modelli.".to_owned();
    }
    if trimmed.starts_with("Block from ") && trimmed.contains(" intersects block from ") {
        return "Il blocco si sovrappone a un altro blocco.".to_owned();
    }
    if trimmed.starts_with("TemplateGroup ") && trimmed.ends_with(" does not exist.") {
        return "Il gruppo di modelli non esiste.".to_owned();
    }
    if trimmed.starts_with("Template ") && trimmed.ends_with(" does not exist.") {
        return "Il modello non esiste.".to_owned();
    }
    if trimmed.starts_with("DecoGroup ") && trimmed.ends_with(" does not exist.") {
        return "Il gruppo deco non esiste.".to_owned();
    }
    if trimmed.starts_with("Deco ") && trimmed.ends_with(" does not exist.") {
        return "Deco non esiste.".to_owned();
    }
    if trimmed.starts_with("DecoTemplateGroup ") && trimmed.ends_with(" does not exist.") {
        return "Il gruppo di modelli deco non esiste.".to_owned();
    }
    if trimmed.starts_with("DecoTemplate ") && trimmed.ends_with(" does not exist.") {
        return "Il modello deco non esiste.".to_owned();
    }
    if trimmed.starts_with("Deco name \"") && trimmed.ends_with("\" is invalid") {
        return "Il nome del deco deve essere di 50 caratteri o meno.".to_owned();
    }
    if trimmed.starts_with("A deco named \"")
        && trimmed.ends_with("\" already exists in that deco group")
    {
        return "Un deco con quel nome esiste già in quel gruppo deco.".to_owned();
    }
    if trimmed.starts_with("Deco template name \"") && trimmed.ends_with("\" is invalid") {
        return "Il nome del modello deco deve essere di 50 caratteri o meno.".to_owned();
    }
    if trimmed.starts_with("A deco template named \"")
        && trimmed.ends_with("\" already exists in that deco template group")
    {
        return "Un modello deco con quel nome esiste già in quel gruppo di modelli deco."
            .to_owned();
    }
    if trimmed.starts_with("Watermark ") && trimmed.ends_with(" does not exist.") {
        return "Watermark non esiste.".to_owned();
    }
    if trimmed.starts_with("Graphics element ") && trimmed.ends_with(" does not exist.") {
        return "L'elemento grafico non esiste.".to_owned();
    }
    if trimmed.starts_with("Deco from ") && trimmed.contains(" intersects deco from ") {
        return "Il deco sovrappone un altro deco.".to_owned();
    }

    match trimmed {
        "Block does not exist." => "Il blocco non esiste.".to_owned(),
        "Block group name is required" => "È richiesto il nome del gruppo di blocchi".to_owned(),
        "Block group name must be unique" => {
            "Il nome del gruppo di blocchi deve essere unico".to_owned()
        }
        "Block name is required" => "È richiesto il nome del blocco".to_owned(),
        "A block named with that value already exists in that block group" => {
            "Un blocco denominato con quel valore esiste già in quel gruppo di blocchi".to_owned()
        }
        "Block duration must be between 5 minutes and 24 hours" => {
            "La durata del blocco deve essere compresa tra 5 minuti e 24 ore".to_owned()
        }
        "[Collection] is required for collection type 'Collection'" => {
            "[Collection] è richiesto per il tipo di raccolta 'Collection'".to_owned()
        }
        "[MediaItem] is required for collection type 'TelevisionShow'" => {
            "[MediaItem] è richiesto per il tipo di raccolta 'TelevisionShow'".to_owned()
        }
        "[MediaItem] is required for collection type 'TelevisionSeason'" => {
            "[MediaItem] è richiesto per il tipo di raccolta 'TelevisionSeason'".to_owned()
        }
        "[MultiCollection] is required for collection type 'MultiCollection'" => {
            "[MultiCollection] è richiesto per il tipo di raccolta 'MultiCollection'".to_owned()
        }
        "[SmartCollection] is required for collection type 'SmartCollection'" => {
            "[SmartCollection] è richiesto per il tipo di raccolta 'SmartCollection'".to_owned()
        }
        "[SearchQuery] is required for collection type 'SearchQuery'" => {
            "[SearchQuery] è richiesto per il tipo di collezione 'SearchQuery'".to_owned()
        }
        "[CollectionType] is invalid" => "[CollectionType] non è valido".to_owned(),
        "[BlockId] does not exist." => "[BlockId] non esiste.".to_owned(),
        "Template does not exist." => "Il modello non esiste.".to_owned(),
        "Template group name is required" => "È richiesto il nome del gruppo modello".to_owned(),
        "Template group name must be 50 characters or fewer" => {
            "Il nome del gruppo modello deve essere di 50 caratteri o meno".to_owned()
        }
        "Template group name must be unique" => {
            "Il nome del gruppo modello deve essere unico".to_owned()
        }
        "Template group does not exist." => "Il gruppo modello non esiste.".to_owned(),
        "[TemplateId] does not exist." => "[TemplateId] non esiste.".to_owned(),
        "Template name is required" => "È richiesto il nome del modello".to_owned(),
        "Template items cannot cross midnight." => {
            "Gli elementi del modello non possono attraversare la mezzanotte.".to_owned()
        }
        "Deco group name is required" => "È richiesto il nome del gruppo deco".to_owned(),
        "Deco group name must be 50 characters or fewer" => {
            "Il nome del gruppo deco deve essere di 50 caratteri o meno".to_owned()
        }
        "Deco group name must be unique" => "Il nome del gruppo deco deve essere unico".to_owned(),
        "Deco group does not exist." => "Il gruppo deco non esiste.".to_owned(),
        "Deco name is required" => "È richiesto il nome deco".to_owned(),
        "Deco does not exist" => "Deco non esiste.".to_owned(),
        "Default filler has unsupported collection type." => {
            "Il riempimento predefinito ha un tipo di collezione non supportato.".to_owned()
        }
        "Dead air fallback has unsupported collection type." => {
            "Il fallback per il silenzio ha un tipo di collezione non supportato.".to_owned()
        }
        "Break content has unsupported collection type." => {
            "Il contenuto di pausa ha un tipo di collezione non supportato.".to_owned()
        }
        "Default filler must have valid item" => {
            "Il riempimento predefinito deve avere un elemento valido".to_owned()
        }
        "Dead air fallback must have valid item" => {
            "Il fallback per il silenzio deve avere un elemento valido".to_owned()
        }
        "Break content collection must have valid item" => {
            "La collezione di contenuto di pausa deve avere un elemento valido".to_owned()
        }
        "Break content multi collection must have valid item" => {
            "La collezione multi-collezione di contenuto di pausa deve avere un elemento valido"
                .to_owned()
        }
        "Break content smart collection must have valid item" => {
            "La collezione intelligente di contenuto di pausa deve avere un elemento valido"
                .to_owned()
        }
        "Break content media item must have valid item" => {
            "L'elemento multimediale di contenuto di pausa deve avere un elemento valido".to_owned()
        }
        "Break content playlist must have valid item" => {
            "La playlist di contenuto di pausa deve avere un elemento valido".to_owned()
        }
        "Default filler item does not exist." => {
            "L'elemento di riempimento predefinito non esiste.".to_owned()
        }
        "Dead air fallback item does not exist." => {
            "L'elemento di fallback per il silenzio non esiste.".to_owned()
        }
        "Break content collection item does not exist." => {
            "L'elemento di collezione di contenuto di pausa non esiste.".to_owned()
        }
        "Break content multi collection item does not exist." => {
            "L'elemento di collezione multi-collezione di contenuto di pausa non esiste.".to_owned()
        }
        "Break content smart collection item does not exist." => {
            "L'elemento di collezione intelligente di contenuto di pausa non esiste.".to_owned()
        }
        "Break content media item item does not exist." => {
            "L'elemento multimediale di contenuto di pausa non esiste.".to_owned()
        }
        "Break content playlist item does not exist." => {
            "La playlist di contenuto di pausa non esiste.".to_owned()
        }
        "Deco may only contain one [Block Start] break content" => {
            "Deco può contenere solo un contenuto di pausa Block Start.".to_owned()
        }
        "Deco may only contain one [Between Block Items] break content" => {
            "Deco può contenere solo un contenuto di pausa Between Block Items.".to_owned()
        }
        "Deco may only contain one [At Chapter Markers] break content" => {
            "Deco può contenere solo un contenuto di pausa At Chapter Markers.".to_owned()
        }
        "Deco may only contain one [Block Finish] break content" => {
            "Deco può contenere solo un contenuto di pausa Block Finish.".to_owned()
        }
        "Deco template group name is required" => {
            "È richiesto il nome del gruppo modello deco".to_owned()
        }
        "Deco template group name must be 50 characters or fewer" => {
            "Il nome del gruppo modello Deco deve essere di 50 caratteri o meno".to_owned()
        }
        "Deco template group name must be unique" => {
            "Il nome del gruppo modello Deco deve essere unico".to_owned()
        }
        "Deco template group does not exist." => "Il gruppo di modelli deco non esiste.".to_owned(),
        "[DecoTemplateId] does not exist." => "Il modello deco non esiste.".to_owned(),
        "Deco template name is required" => "Il nome del modello Deco è obbligatorio".to_owned(),
        "[DecoId] does not exist." => "Deco non esiste.".to_owned(),
        "Deco template items cannot be empty." => {
            "Gli elementi del modello Deco non possono essere vuoti.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn watermark_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("Watermark ") && trimmed.ends_with(" does not exist.") {
        return "Filigrana non trovata.".to_owned();
    }

    match trimmed {
        "Name is required" => "Il nome è obbligatorio".to_owned(),
        "Name must be 50 characters or fewer" => {
            "Il nome deve essere di 50 caratteri o meno".to_owned()
        }
        "Watermark image is required" | "Watermark image is required!" => {
            "L'immagine filigrana è obbligatoria".to_owned()
        }
        "Width must be between 0 and 100%" => {
            "La larghezza deve essere compresa tra 0 e 100%".to_owned()
        }
        "Horizontal margin must be between 0 and 50%" => {
            "Il margine orizzontale deve essere compreso tra 0 e 50%".to_owned()
        }
        "Vertical margin must be between 0 and 50%" => {
            "Il margine verticale deve essere compreso tra 0 e 50%".to_owned()
        }
        "Duration must be positive and less than frequency window" => {
            "La durata deve essere positiva e inferiore alla finestra di frequenza".to_owned()
        }
        "Opacity must be between 1 and 100" => {
            "L'opacità deve essere compresa tra 1 e 100".to_owned()
        }
        "Watermark name must be unique" | "ChannelWatermark name must be unique" => {
            "Il nome filigrana deve essere unico".to_owned()
        }
        "Watermark not found" | "Source watermark not found" | "Watermark does not exist." => {
            "Filigrana non trovata.".to_owned()
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
        _ => return format!("Sconosciuto ({value})"),
    }
    .to_owned()
}

fn ffmpeg_profile_video_format_label(format: i32) -> String {
    match format {
        0 => "Nessuno",
        1 => "H264",
        2 => "H265 (HEVC)",
        3 => "MPEG-2",
        4 => "AV1",
        99 => text(TextKey::Copy),
        _ => return format!("Sconosciuto ({format})"),
    }
    .to_owned()
}

fn ffmpeg_profile_audio_format_label(format: i32) -> String {
    match format {
        0 => "Nessuno",
        1 => "AAC",
        2 => "AC3",
        3 => "AAC (LATM)",
        99 => text(TextKey::Copy),
        _ => return format!("Sconosciuto ({format})"),
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
            "XMLTV giorni per costruire ({current}) non può essere maggiore dei giorni per costruire Playout ({limit})"
        );
    }
    if let Some(name) = trimmed.strip_suffix(" path does not exist") {
        return format!("il percorso {name} non esiste");
    }
    if let Some(name) = trimmed
        .strip_prefix("Unable to verify ")
        .and_then(|value| value.strip_suffix(" version"))
    {
        return format!("Impossibile verificare la versione {name}");
    }
    if let Some(value) = trimmed.strip_prefix("Invalid option: ") {
        return format!("Opzione non valida: {value}");
    }
    if trimmed.starts_with("Resolution ") && trimmed.ends_with(" does not exist.") {
        return "La risoluzione non esiste.".to_owned();
    }

    match trimmed {
        "Windows startup registration is not supported on this platform" => {
            "La registrazione all'avvio di Windows non è supportata su questa piattaforma".to_owned()
        }
        "ErsatzRS.exe was not found next to ersatzrs-web.exe" => {
            "ErsatzRS.exe non è stato trovato accanto a ersatzrs-web.exe".to_owned()
        }
        "Unable to update Windows startup registration" => {
            "Impossibile aggiornare la registrazione all'avvio di Windows".to_owned()
        }
        "Tuner count must be greater than zero" => {
            "Il numero di sintonizzatori deve essere maggiore di zero".to_owned()
        }
        "Library refresh interval must be 0 (do not refresh) or greater than zero" => {
            "L'intervallo di aggiornamento della libreria deve essere 0 (non aggiornare) o maggiore di zero".to_owned()
        }
        "Library refresh interval must be less than 1,000,000. Use 0 to disable automatic refresh" => {
            "L'intervallo di aggiornamento della libreria deve essere inferiore a 1.000.000. Usa 0 per disabilitare l'aggiornamento automatico".to_owned()
        }
        "Playout days to build must be greater than zero" => {
            "I giorni per costruire Playout devono essere maggiori di zero".to_owned()
        }
        "Scripted schedule timeout seconds must be at least 10" => {
            "I secondi di timeout dell'orario programmato devono essere almeno 10".to_owned()
        }
        "XMLTV days to build must be greater than zero" => {
            "XMLTV giorni per costruire devono essere maggiori di zero".to_owned()
        }
        "Resolution width or height is invalid" => {
            "La larghezza o l'altezza della risoluzione non sono valide".to_owned()
        }
        "Resolution width and height must be unique" => {
            "La larghezza e l'altezza della risoluzione devono essere uniche".to_owned()
        }
        "HLS Segmenter idle timeout must be greater than or equal to 30" => {
            "HLS Timeout di inattività del segmentatore deve essere maggiore o uguale a 30".to_owned()
        }
        "Work-Ahead HLS Segmenter limit must be greater than or equal to 0" => {
            "HLS Limite del segmentatore Work-Ahead deve essere maggiore o uguale a 0".to_owned()
        }
        "Active HLS transcode limit must be greater than or equal to 1" => {
            "HLS Limite di transcoding attivo deve essere maggiore o uguale a 1".to_owned()
        }
        "HLS Segmenter initial segment count must be greater than or equal to 1" => {
            "HLS Numero iniziale di segmenti del segmentatore deve essere maggiore o uguale a 1".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn media_action_error_message(error: &str) -> String {
    match error.trim() {
        "Collection name is required" => "Il nome della raccolta è obbligatorio".to_owned(),
        "Collection name must be 50 characters or less" => {
            "Il nome della raccolta deve essere di 50 caratteri o meno".to_owned()
        }
        "Collection name must be unique" => "Il nome della raccolta deve essere unico".to_owned(),
        "Media item does not exist" => "L'elemento multimediale non esiste".to_owned(),
        "Collection does not exist" | "Collection does not exist." => {
            "La raccolta non esiste.".to_owned()
        }
        "Playlist does not exist" | "Playlist does not exist." => {
            "La playlist non esiste.".to_owned()
        }
        "Unable to locate media item" => {
            "Impossibile localizzare l'elemento multimediale".to_owned()
        }
        "Media action failed" => "Azione multimediale fallita".to_owned(),
        _ => error.to_owned(),
    }
}

fn filler_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("FillerPreset ") && trimmed.ends_with(" does not exist.") {
        return "Il preset filler non esiste.".to_owned();
    }

    match trimmed {
        "Filler preset does not exist" => "Il preset filler non esiste.".to_owned(),
        "Filler preset name must be unique" => {
            "Il nome del preset filler deve essere unico".to_owned()
        }
        "Name must not be empty" => "Il nome non può essere vuoto".to_owned(),
        "Name must be 50 characters or fewer" => {
            "Il nome deve essere di 50 caratteri o meno".to_owned()
        }
        "Filler kind must not be None" => "Il tipo di filler non può essere None".to_owned(),
        "Filler mode must not be None" => "La modalità filler non può essere None".to_owned(),
        "Count must be greater than or equal to 0" => {
            "Il conteggio deve essere maggiore o uguale a 0".to_owned()
        }
        "Duration must not be empty" => "La durata non può essere vuota".to_owned(),
        "Pad To Nearest Minute must not be empty" => {
            "\"Pad To Nearest Minute\" non può essere vuoto".to_owned()
        }
        "Use Chapters As Media Items must not be enabled for fallback filler" => {
            "\"Use Chapters As Media Items\" non può essere abilitato per il filler di riserva"
                .to_owned()
        }
        "Collection must not be empty" => "La raccolta non può essere vuota".to_owned(),
        "Multi Collection must not be empty" => "La raccolta multi non può essere vuota".to_owned(),
        "Smart Collection must not be empty" => {
            "La raccolta intelligente non può essere vuota".to_owned()
        }
        "Media item must not be empty" => "L'elemento multimediale non può essere vuoto".to_owned(),
        _ => error.to_owned(),
    }
}

fn media_browser_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed == "Pinned stream selection is invalid or no longer available." {
        return "La selezione dello stream bloccato non è valida o non è più disponibile."
            .to_owned();
    }
    if trimmed == "Movie was not found." {
        return "Il film non è stato trovato.".to_owned();
    }
    if trimmed.starts_with("Collection ") && trimmed.ends_with(" is already present.") {
        return "La raccolta è già presente.".to_owned();
    }
    if trimmed.starts_with("Collection ") && trimmed.ends_with(" does not exist.") {
        return "La raccolta non esiste.".to_owned();
    }
    if trimmed.starts_with("Smart collection ") && trimmed.ends_with(" is already present.") {
        return "La raccolta intelligente è già presente.".to_owned();
    }
    if trimmed.starts_with("Smart collection ") && trimmed.ends_with(" does not exist.") {
        return "La raccolta intelligente non esiste.".to_owned();
    }
    if trimmed.starts_with("MultiCollection ") && trimmed.ends_with(" does not exist.") {
        return "La raccolta multi non esiste.".to_owned();
    }
    if trimmed.starts_with("Rerun collection ") && trimmed.ends_with(" does not exist.") {
        return "La raccolta di ripetizione non esiste.".to_owned();
    }
    if trimmed.starts_with("Referenced item ") && trimmed.ends_with(" does not exist.") {
        return "L'elemento di riferimento non esiste.".to_owned();
    }
    if trimmed.starts_with("PlaylistGroup ") && trimmed.ends_with(" does not exist.") {
        return "Il gruppo della playlist non esiste.".to_owned();
    }
    if trimmed.starts_with("Playlist ") && trimmed.ends_with(" does not exist.") {
        return "La playlist non esiste.".to_owned();
    }
    if trimmed.starts_with("Playlist name \"") && trimmed.ends_with("\" is invalid") {
        return "Il nome della playlist non è valido.".to_owned();
    }
    if let Some(error) = trimmed.strip_prefix("Playlist preview failed: ") {
        return format!("Anteprima della playlist non riuscita: {error}");
    }
    if trimmed.starts_with("Media item ") && trimmed.ends_with(" does not exist.") {
        return "L'elemento multimediale non esiste.".to_owned();
    }

    match trimmed {
        "Collection name is required" => "Il nome della raccolta è obbligatorio".to_owned(),
        "Collection name must be 50 characters or less" => {
            "Il nome della raccolta deve essere di 50 caratteri o meno".to_owned()
        }
        "Collection name must be unique" => "Il nome della raccolta deve essere unico".to_owned(),
        "Collection does not exist." => "La raccolta non esiste.".to_owned(),
        "Smart collection name is required" => {
            "Il nome della raccolta intelligente è richiesto".to_owned()
        }
        "Smart collection query is required" => {
            "La query della raccolta intelligente è richiesta".to_owned()
        }
        "SmartCollection name must be unique" => {
            "Il nome della raccolta intelligente deve essere unico".to_owned()
        }
        "MultiCollection does not exist." => "La raccolta multi non esiste.".to_owned(),
        "Multi-collection name is required!" => {
            "Il nome della raccolta multi è richiesto.".to_owned()
        }
        "Multi-collection name cannot be longer than 50 characters." => {
            "Il nome della raccolta multipla non può superare i 50 caratteri.".to_owned()
        }
        "MultiCollection name must be unique" => {
            "Il nome della raccolta multi deve essere unico".to_owned()
        }
        "Each multi-collection item must reference exactly one collection." => {
            "Ogni elemento della raccolta multi deve fare riferimento a esattamente una raccolta."
                .to_owned()
        }
        "Rerun collection does not exist." => "La raccolta di ripetizione non esiste.".to_owned(),
        "[Name] is an empty string" | "[Name] must not be empty." => {
            "Il nome non può essere vuoto.".to_owned()
        }
        "[Name] must not be longer than 50" | "[Name] must not be longer than 50." => {
            "Il nome non può essere più lungo di 50 caratteri.".to_owned()
        }
        "Rerun collection name must be unique" => {
            "Il nome della raccolta di ripetizione deve essere unico".to_owned()
        }
        "Unsupported collection type." => "Tipo di raccolta non supportato.".to_owned(),
        "[ProgramScheduleId] does not exist." => "L'orario non esiste.".to_owned(),
        "[MediaItem] does not exist." => "L'elemento multimediale non esiste.".to_owned(),
        "[PlaylistGroupId] does not exist." => "Il gruppo della playlist non esiste.".to_owned(),
        "Cannot delete system playlist group" => {
            "Non è possibile eliminare il gruppo della playlist di sistema".to_owned()
        }
        "Cannot delete system (generated) playlist" => {
            "Non è possibile eliminare la playlist di sistema (generata)".to_owned()
        }
        "[PlaylistId] does not exist." => "La playlist non esiste.".to_owned(),
        "Cannot save system (generated) playlist" => {
            "Non è possibile salvare la playlist di sistema (generata)".to_owned()
        }
        "Playlist playout preview has no items" => {
            "L'anteprima di emissione della lista di riproduzione non contiene elementi".to_owned()
        }
        "Playlist name is required" => "Il nome della playlist è richiesto".to_owned(),
        "[Collection] is required for collection type 'Collection'" => {
            "È richiesta una Raccolta.".to_owned()
        }
        "[MultiCollection] is required for collection type 'MultiCollection'" => {
            "La raccolta multi è richiesta.".to_owned()
        }
        "[SmartCollection] is required for collection type 'SmartCollection'" => {
            "La raccolta intelligente è richiesta.".to_owned()
        }
        "[CollectionType] is invalid" => "Il tipo di raccolta non è valido.".to_owned(),
        _ => error.to_owned(),
    }
}

fn media_source_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if let Some(error) = trimmed.strip_prefix("Could not reach Jellyfin: ") {
        return format!("Impossibile raggiungere Jellyfin: {error}");
    }

    match trimmed {
        "Address is required." => "L'indirizzo è obbligatorio.".to_owned(),
        "API key is required." => "La chiave API è obbligatoria.".to_owned(),
        "'Address' must be a valid URL" => "'Address' deve essere un URL valido".to_owned(),
        _ => error.to_owned(),
    }
}

fn addon_operation_error_message(code: &str, detail: &str) -> String {
    let summary = match code {
        "missing-setting" => "Configura l’impostazione obbligatoria dell’add-on.",
        "missing-secret" => {
            "Configura il riferimento segreto obbligatorio nelle impostazioni dell’add-on."
        }
        "provider-unreachable" => "L’add-on non ha potuto raggiungere il provider.",
        "provider-rejected" => "Il provider ha rifiutato la richiesta dell’add-on.",
        "provider-rate-limited" => "Il provider ha limitato temporaneamente l’add-on.",
        "not-found" => "L’elemento richiesto dell’add-on non è stato trovato.",
        "unsupported-url" => "L’add-on non supporta questo URL.",
        _ => "L’add-on ha segnalato un errore.",
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
        return format!("L'add-on ha restituito dati di elenco non validi: {error}");
    }
    if normalized.starts_with("add-on media list with id ") && normalized.ends_with(" not found") {
        return "L'elenco add-on non esiste.".to_owned();
    }

    if let Some(folder) =
        normalized.strip_prefix("another list of this add-on already uses the folder ")
    {
        return format!(
            "Un'altra lista di questo componente aggiuntivo usa già la cartella \"{folder}\"."
        );
    }
    if let Some(folder) = normalized.strip_suffix(" is not usable as a folder name") {
        return format!("\"{folder}\" non è utilizzabile come nome di cartella.");
    }

    match normalized {
        "Invalid add-on list url" => "Elenco add-on URL non valido.".to_owned(),
        "add-on API client is not configured for this operation" => {
            "Il client add-on API non è configurato per questa operazione.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn upload_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if let Some(error) = trimmed.strip_prefix("Cannot create upload directory: ") {
        return format!("Impossibile creare la directory di caricamento: {error}");
    }
    if let Some(error) = trimmed.strip_prefix("Write failed: ") {
        return format!("Scrittura fallita: {error}");
    }
    if let Some(error) = trimmed.strip_prefix("Read field: ") {
        return format!("Impossibile leggere il campo: {error}");
    }

    match trimmed {
        "Upload exceeds maximum allowed size" => {
            "Il caricamento supera la dimensione massima consentita.".to_owned()
        }
        "No file field" => "Nessun campo file trovato.".to_owned(),
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
        return format!("Esiste già un profilo FFmpeg denominato \"{name}\".");
    }

    match trimmed {
        "Profile name is required." => "Il nome del profilo è richiesto.".to_owned(),
        "Profile name must be 50 characters or fewer." => {
            "Il nome del profilo deve essere di 50 caratteri o meno.".to_owned()
        }
        "Thread count must be 0 or greater." => "Il conteggio dei thread deve essere 0 o maggiore.".to_owned(),
        "QSV supports formats (h264, hevc, mpeg2video)" => {
            "QSV supporta i formati (h264, hevc, mpeg2video).".to_owned()
        }
        "NVENC supports formats (h264, hevc, av1)" => {
            "NVENC supporta i formati (h264, hevc, av1).".to_owned()
        }
        "VAAPI supports formats (h264, hevc, av1, mpeg2video)" => {
            "VAAPI supporta i formati (h264, hevc, av1, mpeg2video).".to_owned()
        }
        "VideoToolbox supports formats (h264, hevc)" => {
            "VideoToolbox supporta i formati (h264, hevc).".to_owned()
        }
        "Amf supports formats (h264, hevc)" => "Amf supporta i formati (h264, hevc).".to_owned(),
        "V4L2 M2M supports formats (h264, hevc)" => {
            "V4L2 M2M supporta i formati (h264, hevc).".to_owned()
        }
        "Rkmpp supports formats (h264, hevc)" => "Rkmpp supporta i formati (h264, hevc).".to_owned(),
        "HDR10 output requires hevc or av1; h264 and mpeg2video cannot carry HDR10 static metadata" => {
            "HDR10 output richiede hevc o av1; h264 e mpeg2video non possono trasportare HDR10 static metadata.".to_owned()
        }
        "HDR10 output requires 10-bit; 8-bit cannot represent a PQ signal without banding" => {
            "HDR10 output richiede 10-bit; 8-bit non può rappresentare un segnale PQ senza banding.".to_owned()
        }
        "Mastering display must look like G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1), with chromaticities up to 50000 and max luminance above min" => {
            "Mastering display deve assomigliare a G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1), con cromaticità fino a 50000 e luminosità massima superiore al minimo.".to_owned()
        }
        "MaxCLL and MaxFALL must both be set (or both left empty), at most 65535, with MaxFALL no greater than MaxCLL" => {
            "MaxCLL e MaxFALL devono essere entrambi impostati (o entrambi lasciati vuoti), al massimo 65535, con MaxFALL non superiore a MaxCLL.".to_owned()
        }
        "Mpeg2Video does not support 10-bit content" => {
            "Mpeg2Video non supporta contenuti 10-bit.".to_owned()
        }
        "VideoProfile must be high10 with 10-bit h264" => {
            "Video Profile deve essere high10 con h264 10-bit.".to_owned()
        }
        "VideoProfile must be high444p with NVIDIA 10-bit h264" => {
            "Video Profile deve essere high444p con NVIDIA 10-bit h264.".to_owned()
        }
        "VideoProfile cannot be high10 with 8-bit h264" => {
            "Video Profile non può essere high10 con h264 8-bit.".to_owned()
        }
        "Source profile not found." => "Profilo sorgente non trovato.".to_owned(),
        "No FFmpeg resolutions are available; database bootstrap is incomplete" => {
            "Non sono disponibili risoluzioni FFmpeg; il bootstrap del database è incompleto".to_owned()
        }
        _ => error.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_matches_italian_locale_contract() {
        assert_eq!(SORT_ORDER, 90);
        assert_eq!(CODE, "it-it");
        assert_eq!(LABEL, "Italiano (Italia)");
        assert_eq!(ALIASES, &["it", "it-it", "it_it"]);
    }

    #[test]
    fn representative_admin_text_is_italian() {
        assert_eq!(text(TextKey::Save), "Salva");
        assert_eq!(text(TextKey::Settings), "Impostazioni");
        assert_eq!(text(TextKey::Movies), "Film");
        assert_eq!(text(TextKey::Addons), "Componenti aggiuntivi");
    }

    #[test]
    fn glossary_terms_are_used_consistently() {
        // Playout keeps the product noun so it cannot collide with Playback,
        // which is the term that owns "riproduzione".
        assert_eq!(text(TextKey::Playout), "Playout");
        assert_eq!(text(TextKey::BlockPlayout), "Playout a blocchi");
        assert_eq!(text(TextKey::Playback), "Riproduzione");
        assert_eq!(text(TextKey::PlaybackOrder), "Ordine di riproduzione");
        assert_eq!(
            text(TextKey::RerunPlaybackOrder),
            "Ordine di riproduzione delle ripetizioni"
        );
        assert_eq!(text(TextKey::Collection), "Raccolta");
        assert_eq!(text(TextKey::Collections), "Raccolte");
        assert_eq!(text(TextKey::MultiCollection), "Raccolta multipla");
        assert_eq!(text(TextKey::Watermark), "Filigrana");
        assert_eq!(text(TextKey::Watermarks), "Filigrane");
        assert_eq!(text(TextKey::Playlist), "Playlist");
        assert_eq!(text(TextKey::PlaylistItem), "Elemento della playlist");
    }

    #[test]
    fn localized_helpers_use_italian_wording() {
        assert_eq!(STRINGS.channel_language_label("eng"), "Inglese");
        assert_eq!(STRINGS.channel_streaming_mode_label(2), "HLS diretto");
        assert_eq!(
            STRINGS.episode_display_subtitle(Some("Serie"), Some(2), Some(5)),
            Some("Serie - S02E05".to_owned())
        );
    }

    #[test]
    fn backend_error_guards_match_the_english_inputs() {
        // These branches key off the untranslated message the backend emits;
        // translating the needle silently disables them.
        assert_eq!(
            scheduling_content_error_message("Block from 1 intersects block from 2"),
            "Il blocco si sovrappone a un altro blocco."
        );
        assert_eq!(
            scheduling_content_error_message("Deco from 1 intersects deco from 2"),
            "Il deco sovrappone un altro deco."
        );
        assert_eq!(
            scheduling_content_error_message("Template name \"x\" is invalid"),
            "Il nome del modello deve essere di 50 caratteri o meno."
        );
        assert_eq!(
            media_browser_error_message(
                "Pinned stream selection is invalid or no longer available."
            ),
            "La selezione dello stream bloccato non è valida o non è più disponibile."
        );
        assert_eq!(
            media_browser_error_message("Movie was not found."),
            "Il film non è stato trovato."
        );
        assert_eq!(
            ffmpeg_profile_error_message("An FFmpeg profile named \"HD\" already exists."),
            "Esiste già un profilo FFmpeg denominato \"HD\"."
        );
    }

    #[test]
    fn dynamic_helpers_and_unknown_errors_are_stable() {
        assert_eq!(STRINGS.health_summary(1, 2), "1 errore, 2 avvisi");
        assert_eq!(STRINGS.health_summary(2, 1), "2 errori, 1 avviso");
        assert_eq!(
            schedule_error_message("custom database failure"),
            "custom database failure"
        );
        assert_eq!(
            settings_error_message("custom database failure"),
            "custom database failure"
        );
        assert_eq!(STRINGS.channel_streaming_mode_label(99), "Sconosciuto (99)");
        assert_eq!(STRINGS.channel_language_label("tlh"), "tlh");
    }
}
