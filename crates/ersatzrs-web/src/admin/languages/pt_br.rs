use crate::admin::i18n::{LanguageStrings, TextKey};

pub const SORT_ORDER: u16 = 30;
pub const CODE: &str = "pt-br";
pub const LABEL: &str = "Português (Brasil)";
pub const ALIASES: &[&str] = &["pt-br", "pt_br"];

pub struct PortugueseBrazilStrings;

pub static STRINGS: PortugueseBrazilStrings = PortugueseBrazilStrings;

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

impl LanguageStrings for PortugueseBrazilStrings {
    fn text(&self, key: TextKey) -> &'static str {
        text(key)
    }

    fn selected_count(&self, count: usize) -> String {
        format!("{count} selecionado(s)")
    }

    fn item_count(&self, count: u64) -> String {
        format!("{count} item(ns)")
    }

    fn range_of(&self, start: u32, end: u32, total: u64) -> String {
        format!("{start}-{end} de {total}")
    }

    fn match_status(&self, matched: u64, total: u64) -> String {
        format!("{matched} de {total}")
    }

    fn rows_per_page(&self, page_size: u32) -> String {
        format!("Linhas por página: {page_size}")
    }

    fn page_of(&self, page: u32, total: u32) -> String {
        format!("Página {page} de {total}")
    }

    fn save_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Falha ao salvar: {error}")
    }

    fn delete_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Falha ao excluir: {error}")
    }

    fn empty_trash_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Falha ao esvaziar o lixo: {error}")
    }

    fn search_count(&self, total: u32, label: &'static str) -> String {
        format!("{total} {label}")
    }

    fn selected_items_count(&self, count: usize) -> String {
        if count == 1 {
            "1 item selecionado".to_owned()
        } else {
            format!("{count} itens selecionados")
        }
    }

    fn delete_selected_count(&self, count: usize) -> String {
        format!("Excluir selecionados ({count})")
    }

    fn deleted_items_from_database(&self, count: u64) -> String {
        if count == 1 {
            "1 item excluído do banco de dados.".to_owned()
        } else {
            format!("{count} itens excluídos do banco de dados.")
        }
    }

    fn season_display_name(&self, number: i64) -> String {
        if number == 0 {
            "Especiais".to_owned()
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
        format!("Item de mídia #{id}")
    }

    fn schedule_item_fallback_label(&self, id: i64) -> String {
        format!("Item de programação #{id}")
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
            1 => "MPEG-TS (legado)".to_owned(),
            2 => "HLS direto".to_owned(),
            4 => "Segmentador HLS".to_owned(),
            5 => "MPEG-TS".to_owned(),
            _ => format!("Desconhecido ({mode})"),
        }
    }

    fn channel_language_label(&self, code: &str) -> String {
        match code.trim().to_ascii_lowercase().as_str() {
            "" => String::new(),
            "eng" | "en" => "Inglês".to_owned(),
            "nld" | "dut" | "nl" => "Neerlandês".to_owned(),
            "deu" | "ger" | "de" => "Alemão".to_owned(),
            "fra" | "fre" | "fr" => "Francês".to_owned(),
            "spa" | "es" => "Espanhol".to_owned(),
            "ita" | "it" => "Italiano".to_owned(),
            "jpn" | "ja" => "Japonês".to_owned(),
            "kor" | "ko" => "Coreano".to_owned(),
            "zho" | "chi" | "zh" => "Chinês".to_owned(),
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
            _ => "Desconhecido",
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
            format!("{label} (não disponível neste sistema)")
        }
    }

    fn ffmpeg_tonemap_algorithm_label(&self, algorithm: i32) -> String {
        match algorithm {
            0 => "Linear",
            1 => "Corte",
            2 => "Gama",
            3 => "Reinhard",
            4 => "Mobius",
            5 => "Hable",
            _ => return format!("Desconhecido ({algorithm})"),
        }
        .to_owned()
    }

    fn ffmpeg_output_dynamic_range_label(&self, range: i32) -> String {
        match range {
            0 => "SDR (BT.709)",
            1 => "HDR10 (BT.2020 PQ)",
            _ => return format!("Desconhecido ({range})"),
        }
        .to_owned()
    }

    fn ffmpeg_loudness_mode_label(&self, mode: i32) -> String {
        match mode {
            0 => text(TextKey::Off),
            1 => "loudnorm",
            _ => return format!("Desconhecido ({mode})"),
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
        format!("Lixo esvaziado: {count} itens excluídos.")
    }

    fn select_collection_to_add_selected_items(&self, count: usize) -> String {
        format!("Selecione a coleção para adicionar {count} itens selecionados")
    }

    fn select_playlist_to_add_selected_items(&self, count: usize) -> String {
        format!("Selecione a playlist para adicionar {count} itens selecionados")
    }

    fn added_items_to_collection(&self, count: usize) -> String {
        format!("{count} itens adicionados à coleção")
    }

    fn added_items_to_playlist(&self, count: usize) -> String {
        format!("{count} itens adicionados à playlist")
    }

    fn add_to_collection_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Falha ao adicionar à coleção: {error}")
    }

    fn add_to_playlist_failed(&self, error: &dyn std::fmt::Display) -> String {
        format!("Falha ao adicionar à playlist: {error}")
    }

    fn no_media_source_configured(&self, provider: &'static str) -> String {
        format!(
            "Nenhuma fonte {provider} configurada. Clique em Configurar conexão para adicionar uma."
        )
    }

    fn provider_connection_title(&self, provider: &'static str) -> String {
        format!("Conexão {provider}")
    }

    fn provider_libraries_title(&self, provider: &'static str) -> String {
        format!("Bibliotecas {provider}")
    }

    fn provider_path_replacements_title(&self, provider: &'static str) -> String {
        format!("Substituições de caminho {provider}")
    }

    fn provider_path_label(&self, provider: &'static str) -> String {
        format!("Caminho {provider}")
    }

    fn health_summary(&self, errors: u32, warnings: u32) -> String {
        format!("{errors} erro(s), {warnings} aviso(s)")
    }

    fn classic_playout_state_message(&self, playouts: &str) -> String {
        format!(
            "O estado da programação clássica requer atenção para: {playouts}. Verifique os horários dos programas e o estado de continuação salvo. Atualizar exige um limite de programa verificado e um ponto de controle completo; solicitações inseguras mantêm os programas publicados inalterados."
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
            "O estado da atualização após a varredura requer atenção: {unresolved_resolutions} resolução(ões) de impacto não concluída(s), {stale_resolutions} resolução(ões) de impacto paralisada(s), {failed_intents} solicitação(ões) de atualização da programação com falha e {stale_intents} solicitação(ões) de atualização da programação paralisada(s). Verifique os logs do scanner e do agendamento e tente novamente a varredura ou a atualização afetada."
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
            "{recent} canal(is) entregaram mídia HLS ao vivo mais lentamente que o tempo real nos últimos {minutes} minutos; {active} ainda não haviam se recuperado na última observação; {fallbacks} canal(is) estão usando uma alternativa verificada de codificação por software e/ou menor qualidade por até {fallback_hours} horas. Revise os logs de streaming e execute o teste de reprodução TSDuck sensível à taxa de entrega antes de alterar as configurações do provedor."
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
            "A cardinalidade das imagens requer atenção: {show_duplicate_rows} linha(s) duplicada(s) de imagem da série, {season_duplicate_rows} linha(s) duplicada(s) de imagem da temporada e {episode_duplicate_rows} linha(s) duplicada(s) de imagem do episódio; as contagens da projeção de televisão são {published_items} item(ns) de episódio publicado(s) de listas de complementos, {projection_rows} linha(s) de projeção, {distinct_episodes} episódio(s) projetado(s) distinto(s), {complete_hierarchy_rows} projeção(ões) de hierarquia completa(s) e {hierarchy_episodes} episódio(s) na hierarquia. Atualize ou publique novamente as listas de televisão de complementos afetadas."
        )
    }

    fn unable_to_load_playback_troubleshooting(&self, error: &dyn std::fmt::Display) -> String {
        format!(
            "Não foi possível carregar o contexto de solução de problemas de reprodução: {error}"
        )
    }

    fn playout_reset_prompt(&self, channel: &str, schedule: &str) -> String {
        format!(
            "Informe o horário em que a programação no canal {channel} com a grade {schedule} deve ser reconstruída todos os dias"
        )
    }
}

fn schedule_error_message(error: &str) -> String {
    match error.trim() {
        "Schedule name is required!" => "O nome da programação é obrigatório!".to_owned(),
        "New Schedule Name is required." => {
            "O nome da nova programação é obrigatório.".to_owned()
        }
        "Schedule name must be 50 characters or less." => {
            "O nome da programação deve ter no máximo 50 caracteres.".to_owned()
        }
        "Schedule name must be unique" => "O nome da programação deve ser único".to_owned(),
        "Schedule does not exist" => "A programação não existe".to_owned(),
        "[ProgramScheduleId] does not exist." => "[ProgramScheduleId] não existe.".to_owned(),
        "A collection must use the same playback order in every schedule item." => {
            "Uma coleção deve usar a mesma ordem de reprodução em todos os itens da programação.".to_owned()
        }
        "Shuffle In Order cannot be used with Fill With Group Mode." => {
            "Embaralhar em ordem não pode ser usado com o modo Preencher com grupo.".to_owned()
        }
        "Fixed schedule items require a start time." => {
            "Itens de programação fixos exigem um horário de início.".to_owned()
        }
        "Multi Collection items can only use Shuffle or Shuffle In Order." => {
            "Itens de multicoleção só podem usar Embaralhar ou Embaralhar em ordem.".to_owned()
        }
        "Playlist Item Size can only be used with Playlist schedule items." => {
            "Tamanho do item da playlist só pode ser usado com itens de programação do tipo playlist.".to_owned()
        }
        "Multiple playout mode with Count requires a count expression." => {
            "O modo de playout múltiplo com contagem exige uma expressão de contagem.".to_owned()
        }
        "Duration playout mode requires a duration between 1 minute and 24 hours." => {
            "O modo de playout por duração exige uma duração entre 1 minuto e 24 horas.".to_owned()
        }
        "Duration playout mode requires discard-to-fill attempts." => {
            "O modo de playout por duração exige tentativas de descarte para preencher.".to_owned()
        }
        "Tail Mode Filler requires a tail filler preset." => {
            "O modo de cauda Filler exige um preset de filler de cauda.".to_owned()
        }
        "Tail filler can only be set when Tail Mode is Filler." => {
            "O filler de cauda só pode ser definido quando o modo de cauda é Filler.".to_owned()
        }
        "Collection is required." => "A coleção é obrigatória.".to_owned(),
        "Media item is required." => "O item de mídia é obrigatório.".to_owned(),
        "Multi Collection is required." => "A multicoleção é obrigatória.".to_owned(),
        "Smart Collection is required." => "A coleção inteligente é obrigatória.".to_owned(),
        "Playlist is required." => "A playlist é obrigatória.".to_owned(),
        "Rerun Collection is required." => "A coleção de reprises é obrigatória.".to_owned(),
        "Search Query is required." => "A consulta de pesquisa é obrigatória.".to_owned(),
        "Only one pre-, mid-, or post-roll filler can pad to the nearest minute." => {
            "Apenas um filler pre-roll, mid-roll ou post-roll pode preencher até o minuto mais próximo.".to_owned()
        }
        "Padding filler requires a fallback filler." => {
            "O filler de preenchimento exige um filler de fallback.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn scheduling_content_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("Block group name \"") && trimmed.ends_with("\" is invalid") {
        return "O nome do grupo de blocos deve ter no máximo 50 caracteres.".to_owned();
    }
    if trimmed.starts_with("Block name \"") && trimmed.ends_with("\" is invalid") {
        return "O nome do bloco deve ter no máximo 50 caracteres.".to_owned();
    }
    if trimmed.starts_with("Template name \"") && trimmed.ends_with("\" is invalid") {
        return "O nome do modelo deve ter no máximo 50 caracteres.".to_owned();
    }
    if trimmed.starts_with("A template named \"")
        && trimmed.ends_with("\" already exists in that template group")
    {
        return "Já existe um modelo com esse nome nesse grupo de modelos.".to_owned();
    }
    if trimmed.starts_with("Block from ") && trimmed.contains(" intersects block from ") {
        return "O bloco se sobrepõe a outro bloco.".to_owned();
    }
    if trimmed.starts_with("TemplateGroup ") && trimmed.ends_with(" does not exist.") {
        return "O grupo de modelos não existe.".to_owned();
    }
    if trimmed.starts_with("Template ") && trimmed.ends_with(" does not exist.") {
        return "O modelo não existe.".to_owned();
    }
    if trimmed.starts_with("DecoGroup ") && trimmed.ends_with(" does not exist.") {
        return "O grupo de decorações não existe.".to_owned();
    }
    if trimmed.starts_with("Deco ") && trimmed.ends_with(" does not exist.") {
        return "A decoração não existe.".to_owned();
    }
    if trimmed.starts_with("DecoTemplateGroup ") && trimmed.ends_with(" does not exist.") {
        return "O grupo de modelos de decoração não existe.".to_owned();
    }
    if trimmed.starts_with("DecoTemplate ") && trimmed.ends_with(" does not exist.") {
        return "O modelo de decoração não existe.".to_owned();
    }
    if trimmed.starts_with("Deco name \"") && trimmed.ends_with("\" is invalid") {
        return "O nome da decoração deve ter no máximo 50 caracteres.".to_owned();
    }
    if trimmed.starts_with("A deco named \"")
        && trimmed.ends_with("\" already exists in that deco group")
    {
        return "Já existe uma decoração com esse nome nesse grupo de decorações.".to_owned();
    }
    if trimmed.starts_with("Deco template name \"") && trimmed.ends_with("\" is invalid") {
        return "O nome do modelo de decoração deve ter no máximo 50 caracteres.".to_owned();
    }
    if trimmed.starts_with("A deco template named \"")
        && trimmed.ends_with("\" already exists in that deco template group")
    {
        return "Já existe um modelo de decoração com esse nome nesse grupo de modelos de decoração."
            .to_owned();
    }
    if trimmed.starts_with("Watermark ") && trimmed.ends_with(" does not exist.") {
        return "A marca d'água não existe.".to_owned();
    }
    if trimmed.starts_with("Graphics element ") && trimmed.ends_with(" does not exist.") {
        return "O elemento gráfico não existe.".to_owned();
    }
    if trimmed.starts_with("Deco from ") && trimmed.contains(" intersects deco from ") {
        return "A decoração se sobrepõe a outra decoração.".to_owned();
    }

    match trimmed {
        "Block does not exist." => "O bloco não existe.".to_owned(),
        "Block group name is required" => "O nome do grupo de blocos é obrigatório".to_owned(),
        "Block group name must be unique" => "O nome do grupo de blocos deve ser único".to_owned(),
        "Block name is required" => "O nome do bloco é obrigatório".to_owned(),
        "A block named with that value already exists in that block group" => {
            "Já existe um bloco com esse nome nesse grupo de blocos.".to_owned()
        }
        "Block duration must be between 5 minutes and 24 hours" => {
            "A duração do bloco deve ser entre 5 minutos e 24 horas.".to_owned()
        }
        "[Collection] is required for collection type 'Collection'" => {
            "A coleção é obrigatória para o tipo de coleção Coleção.".to_owned()
        }
        "[MediaItem] is required for collection type 'TelevisionShow'" => {
            "O item de mídia é obrigatório para o tipo de coleção Série de TV.".to_owned()
        }
        "[MediaItem] is required for collection type 'TelevisionSeason'" => {
            "O item de mídia é obrigatório para o tipo de coleção Temporada de TV.".to_owned()
        }
        "[MultiCollection] is required for collection type 'MultiCollection'" => {
            "A multicoleção é obrigatória para o tipo de coleção Multicoleção.".to_owned()
        }
        "[SmartCollection] is required for collection type 'SmartCollection'" => {
            "A coleção inteligente é obrigatória para o tipo de coleção Coleção inteligente."
                .to_owned()
        }
        "[SearchQuery] is required for collection type 'SearchQuery'" => {
            "A consulta de pesquisa é obrigatória para o tipo de coleção Consulta de pesquisa."
                .to_owned()
        }
        "[CollectionType] is invalid" => "O tipo de coleção é inválido.".to_owned(),
        "[BlockId] does not exist." => "O bloco não existe.".to_owned(),
        "Template does not exist." => "O modelo não existe.".to_owned(),
        "Template group name is required" => "O nome do grupo de modelos é obrigatório".to_owned(),
        "Template group name must be 50 characters or fewer" => {
            "O nome do grupo de modelos deve ter no máximo 50 caracteres.".to_owned()
        }
        "Template group name must be unique" => {
            "O nome do grupo de modelos deve ser único".to_owned()
        }
        "Template group does not exist." => "O grupo de modelos não existe.".to_owned(),
        "[TemplateId] does not exist." => "O modelo não existe.".to_owned(),
        "Template name is required" => "O nome do modelo é obrigatório".to_owned(),
        "Template items cannot cross midnight." => {
            "Os itens do modelo não podem atravessar a meia-noite.".to_owned()
        }
        "Deco group name is required" => "O nome do grupo de decorações é obrigatório".to_owned(),
        "Deco group name must be 50 characters or fewer" => {
            "O nome do grupo de decorações deve ter no máximo 50 caracteres.".to_owned()
        }
        "Deco group name must be unique" => {
            "O nome do grupo de decorações deve ser único".to_owned()
        }
        "Deco group does not exist." => "O grupo de decorações não existe.".to_owned(),
        "Deco name is required" => "O nome da decoração é obrigatório".to_owned(),
        "Deco does not exist" => "A decoração não existe.".to_owned(),
        "Default filler has unsupported collection type." => {
            "O preenchimento padrão tem um tipo de coleção sem suporte.".to_owned()
        }
        "Dead air fallback has unsupported collection type." => {
            "O fallback de dead air tem um tipo de coleção sem suporte.".to_owned()
        }
        "Break content has unsupported collection type." => {
            "O conteúdo de intervalo tem um tipo de coleção sem suporte.".to_owned()
        }
        "Default filler must have valid item" => {
            "O preenchimento padrão deve ter um item válido.".to_owned()
        }
        "Dead air fallback must have valid item" => {
            "O fallback de dead air deve ter um item válido.".to_owned()
        }
        "Break content collection must have valid item" => {
            "A coleção do conteúdo de intervalo deve ter um item válido.".to_owned()
        }
        "Break content multi collection must have valid item" => {
            "A multi-coleção do conteúdo de intervalo deve ter um item válido.".to_owned()
        }
        "Break content smart collection must have valid item" => {
            "A coleção inteligente do conteúdo de intervalo deve ter um item válido.".to_owned()
        }
        "Break content media item must have valid item" => {
            "O item de mídia do conteúdo de intervalo deve ser válido.".to_owned()
        }
        "Break content playlist must have valid item" => {
            "A playlist do conteúdo de intervalo deve ter um item válido.".to_owned()
        }
        "Default filler item does not exist." => {
            "O item do preenchimento padrão não existe.".to_owned()
        }
        "Dead air fallback item does not exist." => {
            "O item do fallback de dead air não existe.".to_owned()
        }
        "Break content collection item does not exist." => {
            "O item da coleção do conteúdo de intervalo não existe.".to_owned()
        }
        "Break content multi collection item does not exist." => {
            "O item da multi-coleção do conteúdo de intervalo não existe.".to_owned()
        }
        "Break content smart collection item does not exist." => {
            "O item da coleção inteligente do conteúdo de intervalo não existe.".to_owned()
        }
        "Break content media item item does not exist." => {
            "O item de mídia do conteúdo de intervalo não existe.".to_owned()
        }
        "Break content playlist item does not exist." => {
            "A playlist do conteúdo de intervalo não existe.".to_owned()
        }
        "Deco may only contain one [Block Start] break content" => {
            "A decoração só pode conter um conteúdo de intervalo no início do bloco.".to_owned()
        }
        "Deco may only contain one [Between Block Items] break content" => {
            "A decoração só pode conter um conteúdo de intervalo entre itens do bloco.".to_owned()
        }
        "Deco may only contain one [At Chapter Markers] break content" => {
            "A decoração só pode conter um conteúdo de intervalo nos marcadores de capítulo."
                .to_owned()
        }
        "Deco may only contain one [Block Finish] break content" => {
            "A decoração só pode conter um conteúdo de intervalo no fim do bloco.".to_owned()
        }
        "Deco template group name is required" => {
            "O nome do grupo de modelos de decoração é obrigatório".to_owned()
        }
        "Deco template group name must be 50 characters or fewer" => {
            "O nome do grupo de modelos de decoração deve ter no máximo 50 caracteres.".to_owned()
        }
        "Deco template group name must be unique" => {
            "O nome do grupo de modelos de decoração deve ser único".to_owned()
        }
        "Deco template group does not exist." => {
            "O grupo de modelos de decoração não existe.".to_owned()
        }
        "[DecoTemplateId] does not exist." => "O modelo de decoração não existe.".to_owned(),
        "Deco template name is required" => {
            "O nome do modelo de decoração é obrigatório".to_owned()
        }
        "[DecoId] does not exist." => "A decoração não existe.".to_owned(),
        "Deco template items cannot be empty." => {
            "Os itens do modelo de decoração não podem ficar vazios.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn watermark_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("Watermark ") && trimmed.ends_with(" does not exist.") {
        return "Marca d'água não encontrada.".to_owned();
    }

    match trimmed {
        "Name is required" => "O nome é obrigatório".to_owned(),
        "Name must be 50 characters or fewer" => {
            "O nome deve ter 50 caracteres ou menos".to_owned()
        }
        "Watermark image is required" | "Watermark image is required!" => {
            "A imagem da marca d'água é obrigatória".to_owned()
        }
        "Width must be between 0 and 100%" => "A largura deve estar entre 0 e 100%".to_owned(),
        "Horizontal margin must be between 0 and 50%" => {
            "A margem horizontal deve estar entre 0 e 50%".to_owned()
        }
        "Vertical margin must be between 0 and 50%" => {
            "A margem vertical deve estar entre 0 e 50%".to_owned()
        }
        "Duration must be positive and less than frequency window" => {
            "A duração deve ser positiva e menor que a janela de frequência".to_owned()
        }
        "Opacity must be between 1 and 100" => "A opacidade deve estar entre 1 e 100".to_owned(),
        "Watermark name must be unique" | "ChannelWatermark name must be unique" => {
            "O nome da marca d'água deve ser único".to_owned()
        }
        "Watermark not found" | "Source watermark not found" | "Watermark does not exist." => {
            "Marca d'água não encontrada.".to_owned()
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
        _ => return format!("Desconhecido ({value})"),
    }
    .to_owned()
}

fn ffmpeg_profile_video_format_label(format: i32) -> String {
    match format {
        0 => "Nenhum",
        1 => "H264",
        2 => "H265 (HEVC)",
        3 => "MPEG-2",
        4 => "AV1",
        99 => text(TextKey::Copy),
        _ => return format!("Desconhecido ({format})"),
    }
    .to_owned()
}

fn ffmpeg_profile_audio_format_label(format: i32) -> String {
    match format {
        0 => "Nenhum",
        1 => "AAC",
        2 => "AC3",
        3 => "AAC (LATM)",
        99 => text(TextKey::Copy),
        _ => return format!("Desconhecido ({format})"),
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
            return format!("Já existe um perfil FFmpeg chamado \"{name}\".");
        }
    }

    match trimmed {
        "Profile name is required." => "O nome do perfil é obrigatório.".to_owned(),
        "Profile name must be 50 characters or fewer." => {
            "O nome do perfil deve ter 50 caracteres ou menos.".to_owned()
        }
        "Thread count must be 0 or greater." => {
            "A contagem de threads deve ser 0 ou maior.".to_owned()
        }
        "QSV supports formats (h264, hevc, mpeg2video)" => {
            "QSV oferece suporte aos formatos (h264, hevc, mpeg2video).".to_owned()
        }
        "NVENC supports formats (h264, hevc, av1)" => {
            "NVENC oferece suporte aos formatos (h264, hevc, av1).".to_owned()
        }
        "VAAPI supports formats (h264, hevc, av1, mpeg2video)" => {
            "VAAPI oferece suporte aos formatos (h264, hevc, av1, mpeg2video).".to_owned()
        }
        "VideoToolbox supports formats (h264, hevc)" => {
            "VideoToolbox oferece suporte aos formatos (h264, hevc).".to_owned()
        }
        "Amf supports formats (h264, hevc)" => {
            "Amf oferece suporte aos formatos (h264, hevc).".to_owned()
        }
        "V4L2 M2M supports formats (h264, hevc)" => {
            "V4L2 M2M oferece suporte aos formatos (h264, hevc).".to_owned()
        }
        "Rkmpp supports formats (h264, hevc)" => {
            "Rkmpp oferece suporte aos formatos (h264, hevc).".to_owned()
        }
        "HDR10 output requires hevc or av1; h264 and mpeg2video cannot carry HDR10 static metadata" => {
            "A saída HDR10 requer hevc ou av1; h264 e mpeg2video não podem carregar metadados estáticos HDR10.".to_owned()
        }
        "HDR10 output requires 10-bit; 8-bit cannot represent a PQ signal without banding" => {
            "A saída HDR10 requer 10 bits; 8 bits não representam um sinal PQ sem banding.".to_owned()
        }
        "Mastering display must look like G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1), with chromaticities up to 50000 and max luminance above min" => {
            "O monitor de masterização deve ter a forma G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1), com cromaticidades até 50000 e luminância máxima acima da mínima.".to_owned()
        }
        "MaxCLL and MaxFALL must both be set (or both left empty), at most 65535, with MaxFALL no greater than MaxCLL" => {
            "MaxCLL e MaxFALL devem estar ambos preenchidos (ou ambos vazios), no máximo 65535, com MaxFALL não maior que MaxCLL.".to_owned()
        }
        "Mpeg2Video does not support 10-bit content" => {
            "Mpeg2Video não oferece suporte a conteúdo de 10 bits.".to_owned()
        }
        "VideoProfile must be high10 with 10-bit h264" => {
            "O perfil de vídeo deve ser high10 com h264 de 10 bits.".to_owned()
        }
        "VideoProfile must be high444p with NVIDIA 10-bit h264" => {
            "O perfil de vídeo deve ser high444p com h264 de 10 bits da NVIDIA.".to_owned()
        }
        "VideoProfile cannot be high10 with 8-bit h264" => {
            "O perfil de vídeo não pode ser high10 com h264 de 8 bits.".to_owned()
        }
        "Source profile not found." => "Perfil de origem não encontrado.".to_owned(),
        "No FFmpeg resolutions are available; database bootstrap is incomplete" => {
            "Nenhuma resolução FFmpeg está disponível; o bootstrap do banco de dados está incompleto"
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
            "Dias de XMLTV para construir ({current}) não podem ser maiores que dias de playout para construir ({limit})"
        );
    }
    if let Some(name) = trimmed.strip_suffix(" path does not exist") {
        return format!("O caminho de {name} não existe");
    }
    if let Some(name) = trimmed
        .strip_prefix("Unable to verify ")
        .and_then(|value| value.strip_suffix(" version"))
    {
        return format!("Não foi possível verificar a versão de {name}");
    }
    if let Some(value) = trimmed.strip_prefix("Invalid option: ") {
        return format!("Opção inválida: {value}");
    }
    if trimmed.starts_with("Resolution ") && trimmed.ends_with(" does not exist.") {
        return "A resolução não existe.".to_owned();
    }

    match trimmed {
        "Windows startup registration is not supported on this platform" => {
            "O registro de inicialização com o Windows não é compatível com esta plataforma."
                .to_owned()
        }
        "ErsatzRS.exe was not found next to ersatzrs-web.exe" => {
            "ErsatzRS.exe não foi encontrado ao lado de ersatzrs-web.exe.".to_owned()
        }
        "Unable to update Windows startup registration" => {
            "Não foi possível atualizar o registro de inicialização com o Windows.".to_owned()
        }
        "Tuner count must be greater than zero" => "A contagem de sintonizadores deve ser maior que zero.".to_owned(),
        "Library refresh interval must be 0 (do not refresh) or greater than zero" => {
            "O intervalo de atualização da biblioteca deve ser 0 (não atualizar) ou maior que zero.".to_owned()
        }
        "Library refresh interval must be less than 1,000,000. Use 0 to disable automatic refresh" => {
            "O intervalo de atualização da biblioteca deve ser menor que 1.000.000. Use 0 para desativar a atualização automática.".to_owned()
        }
        "Playout days to build must be greater than zero" => "Dias de playout para construir devem ser maiores que zero.".to_owned(),
        "Scripted schedule timeout seconds must be at least 10" => {
            "O tempo limite do agendamento por script deve ser de pelo menos 10 segundos.".to_owned()
        }
        "XMLTV days to build must be greater than zero" => "Dias de XMLTV para construir devem ser maiores que zero.".to_owned(),
        "Resolution width or height is invalid" => "A largura ou altura da resolução é inválida.".to_owned(),
        "Resolution width and height must be unique" => "A largura e a altura da resolução devem ser únicas.".to_owned(),
        "HLS Segmenter idle timeout must be greater than or equal to 30" => {
            "O tempo limite ocioso do segmentador HLS deve ser maior ou igual a 30.".to_owned()
        }
        "Work-Ahead HLS Segmenter limit must be greater than or equal to 0" => {
            "O limite work-ahead do segmentador HLS deve ser maior ou igual a 0.".to_owned()
        }
        "Active HLS transcode limit must be greater than or equal to 1" => {
            "O limite de transcodificações HLS ativas deve ser maior ou igual a 1.".to_owned()
        }
        "HLS Segmenter initial segment count must be greater than or equal to 1" => {
            "A contagem inicial de segmentos do segmentador HLS deve ser maior ou igual a 1.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn media_action_error_message(error: &str) -> String {
    match error.trim() {
        "Collection name is required" => "O nome da coleção é obrigatório.".to_owned(),
        "Collection name must be 50 characters or less" => {
            "O nome da coleção deve ter 50 caracteres ou menos.".to_owned()
        }
        "Collection name must be unique" => "O nome da coleção deve ser único.".to_owned(),
        "Media item does not exist" => "O item de mídia não existe.".to_owned(),
        "Collection does not exist" | "Collection does not exist." => {
            "A coleção não existe.".to_owned()
        }
        "Playlist does not exist" | "Playlist does not exist." => {
            "A playlist não existe.".to_owned()
        }
        "Unable to locate media item" => "Não foi possível localizar o item de mídia.".to_owned(),
        "Media action failed" => "Falha na ação de mídia.".to_owned(),
        _ => error.to_owned(),
    }
}

fn filler_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.starts_with("FillerPreset ") && trimmed.ends_with(" does not exist.") {
        return "O preset de preenchimento não existe.".to_owned();
    }

    match trimmed {
        "Filler preset does not exist" => "O preset de preenchimento não existe.".to_owned(),
        "Filler preset name must be unique" => {
            "O nome do preset de preenchimento deve ser único.".to_owned()
        }
        "Name must not be empty" => "O nome não pode ficar vazio.".to_owned(),
        "Name must be 50 characters or fewer" => {
            "O nome deve ter 50 caracteres ou menos.".to_owned()
        }
        "Filler kind must not be None" => {
            "O tipo de preenchimento não pode ser vazio.".to_owned()
        }
        "Filler mode must not be None" => {
            "O modo de preenchimento não pode ser vazio.".to_owned()
        }
        "Count must be greater than or equal to 0" => {
            "A contagem deve ser maior ou igual a 0.".to_owned()
        }
        "Duration must not be empty" => "A duração não pode ficar vazia.".to_owned(),
        "Pad To Nearest Minute must not be empty" => {
            "Preencher até o minuto mais próximo não pode ficar vazio.".to_owned()
        }
        "Use Chapters As Media Items must not be enabled for fallback filler" => {
            "Usar capítulos como itens de mídia não pode estar ativado para preenchimento de fallback.".to_owned()
        }
        "Collection must not be empty" => "A coleção não pode ficar vazia.".to_owned(),
        "Multi Collection must not be empty" => {
            "A multi-coleção não pode ficar vazia.".to_owned()
        }
        "Smart Collection must not be empty" => {
            "A coleção inteligente não pode ficar vazia.".to_owned()
        }
        "Media item must not be empty" => "O item de mídia não pode ficar vazio.".to_owned(),
        _ => error.to_owned(),
    }
}

fn media_browser_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if trimmed.ends_with("Collection does not contain the selected item.") {
        return "A coleção não contém o item selecionado.".to_owned();
    }
    if trimmed.contains("collection with id ") && trimmed.ends_with(" not found") {
        return "A coleção não existe.".to_owned();
    }
    if trimmed == "Pinned stream selection is invalid or no longer available." {
        return "A seleção de fluxo fixada é inválida ou não está mais disponível.".to_owned();
    }
    if trimmed == "Movie was not found." {
        return "O filme não foi encontrado.".to_owned();
    }
    if trimmed.starts_with("Collection ") && trimmed.ends_with(" is already present.") {
        return "A coleção já está presente.".to_owned();
    }
    if trimmed.starts_with("Collection ") && trimmed.ends_with(" does not exist.") {
        return "A coleção não existe.".to_owned();
    }
    if trimmed.starts_with("Smart collection ") && trimmed.ends_with(" is already present.") {
        return "A coleção inteligente já está presente.".to_owned();
    }
    if trimmed.starts_with("Smart collection ") && trimmed.ends_with(" does not exist.") {
        return "A coleção inteligente não existe.".to_owned();
    }
    if trimmed.starts_with("MultiCollection ") && trimmed.ends_with(" does not exist.") {
        return "A multi-coleção não existe.".to_owned();
    }
    if trimmed.starts_with("Rerun collection ") && trimmed.ends_with(" does not exist.") {
        return "A coleção de reprises não existe.".to_owned();
    }
    if trimmed.starts_with("Referenced item ") && trimmed.ends_with(" does not exist.") {
        return "O item referenciado não existe.".to_owned();
    }
    if trimmed.starts_with("PlaylistGroup ") && trimmed.ends_with(" does not exist.") {
        return "O grupo de playlists não existe.".to_owned();
    }
    if trimmed.starts_with("Playlist ") && trimmed.ends_with(" does not exist.") {
        return "A playlist não existe.".to_owned();
    }
    if trimmed.starts_with("Playlist name \"") && trimmed.ends_with("\" is invalid") {
        return "O nome da playlist é inválido.".to_owned();
    }
    if let Some(error) = trimmed.strip_prefix("Playlist preview failed: ") {
        return format!("Falha na prévia da playlist: {error}");
    }
    if trimmed.starts_with("Media item ") && trimmed.ends_with(" does not exist.") {
        return "O item de mídia não existe.".to_owned();
    }

    match trimmed {
        "Collection name is required" => "O nome da coleção é obrigatório.".to_owned(),
        "Collection name must be 50 characters or less" => {
            "O nome da coleção deve ter 50 caracteres ou menos.".to_owned()
        }
        "Collection name must be unique" => "O nome da coleção deve ser único.".to_owned(),
        "Collection does not exist." => "A coleção não existe.".to_owned(),
        "Smart collection name is required" => {
            "O nome da coleção inteligente é obrigatório.".to_owned()
        }
        "Smart collection query is required" => {
            "A consulta da coleção inteligente é obrigatória.".to_owned()
        }
        "SmartCollection name must be unique" => {
            "O nome da coleção inteligente deve ser único.".to_owned()
        }
        "MultiCollection does not exist." => "A multi-coleção não existe.".to_owned(),
        "Multi-collection name is required!" => "O nome da multi-coleção é obrigatório.".to_owned(),
        "Multi-collection name cannot be longer than 50 characters." => {
            "O nome da multi-coleção deve ter 50 caracteres ou menos.".to_owned()
        }
        "MultiCollection name must be unique" => {
            "O nome da multi-coleção deve ser único.".to_owned()
        }
        "Each multi-collection item must reference exactly one collection." => {
            "Cada item da multi-coleção deve referenciar exatamente uma coleção.".to_owned()
        }
        "Rerun collection does not exist." => "A coleção de reprises não existe.".to_owned(),
        "[Name] is an empty string" | "[Name] must not be empty." => {
            "O nome não pode ficar vazio.".to_owned()
        }
        "[Name] must not be longer than 50" | "[Name] must not be longer than 50." => {
            "O nome deve ter 50 caracteres ou menos.".to_owned()
        }
        "Rerun collection name must be unique" => {
            "O nome da coleção de reprises deve ser único.".to_owned()
        }
        "Unsupported collection type." => "Tipo de coleção sem suporte.".to_owned(),
        "[ProgramScheduleId] does not exist." => "A programação não existe.".to_owned(),
        "[MediaItem] does not exist." => "O item de mídia não existe.".to_owned(),
        "[PlaylistGroupId] does not exist." => "O grupo de playlists não existe.".to_owned(),
        "Cannot delete system playlist group" => {
            "Não é possível excluir o grupo de playlists do sistema.".to_owned()
        }
        "Cannot delete system (generated) playlist" => {
            "Não é possível excluir a playlist do sistema.".to_owned()
        }
        "[PlaylistId] does not exist." => "A playlist não existe.".to_owned(),
        "Cannot save system (generated) playlist" => {
            "Não é possível salvar a playlist do sistema.".to_owned()
        }
        "Playlist playout preview has no items" => {
            "A prévia de playout da playlist não tem itens.".to_owned()
        }
        "Playlist name is required" => "O nome da playlist é obrigatório.".to_owned(),
        "[Collection] is required for collection type 'Collection'" => {
            "A coleção é obrigatória.".to_owned()
        }
        "[MultiCollection] is required for collection type 'MultiCollection'" => {
            "A multi-coleção é obrigatória.".to_owned()
        }
        "[SmartCollection] is required for collection type 'SmartCollection'" => {
            "A coleção inteligente é obrigatória.".to_owned()
        }
        "[CollectionType] is invalid" => "O tipo de coleção é inválido.".to_owned(),
        _ => error.to_owned(),
    }
}

fn media_source_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if let Some(error) = trimmed.strip_prefix("Could not reach Jellyfin: ") {
        return format!("Não foi possível acessar o Jellyfin: {error}");
    }

    match trimmed {
        "Address is required." => "O endereço é obrigatório.".to_owned(),
        "API key is required." => "A chave de API é obrigatória.".to_owned(),
        "'Address' must be a valid URL" => "O endereço deve ser uma URL válida.".to_owned(),
        _ => error.to_owned(),
    }
}

fn addon_operation_error_message(code: &str, detail: &str) -> String {
    let summary = match code {
        "missing-setting" => "Configure a opção obrigatória do add-on.",
        "missing-secret" => "Configure a referência secreta obrigatória nas opções do add-on.",
        "provider-unreachable" => "O add-on não conseguiu acessar o provedor.",
        "provider-rejected" => "O provedor rejeitou a solicitação do add-on.",
        "provider-rate-limited" => "O provedor limitou temporariamente o add-on.",
        "not-found" => "O item solicitado do add-on não foi encontrado.",
        "unsupported-url" => "O add-on não oferece suporte a esta URL.",
        _ => "O add-on relatou uma falha.",
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
        return format!("O add-on retornou dados de lista inválidos: {error}");
    }
    if normalized.starts_with("add-on media list with id ") && normalized.ends_with(" not found") {
        return "A lista do add-on não existe.".to_owned();
    }

    if let Some(folder) =
        normalized.strip_prefix("another list of this add-on already uses the folder ")
    {
        return format!("Outra lista deste complemento já usa a pasta \"{folder}\".");
    }
    if let Some(folder) = normalized.strip_suffix(" is not usable as a folder name") {
        return format!("\"{folder}\" não pode ser usada como nome de pasta.");
    }

    match normalized {
        "Invalid add-on list url" => "URL da lista do add-on inválida.".to_owned(),
        "add-on API client is not configured for this operation" => {
            "O cliente da API do add-on não está configurado para esta operação.".to_owned()
        }
        _ => error.to_owned(),
    }
}

fn upload_error_message(error: &str) -> String {
    let trimmed = error.trim();
    if let Some(error) = trimmed.strip_prefix("Cannot create upload directory: ") {
        return format!("Não foi possível criar o diretório de upload: {error}");
    }
    if let Some(error) = trimmed.strip_prefix("Write failed: ") {
        return format!("Falha ao gravar: {error}");
    }
    if let Some(error) = trimmed.strip_prefix("Read field: ") {
        return format!("Falha ao ler o campo: {error}");
    }

    match trimmed {
        "Upload exceeds maximum allowed size" => {
            "O upload excede o tamanho máximo permitido.".to_owned()
        }
        "No file field" => "Nenhum campo de arquivo encontrado.".to_owned(),
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
        TextKey::Center => "Centro",
        TextKey::FilePath => "Caminho do arquivo",
        TextKey::Format => "Formato",
        TextKey::GraphicsAuthoringInvalid => "Verifique tipo, nome, geometria, tempos e valores YAML.",
        TextKey::GraphicsAuthoringPreviewFailed => "A renderização nativa falhou ou excedeu o prazo. Verifique o renderizador e os arquivos.",
        TextKey::GraphicsBaseStyle => "Estilo base",
        TextKey::GraphicsDisappear => "Desaparecer",
        TextKey::GraphicsEndBehavior => "Comportamento final",
        TextKey::GraphicsEpgEntries => "Entradas do guia",
        TextKey::GraphicsExport => "Exportar YAML",
        TextKey::GraphicsFixedSize => "Tamanho fixo",
        TextKey::GraphicsFontDirectory => "Pasta de fontes",
        TextKey::GraphicsFontFamily => "Família da fonte",
        TextKey::GraphicsFontSize => "Tamanho da fonte (pixels)",
        TextKey::GraphicsGeometry => "Posição e camadas",
        TextKey::GraphicsHold => "Manter",
        TextKey::GraphicsHoldSeconds => "Espera (segundos)",
        TextKey::GraphicsLineHeight => "Altura da linha (pixels)",
        TextKey::GraphicsLoop => "Repetir",
        TextKey::GraphicsLuaCommand => "Código Lua ou arquivo de script",
        TextKey::GraphicsMissingFont => "O arquivo de fonte selecionado está ausente ou vazio.",
        TextKey::GraphicsMissingMedia => "O arquivo de imagem, vídeo ou legenda selecionado está ausente ou vazio.",
        TextKey::GraphicsMissingVariable => "Uma variável está ausente. Selecione canal/data ou forneça valores explícitos para a prévia.",
        TextKey::GraphicsPixelX => "X (pixels)",
        TextKey::GraphicsPixelY => "Y (pixels)",
        TextKey::GraphicsPreviewVariables => "Variáveis da prévia (objeto JSON)",
        TextKey::GraphicsRealPreview => "Prévia gráfica real",
        TextKey::GraphicsScaleWidth => "Largura de escala (%)",
        TextKey::GraphicsScriptArgs => "Argumentos do script (um por linha)",
        TextKey::GraphicsSourceContent => "Posicionar dentro do conteúdo original",
        TextKey::GraphicsStartSeconds => "Início (segundos)",
        TextKey::GraphicsTextAlign => "Alinhamento do texto",
        TextKey::GraphicsTextColor => "Cor do texto",
        TextKey::GraphicsTextFit => "Ajuste do texto",
        TextKey::GraphicsTextWidth => "Largura do texto (%)",
        TextKey::GraphicsWrap => "Quebra de linha",
        TextKey::GraphicsYaml => "YAML de gráficos",
        TextKey::Left => "Esquerda",
        TextKey::None => "Nenhum",
        TextKey::PixelFormat => "Formato de pixel",
        TextKey::ProgrammeSeekSeconds => "Posição desde o início do programa (segundos)",
        TextKey::Right => "Direita",
        TextKey::GraphicsDrawvgTypography => "Drawvg usa caracteres bitmap integrados. Não usa arquivos, famílias ou pastas de fontes. Largura, quebra de linha e escala usam essas métricas bitmap.",
        TextKey::GraphicsDrawtextTypography => "Drawtext usa a fonte real selecionada em tamanho fixo. Largura, quebra de linha e escala não limitam o texto; insira quebras explícitas. Confira títulos longos e não ASCII na prévia renderizada. O arquivo de fonte tem prioridade sobre a família. A busca por família pode substituir uma fonte do sistema; use um arquivo para obter a fonte exata. Sem seleção, a Roboto incluída é usada.",
        TextKey::GraphicsJpegXlHelp => "JPEG XL animado compacto é detectado automaticamente. Image repete a partir da fase do programa; Motion usa Início e o comportamento final. A decodificação nativa preserva a transparência e avança pelos quadros decodificados.",
        TextKey::GraphicsGeometryHelp => "Image, Text e Motion usam pixels exatos de saída quando X e Y estão definidos; eles substituem âncora, margens e preenchimento do conteúdo de origem. Escala e opacidade seguem as capacidades do renderizador nativo. A posição de Subtitle e Script vem de ASS ou Lua; os valores comuns são preservados, mas não aplicados.",
        TextKey::GraphicsLuaHelp => "Lua executa no motor gráfico nativo e retorna um script VGS. Não inicia comandos externos. Início e duração controlam a visibilidade. Formato e formato de pixel são preservados para compatibilidade YAML, mas não alteram a saída VGS.",
        TextKey::GraphicsRealPreviewHelp => "Renderize a definição não salva em um canvas transparente com o motor nativo. Canal e data escolhem o contexto real; a posição avança a partir desse instante. A prévia não salva nem altera a transmissão.",
        TextKey::GraphicsYamlHelp => "A importação valida e salva o tipo escolhido, substituindo esta definição na edição. A exportação inclui campos não salvos. Campos YAML desconhecidos são rejeitados.",

        TextKey::ExternalJsonPlayoutState => "Estado da programação External JSON",
        TextKey::ExternalJsonDirectHelp => "O arquivo de origem é lido diretamente. Para ativar atualizações controladas, escolha {refresh} ou {reset} e desative uma vez a preservação do programa. Isso pode interromper a reprodução.",
        TextKey::ExternalJsonAcceptedHelp => "{continue} apenas acrescenta programas. {refresh} e {reset} adotam programas futuros sem alterar horários externos. {reset} não pode reiniciar a rotação do autor externo.",
        TextKey::ExternalJsonSourceHelp => "Após ativar atualizações controladas, alterar este caminho mantém a programação aceita. Use {refresh} ou {reset} para adotar o novo arquivo.",
        TextKey::ExternalJsonInvalidHelp => "A programação aceita é inválida ou não é compatível. Verifique o arquivo de origem e substitua a programação com {refresh} ou {reset}, desativando a preservação do programa.",
        TextKey::ExternalJsonSourceChangedHelp => "O caminho de origem mudou. A programação aceita é mantida; use {refresh} ou {reset} para adotar o novo arquivo.",
        TextKey::ExternalJsonNextHelp => "Atualizações External JSON exigem reprodução nativa neste canal e em seus canais espelho. O Next não lê programação externa.",
        TextKey::ExternalJsonExpiredHelp => "A programação aceita não contém programas futuros. Verifique e amplie o arquivo de origem e depois atualize a programação.",
        TextKey::ExternalJsonBuildFailedHelp => "Não foi possível adotar a programação externa. Verifique o arquivo de origem, os limites dos programas e a ação escolhida. A última programação aceita continua em uso.",
        TextKey::ExternalJsonDirect => "Atualizações diretas do arquivo",
        TextKey::ExternalJsonControlled => "Atualizações controladas",
        TextKey::AcceptedRevision => "Revisão aceita",
        TextKey::PlayoutContinuityUnavailable => "O ponto de retomada do programa está ausente ou é incompatível. Use Reset para criar um estado compatível; Refresh preservará a programação existente até lá.",
        TextKey::BlockPlayoutState => "Estado da programação em blocos",
        TextKey::YamlPlayoutState => "Estado da programação YAML",
        TextKey::ScriptedPlayoutState => "Estado da programação por script",
        TextKey::ScriptedTeletextOverrideUnavailable => "Canal {channel}: a seleção ativa de teletexto não está disponível. A programação publicada é mantida.",
        TextKey::ScriptedTeletextGenerationUnavailable => "Canal {channel}: as páginas do perfil de teletexto ativo não estão prontas. A programação publicada é mantida.",
        TextKey::ScriptedTimelineRejected => "A linha do tempo foi rejeitada. Os programas publicados permanecem inalterados. Corrija o script e tente novamente.",
        TextKey::ItemDetailsNotRequested => "Detalhes não obtidos",
        TextKey::ItemDetailsFailed => "Falha ao obter detalhes",
        TextKey::ItemDetailsRetry => "Detalhes aguardando nova tentativa",
        TextKey::ItemProviderPlayback => "Reprodução do provedor",
        TextKey::ItemPosition => "Posição",
        TextKey::ItemPrevious => "Anterior",
        TextKey::ItemNext => "Próximo",
        TextKey::ItemBack => "Voltar aos itens",
        TextKey::ItemTools => "Ferramentas do item",
        TextKey::ItemEpisodePosition => "Posição do episódio",
        TextKey::ItemNumbered => "Temporada e episódio",
        TextKey::ItemUnnumberedHelp => "Este item não terá número de temporada ou episódio após salvar.",
        TextKey::ItemNumberingHelp => "Um item numerado exige ambos os valores. A temporada 0 é para especiais.",
        TextKey::ItemMetadata => "Resumo, gêneros e etiquetas",
        TextKey::ItemMetadataHelp => "Outros títulos, idiomas e créditos",
        TextKey::ItemSaveHelp => "Salve as alterações para aplicá-las automaticamente.",
        TextKey::ItemEditNumbering => "Editar numeração na lista",
        TextKey::ItemReplacementHelp => "Escolher ou remover uma substituição altera este rascunho. Use Salvar item para manter a alteração.",
        TextKey::ItemReplacementPending => "A alteração da substituição não foi salva.",
        TextKey::ItemSearchReplacement => "Buscar mídias locais",
        TextKey::ItemRemoveReplacement => "Remover substituição",
        TextKey::ItemInformation => "Informações e origem da mídia",
        TextKey::ItemInformationHelp => "Áudio, legendas, detalhes do fluxo e opções do provedor",
        TextKey::ItemSavedToolsHelp => "Estas ações usam o item salvo. Alterações não salvas não são incluídas.",
        TextKey::ItemViewMedia => "Ver detalhes da mídia",
        TextKey::ItemSavedValue => "Valor salvo",
        TextKey::AddonWorkspaceSelectFirst => "Primeiro selecione os itens para editar.",
        TextKey::AddonsNoneInstalled => "Nenhum complemento foi instalado ainda.",
        TextKey::AddonsNoneAvailable => "Suas fontes ainda não oferecem complementos.",
        TextKey::AddonsSetupHelp => "Abra Fontes para adicionar um repositório ou instalar de um arquivo.",
        TextKey::AddonPackageHelp => "Use esta linha para um ZIP contendo um único complemento.",
        TextKey::AddonRepositoryPackageHelp => "Use esta linha para um ZIP de repositório, como ErsatzRS-addons.zip, contendo um catálogo de complementos.",
        TextKey::AddonPackageUploadHelp => "Não foi possível instalar este complemento. Para um ZIP de repositório, use a linha Pacote de repositório abaixo.",
        TextKey::AddonRepositoryUploadHelp => "Não foi possível adicionar este repositório. Se o ZIP contém um único complemento, use a linha Pacote de complemento acima.",
        TextKey::AddonConfigurationSavedRecheck => "Salvo. O complemento fica desativado até você verificá-lo e ativá-lo novamente.",
        TextKey::AddonCheckRequired => "Configuração alterada. Verifique o complemento novamente antes de ativá-lo.",
        TextKey::AddonCheckPending => "Verificando complemento…",
        TextKey::AddonWorkspaceIncludeSelected => "Incluir itens selecionados",
        TextKey::AddonWorkspaceExcludeSelected => "Excluir itens selecionados",
        TextKey::AddonWorkspaceInclusionHelp => "Salve as alterações de seleção para aplicá-las automaticamente.",
        TextKey::AddonWorkspacePublicationFailed => "Falha na publicação",
        TextKey::AddonWorkspacePublicationRestoredHelp => "A versão anterior foi restaurada. As alterações salvas são mantidas e tentadas novamente de forma automática. Atividade mostra os erros técnicos.",

        TextKey::AddonWorkspaceParentMetadataPreserved => "O processamento parou para preservar metadados NFO principais alterados ou inválidos. Corrija o arquivo antes de tentar novamente.",

        TextKey::AddonWorkspaceImporting => "Importando lista…",
        TextKey::AddonWorkspaceImportingHelp => "A lista abrirá automaticamente ao concluir a descoberta. Sem JavaScript, use Atualizar.",
        TextKey::AddonWorkspaceImportSteps => "Importe a lista. Os metadados e as mídias necessárias são processados e publicados automaticamente.",
        TextKey::AddonWorkspaceChooseProvider => "Escolha um complemento abaixo se nenhum for detectado para esta URL.",
        TextKey::AddonWorkspaceExcluded => "Excluído da publicação",
        TextKey::AddonWorkspaceSort => "Ordem de classificação",
        TextKey::AddonWorkspaceAscending => "Crescente",
        TextKey::AddonWorkspaceDescending => "Decrescente",
        TextKey::AddonWorkspaceItemDate => "Data",
        TextKey::AddonWorkspaceSelectVisible => "Selecionar itens visíveis",
        TextKey::AddonWorkspaceDeselectVisible => "Desmarcar itens visíveis",
        TextKey::ItemFromList => "Da lista",
        TextKey::ItemUseListSetting => "Usar configuração da lista",
        TextKey::ItemOverride => "Valor do item",
        TextKey::ItemPackageDefault => "Padrão do pacote; ainda não ativado",
        TextKey::ItemPreviewOptions => "Prévia das alterações de opções",
        TextKey::ItemAccessActive => "Uma verificação de acesso já está em andamento.",
        TextKey::OpenItemInNewTab => "Abrir em uma nova aba",
        TextKey::BulkItemsToChange => "Itens a alterar",
        TextKey::BulkScopeHelp => "A numeração, a detecção de temporadas e as mudanças de tipo de mídia usam estes itens. O filtro de duração se aplica à numeração e à detecção de temporadas.",
        TextKey::BulkDetectSeasons => "Detectar temporadas",
        TextKey::BulkFirstSeason => "Primeira temporada",
        TextKey::BulkReviewChanges => "Revisar alterações",
        TextKey::BulkPreviewSaves => "Aplicar salva estes valores; as alterações salvas são publicadas automaticamente.",
        TextKey::BulkPreviewNothing => "Esta proposta não altera nada.",
        TextKey::BulkApplyNumbering => "Aplicar numeração",
        TextKey::BulkApplyChanges => "Aplicar alterações",
        TextKey::BulkCancelPreview => "Cancelar pré-visualização",
        TextKey::ActiveFilter => "Filtro ativo",
        TextKey::AddonWorkspaceLengthFilter => "Filtrar por duração",
        TextKey::AddonWorkspaceMinimumLength => "Duração mínima",
        TextKey::AddonWorkspaceMaximumLength => "Duração máxima",
        TextKey::AddonWorkspaceUnknownLength => "Incluir durações desconhecidas",
        TextKey::AddonWorkspaceLengthHelp => "Minutos:segundos. Deixe em branco para não limitar.",
        TextKey::AddonWorkspaceReviewLatest => "Revisar a última versão salva",
        TextKey::AddonWorkspaceReviewRetained => "Os dados inseridos foram mantidos. Compare-os com os últimos valores salvos antes de salvar. Revise separadamente as outras tarefas abertas.",
        TextKey::AddonWorkspaceStreamHelp => "Reproduzir diretamente da fonte.",
        TextKey::AddonWorkspaceDownloadHelp => "Preparar cada vídeo antes da reprodução.",
        TextKey::AddonWorkspaceNoOptions => "Este complemento não oferece configurações de acesso ou segmentos.",
        TextKey::AddonWorkspaceNewHelp => "Baixar vídeos recém-importados; transmitir os existentes.",
        TextKey::AddonWorkspaceSizeLimit => "Este formulário é muito grande. Volte aos dados inseridos e salve uma tarefa menor.",
        TextKey::AddonWorkspaceItems => "Itens",
        TextKey::AddonWorkspaceSettings => "Configurações da lista",
        TextKey::AddonWorkspacePlayback => "Reprodução e acesso",
        TextKey::AddonWorkspaceActivity => "Atividade e recuperação",
        TextKey::AddonWorkspaceSections => "Seções da lista",
        TextKey::AddonWorkspaceSaveSettings => "Salvar configurações",
        TextKey::AddonWorkspaceSaveItem => "Salvar item",
        TextKey::AddonWorkspaceSaveNumbering => "Salvar numeração",
        TextKey::AddonWorkspacePublish => "Publicar alterações salvas",
        TextKey::AddonWorkspacePublished => "A lista publicada está atualizada",
        TextKey::AddonWorkspacePending => "As alterações salvas são aplicadas automaticamente",
        TextKey::AddonWorkspacePreparationRequired => "As mídias necessárias são preparadas automaticamente",
        TextKey::AddonWorkspacePreparing => "Preparando mídia para publicação",
        TextKey::AddonWorkspaceAddIncomplete => "A lista foi adicionada, mas algumas configurações não foram salvas. Abra Configurações da lista, confira e salve novamente.",
        TextKey::AddonWorkspacePreparationFailed => "Falha na preparação de mídia",
        TextKey::RetryPreparation => "Tentar preparação novamente",
        TextKey::PreparationFailedItems => "Tarefas de preparação",
        TextKey::PreparationFailureFailed => "O complemento não conseguiu preparar a mídia. Tente a preparação novamente e verifique o complemento se a falha continuar.",
        TextKey::PreparationFailureInputChanged => "A solicitação de preparação não correspondia mais à lista salva nem ao complemento instalado. Tente a preparação novamente.",
        TextKey::PreparationFailureInvalidRequest => "As opções salvas não correspondem ao complemento instalado. Revise Reprodução e acesso, salve e tente a preparação novamente.",
        TextKey::PreparationFailureCancelled => "A preparação foi cancelada. Tente a preparação novamente para recomeçar.",
        TextKey::PreparationFailureAttemptsExhausted => "A preparação parou após várias tentativas com falha. Tente a preparação novamente e verifique o complemento se a falha continuar.",
        TextKey::PreparationFailureStorageFull => "Não havia espaço de armazenamento suficiente para preparar a mídia. Libere espaço e tente a preparação novamente.",
        TextKey::PreparationFailureUnsupported => "O complemento não pode preparar esta mídia.",
        TextKey::PreviousSavedRevision => "Revisão salva anterior",
        TextKey::AddonWorkspacePreparationActivity => "Preparação de mídia",
        TextKey::AddonWorkspaceBulk => "Editar vários itens",
        TextKey::AddonWorkspaceSelection => "Selecionados para edição",
        TextKey::AddonWorkspaceOffPage => "Em outras páginas",
        TextKey::AddonWorkspaceDiscard => "Descartar alterações",
        TextKey::AddonWorkspaceSelected => "Itens selecionados",
        TextKey::AddonWorkspaceMatching => "Todos os itens correspondentes",
        TextKey::AddonWorkspacePage => "Página atual",
        TextKey::AddonWorkspaceAccess => "Acesso e segmentos",
        TextKey::AddonWorkspaceSaveConflict => "As configurações salvas foram alteradas. Revise a versão atual antes de salvar.",
        TextKey::AddonWorkspaceSaveInvalid => "Não foi possível validar estas configurações. Revise os dados e tente novamente.",
        TextKey::AddonWorkspaceSaveStorage => "Não foi possível salvar as configurações. Os dados foram mantidos; tente salvar novamente.",
        TextKey::AddonWorkspaceLoadIncomplete => "Algumas informações do editor não puderam ser carregadas de forma consistente. Os dados foram mantidos. Revise a versão atual para tentar novamente.",
        TextKey::TechnicalDetails => "Detalhes técnicos",
        TextKey::AddonPermissionNetworkHelp => "Conectar aos hosts de rede listados",
        TextKey::AddonPermissionMediaHelp => "Ler mídias disponíveis para o ErsatzRS",
        TextKey::AddonPermissionFfmpegHelp => "Executar a versão gerenciada do FFmpeg",
        TextKey::AddonPermissionCommandHelp => "Executar os comandos externos listados",
        TextKey::PreparationTasks => "Tarefas de preparação",
        TextKey::PreparationAffectedItems => "Itens afetados",
        TextKey::PreparationIntervals => "Tarefas de intervalos / SponsorBlock",
        TextKey::PreparationDownloads => "Tarefas de download",
        TextKey::PreparationSubtitles => "Tarefas de legendas",
        TextKey::PreparationQueued => "Na fila",
        TextKey::PreparationRunning => "Em execução",
        TextKey::PreparationWaiting => "Aguardando",
        TextKey::PreparationNotQueued => "Fora da fila",
        TextKey::LastActivity => "Última atividade",
        TextKey::AddonAccessUnknownFailure => "A causa desta falha de acesso é desconhecida. Revise as configurações de acesso salvas, teste a fonte no navegador e tente verificar o acesso novamente.",
        TextKey::AddonWorkspaceRetained => "Não foi possível salvar as alterações. Seus dados foram mantidos; revise os erros ou recarregue a revisão atual.",
        TextKey::AddonWorkspaceNavigation => "A navegação mantém as alterações não salvas. As alterações salvas são aplicadas automaticamente.",
        TextKey::AddonWorkspaceSeasonOnly => "Alterar apenas temporada",
        TextKey::AddonWorkspaceSeasonPerYear => "Nova temporada por ano civil",
        TextKey::AddonWorkspaceYearDatesIncomplete => "Ainda faltam datas de envio. Aguarde os detalhes ou preencha as datas ausentes antes de agrupar as temporadas por ano.",
        TextKey::AddonWorkspacePageSize => "Itens por página",

        TextKey::AddonAccessTest => "Testar acesso",
        TextKey::AddonAccessHelp => {
            "Verifique este item com as opções salvas do rascunho. Alterações não salvas são excluídas; a reprodução publicada permanece igual."
        }
        TextKey::AddonAccessSaveFirst => {
            "Salve uma opção primeiro para ativar as configurações do rascunho antes de testar o acesso."
        }
        TextKey::AddonAccessError => {
            "Não foi possível carregar ou alterar a verificação de acesso. Atualize o item e tente novamente."
        }
        TextKey::AddonAccessNotChecked => "Não verificado",
        TextKey::AddonAccessQueued => "Verificação de acesso na fila",
        TextKey::AddonAccessRunning => "Verificando acesso",
        TextKey::AddonAccessWaiting => "Aguardando nova tentativa de acesso",
        TextKey::AddonAccessCancelled => "Verificação de acesso cancelada",
        TextKey::AddonAccessStale => "Resultado de acesso expirado ou configurações alteradas",
        TextKey::AddonAccessAccessible => "Acessível",
        TextKey::AddonAccessProfileUnavailable => "O perfil de navegador selecionado não está disponível para o servidor. Confira o perfil salvo e verifique o acesso novamente.",
        TextKey::AddonAccessProfileLocked => "O perfil do navegador está bloqueado. Feche o navegador que usa esse perfil e tente verificar o acesso novamente.",
        TextKey::AddonAccessDecryptionFailed => "O servidor não conseguiu descriptografar as credenciais do navegador. Confira o perfil e a conta que executa o servidor e tente novamente.",
        TextKey::AddonDerivedMetadataLimited => "Os metadados da série atingiram o limite de tamanho. Os metadados dos episódios continuam completos.",
        TextKey::AddonLocalPlaybackAvailable => "Cópia local reproduzível",
        TextKey::AddonAccessRestricted => "O provedor restringiu o acesso. Abra a fonte no navegador para conferir restrições de conta ou região e verifique o acesso novamente.",
        TextKey::AddonAccessNextAttempt => "Próxima tentativa:",
        TextKey::AddonAccessExpires => "O resultado expira:",
        TextKey::AddonAccessRefresh => "Atualizar status",
        TextKey::AddonOptionsCurrentFilter => "Filtro atual",
        TextKey::AddonOptions => "Opções do provedor",
        TextKey::AddonOptionsDraftHelp => "As opções salvas são aplicadas automaticamente. A reprodução atual permanece disponível durante o processamento.",
        TextKey::AddonOptionsProvisional => {
            "Estes padrões do pacote são provisórios. Salvar uma opção os ativa para este rascunho."
        }
        TextKey::AddonOptionsListDefault => "Padrão da lista",
        TextKey::AddonOptionsInherited => "Herdado",
        TextKey::AddonOptionsOverride => "Personalizado",
        TextKey::AddonOptionsFiniteOnly => {
            "Indisponível para itens ao vivo ou de duração desconhecida: esta opção exige mídia de duração finita. A reprodução direta continua disponível."
        }
        TextKey::AddonOptionsResetPackage => "Restaurar padrão do pacote",
        TextKey::AddonOptionsResetList => "Restaurar padrão da lista",
        TextKey::AddonOptionsMixed => "Valores diferentes — escolha um valor",
        TextKey::AddonOptionsPreviewHelp => {
            "Revise estes itens específicos antes de aplicar. Uma alteração intermediária no rascunho ou esquema exige uma nova prévia."
        }
        TextKey::AddonOptionsBefore => "Antes",
        TextKey::AddonOptionsAfter => "Depois",
        TextKey::AddonOptionsConflict => {
            "Não foi possível salvar ou carregar as opções. O rascunho, a seleção ou o esquema do pacote pode ter mudado. Recarregue o editor e tente novamente."
        }
        TextKey::AddonOptionsNone => "Este pacote não define opções.",
        TextKey::Addon => "Complemento",
        TextKey::PrimaryFeature => "Filme principal",
        TextKey::MovieExtra => "Extra do filme",
        TextKey::NeedReview => "Avisos",

        TextKey::ReviewRecommended => "Revisão recomendada",
        TextKey::PublicationReadiness => "Prontidão para publicação",
        TextKey::PublicationBlockers => "Bloqueios",
        TextKey::PublicationActivity => "Atividade",
        TextKey::PublicationJobs => "tarefas de publicação",
        TextKey::PublicationEvents => "eventos",
        TextKey::NoPublicationActivity => "Nenhuma atividade de publicação foi registrada ainda.",
        TextKey::CopyRedactedDiagnostics => "Copiar diagnóstico redigido",
        TextKey::DownloadRedactedDiagnostics => "Baixar diagnóstico redigido",
        TextKey::RedactedDiagnosticsCopied => "Diagnóstico redigido copiado.",
        TextKey::PublicationGeneration => "Geração",
        TextKey::CorrelationId => "ID de correlação",
        TextKey::TargetPublishedRevision => "Revisão publicada de destino",
        TextKey::RelatedOperation => "Operação relacionada",
        TextKey::FilesystemTransition => "Transição do sistema de arquivos",
        TextKey::RetryClass => "Classe de nova tentativa",
        TextKey::RecoveryAction => "Ação de recuperação",
        TextKey::OperatingSystemErrorCode => "Código de erro do sistema operacional",
        TextKey::PublicationOutcome => "Resultado",
        TextKey::PublicationExpired => "Expirada",
        TextKey::EventExistingJobAdopted => "Publicação existente adotada",
        TextKey::EventEnqueued => "Publicação colocada na fila",
        TextKey::EventLeaseWaitStarted => "Aguardando acesso à biblioteca",
        TextKey::EventLeaseAcquired => "Acesso à biblioteca obtido",
        TextKey::EventLeaseReclaimed => {
            "Publicação retomada após interrupção ou concessão expirada"
        }
        TextKey::EventPhaseAdvanced => "Fase de publicação avançou",
        TextKey::EventChildScanStarted => "Verificação iniciada",
        TextKey::EventChildScanCompleted => "Verificação concluída",
        TextKey::EventChildScanFailed => "Falha na verificação",
        TextKey::EventOriginalFailure => "Falha original de publicação",
        TextKey::EventRecoveryFailure => "Falha de recuperação",
        TextKey::EventRetryScheduled => "Nova tentativa de recuperação agendada",
        TextKey::EventRecoveryRequested => "Recuperação solicitada",
        TextKey::EventCompleted => "Publicação concluída",
        TextKey::EventHistoryTruncated => "A atividade anterior foi truncada",
        TextKey::TriggerAutomaticRefresh => "Atualização agendada",
        TextKey::TriggerOperator => "Solicitada por um operador",
        TextKey::TriggerRestoreRevision => "Solicitada ao restaurar uma revisão",
        TextKey::TestSource => "Testar fonte",
        TextKey::TestSourceHelp => "Verifique a fonte salva sem importar ou alterar a lista.",
        TextKey::SourceTestQueued => "Teste de fonte na fila",
        TextKey::SourceTestRunning => "Teste de fonte em andamento…",
        TextKey::SourceTestStale => {
            "O teste está desatualizado porque as entradas salvas não correspondem mais à lista, ao pacote do complemento ou aos padrões atuais."
        }
        TextKey::SourceAvailable => "A fonte está disponível",
        TextKey::SourceUnsupported => "A fonte não é compatível",
        TextKey::SourceTestFailed => "Falha no teste de fonte",
        TextKey::SourceRevision => "Revisão da fonte",
        TextKey::LastSuccessfulPlayoutBuild => "Última geração bem-sucedida",
        TextKey::PlayoutRefreshPending => "Atualização pendente",
        TextKey::PlayoutRefreshFailed => "A atualização falhou; a programação publicada é mantida.",
        TextKey::PlayoutSourceCheckPending => "Verificando alterações nas fontes",
        TextKey::PlayoutSourceCheckFailed => "A comparação de fontes falhou; uma nova tentativa está pendente.",
        TextKey::FirstPageComplete => "Primeira página completa",
        TextKey::ProviderIdentity => "Identidade do provedor",
        TextKey::ReturnedFirstPageItems => "Itens retornados na primeira página",
        TextKey::ProviderTotalItemCount => "Total de itens informado pelo provedor",
        TextKey::DraftChanges => "Alterações do rascunho salvo",
        TextKey::ExactDraftDiffHelp => {
            "Diferenças exatas entre o rascunho salvo e a revisão publicada atual."
        }
        TextKey::DraftDiffUnavailable => {
            "A comparação exata ficará disponível após publicar uma revisão completa."
        }
        TextKey::MetadataChanges => "Alterações de metadados",
        TextKey::ExcludedItems => "Itens excluídos",
        TextKey::Conflicts => "Conflitos",
        TextKey::PendingRemovals => "Remoções pendentes",
        TextKey::PendingRemovalHelp => {
            "Ausente uma vez em uma reconciliação completa. Confirme a remoção agora ou aguarde uma segunda observação completa pelo menos 24 horas depois."
        }
        TextKey::Reappearances => "Reaparições",
        TextKey::RemovalCandidates => "Candidatos à remoção",
        TextKey::ConfirmRemoval => "Confirmar remoção",
        TextKey::RemovalConfirmed => "Remoção confirmada.",
        TextKey::RemovalConfirmationFailed => "Falha ao confirmar a remoção.",
        TextKey::RevisionHistory => "Histórico de revisões",
        TextKey::RevisionHistoryHelp => {
            "Restaurar substitui o rascunho atual e publica o instantâneo escolhido como uma nova revisão."
        }
        TextKey::NoRevisionHistory => "Ainda não há revisões publicadas retidas.",
        TextKey::Revision => "Revisão",
        TextKey::CompareRevisions => "Comparar revisões",
        TextKey::RestoreRevision => "Restaurar revisão",
        TextKey::RestoreRevisionWarning => "Isso substitui o rascunho atual e publica a revisão",
        TextKey::RestoreRevisionQueued => "Restauração de revisão na fila",
        TextKey::RestoreRevisionFailed => "Não foi possível restaurar a revisão.",
        TextKey::PublishedItems => "Itens publicados",
        TextKey::GeneratedPlaylists => "Playlists geradas",
        TextKey::ActiveOrFuturePlayouts => "Exibições ativas ou futuras",
        TextKey::EffectiveOptions => "Opções efetivas",
        TextKey::Files => "Arquivos",
        TextKey::Artifacts => "Artefatos",
        TextKey::Added => "Adicionado",
        TextKey::Removed => "Removido",
        TextKey::Changed => "Alterado",
        TextKey::FilesystemPreviousBackedUp => "Revisão anterior salva",
        TextKey::FilesystemCandidateCommitted => "Revisão candidata confirmada",
        TextKey::FilesystemPreviousRestored => "Revisão anterior restaurada",
        TextKey::FilesystemProjectionRestored => "Projeção do banco de dados restaurada",
        TextKey::FilesystemRevisionVerified => "Revisão verificada",
        TextKey::FilesystemCleanupCompleted => "Limpeza da publicação concluída",
        TextKey::RetryTransientRecovery => "Recuperação transitória",
        TextKey::ErrorFilesystemIo => "E/S do sistema de arquivos",
        TextKey::ErrorPermissionDenied => "Permissão negada",
        TextKey::ErrorSharingViolation => "Violação de compartilhamento de arquivo",
        TextKey::ErrorUnsafeFilesystemState => "Estado inseguro do sistema de arquivos",
        TextKey::ErrorInvalidPublicationMarker => "Marcador de publicação inválido",
        TextKey::ErrorPublicationIntegrity => "Verificação da integridade da publicação",
        TextKey::OutcomePublished => "Publicada",
        TextKey::OutcomeRestoredAfterFailure => "Revisão anterior restaurada após a falha",
        TextKey::ReviewRecommendations => "Recomendações de revisão",
        TextKey::OptionalDetails => "Detalhes opcionais",
        TextKey::DraftSaved => "Rascunho salvo",
        TextKey::SavedRevision => "Revisão salva",
        TextKey::PublishList => "Publicar lista",
        TextKey::PreparePublication => "Preparar publicação",
        TextKey::ProcessedMediaRequired => "Tarefas de preparação",
        TextKey::ProcessedMediaQueued => "Preparação de mídia na fila",
        TextKey::PreparingProcessedMedia => "Preparando arquivos de mídia",
        TextKey::PreparingPublication => "Preparando publicação",
        TextKey::Publishing => "Publicando…",
        TextKey::PublicationQueued => "Publicação na fila",
        TextKey::RestoringPublication => "Restaurando a revisão anterior…",
        TextKey::Published => "Publicado",
        TextKey::PublishSavedRevisionHelp => {
            "A publicação usa exatamente a revisão salva. Salve primeiro as alterações da lista."
        }
        TextKey::ChangesSavedImmediately => {
            "Alterações de itens e em massa são salvas imediatamente e podem ser desfeitas."
        }
        TextKey::ReviewItem => "Revisar item",
        TextKey::ShowFullDescription => "Mostrar descrição completa",
        TextKey::ShowLess => "Mostrar menos",
        TextKey::ImportAList => "Importar uma lista",
        TextKey::ImportOverview => "Importar visão geral",
        TextKey::AddonContextSelected => "Add-on selecionado",
        TextKey::CompletedIn => "Concluído em",
        TextKey::NoPublicationBlockers => "Nenhum bloqueio de publicação.",
        TextKey::BlockerOverviewIncomplete => "A visão geral completa deve ser verificada.",
        TextKey::BlockerPublicationInProgress => "A publicação já está em andamento.",
        TextKey::BlockerRecoveryRequired => "Uma publicação anterior ainda precisa de recuperação.",
        TextKey::BlockerCollectionTitleMissing => "A coleção precisa de um título.",
        TextKey::BlockerInvalidArtwork => "As imagens devem usar uma URL HTTPS válida e limitada.",
        TextKey::BlockerNoIncludedItems => "Inclua pelo menos um item.",
        TextKey::BlockerMoviePrimaryCount => {
            "A publicação de filmes exige exatamente um filme principal."
        }
        TextKey::BlockerPlaybackDecisionRequired => "Escolha reprodução finita ou ao vivo.",
        TextKey::BlockerPlaybackDetailsUnavailable => {
            "Este add-on não fornece os detalhes de reprodução necessários."
        }
        TextKey::BlockerPlaybackDetailsFailed => {
            "Os detalhes do provedor não contêm duração positiva."
        }
        TextKey::BlockerUnavailableWithoutReplacement => {
            "O item indisponível precisa de uma substituição local válida."
        }
        TextKey::BlockerItemTitleMissing => "O item precisa de um título.",
        TextKey::BlockerSeasonEpisodeMissing => "Informe temporada e episódio válidos.",
        TextKey::BlockerArtistMissing => "Informe pelo menos um artista.",
        TextKey::PublishUnsavedChanges => "Salve as alterações antes de publicar esta revisão.",
        TextKey::PublicationBlocked => "A publicação está bloqueada.",
        TextKey::PublishBlocked => "Publicação bloqueada",
        TextKey::RetryRecovery => "Tentar recuperação novamente",
        TextKey::RestorePreviousRevision => "Restaurar revisão anterior",
        TextKey::PublicationPhase => "Fase da publicação",
        TextKey::PublicationAttempts => "Tentativas",
        TextKey::LastProgress => "Último progresso",
        TextKey::FailureReason => "Motivo original",
        TextKey::RestrictedItemsExcluded => "Itens restritos excluídos desta revisão",
        TextKey::RecoveryQueued => "Recuperação na fila",
        TextKey::RecoveryFailed => "Não foi possível colocar a recuperação na fila.",
        TextKey::PhaseAwaitingLease => "Aguardando acesso à biblioteca",
        TextKey::PhaseStaging => "Preparando revisão",
        TextKey::PhaseScanning => "Verificando a revisão preparada",
        TextKey::PhaseCommitting => "Confirmando revisão",
        TextKey::PhaseRestoringFiles => "Restaurando arquivos",
        TextKey::PhaseRestoringProjection => "Restaurando a projeção do banco de dados",
        TextKey::PhaseReconciling => "Reconciliando a revisão restaurada",
        TextKey::PhaseVerifying => "Verificando revisão",
        TextKey::PhaseCleaningUp => "Limpando arquivos de publicação",
        TextKey::PhaseComplete => "Concluída",
        TextKey::InstallLeavesDisabled => {
            "Após a instalação, o add-on permanece desativado até ser habilitado."
        }
        TextKey::EnableActivatesPermissions => {
            "Habilitar ativa as permissões declaradas do add-on."
        }
        TextKey::FirstAired => "Primeira exibição",
        TextKey::LoadItemDetailsAfterPublishing => "Carregar detalhes automaticamente",
        TextKey::LoadItemDetailsHelp => {
            "Importe primeiro a visão geral. Os detalhes técnicos necessários para reprodução são carregados antes da primeira publicação; os demais metadados ampliados seguem em ritmo controlado."
        }
        TextKey::OverviewOnlyManualDetailsHelp => {
            "Importe apenas a visão geral e solicite manualmente os detalhes descritivos. Os detalhes técnicos necessários para reprodução são preparados automaticamente antes da publicação."
        }
        TextKey::PlaybackDetailsQueued => "Detalhes de reprodução na fila",
        TextKey::PublishAgainAfterDetails => "Os detalhes concluídos são aplicados automaticamente.",

        TextKey::FetchDetails => "Buscar detalhes",
        TextKey::DetailsQueued => "Detalhes na fila",
        TextKey::DetailsLoading => "Carregando detalhes",
        TextKey::RetryDetails => "Tentar detalhes novamente",
        TextKey::DetailsComplete => "Detalhes completos",
        TextKey::DetailsSkipped => "Detalhes ignorados",
        TextKey::FindingProgrammes => "Procurando programas",
        TextKey::CheckingOverview => "Verificando a visão geral",
        TextKey::ReadyToReview => "Processamento automático",

        TextKey::LoadingItemDetails => "Carregando detalhes dos itens",
        TextKey::WaitingForProvider => "Aguardando o provedor",
        TextKey::OverviewOnly => "Somente visão geral",
        TextKey::PauseImport => "Pausar importação",
        TextKey::ResumeImport => "Retomar importação",
        TextKey::RetryImportNow => "Tentar importação novamente agora",
        TextKey::EmptyImportNeedsRetry => "A importação vazia precisa ser repetida",
        TextKey::EmptyImportRetryHelp => {
            "Esta importação concluída não contém itens. Atualize o complemento e tente novamente; as configurações e edições da lista serão preservadas."
        }
        TextKey::CancelImport => "Cancelar importação",
        TextKey::OtherPageItemsAffected => "Outros itens afetados estão em outras páginas",
        TextKey::ElapsedTime => "Tempo decorrido",
        TextKey::LastSuccessfulProgress => "Último progresso bem-sucedido",
        TextKey::EstimatedRemainingTime => "Tempo restante estimado",
        TextKey::PlaybackBehavior => "Comportamento de reprodução",
        TextKey::UseProviderValue => "Usar valor do provedor",
        TextKey::FiniteMedia => "Mídia finita",
        TextKey::LiveSource => "Fonte ao vivo",
        TextKey::ProviderValueChanged => "Valor do provedor alterado",
        TextKey::ApplyToSelected => "Aplicar aos selecionados",
        TextKey::ApplyToAllRemoteStreams => "Aplicar a todos os streams remotos",
        TextKey::SelectItem => "Selecionar item",
        TextKey::NumberEpisodes => "Numerar episódios",
        TextKey::NumberEpisodesHelp => {
            "Atribua uma temporada e números de episódio consecutivos na ordem original do complemento. Itens excluídos e fragmentos de capítulo são ignorados."
        }
        TextKey::FirstEpisode => "Primeiro episódio",
        TextKey::NumberToAssign => "Quantidade a atribuir",
        TextKey::AllRemaining => "Todos os restantes",
        TextKey::ReplaceExistingEpisodeNumbers => "Substituir números de episódio existentes",
        TextKey::AutoDetectSeasons => "Detectar temporadas pelas datas de exibição",
        TextKey::AutoDetectSeasonsHelp => {
            "Uma nova temporada começa quando o intervalo entre duas datas conhecidas atinge o mínimo configurado. Itens sem data continuam na temporada atual."
        }
        TextKey::MinimumSeasonGapDays => "Intervalo mínimo entre temporadas (dias)",
        TextKey::NumberingStartsAt => "A numeração começa em",
        TextKey::FirstIncludedItem => "Primeiro item incluído",
        TextKey::StartWithFirstItem => "Começar pelo primeiro item",
        TextKey::StartHere => "Começar aqui",
        TextKey::SaveSeasonEpisode => "Salvar temporada e episódio",
        TextKey::BulkEdit => "Edição em massa",
        TextKey::Scope => "Escopo",
        TextKey::AllItems => "Todos os itens",
        TextKey::CurrentPage => "Página atual",
        TextKey::ItemsNeedingReview => "Itens com avisos",

        TextKey::KeepCurrentValue => "Manter o valor atual",
        TextKey::UndoLastChange => "Desfazer a última alteração",
        TextKey::Undo => "Desfazer",
        TextKey::ChangeUndone => "Alteração desfeita",
        TextKey::MetadataConflicts => "Conflitos de metadados",
        TextKey::MetadataConflictsHelp => "O provedor alterou campos personalizados. Seus valores salvos continuam em uso.",
TextKey::PreviousProviderValue => "Valor anterior do provedor",
        TextKey::NewProviderValue => "Novo valor do provedor",
        TextKey::OperatorValue => "Valor do operador",
        TextKey::Discovered => "Detectados",
        TextKey::Examined => "Examinados",
        TextKey::ArchivedSkipped => "Ignorados por estarem arquivados",
        TextKey::Emitted => "Emitidos",
        TextKey::EditMetadata => "Editar metadados",
        TextKey::OneTagPerLine => "Uma tag por linha",
        TextKey::OriginalTitle => "Título original",
        TextKey::ReleaseDate => "Data de lançamento",
        TextKey::Year => "Ano",
        TextKey::SortTitle => "Título de ordenação",
        TextKey::MovieFilesAndExtras => "Arquivos do filme e extras",
        TextKey::MovieFilesAndExtrasHelp => {
            "Atribua exatamente um filme principal. A ordem da fonte não define a função; um trailer pode aparecer primeiro."
        }
        TextKey::EpisodesMetadataHelp => {
            "Revise a identidade e a numeração dos episódios; a ordem do provedor permanece inalterada."
        }
        TextKey::MusicVideosMetadataHelp => {
            "Artista, álbum e faixa pertencem a cada vídeo; listas com vários artistas continuam válidas."
        }
        TextKey::OtherVideosMetadataHelp => {
            "Use este tipo para vídeos independentes, não para um filme com extras."
        }
        TextKey::SongsMetadataHelp => {
            "Faixa, disco, artista e álbum são editados por música; a ordem da lista não substitui o número da faixa."
        }
        TextKey::ImagesMetadataHelp => {
            "Legenda e criador são editáveis; formato, dimensões e identidade da fonte continuam sendo dados factuais."
        }
        TextKey::RemoteStreamsMetadataHelp => {
            "Edite aqui os metadados de apresentação; conectividade e protocolo continuam sendo fatos da fonte do provedor."
        }
        TextKey::CastAndCredits => "Elenco e créditos",
        TextKey::OneCreditPerLine => "Uma pessoa por linha: nome | função",
        TextKey::Poster => "Pôster",
        TextKey::Artwork => "Imagens",
        TextKey::PublishMetadata => "Publicar",
        TextKey::IncludeItem => "Incluir",
        TextKey::IncludeInPublication => "Incluir na publicação",
        TextKey::MetadataDraftNotice => "Metadados e alterações de seleção salvos são aplicados automaticamente.",
TextKey::ApplyToAllItemsPrompt => {
            "Cada item é reclassificado com o tipo de mídia selecionado, substituindo valores derivados pelo complemento ou definidos por você."
        }
        TextKey::TelevisionEpisode => "Episódio de televisão",
        TextKey::CollectionTypeAuto => "Determinar pelos itens",
        TextKey::CurrentLibraryType => "Tipo atual da biblioteca",
        TextKey::CollectionTypeChangeHelp => "Salve para reorganizar automaticamente os itens existentes. A ordem, a seleção e as alterações de metadados são mantidas.",
TextKey::NonPlayableItems => "Itens não reproduzíveis",
        TextKey::UnknownItems => "Itens desconhecidos",
        TextKey::PlayableItems => "Itens reproduzíveis",
        TextKey::ApplyToAllItems => "Aplicar a todos os itens",
        TextKey::ChangeAddon => "Alterar complemento",
        TextKey::ChangeAddonHelp => "Selecione outro complemento para reimportar a lista por ele.",
        TextKey::ChangeAddonPrompt => {
            "A lista é reimportada pelo complemento selecionado. Metadados de itens, substituições e correções de capítulos são derivados novamente do novo provedor."
        }
        TextKey::AddonAutoDetect => "Determinar pela URL",
        TextKey::AddonAutoDetected => "Selecionado automaticamente a partir da URL.",
        TextKey::AddonMediaListNoAddonForUrl => {
            "Nenhum complemento instalado processa esta URL. Escolha um ou defina um processador padrão de transmissões remotas."
        }
        TextKey::DefaultRemoteStreamAddon => "Processador padrão de transmissões remotas",
        TextKey::DefaultRemoteStreamAddonHelp => {
            "Usado para fontes HTTP e HTTPS que nenhum complemento instalado reivindica. O complemento ainda precisa ter permissão para contatar o host."
        }
        TextKey::AddonDetailWait => "Ritmo dos detalhes dos itens",
        TextKey::MinimumDetailWait => "Espera mínima pelos detalhes",
        TextKey::MaximumDetailWait => "Espera máxima pelos detalhes",
        TextKey::AddonDetailWaitHelp => {
            "Após cada tentativa de carregar os detalhes de um item, o ErsatzRS espera um novo número aleatório de segundos dentro deste intervalo global antes que qualquer lista possa usar novamente o mesmo host do provedor."
        }
        TextKey::AddonDetailWaitInvalid => {
            "Informe segundos inteiros positivos e certifique-se de que o mínimo não seja maior que o máximo."
        }
        TextKey::Addons => "Complementos",
        TextKey::AddonOverview => "Visão geral",
        TextKey::AddonUpdates => "Atualizações",
        TextKey::AddonSources => "Fontes",
        TextKey::AddonNotFound => "Complemento não encontrado",
        TextKey::BackToAddons => "Voltar aos complementos",
        TextKey::AvailableVersion => "Versão disponível",
        TextKey::BrowseAddons => "Explorar complementos",
        TextKey::CheckForUpdates => "Verificar atualizações",
        TextKey::InstallFromFile => "Instalar de arquivo",
        TextKey::InstallRepositoryFromFile => "Instalar repositório de arquivo",
        TextKey::License => "Licença",
        TextKey::UnknownSources => "Fontes desconhecidas",
        TextKey::UnknownSourcesHelp => {
            "Permite complementos de checkouts Git locais e arquivos ZIP. Os complementos são executados com o acesso do usuário do serviço ErsatzRS."
        }
        TextKey::Verified => "Verificado",
        TextKey::Unverified => "Não verificado",
        TextKey::Sideloaded => "Instalado localmente",
        TextKey::GitCheckout => "Checkout do Git",
        TextKey::OfflineBundle => "Pacote offline",
        TextKey::LegacyUnmanaged => "Catálogo legado · não gerenciado",
        TextKey::LegacyAdoptionHelp => {
            "Isso substitui a instalação de catálogo não gerenciada pela fonte local selecionada, preservando suas configurações e seus dados."
        }
        TextKey::Enabled => "Ativado",
        TextKey::Disabled => "Desativado",
        TextKey::Source => "Fonte",
        TextKey::ViewSourceCode => "Ver o código-fonte",
        TextKey::Version => "Versão",
        TextKey::NextVersion => "Versão do mecanismo Next",
        TextKey::NoAddonsMatch => "Nenhum complemento corresponde a estes filtros.",
        TextKey::AddonsUpToDate => "Tudo está atualizado.",
        TextKey::AddonRepositories => "Repositórios de complementos",
        TextKey::AvailableAddons => "Complementos disponíveis",
        TextKey::InstalledAddons => "Complementos instalados",
        TextKey::AddRepository => "Adicionar repositório",
        TextKey::LastRefresh => "Última verificação",
        TextKey::NeverRefreshed => "Nunca",
        TextKey::ChooseFile => "Escolher arquivo…",
        TextKey::NoFileSelected => "Nenhum arquivo selecionado",
        TextKey::EditRepository => "Editar repositório",
        TextKey::OfficialRepositoryProtected => "O repositório oficial não pode ser removido.",
        TextKey::UnknownSourcesRequired => "Ative primeiro as fontes desconhecidas.",
        TextKey::NoRepositories => {
            "Nenhum repositório local encontrado. Clone um repositório na pasta de checkout acima e atualize."
        }
        TextKey::AddonPackage => "Pacote de complemento",
        TextKey::RepositoryPackage => "Pacote de repositório",
        TextKey::NoSigningKeyPinned => "Nenhuma chave de assinatura pública está fixada.",
        TextKey::RepositoryUrl => "URL do repositório",
        TextKey::PublicSigningKey => "Chave pública de assinatura",
        TextKey::DeploymentManagedRepositories => {
            "Os repositórios são gerenciados pela configuração da implantação."
        }
        TextKey::Refresh => "Atualizar",
        TextKey::Install => "Instalar",
        TextKey::UpdateAddon => "Atualizar",
        TextKey::Uninstall => "Desinstalar",
        TextKey::Configuration => "Configuração",
        TextKey::SecretReference => "Referência do segredo",
        TextKey::EnvironmentVariable => "Variável de ambiente",
        TextKey::SecretFile => "Arquivo de segredo",
        TextKey::Incompatible => "Incompatível",
        TextKey::PackageUnavailable => "Pacote indisponível",
        TextKey::Official => "Oficial",
        TextKey::ThirdParty => "Repositório de terceiros",
        TextKey::Permissions => "Permissões",
        TextKey::Capabilities => "Recursos",
        TextKey::Dependencies => "Dependências",
        TextKey::Optional => "Opcional",
        TextKey::MediaReadAccess => "Acesso de leitura à mídia",
        TextKey::ExternalCommands => "Comandos externos",
        TextKey::ThirdPartyRepositoryWarning => {
            "Carregue apenas repositórios locais e arquivos ZIP confiáveis; os complementos são executados como o usuário do serviço ErsatzRS."
        }
        TextKey::AddCustomResolution => "Adicionar resolução personalizada",
        TextKey::AddBlock => "Adicionar bloco",
        TextKey::AddBlockGroup => "Adicionar grupo de blocos",
        TextKey::AddBlockItem => "Adicionar item de bloco",
        TextKey::AddBlockToTemplate => "Adicionar bloco ao modelo",
        TextKey::AddCollection => "Adicionar coleção",
        TextKey::AddContent => "Adicionar conteúdo",
        TextKey::AddFailed => "Falha ao adicionar",
        TextKey::AddMultiCollection => "Adicionar multi-coleção",
        TextKey::AddPlaylist => "Adicionar playlist",
        TextKey::AddPlaylistGroup => "Adicionar grupo de playlists",
        TextKey::AddPlaylistItem => "Adicionar item de playlist",
        TextKey::AddRerunCollection => "Adicionar coleção de reprises",
        TextKey::AddTemplate => "Adicionar modelo",
        TextKey::AddTemplateGroup => "Adicionar grupo de modelos",
        TextKey::AddAddonMediaList => "Adicionar lista add-on",
        TextKey::AddToCollection => "Adicionar à coleção",
        TextKey::AddToEllipsis => "Adicionar a ...",
        TextKey::AddToPlaylist => "Adicionar à playlist",
        TextKey::AddToSchedule => "Adicionar ao horário",
        TextKey::Actions => "Ações",
        TextKey::All => "Todos",
        TextKey::AreYouSureDelete => "Tem certeza de que deseja excluir",
        TextKey::Actors => "Atores",
        TextKey::Artist => "Artista",
        TextKey::ArtistNotFound => "Artista não encontrado",
        TextKey::AfterDurationEnd => "Após o fim da duração",
        TextKey::AutomaticRefresh => "Atualização automática",
        TextKey::AddonDeliveryMode => "Distribuição",
        TextKey::AddonDeliveryStream => "Streaming",
        TextKey::AddonDeliveryDownloadAhead => "Baixar antecipadamente",
        TextKey::AddonDeliveryNewVideos => "Baixar antecipadamente — apenas vídeos novos",
        TextKey::AddonDeliveryHelp => "As alterações entram em vigor após a próxima publicação. Apenas vídeos importados pela primeira vez após a data limite salva são baixados; os já conhecidos são transmitidos por streaming. Reativar este modo define uma nova data limite. Somente mídias com duração finita podem ser baixadas antecipadamente. Conteúdos ao vivo e aqueles com duração desconhecida continuam sendo transmitidos por streaming.",
        TextKey::AddonDeliveryUnavailable => "O download requer um complemento compatível e ativado e mídia com duração finita.",
        TextKey::AddonSelectionInitialReview => "Selecionar antes da primeira publicação",
        TextKey::AddonSelectionInitialReviewHelp => "Importe primeiro somente a visão geral. Revise e salve sua seleção antes de adquirir mídia ou publicar a lista.",
        TextKey::AddonSelectionReviewRequired => "Revise e salve a seleção inicial. A publicação automática e a aquisição estão aguardando.",
        TextKey::AddonSelectionEvaluatedAt => "Última avaliação da seleção",
        TextKey::AddonSelection => "Seleção contínua",
        TextKey::AddonSelectionHelp => "Limites vazios aceitam qualquer valor. Os termos são buscados no título sem diferenciar maiúsculas. Itens fixados ignoram os filtros e não contam entre os N mais recentes. Arquivos arquivados são preservados.",
        TextKey::AddonSelectionEnabled => "Ativar filtros de seleção",
        TextKey::AddonSelectionPause => "Pausar consultas automáticas",
        TextKey::AddonSelectionAge => "Idade máxima (dias)",
        TextKey::AddonSelectionNewest => "Itens mais recentes (sem fixados)",
        TextKey::AddonSelectionMinimum => "Duração mínima (segundos)",
        TextKey::AddonSelectionMaximum => "Duração máxima (segundos)",
        TextKey::AddonSelectionInclude => "Termos do título para incluir (um por linha)",
        TextKey::AddonSelectionExclude => "Termos do título para excluir (um por linha)",
        TextKey::AddonSelectionActive => "Seleção ativa",
        TextKey::AddonSelectionArchived => "Seleção arquivada",
        TextKey::AddonSelectionPreviewHelp => "A prévia não publica nem baixa. A tabela mostra até 100 itens; todas as decisões são contadas. Salvar aplica os filtros revisados pelo serviço existente.",
        TextKey::AddonSelectionManual => "Seleção manual",
        TextKey::AddonSelectionPinned => "Fixado",
        TextKey::AddonSelectionExcluded => "Excluído manualmente",
        TextKey::AddonSelectionAutomatic => "Seguir os filtros de seleção",
        TextKey::AddonSelectionCutoff => "Limite de antiguidade efetivo",
        TextKey::AddonSelectionMissingDate => "Data de publicação ausente",
        TextKey::AddonSelectionFutureDate => "Data de publicação futura",
        TextKey::AddonSelectionTooOld => "Fora da janela de idade",
        TextKey::AddonSelectionMissingDuration => "Duração ausente",
        TextKey::AddonSelectionTooShort => "Abaixo da duração mínima",
        TextKey::AddonSelectionTooLong => "Acima da duração máxima",
        TextKey::AddonSelectionTitleNotIncluded => "Nenhum termo incluído encontrado",
        TextKey::AddonSelectionTitleExcluded => "Um termo excluído foi encontrado",
        TextKey::AddonSelectionNewestLimit => "Fora dos N mais recentes",
        TextKey::AddonSelectionInvalid => "Verifique os limites e termos, ou recarregue após uma alteração simultânea.",
        TextKey::AddonDeliveryCutoff => "Data limite dos vídeos novos",
        TextKey::AddonDeliveryFreshCutoff => "Uma nova data limite é registrada ao salvar.",
        TextKey::AdaptiveRefreshUnavailable => {
            "A atualização adaptativa está indisponível porque este complemento não oferece mais verificações da origem. Escolha outra frequência para retomar a atualização automática."
        }
        TextKey::RefreshPublicationPolicy => "Publicação após atualização",
        TextKey::RefreshPublicationReview => "Revisar cada atualização",
        TextKey::RefreshPublicationAutomatic => "Aplicar automaticamente as alterações salvas",
        TextKey::RefreshPublicationPolicyHelp => "Importações, atualizações, metadados e alterações salvas são aplicados automaticamente.",
        TextKey::AutoPublicationHeld => "O processamento automático precisa de atenção",
        TextKey::PreparationWorkerUnavailable => {
            "O processo de preparação de mídia está indisponível. Reinicie o servidor antes de tentar a preparação novamente."
        }
        TextKey::PreparationProcessRecoveryRequired => {
            "A preparação de mídia requer recuperação de processos. Confirme que os processos anteriores do provedor foram encerrados antes de recuperar o armazenamento reservado e tentar preparar novamente."
        }
        TextKey::AutoPublicationBaselineUnavailable => "Nenhuma base publicada está disponível.",
        TextKey::AutoPublicationExistingChanges => "Os itens existentes foram alterados.",
        TextKey::AutoPublicationConflicts => {
            "Conflitos de metadados ou substituições precisam de revisão."
        }
        TextKey::AutoPublicationRemovals => "Itens foram removidos.",
        TextKey::AutoPublicationRestricted => "Algumas adições têm restrições de acesso.",
        TextKey::AutoPublicationUnavailable => "Algumas adições estão indisponíveis.",
        TextKey::AutoPublicationOptionsChanged => "As opções de publicação foram alteradas.",
        TextKey::AutoPublicationPreparationFailed => "A preparação da publicação falhou.",
        TextKey::AutoPublicationConcurrentChange => "A lista mudou durante a preparação.",
        TextKey::RefreshCadenceAdaptive => "Adaptativa",
        TextKey::RefreshCadenceDaily => "Diária",
        TextKey::RefreshCadenceWeekly => "Semanal",
        TextKey::RefreshCadenceMonthly => "Mensal",
        TextKey::RefreshCadenceYearly => "Anual",
        TextKey::RefreshCadenceNever => "Nunca",
        TextKey::LastSuccessfulRefresh => "Última atualização bem-sucedida",
        TextKey::NextScheduledRefresh => "Próxima atualização agendada",
        TextKey::RefreshRetryAt => "Nova tentativa disponível a partir de",
        TextKey::Back => "Voltar",
        TextKey::BackToSettings => "Voltar para as definições",
        TextKey::BackToArtists => "Voltar para artistas",
        TextKey::BackToCollections => "Voltar para coleções",
        TextKey::BackToMovies => "Voltar para filmes",
        TextKey::BackToOtherVideos => "Voltar para outros vídeos",
        TextKey::BackToTvShows => "Voltar para séries",
        TextKey::BeforeDurationEnd => "Antes do fim da duração",
        TextKey::BackToTemplates => "Voltar para modelos",
        TextKey::Block => "Bloco",
        TextKey::BlockGroup => "Grupo de blocos",
        TextKey::BlockCannotBeMoved => {
            "O bloco não pode ser movido para lá porque iria sobrepor outro bloco ou passar da meia-noite."
        }
        TextKey::BlockCopied => "Bloco copiado.",
        TextKey::BlockCreated => "Bloco criado.",
        TextKey::BlockDeleted => "Bloco excluído.",
        TextKey::BlockGroupCreated => "Grupo de blocos criado.",
        TextKey::BlockGroupDeleted => "Grupo de blocos excluído.",
        TextKey::BlockGroupName => "Nome do grupo de blocos",
        TextKey::BlockGroups => "Grupos de blocos",
        TextKey::BlockItem => "Item de bloco",
        TextKey::BlockItems => "Itens de bloco",
        TextKey::BlockName => "Nome do bloco",
        TextKey::BlockNotFound => "Bloco não encontrado.",
        TextKey::BlockPreviewFailed => {
            "Não foi possível gerar a prévia do bloco. Verifique os itens do bloco e tente novamente."
        }
        TextKey::Blocks => "Blocos",
        TextKey::BlockToRemove => "Bloco a remover",
        TextKey::BlockWillNotFit => {
            "O bloco não cabe na hora de início selecionada ou depois dela."
        }
        TextKey::Cancel => "Cancelar",
        TextKey::Clear => "Limpar",
        TextKey::Available => "Disponível",
        TextKey::UnknownAvailability => "Desconhecido",
        TextKey::LocalReplacement => "Substituição local",
        TextKey::NoReplacement => "Sem substituição",
        TextKey::InvalidReplacement => "Substituição inválida",
        TextKey::Channels => "Canais",
        TextKey::Close => "Fechar",
        TextKey::Collections => "Coleções",
        TextKey::CollectionDeleted => "Coleção excluída.",
        TextKey::CollectionSaved => "Coleção salva.",
        TextKey::SaveCollection => "Salvar coleção",
        TextKey::ComputingPreview => "Calculando prévia...",
        TextKey::Confirm => "Confirmar",
        TextKey::ConfirmDelete => "Confirmar exclusão",
        TextKey::CommentaryTracks => "Faixas de comentário",
        TextKey::Content => "Conteúdo",
        TextKey::ContentVariantAcoustic => "Acústico",
        TextKey::ContentVariantAlternateVersion => "Versão",
        TextKey::ContentVariantBehindTheScenes => "Bastidores",
        TextKey::ContentVariantDeletedScene => "Cena excluída",
        TextKey::ContentVariantDirectorsCut => "Versão do diretor",
        TextKey::ContentVariantExtended => "Estendida",
        TextKey::ContentVariantExtra => "Extra",
        TextKey::ContentVariantFeaturette => "Featurette",
        TextKey::ContentVariantInterview => "Entrevista",
        TextKey::ContentVariantLive => "Ao vivo",
        TextKey::ContentVariantOfficialVideo => "Vídeo oficial",
        TextKey::ContentVariantOther => "Outro",
        TextKey::ContentVariantRemaster => "Remaster",
        TextKey::ContentVariantRemix => "Remix",
        TextKey::ContentVariantScene => "Cena",
        TextKey::ContentVariantSearchHelp => {
            "Tokens de variantes: content_kind:extra, content_kind:alternate_version, sub_kind:featurette, sub_kind:unplugged, parent_title:\"Título do filme\", audio_title:commentary, subtitle_title:commentary, has_commentary:true"
        }
        TextKey::ContentVariantShort => "Curta",
        TextKey::ContentVariantSpecialFeatures => "Extras / recursos especiais",
        TextKey::ContentVariantTrailer => "Trailer",
        TextKey::ContentVariantUnplugged => "Unplugged",
        TextKey::ContentVariantVersions => "Versões",
        TextKey::ContentRatings => "Classificações indicativas",
        TextKey::CustomResolutionHeight => "Altura da resolução personalizada",
        TextKey::CustomResolutionWidth => "Largura da resolução personalizada",
        TextKey::CustomResolutions => "Resoluções personalizadas",
        TextKey::Dark => "Escuro",
        TextKey::DaysToBuild => "Dias para gerar",
        TextKey::AddBreakContent => "Adicionar conteúdo de intervalo",
        TextKey::AddDeco => "Adicionar decoração",
        TextKey::AddDecoGroup => "Adicionar grupo de decorações",
        TextKey::AddDecoTemplate => "Adicionar modelo de decoração",
        TextKey::AddDecoTemplateGroup => "Adicionar grupo de modelos de decoração",
        TextKey::AtChapterMarkers => "Nos marcadores de capítulo",
        TextKey::BackToDecos => "Voltar para decorações",
        TextKey::BackToDecoTemplates => "Voltar para modelos de decoração",
        TextKey::BetweenBlockItems => "Entre itens do bloco",
        TextKey::BlockFinish => "Fim do bloco",
        TextKey::BlockStart => "Início do bloco",
        TextKey::BreakContent => "Conteúdo de intervalo",
        TextKey::BreakContentBody => {
            "O conteúdo de intervalo pode ser usado para reproduzir itens específicos (usando playlists) e preencher blocos até a duração configurada."
        }
        TextKey::BreakContentItem => "Item de conteúdo de intervalo",
        TextKey::BreakContentMode => "Modo de conteúdo de intervalo",
        TextKey::DeadAirFallback => "Fallback de tela sem programação",
        TextKey::DeadAirFallbackBody => {
            "Quando nenhum item de playout for encontrado para o horário atual, um item será selecionado aleatoriamente desta coleção, repetido e ajustado para caber exatamente até o início do próximo item de playout. Isso substitui a imagem Canal offline que seria exibida."
        }
        TextKey::DeadAirFallbackCollectionType => {
            "Tipo de coleção do fallback de tela sem programação"
        }
        TextKey::DeadAirFallbackMode => "Modo do fallback de tela sem programação",
        TextKey::Deco => "Decoração",
        TextKey::DecoCannotBeAdded => {
            "A decoração não pode ser adicionada porque sobreporia outra ou passaria da meia-noite."
        }
        TextKey::DecoCannotBeMoved => {
            "A decoração não pode ser movida para lá porque sobreporia outra ou passaria da meia-noite."
        }
        TextKey::DecoCreated => "Decoração criada.",
        TextKey::DecoDeleted => "Decoração excluída.",
        TextKey::DecoGroup => "Grupo de decorações",
        TextKey::DecoGroupCreated => "Grupo de decorações criado.",
        TextKey::DecoGroupDeleted => "Grupo de decorações excluído.",
        TextKey::DecoGroupName => "Nome do grupo de decorações",
        TextKey::DecoGroups => "Grupos de decorações",
        TextKey::DecoName => "Nome da decoração",
        TextKey::DecoNotFound => "Decoração não encontrada.",
        TextKey::DecoSaved => "Decoração salva.",
        TextKey::Decos => "Decorações",
        TextKey::DecoTemplate => "Modelo de decoração",
        TextKey::DecoTemplateCreated => "Modelo de decoração criado.",
        TextKey::DecoTemplateDeleted => "Modelo de decoração excluído.",
        TextKey::DecoTemplateGroup => "Grupo de modelos de decoração",
        TextKey::DecoTemplateGroupCreated => "Grupo de modelos de decoração criado.",
        TextKey::DecoTemplateGroupDeleted => "Grupo de modelos de decoração excluído.",
        TextKey::DecoTemplateGroupName => "Nome do grupo de modelos de decoração",
        TextKey::DecoTemplateGroups => "Grupos de modelos de decoração",
        TextKey::DecoTemplateName => "Nome do modelo de decoração",
        TextKey::DecoTemplateNotFound => "Modelo de decoração não encontrado.",
        TextKey::DecoTemplateSaved => "Modelo de decoração salvo.",
        TextKey::AddDecoToTemplate => "Adicionar decoração ao modelo",
        TextKey::DecoToRemove => "Decoração a remover",
        TextKey::DefaultFiller => "Preenchimento padrão",
        TextKey::DefaultFillerBody => {
            "Depois que todos os blocos forem agendados, uma segunda passagem preencherá o tempo não agendado com itens aleatórios desta coleção."
        }
        TextKey::DefaultFillerCollectionType => "Tipo de coleção do preenchimento padrão",
        TextKey::DefaultFillerMode => "Modo do preenchimento padrão",
        TextKey::DeleteDeco => "Excluir decoração",
        TextKey::DeleteDecoGroup => "Excluir grupo de decorações",
        TextKey::DeleteDecoTemplate => "Excluir modelo de decoração",
        TextKey::DeleteDecoTemplateGroup => "Excluir grupo de modelos de decoração",
        TextKey::Disable => "Desativar",
        TextKey::EditDeco => "Editar decoração",
        TextKey::EditDecoTemplate => "Editar modelo de decoração",
        TextKey::GraphicsElementsMode => "Modo dos elementos gráficos",
        TextKey::Inherit => "Herdar",
        TextKey::Merge => "Mesclar",
        TextKey::NoDecosFound => "Nenhuma decoração encontrada.",
        TextKey::NoDecoTemplatesFound => "Nenhum modelo de decoração encontrado.",
        TextKey::Override => "Substituir",
        TextKey::RemoveBreakContent => "Remover conteúdo de intervalo",
        TextKey::RemoveDecoFromTemplate => "Remover decoração do modelo",
        TextKey::Replace => "Trocar",
        TextKey::SavedDecos => "Decorações salvas",
        TextKey::SavedDecoTemplates => "Modelos de decoração salvos",
        TextKey::SearchForDecos => "Pesquisar decorações",
        TextKey::SearchForDecoTemplates => "Pesquisar modelos de decoração",
        TextKey::TrimToFit => "Ajustar ao tamanho",
        TextKey::UseGraphicsElementsDuringFiller => {
            "Usar elementos gráficos durante o preenchimento"
        }
        TextKey::UseWatermarkDuringFiller => "Usar marca d'água durante o preenchimento",
        TextKey::WatermarkMode => "Modo da marca d'água",
        TextKey::DecoTemplates => "Modelos de decoração",
        TextKey::DefaultFfmpegProfile => "Perfil FFmpeg padrão",
        TextKey::DefaultFfmpegProfileHelp => "O perfil FFmpeg usado ao criar novos canais",
        TextKey::DefaultMinimumLogLevel => "Nível mínimo de log padrão",
        TextKey::DefaultMpegTsScript => "Script MPEG-TS padrão",
        TextKey::DefaultMpegTsScriptHelp => "Escolha a saída integrada para o teletexto criado no ErsatzRS. Os scripts controlam sua própria saída.",
        TextKey::BuiltInMpegTsWrapper => "Saída MPEG-TS integrada",
        TextKey::Decorations => "Decorações",
        TextKey::Debug => "Depuração",
        TextKey::Delete => "Excluir",
        TextKey::DeleteAll => "Excluir tudo",
        TextKey::DeleteBlock => "Excluir bloco",
        TextKey::DeleteBlockGroup => "Excluir grupo de blocos",
        TextKey::AddFillerPreset => "Adicionar predefinição de preenchimento",
        TextKey::DeleteCustomResolution => "Excluir resolução personalizada",
        TextKey::DeleteCollection => "Excluir coleção",
        TextKey::DeleteFillerPreset => "Excluir predefinição de preenchimento",
        TextKey::DeleteMultiCollection => "Excluir multi-coleção",
        TextKey::DeletePlaylist => "Excluir playlist",
        TextKey::DeletePlaylistGroup => "Excluir grupo de playlists",
        TextKey::DeleteRerunCollection => "Excluir coleção de reprises",
        TextKey::DeleteTemplate => "Excluir modelo",
        TextKey::DeleteTemplateGroup => "Excluir grupo de modelos",
        TextKey::DeleteAddonMediaList => "Excluir lista add-on",
        TextKey::DisabledCollectionsAlreadyPresent => {
            "Coleções desativadas já estão presentes nesta multi-coleção"
        }
        TextKey::EmptyTrash => "Esvaziar lixo",
        TextKey::ExtractEmbeddedSubtitles => "Extrair legendas embutidas (texto)",
        TextKey::ErasedPlayoutItemsAndHistory => "Itens da programação e histórico foram apagados.",
        TextKey::EraseFailed => "Falha ao apagar",
        TextKey::EraseItemsAndHistory => "Apagar itens e histórico",
        TextKey::ErasePlayoutItemsAndHistoryConfirm => {
            "Isso excluirá todos os itens da programação e o histórico desta programação sequencial. Continuar?"
        }
        TextKey::Episode => "Episódio",
        TextKey::FanArt => "fan art",
        TextKey::Ffmpeg => "FFmpeg",
        TextKey::FileNotFound => "Arquivo não encontrado",
        TextKey::Finish => "Fim",
        TextKey::Fatal => "Fatal",
        TextKey::FfmpegPath => "Caminho do FFmpeg",
        TextKey::FfmpegPathHelp => "O caminho completo para o arquivo executável ffmpeg",
        TextKey::FfmpegProfiles => "Perfis FFmpeg",
        TextKey::FfprobePath => "Caminho do FFprobe",
        TextKey::FfprobePathHelp => "O caminho completo para o arquivo executável ffprobe",
        TextKey::FillerPreset => "Predefinição de preenchimento",
        TextKey::FillerPresetActionFailed => "A ação da predefinição de preenchimento falhou.",
        TextKey::FillerPresetCouldNotBeSaved => {
            "A predefinição de preenchimento não pôde ser salva. Verifique os valores do formulário e tente novamente."
        }
        TextKey::FillerPresetDeleted => "Predefinição de preenchimento excluída.",
        TextKey::FillerPresetExpressionHelp => {
            "Para preenchimento mid-roll, adicione preenchimento somente quando esta expressão for verdadeira para um determinado ponto mid-roll."
        }
        TextKey::FillerPresetSaved => "Predefinição de preenchimento salva.",
        TextKey::FillerPresets => "Predefinições de preenchimento",
        TextKey::Flags => "Marcadores",
        TextKey::GlobalFallbackFiller => "Preenchimento de fallback global",
        TextKey::GlobalWatermark => "Marca d'água global",
        TextKey::GoHome => "Ir para início",
        TextKey::HdHomeRun => "HDHomeRun",
        TextKey::HealthChecks => "Verificações de integridade",
        TextKey::Height => "Altura",
        TextKey::HlsDirectOutputFormat => "Formato de saída direta HLS",
        TextKey::HlsSegmenterIdleTimeout => "Tempo limite ocioso do segmentador HLS",
        TextKey::HlsSegmenterIdleTimeoutHelp => {
            "O número de segundos para continuar transcodificando um canal quando nenhum cliente recebeu novas solicitações"
        }
        TextKey::HlsSegmenterInitialSegmentCount => "Contagem inicial de segmentos HLS",
        TextKey::HlsSegmenterInitialSegmentCountHelp => {
            "Atrasa o início do stream até que a quantidade especificada de segmentos de 4 segundos seja transcodificada; um número maior inicia mais devagar, mas pode reduzir buffering"
        }
        TextKey::HomeRunning => "O servidor IPTV está em execução.",
        TextKey::ReleaseNotes => "Notas de lançamento",
        TextKey::FullChangelog => "Registro completo de alterações",
        TextKey::HttpMinimumLogLevel => "Nível mínimo de log de solicitações",
        TextKey::Id => "Id",
        TextKey::Images => "Imagens",
        TextKey::Item => "Item",
        TextKey::ItemActions => "Ações do item",
        TextKey::ItemType => "Tipo de item",
        TextKey::Items => "Itens",
        TextKey::Language => "Idioma",
        TextKey::Languages => "Idiomas",
        TextKey::MusicLabels => "Gravadoras",
        TextKey::LeaveEmptyToAutoDetect => "Deixe em branco para detectar automaticamente",
        TextKey::Libraries => "Bibliotecas",
        TextKey::LibraryRefreshInterval => "Intervalo de atualização da biblioteca",
        TextKey::Light => "Claro",
        TextKey::Lists => "Listas",
        TextKey::Loading => "Carregando...",
        TextKey::LoadingMembers => "Carregando membros...",
        TextKey::Local => "Local",
        TextKey::Logging => "Logs",
        TextKey::M3u => "M3U",
        TextKey::Media => "Mídia",
        TextKey::MediaInfo => "Informações de mídia",
        TextKey::MediaSources => "Fontes de mídia",
        TextKey::Movie => "Filme",
        TextKey::MovieNotFound => "Filme não encontrado",
        TextKey::Movies => "Filmes",
        TextKey::MusicVideo => "Videoclipe",
        TextKey::MultiCollections => "Multi-coleções",
        TextKey::ManualCollections => "Coleções manuais",
        TextKey::MatchFailed => "Falha ao combinar",
        TextKey::MatchStatus => "Status de combinação",
        TextKey::MatchAddonMediaListItems => "Combinar itens da lista add-on",
        TextKey::Members => "Membros",
        TextKey::MultiCollection => "Multi-coleção",
        TextKey::MultiCollectionDeleted => "Multi-coleção excluída.",
        TextKey::MultiCollectionItems => "Itens da multi-coleção",
        TextKey::MultiCollectionNameRequired => "O nome da multi-coleção é obrigatório!",
        TextKey::MultiCollectionNameTooLong => {
            "O nome da multi-coleção não pode ter mais de 50 caracteres."
        }
        TextKey::MultiCollectionSaved => "Multi-coleção salva.",
        TextKey::Music => "Música",
        TextKey::Name => "Nome",
        TextKey::NameRequired => "Nome é obrigatório",
        TextKey::Networks => "Redes",
        TextKey::NewCollection => "(Nova coleção)",
        TextKey::NewCollectionTitle => "Nova coleção",
        TextKey::NewFillerPreset => "Nova predefinição de preenchimento",
        TextKey::NewMultiCollection => "Nova multi-coleção",
        TextKey::NewTemplateGroup => "Novo grupo de modelos",
        TextKey::NewTemplateName => "Novo nome do modelo",
        TextKey::Next24Hours => "Próximas 24 h",
        TextKey::NextPage => "Próxima página",
        TextKey::NoFillerPresets => "Ainda não há predefinições de preenchimento.",
        TextKey::NoBlockItems => "Nenhum item de bloco.",
        TextKey::NoItems => "Nenhum item.",
        TextKey::NoItemsToDisplay => "Nenhum item para exibir.",
        TextKey::NoAddonMediaListsYet => "Ainda não há listas add-on.",
        TextKey::No => "Não",
        TextKey::NoCollections => "Ainda não há coleções.",
        TextKey::NoMembers => "Ainda não há membros.",
        TextKey::NoMultiCollections => "Ainda não há multi-coleções.",
        TextKey::NoTemplatesFound => "Nenhum modelo encontrado.",
        TextKey::NotFound => "Página não encontrada.",
        TextKey::OtherVideo => "Outro vídeo",
        TextKey::OtherVideoNotFound => "Outro vídeo não encontrado",
        TextKey::OtherVideos => "Outros vídeos",
        TextKey::OpenBrowser => "Abrir navegador",
        TextKey::Playout => "Programação",
        TextKey::PlayoutItemsAndHistory => "Itens da programação e histórico",
        TextKey::PlayoutNotFound => "Programação não encontrada.",
        TextKey::Playouts => "Programações",
        TextKey::AddPlayout => "Adicionar programação",
        TextKey::ClassicScheduleMenu => "Grade clássica",
        TextKey::BlockScheduleMenu => "Grade em blocos",
        TextKey::SequentialScheduleMenu => "Grade sequencial",
        TextKey::ScriptedScheduleMenu => "Grade por script",
        TextKey::JsonDizqueTvScheduleMenu => "Grade JSON (dizqueTV)",
        TextKey::ResetAllPlayouts => "Reconstruir todas as programações",
        TextKey::PlayoutBuildQueued => "Reconstrução da programação enfileirada",
        TextKey::PlayoutBuildsQueued => "reconstruções de programação enfileiradas",
        TextKey::SearchForPlayouts => "Pesquisar programações",
        TextKey::PlayoutDetail => "Detalhes da programação",
        TextKey::ShowFiller => "Mostrar preenchimento",
        TextKey::EditPlayout => "Editar programação",
        TextKey::RefreshPlayout => "Atualizar programação",
        TextKey::ResetPlayout => "Reconstruir programação",
        TextKey::RebuildPlayout => "Atualizar programação",
        TextKey::PlayoutRebuildMode => "Ação",
        TextKey::ContinuePlayout => "Estender programação",
        TextKey::PlayoutContinueHelp => "Adicionar programas após a programação existente sem substituí-la.",
        TextKey::PlayoutRefreshHelp => "Reconstruir a programação futura continuando a rotação salva.",
        TextKey::PlayoutResetHelp => "Iniciar uma nova rotação de programas.",
        TextKey::KeepCurrentProgramme => "Manter o programa atual",
        TextKey::KeepCurrentProgrammeHelp => "Manter o programa completo e os programas já preparados pelos reprodutores. As alterações serão aplicadas depois.",
        TextKey::ReplaceCurrentProgrammeHelp => "O programa atual pode ser interrompido.",
        TextKey::QueuePlayoutBuild => "Enfileirar atualização",
        TextKey::InvalidPlayoutBuildMode => "Escolha uma ação válida para a programação.",
        TextKey::PlayoutContinueStateIncomplete => "O estado de continuação salvo está incompleto. A programação existente foi mantida. Escolha Atualizar ou Reconstruir mantendo o programa.",
        TextKey::PlayoutClassicTimelineRejected => "A programação contém horários de programas sobrepostos ou inválidos. Os programas publicados permanecem inalterados. Revise a programação e reconstrua-a a partir de um limite de programa verificado.",
        TextKey::ScheduleReset => "Agendar reconstrução",
        TextKey::SchedulePlayoutReset => "Agendar reconstrução da programação",
        TextKey::EditExternalJsonFile => "Editar arquivo JSON externo",
        TextKey::DeletePlayout => "Excluir programação",
        TextKey::Troubleshoot => "Solução de problemas",
        TextKey::DailyResetTime => "Horário de reconstrução diária",
        TextKey::DoNotAutomaticallyReset => "Não reconstruir automaticamente",
        TextKey::SaveChanges => "Salvar alterações",
        TextKey::ExternalJsonFileLabel => "Arquivo JSON externo",
        TextKey::EditExternalJsonFilePrompt => "Edite o arquivo JSON externo da programação",
        TextKey::UnscheduledPlayoutItem => "NÃO PROGRAMADO",
        TextKey::DisabledChannelsAlreadyHavePlayout => "Canais desativados já têm uma programação",
        TextKey::ProgramSchedule => "Grade de programação",
        TextKey::SequentialScheduleFileHelp => {
            "O caminho completo para o arquivo de grade sequencial (YAML)"
        }
        TextKey::BlockTemplatesAddedLater => "Modelos de blocos são adicionados depois",
        TextKey::UnsupportedPlayoutKind => "Tipo de programação sem suporte.",
        TextKey::ClassicPlayout => "Programação clássica",
        TextKey::ClassicPlayoutState => "Estado da programação clássica",
        TextKey::ScanRefreshState => "Estado da atualização após a varredura",
        TextKey::SubtitleCleanup => "Limpeza de legendas",
        TextKey::SubtitleCleanupMessage => "Limpeza pendente: {count} registros de legendas sem proprietário ou de legendas externas sem caminho. A limpeza automática é executada a cada hora. Os arquivos de legendas em cache são preservados.",
        TextKey::SubtitleCleanupBrief => "{count} registros de legendas aguardando limpeza",
        TextKey::SubtitleCacheRepairPendingMessage => "O reparo do cache de legendas ainda não foi concluído. Algumas legendas em cache podem não aparecer. A manutenção automática do cache tentará novamente. Se este aviso persistir, verifique o acesso de gravação ao cache de legendas e o espaço disponível em disco.",
        TextKey::SubtitleCacheRepairPendingBrief => "Reparo do cache de legendas pendente",
        TextKey::ArtworkOrphans => "Imagens órfãs",
        TextKey::ArtworkOrphansMessage => "Limpeza pendente: {actors} registros de atores e {artwork} registros de imagens sem proprietário. A limpeza automática é executada a cada hora. Para solicitá-la agora, use POST /api/maintenance/clean_artwork. Os registros vinculados e as imagens referenciadas em cache são preservados.",
        TextKey::ArtworkOrphansBrief => "{actors} registros de atores órfãos e {artwork} registros de imagens órfãs",
        TextKey::ArtworkCardinality => "Cardinalidade das imagens",
        TextKey::Av1OutputCompatibility => "A saída AV1 requer HLS nativo. Altere o mecanismo/modo do canal ou selecione H.264/HEVC no perfil FFmpeg.",
        TextKey::HlsLiveDelivery => "Entrega HLS ao vivo",
        TextKey::BlockPlayout => "Programação por blocos",
        TextKey::ScriptedPlayout => "Grade com script",
        TextKey::AlternateSchedules => "Grades alternativas",
        TextKey::EditAlternateSchedules => "Editar grades alternativas",
        TextKey::PlayoutTemplates => "Modelos de programação",
        TextKey::EditTemplates => "Editar modelos",
        TextKey::DefaultDeco => "Deco padrão",
        TextKey::EnableDefaultDeco => "Ativar deco padrão",
        TextKey::SaveDefaultDeco => "Salvar deco padrão",
        TextKey::SaveScriptedSchedule => "Salvar grade com script",
        TextKey::EraseItems => "Apagar itens",
        TextKey::ErasedPlayoutItems => "Itens da programação apagados.",
        TextKey::SavedDefaultDeco => "Deco padrão salvo.",
        TextKey::SavedScriptedSchedule => "Grade com script salva.",
        TextKey::Maintenance => "Manutenção",
        TextKey::SaveAlternateSchedules => "Salvar grades alternativas",
        TextKey::AddAlternateSchedule => "Adicionar grade alternativa",
        TextKey::SaveTemplates => "Salvar modelos",
        TextKey::PreviewCalendar => "Pré-visualizar calendário",
        TextKey::EditContent => "Editar conteúdo",
        TextKey::PlayoutNestedCatchAllHelp => {
            "Em ordem de prioridade de cima para baixo. A entrada inferior sempre corresponde a todos os dias e meses como padrão."
        }
        TextKey::PlayoutNestedPriorityHelp => "Em ordem de prioridade de cima para baixo.",
        TextKey::DaysOfWeek => "Dias da semana",
        TextKey::DaysOfMonth => "Dias do mês",
        TextKey::Months => "Meses",
        TextKey::LimitToDateRange => "Limitar ao intervalo de datas",
        TextKey::StartDay => "Dia inicial",
        TextKey::EndDay => "Dia final",
        TextKey::Enable => "Ativar",
        TextKey::Up => "Para cima",
        TextKey::Down => "Para baixo",
        TextKey::Weekdays => "Dias úteis",
        TextKey::Weekends => "Fins de semana",
        TextKey::Sunday => "Domingo",
        TextKey::Monday => "Segunda-feira",
        TextKey::Tuesday => "Terça-feira",
        TextKey::Wednesday => "Quarta-feira",
        TextKey::Thursday => "Quinta-feira",
        TextKey::Friday => "Sexta-feira",
        TextKey::Saturday => "Sábado",
        TextKey::January => "Janeiro",
        TextKey::February => "Fevereiro",
        TextKey::March => "Março",
        TextKey::April => "Abril",
        TextKey::May => "Maio",
        TextKey::June => "Junho",
        TextKey::July => "Julho",
        TextKey::August => "Agosto",
        TextKey::September => "Setembro",
        TextKey::October => "Outubro",
        TextKey::November => "Novembro",
        TextKey::December => "Dezembro",
        TextKey::JanuaryShort => "Jan",
        TextKey::FebruaryShort => "Fev",
        TextKey::MarchShort => "Mar",
        TextKey::AprilShort => "Abr",
        TextKey::MayShort => "Mai",
        TextKey::JuneShort => "Jun",
        TextKey::JulyShort => "Jul",
        TextKey::AugustShort => "Ago",
        TextKey::SeptemberShort => "Set",
        TextKey::OctoberShort => "Out",
        TextKey::NovemberShort => "Nov",
        TextKey::DecemberShort => "Dez",
        TextKey::TeletextElements => "Elementos de teletexto",
        TextKey::TeletextElementsHelp => "Páginas de teletexto próprias, por exemplo feitas no QTeletextMaker. Adicione-as a um canal pela página de teletexto do canal.",
        TextKey::NoTeletextElements => "Ainda não há elementos de teletexto",
        TextKey::NoTeletextElementsHelp => "Envie um arquivo TTI, por exemplo do QTeletextMaker.",
        TextKey::UploadTeletextPage => "Enviar página",
        TextKey::UploadTeletextPageHelp => "Você vê uma prévia logo após o envio. Nome e número da página vêm do arquivo; você pode alterá-los depois.",
        TextKey::TeletextElementAdvanced => "Avançado",
        TextKey::TeletextElementCharacterSet => "Conjunto de caracteres (designação)",
        TextKey::TeletextElementCountChannels => "{count} canais",
        TextKey::TeletextElementOneChannel => "1 canal",
        TextKey::TeletextElementCountFieldsLinked => "{count} campos vinculados",
        TextKey::TeletextElementOneFieldLinked => "1 campo vinculado",
        TextKey::TeletextElementCountFieldsToChoose => "{count} campos a configurar",
        TextKey::TeletextElementOneFieldToChoose => "1 campo a configurar",
        TextKey::TeletextElementCountSubpages => "{count} subpáginas",
        TextKey::TeletextElementOneSubpage => "1 subpágina",
        TextKey::TeletextElementDeleteConfirm => "Sim, excluir este elemento de teletexto.",
        TextKey::TeletextElementDeleteConfirmUsed => "Sim, excluir. Ele também sai de {count} canal(is).",
        TextKey::TeletextElementDeleteElement => "Excluir elemento de teletexto",
        TextKey::TeletextElementDeleted => "Elemento de teletexto excluído.",
        TextKey::TeletextElementDetails => "Detalhes",
        TextKey::TeletextElementDownloadTti => "Baixar (TTI)",
        TextKey::TeletextElementEnhancementPackets => "Pacotes de aprimoramento (X/26–X/28)",
        TextKey::TeletextElementErrorConfirmDelete => "Marque a caixa para confirmar a exclusão.",
        TextKey::TeletextElementErrorCycle => "Escolha um tempo entre 1 e 600 segundos.",
        TextKey::TeletextElementErrorDamaged => "Este arquivo não tem uma página de teletexto legível.",
        TextKey::TeletextElementErrorDuplicateName => "Outro elemento de teletexto já tem este nome.",
        TextKey::TeletextElementErrorEmpty => "Escolha um arquivo para enviar.",
        TextKey::TeletextElementErrorFields => "Não foi possível salvar estas escolhas de campos.",
        TextKey::TeletextElementErrorFormat => "Use um arquivo TTI, T42, EP1 ou HTT.",
        TextKey::TeletextElementErrorName => "Informe um nome com até 100 caracteres.",
        TextKey::TeletextElementErrorPageNumber => "Informe um número de página de 100 a 899.",
        TextKey::TeletextElementErrorStale => "Outra pessoa alterou este elemento. Confira a página e tente de novo.",
        TextKey::TeletextElementErrorTooLarge => "O arquivo é maior que 256 KB.",
        TextKey::TeletextElementField => "Campo",
        TextKey::TeletextElementFieldChooseData => "A escolher",
        TextKey::TeletextElementFieldData => "Dados",
        TextKey::TeletextElementFieldLinks => "Vínculos de dados",
        TextKey::TeletextElementFieldLinksHelp => "Escolha quais dados o ErsatzRS escreve em cada campo.",
        TextKey::TeletextElementFieldMarkerTip => "No editor, digite um sinal de porcentagem, um nome e mais sinais onde os dados devem ficar, por exemplo %NOW%%%%%%. A sequência inteira é a largura do campo. Depois do envio você escolhe os dados de cada campo.",
        TextKey::TeletextElementFieldMarkerTipTitle => "Dica: lugares para dados dinâmicos",
        TextKey::TeletextElementFieldPosition => "subpágina {subpage}, linha {row}, coluna {column}, {length} caracteres",
        TextKey::TeletextElementFieldsCutHelp => "Texto longo demais é cortado na largura do campo, para o layout da página não mudar.",
        TextKey::TeletextElementFile => "Arquivo",
        TextKey::TeletextElementFileFormatsHelp => "TTI, T42, EP1 ou HTT · até 256 KB · uma página, com ou sem subpáginas",
        TextKey::TeletextElementFileHelp => "Para mudar o design, baixe-o, edite no QTeletextMaker e envie a nova versão.",
        TextKey::TeletextElementFileName => "Nome do arquivo",
        TextKey::TeletextElementHeaderRowHelp => "O canal preenche a linha de cabeçalho com seu número de página e relógio.",
        TextKey::TeletextElementLevel1PreviewNote => "A prévia mostra a página de nível 1. Aprimoramentos (X/26–X/28) ficam no arquivo, mas não são desenhados aqui.",
        TextKey::TeletextElementLinked => "Ligado",
        TextKey::TeletextElementNewVersion => "Nova versão",
        TextKey::TeletextElementNewVersionHelp => "As escolhas de campos ficam se os mesmos campos ainda estiverem na página.",
        TextKey::TeletextElementNoFields => "Nenhum campo nesta página",
        TextKey::TeletextElementNotFound => "Este elemento de teletexto não existe (mais).",
        TextKey::TeletextElementNotUsed => "Não usado",
        TextKey::TeletextElementOff => "Desligado",
        TextKey::TeletextElementPageNumber => "Página",
        TextKey::TeletextElementPreviewAsDesigned => "Como projetada (sem dados)",
        TextKey::TeletextElementPreviewWith => "Prévia com dados de",
        TextKey::TeletextElementRevision => "Revisão",
        TextKey::TeletextElementSaveDetails => "Salvar detalhes",
        TextKey::TeletextElementSaveFieldLinks => "Salvar vínculos",
        TextKey::TeletextElementSavedDetails => "Detalhes salvos.",
        TextKey::TeletextElementSavedFields => "Vínculos salvos.",
        TextKey::TeletextElementSavedReplaced => "Nova versão enviada.",
        TextKey::TeletextElementSavedUploaded => "Página enviada. Confira a prévia e escolha os dados dos campos.",
        TextKey::TeletextElementShowPreview => "Mostrar prévia",
        TextKey::TeletextElementSourceChannelName => "Nome do canal",
        TextKey::TeletextElementSourceChannelNumber => "Número do canal",
        TextKey::TeletextElementSourceDate => "Data",
        TextKey::TeletextElementSourceFormat => "Formato enviado",
        TextKey::TeletextElementSourceNextStart => "A seguir: horário de início",
        TextKey::TeletextElementSourceNextTitle => "A seguir: título",
        TextKey::TeletextElementSourceNowStart => "Agora: horário de início",
        TextKey::TeletextElementSourceNowTitle => "Agora: título",
        TextKey::TeletextElementSourceStatic => "Texto fixo",
        TextKey::TeletextElementSourceTime => "Hora",
        TextKey::TeletextElementSourceUnset => "— Manter como projetado —",
        TextKey::TeletextElementStaticTextPlaceholder => "Texto fixo",
        TextKey::TeletextElementStepPutOnChannel => "Colocar em um canal",
        TextKey::TeletextElementStepUploaded => "Enviado",
        TextKey::TeletextElementSteps => "Etapas",
        TextKey::TeletextElementSubpage => "subpágina",
        TextKey::TeletextElementSubpageCycle => "Tempo das subpáginas (segundos)",
        TextKey::TeletextElementSubpageCycleHelp => "Quanto tempo cada subpágina fica antes da próxima.",
        TextKey::TeletextElementSuggestedPageNumber => "Número de página sugerido",
        TextKey::TeletextElementSuggestedPageNumberHelp => "Um canal pode escolher outro número.",
        TextKey::TeletextElementUpdatedAt => "Última alteração",
        TextKey::TeletextElementUploadNewVersion => "Enviar nova versão",
        TextKey::TeletextElementUsageEmpty => "Ainda não está em nenhum canal. Adicione pela página de teletexto de um canal.",
        TextKey::TeletextElementUsedOn => "Usado em",
        TextKey::TeletextElementUsedOnHelp => "Canais que mostram esta página. O vínculo é feito na página de teletexto do canal.",
        TextKey::TeletextElementWarningEnhancements => "Os dados de aprimoramento deste formato não foram importados; a página de nível 1 está completa.",
        TextKey::TeletextElementWarningExtraPages => "O arquivo tinha mais de uma página; só a primeira foi mantida.",
        TextKey::TeletextElementWarningLanguage => "O código de idioma do arquivo é desconhecido; o conjunto inglês é usado.",
        TextKey::TeletextElementWarningSkippedRecords => "Algumas linhas ilegíveis do arquivo foram ignoradas.",
        TextKey::TeletextElementWarningSubpages => "Só as primeiras 64 subpáginas foram mantidas.",
        TextKey::PlayAll => "Reproduzir todos",
        TextKey::Playlist => "Playlist",
        TextKey::PlaylistAdded => "Playlist adicionada.",
        TextKey::PlaylistDeleted => "Playlist excluída.",
        TextKey::PlaylistGroup => "Grupo de playlists",
        TextKey::PlaylistGroupAdded => "Grupo de playlists adicionado.",
        TextKey::PlaylistGroupDeleted => "Grupo de playlists excluído.",
        TextKey::PlaylistGroupName => "Nome do grupo de playlists",
        TextKey::PlaylistGroups => "Grupos de playlists",
        TextKey::PlaylistItem => "Item de playlist",
        TextKey::PlaylistItems => "Itens de playlist",
        TextKey::PlaylistName => "Nome da playlist",
        TextKey::PlaylistNotFound => "Playlist não encontrada.",
        TextKey::PlaylistPlayoutPreview => "Prévia da programação da playlist",
        TextKey::PlaylistPreviewEmpty => "A prévia da programação da playlist não tem itens",
        TextKey::Playlists => "Playlists",
        TextKey::ProbeForInterlacedFrames => "Procurar quadros entrelaçados",
        TextKey::PreferredAudioLanguage => "Idioma de áudio preferido",
        TextKey::DefaultSubtitleLanguage => "Idioma de legenda padrão",
        TextKey::FallbackSubtitleLanguage => "Idioma de legenda alternativo",
        TextKey::OriginalLanguageOption => "Original (und)",
        TextKey::UndefinedLanguageOption => "Indefinido (und)",
        TextKey::Previous24Hours => "24 h anteriores",
        TextKey::PreviousPage => "Página anterior",
        TextKey::RerunCollections => "Coleções de reprises",
        TextKey::RerunCollection => "Coleção de reprises",
        TextKey::RerunCollectionActionFailed => "A ação da coleção de reprises falhou.",
        TextKey::RerunCollectionDeleted => "Coleção de reprises excluída.",
        TextKey::RerunCollectionNameRequired => "Nome da coleção de reprises é obrigatório!",
        TextKey::RerunCollectionSaved => "Coleção de reprises salva.",
        TextKey::RerunPlaybackOrder => "Ordem de reprodução das reprises",
        TextKey::RemoteStreams => "Streams remotos",
        TextKey::RemoteStream => "Stream remoto",
        TextKey::PublishedOptions => "Opções publicadas",
        TextKey::PlaybackHealth => "Status de reprodução",
        TextKey::Configure => "Configurar",
        TextKey::Watch => "Assistir",
        TextKey::RemoteStreamPlaylists => "Playlists de streams remotos",
        TextKey::PlaylistUrl => "URL da playlist",
        TextKey::Extractor => "Extrator",
        TextKey::DestinationFolder => "Pasta de destino",
        TextKey::SyncInterval => "Intervalo de sincronização",
        TextKey::MaxItems => "Máximo de itens",
        TextKey::RemovedEntries => "Itens removidos",
        TextKey::CreateOrderedPlaylist => "Criar uma playlist ordenada",
        TextKey::SyncNow => "Sincronizar agora",
        TextKey::LastSync => "Última sincronização",
        TextKey::NeverSynced => "Nunca sincronizado",
        TextKey::ManagedManifestReadOnly => {
            "Este manifesto criado manualmente é somente leitura aqui."
        }
        TextKey::Truncated => "Truncado",
        TextKey::MissingDuration => "Duração ausente",
        TextKey::Orphaned => "Órfão",
        TextKey::Materialized => "Materializado",
        TextKey::State => "Estado",
        TextKey::Upstream => "Ordem da origem",
        TextKey::NewestFirst => "Mais recentes primeiro",
        TextKey::Keep => "Manter",
        TextKey::ResetChannels => "Redefinir canais",
        TextKey::RowsPerPage => "Linhas por página",
        TextKey::Save => "Salvar",
        TextKey::SaveMultiCollection => "Salvar multi-coleção",
        TextKey::SavePlaylist => "Salvar playlist",
        TextKey::SaveTroubleshootingReportsToDisk => {
            "Salvar relatórios de solução de problemas em disco"
        }
        TextKey::SaveAsSmartCollection => "Salvar como coleção inteligente",
        TextKey::SearchQueryRequired => "A consulta de pesquisa é obrigatória",
        TextKey::SaveSettings => "Salvar configurações",
        TextKey::Scanner => "Scanner",
        TextKey::ScanningMinimumLogLevel => "Nível mínimo de log da varredura",
        TextKey::Scheduling => "Agendamento",
        TextKey::SchedulingMinimumLogLevel => "Nível mínimo de log do agendamento",
        TextKey::StartWithWindows => "Iniciar o ErsatzRS ao entrar no Windows",
        TextKey::StartWithWindowsHelp => {
            "Inicia automaticamente o aplicativo da bandeja para o usuário atual do Windows."
        }
        TextKey::Schedules => "Horários",
        TextKey::AddSchedule => "Adicionar horário",
        TextKey::AddScheduleItem => "Adicionar item do horário",
        TextKey::AddSmartCollection => "Adicionar coleção inteligente",
        TextKey::Album => "Álbum",
        TextKey::Chronological => "Cronológico",
        TextKey::CollectionSize => "Tamanho da coleção",
        TextKey::CollectionType => "Tipo de coleção",
        TextKey::Count => "Contagem",
        TextKey::CopySchedule => "Copiar horário",
        TextKey::CopyScheduleItem => "Copiar item do horário",
        TextKey::CustomTitle => "Título personalizado",
        TextKey::DeleteSchedule => "Excluir horário",
        TextKey::DeleteScheduleItem => "Excluir item do horário",
        TextKey::DeleteSmartCollection => "Excluir coleção inteligente",
        TextKey::DetailRowsPerPage => "Linhas de detalhes por página",
        TextKey::Director => "Diretor",
        TextKey::Directors => "Diretores",
        TextKey::DiscardToFillAttempts => "Tentativas de descartar para preencher",
        TextKey::Duration => "Duração",
        TextKey::Dynamic => "Dinâmico",
        TextKey::EditProperties => "Editar propriedades",
        TextKey::EditScheduleItems => "Editar itens do horário",
        TextKey::EditSmartCollection => "Editar coleção inteligente",
        TextKey::EnterNameForNewSchedule => "Digite um nome para o novo horário",
        TextKey::Fixed => "Fixo",
        TextKey::FixedStartTimeBehavior => "Comportamento de hora fixa de início",
        TextKey::Flood => "Flood",
        TextKey::Flexible => "Flexível",
        TextKey::FlexibleStartSoonAsPossible => "Flexível - iniciar assim que possível",
        TextKey::FlexibleStartSoonAsPossibleAfterStartTime => {
            "Flexível - iniciar assim que possível após a hora de início"
        }
        TextKey::FillWithGroupMode => "Modo de preencher com grupos (série ou artista)",
        TextKey::GraphicsElements => "Elementos gráficos",
        TextKey::AddGraphicsElement => "Adicionar elemento gráfico",
        TextKey::DrawtextRendererOption => "Drawtext — dinâmico/baseado em arquivo",
        TextKey::GraphicsMetadataRefreshSeconds => "Atualizar metadados do programa (segundos)",
        TextKey::GraphicsMetadataRefreshHelp => "0 mantém os metadados do início. 5–60 recarrega os detalhes do programa e as variáveis do item. O relógio Drawtext e a recarga do arquivo FFmpeg são independentes; drawvg usa os dados do início.",
        TextKey::GraphicsMetadataRefreshBounds => "Use 0 para manter os dados do início ou 5–60 segundos para atualizar os metadados.",
        TextKey::ProgrammeTemplateHelp => "CurrentProgramme, NextProgramme, ContinuingProgramme e LaterToday identificam programas completos. Oculte o texto indisponível com {{ NextProgramme.Title | default(\"\", true) }}. Epg permanece disponível.",
        TextKey::GraphicsItemVariables => "Variáveis deste item (JSON)",
        TextKey::GraphicsItemVariablesHelp => "As substituições se aplicam aos gráficos selecionados deste item agendado. Um objeto vazio usa os metadados nativos do programa. O salvamento adiado vale para os novos itens gerados.",
        TextKey::GraphicsPreviewAmbiguous => "Vários itens programados se sobrepõem nesse horário. Não é possível escolher um programa com segurança.",
        TextKey::GraphicsContextPreview => "Prévia do texto do programa",
        TextKey::GraphicsContextPreviewHelp => "Resolver o rascunho com o canal escolhido, o item realmente programado e suas variáveis. Esta prévia textual não mostra métricas da fonte nem posicionamento.",
        TextKey::GraphicsPreviewTimeHelp => "Insira um horário ISO com diferença de fuso, por exemplo 2026-10-05T21:30:00+02:00. Horários históricos usam a programação salva.",
        TextKey::GraphicsPreviewSavedHelp => "Sem JavaScript, a prévia usa o texto salvo. Salve o rascunho antes de visualizá-lo.",
        TextKey::GraphicsItemVariablesInvalid => "Use objetos JSON para os elementos gráficos selecionados, com no máximo 64 KiB por elemento.",
        TextKey::DrawtextSettings => "Configurações do Drawtext",
        TextKey::DrawvgRendererOption => "Drawvg — estático/vetorial",
        TextKey::EditGraphicsElement => "Editar elemento gráfico",
        TextKey::FontFile => "Arquivo de fonte",
        TextKey::FontFileOptionalHelp => "Opcional. Deixe vazio para usar a fonte Roboto incluída.",
        TextKey::GraphicsElement => "Elemento gráfico",
        TextKey::GraphicsElementConfigInvalid => {
            "Este elemento gráfico de texto tem uma configuração inválida e não foi alterado."
        }
        TextKey::GraphicsElementKindImage => "Imagem",
        TextKey::GraphicsElementKindMotion => "Movimento",
        TextKey::GraphicsElementKindScript => "Script",
        TextKey::GraphicsElementKindSubtitle => "Legenda",
        TextKey::GraphicsElementKindText => "Texto",
        TextKey::GraphicsElementNameTooLong => "O nome deve ter no máximo 50 caracteres.",
        TextKey::GraphicsElementNotEditable => "Não editável aqui",
        TextKey::GraphicsElementNotFound => "Elemento gráfico não encontrado.",
        TextKey::GraphicsElementNotText => {
            "Somente elementos gráficos de texto podem ser editados aqui."
        }
        TextKey::GraphicsElementSaved => "Elemento gráfico salvo.",
        TextKey::GraphicsElementSaveFailed => "Não foi possível salvar o elemento gráfico.",
        TextKey::GraphicsElementType => "Tipo",
        TextKey::GraphicsExistingDrawvgHelp => {
            "Elementos existentes e novos usam Drawvg por padrão."
        }
        TextKey::GraphicsStreamRestartHelp => {
            "As alterações afetam novos streams e não reiniciam streams ativos."
        }
        TextKey::NoGraphicsElements => "Nenhum elemento gráfico encontrado.",
        TextKey::RefreshPeriodBounds => {
            "O período de atualização deve estar entre 250 e 60000 milissegundos."
        }
        TextKey::RefreshPeriodMilliseconds => "Período de atualização (milissegundos)",
        TextKey::ReloadFrameInterval => "Intervalo de quadros para recarga",
        TextKey::ReloadFrameIntervalBounds => {
            "O intervalo de quadros para recarga deve ser pelo menos 1."
        }
        TextKey::Renderer => "Renderizador",
        TextKey::SaveGraphicsElement => "Salvar elemento gráfico",
        TextKey::TextTemplate => "Modelo de texto",
        TextKey::TextTemplateRequired => "O modelo de texto é obrigatório.",
        TextKey::TextTemplateTooLong => "O modelo deve ser texto UTF-8 válido de até 64 KiB.",
        TextKey::GraphicsIds => "IDs de gráficos",
        TextKey::Genres => "Gêneros",
        TextKey::GroupName => "Nome do grupo",
        TextKey::GroupMultiPartEpisodesHelp => "Agrupa episódios em várias partes ao embaralhar",
        TextKey::GuideMode => "Modo do guia",
        TextKey::KeepMultiPartEpisodesTogether => "Manter episódios em várias partes juntos",
        TextKey::Marathon => "Maratona",
        TextKey::MarathonBatchSize => "Tamanho do lote da maratona",
        TextKey::MarathonBatchSizeHelp => {
            "Quantos itens tocar de cada grupo antes de avançar; vazio ou zero tocará todos os itens"
        }
        TextKey::MarathonGroupBy => "Agrupar maratona por",
        TextKey::MarathonShuffleGroups => "Embaralhar grupos da maratona",
        TextKey::MarathonShuffleItems => "Embaralhar itens da maratona",
        TextKey::EditFillerPreset => "Editar predefinição de preenchimento",
        TextKey::MidRollFiller => "Preenchimento mid-roll",
        TextKey::MidRoll => "Mid-roll",
        TextKey::MoveDown => "Mover para baixo",
        TextKey::MoveUp => "Mover para cima",
        TextKey::ReorderPlaylistItem => "Arraste para reordenar o item da playlist",
        TextKey::MultiEpisodeGroupSize => "Tamanho do grupo de múltiplos episódios",
        TextKey::MultiEpisodeShuffle => "Embaralhamento de múltiplos episódios",
        TextKey::Multiple => "Múltiplos",
        TextKey::MultipleCount => "Contagem de múltiplos",
        TextKey::MultipleCountHelp => {
            "Pode ser uma expressão com 'count' (tamanho da coleção) e 'random' (0 até count-1)"
        }
        TextKey::MultipleMode => "Modo de múltiplos",
        TextKey::NewScheduleName => "Novo nome do horário",
        TextKey::NewSmartCollection => "Nova coleção inteligente",
        TextKey::NoScheduleItems => "Nenhum item de horário.",
        TextKey::NoSchedules => "Nenhum horário.",
        TextKey::Normal => "Normal",
        TextKey::Offline => "Offline",
        TextKey::One => "Um",
        TextKey::OrderedGroups => "Grupos ordenados",
        TextKey::Playback => "Reprodução",
        TextKey::PlaybackOrder => "Ordem de reprodução",
        TextKey::PlaybackOrderUpdateFailed => "Falha ao atualizar a ordem de reprodução",
        TextKey::PlaybackOrderUpdated => "Ordem de reprodução atualizada.",
        TextKey::PlaylistItemSize => "Tamanho do item da playlist",
        TextKey::PlayoutDurationHours => "Duração da programação em horas",
        TextKey::PlayoutDurationMinutes => "Duração da programação em minutos",
        TextKey::PostRollFiller => "Preenchimento pós-roll",
        TextKey::PostRoll => "Pós-roll",
        TextKey::PreRollFiller => "Preenchimento pré-roll",
        TextKey::PreRoll => "Pré-roll",
        TextKey::Random => "Aleatório",
        TextKey::RandomCount => "Contagem aleatória",
        TextKey::RandomRotation => "Rotação aleatória",
        TextKey::RemainingDurationHelp => {
            "duração do conteúdo após este ponto mid-roll, em segundos"
        }
        TextKey::RandomStartPoint => "Ponto inicial aleatório",
        TextKey::RemoveFailed => "Falha ao remover",
        TextKey::RemovedFromCollection => "Removido da coleção.",
        TextKey::RemoveFromCollection => "Remover da coleção",
        TextKey::Rename => "Renomear",
        TextKey::RenameFailed => "Falha ao renomear",
        TextKey::ReorderFailed => "Falha ao reordenar",
        TextKey::RerunFirstRun => "Reexibição - primeira exibição",
        TextKey::RerunRerun => "Reexibição - reprise",
        TextKey::ReadOnlyTwentyFourHourPlayoutView => {
            "Visualização somente leitura de 24 horas dos itens de programação agendados."
        }
        TextKey::SaveSchedule => "Salvar horário",
        TextKey::SaveScheduleItems => "Salvar itens do horário",
        TextKey::ScheduleSaveDeferred => "Salvar para a próxima extensão",
        TextKey::ScheduleSaveAndRefresh => "Salvar e atualizar",
        TextKey::ScheduleDeferredHelp => "Salvar para a próxima extensão mantém os programas publicados e a rotação. Permite alterações de preenchimento, gráficos, guia e faixas. Mudanças de fontes, ordem ou estrutura exigem Salvar e atualizar.",
        TextKey::ScheduleDeferredPending => "Aguardando a próxima extensão",
        TextKey::ScheduleDeferredApplied => "Aplicado a partir de",
        TextKey::ScheduleDeferredBoundary => "Já programado até",
        TextKey::ScheduleDeferredLastBuild => "Última geração",
        TextKey::ScheduleDeferredBuildSucceeded => "Concluída",
        TextKey::ScheduleDeferredSaved => "Salvo; os programas publicados permanecem inalterados.",
        TextKey::ScheduleDeferredIncompatible => "O salvamento adiado não pode alterar fontes, ordem de reprodução ou estrutura. Use Salvar e atualizar para essas alterações.",
        TextKey::ScheduleDeferredBusy => "Há uma geração de programação em andamento. Aguarde o término e tente novamente.",
        TextKey::ScheduleDeferredInvalidTimeline => "A programação publicada contém horários inválidos. Corrija-a antes de salvar uma alteração adiada.",
        TextKey::SaveSmartCollection => "Salvar coleção inteligente",
        TextKey::Schedule => "Horário",
        TextKey::ScheduleActionFailed => "A ação do horário falhou.",
        TextKey::ScheduleAsGroup => "Agendar como grupo",
        TextKey::ScheduleKind => "Tipo de horário",
        TextKey::ScheduleKindBlock => "Bloco",
        TextKey::ScheduleKindClassic => "Clássico",
        TextKey::ScheduleKindExternalJson => "JSON (dizqueTV)",
        TextKey::ScheduleKindScripted => "Por script",
        TextKey::ScheduleKindSequential => "Sequencial",
        TextKey::SchedulingContext => "Contexto do agendamento",
        TextKey::ScheduleItem => "Item do horário",
        TextKey::ScheduleItemActionFailed => "A ação do item do horário falhou.",
        TextKey::ScheduleItemFiller => "Preenchimento do item do horário",
        TextKey::ScheduleItemFillerHelp => {
            "O preenchimento pré-roll, mid-roll e pós-roll se aplica a cada mídia selecionada por este item do horário. Para reproduzir um conteúdo uma vez após um bloco de múltiplos, adicione-o como o próximo item do horário."
        }
        TextKey::ScheduleItemOverrides => "Substituições do item do horário",
        TextKey::ScheduleItems => "Itens do horário",
        TextKey::ScheduleNotFound => "Horário não encontrado.",
        TextKey::ScheduleSaveFailed => "Falha ao salvar o horário.",
        TextKey::SmartShuffle => "Ordem aleatória inteligente",
        TextKey::SmartShufflePreset => "Predefinição de intervalo",
        TextKey::Balanced => "Equilibrado",
        TextKey::SmartShuffleScope => "Escopo do intervalo",
        TextKey::SmartShuffleScopePlayout => "Programação",
        TextKey::SmartShuffleScopeScheduleItem => "Item do horário",
        TextKey::SmartShuffleValidation => {
            "Escolha um limite de espaçamento e pelo menos uma regra de similaridade."
        }
        TextKey::SmartShuffleScopeContentSource => "Fonte de conteúdo",
        TextKey::SmartShuffleAfterItems => "Vídeos entre itens semelhantes",
        TextKey::SmartShuffleAfterDurationMinutes => "Duração do intervalo (minutos)",
        TextKey::SmartShuffleIncludeFiller => "Incluir preenchimento",
        TextKey::SmartShuffleInheritedBlock => {
            "Este item não tem substituição de Smart Shuffle e mantém qualquer política de programação legada."
        }
        TextKey::SmartShuffleInactive => {
            "A ordem aleatória inteligente está inativa porque esta ordem de reprodução não é aleatória."
        }
        TextKey::SmartShuffleOverrideBlock => "Substituir para este item do bloco",
        TextKey::SimilarityRules => "Regras de similaridade",
        TextKey::AlbumArtist => "Artista do álbum",
        TextKey::Apply => "Aplicar",
        TextKey::ContentParent => "Item pai ou variante",
        TextKey::SmartShuffleFilterRules => "Regras de filtro",
        TextKey::SearchForSchedules => "Pesquisar horários",
        TextKey::SearchForManualCollections => "Pesquisar coleções manuais",
        TextKey::SearchForRerunCollections => "Pesquisar coleções de reprises",
        TextKey::SearchForSmartCollections => "Pesquisar coleções inteligentes",
        TextKey::SearchForTemplates => "Pesquisar modelos",
        TextKey::SearchForBlocks => "Pesquisar blocos",
        TextKey::SearchForMultiCollections => "Pesquisar multi-coleções",
        TextKey::SearchQuery => "Consulta de pesquisa",
        TextKey::BlockSearchQueryPlaceholder => "ex.: genre:action",
        TextKey::SearchTitle => "Título da pesquisa",
        TextKey::SearchTitleDisplay => "Título da pesquisa (exibição)",
        TextKey::SearchSmartCollection => "Pesquisar coleção inteligente",
        TextKey::Season => "Temporada",
        TextKey::SeasonEpisode => "Temporada, episódio",
        TextKey::Unnumbered => "Sem numeração",
        TextKey::SeasonNotFound => "Temporada não encontrada",
        TextKey::Select => "Selecionar",
        TextKey::SelectCollection => "Selecionar coleção",
        TextKey::SelectGroupForNewTemplate => "Selecione um grupo para o novo modelo",
        TextKey::SelectScheduleItemPrompt => {
            "Selecione um item do horário para editar ou adicione um novo item."
        }
        TextKey::SelectSchedule => "Selecione um horário",
        TextKey::SelectSmartCollection => "Selecionar coleção inteligente",
        TextKey::Shuffle => "Embaralhar",
        TextKey::ShuffleInOrder => "Embaralhar em ordem",
        TextKey::ShuffleScheduleItems => "Embaralhar itens do horário",
        TextKey::ShuffleScheduleItemsHelp => {
            "Observação: isso desativa horários fixos de início e o modo flood"
        }
        TextKey::ShuffledGroups => "Grupos embaralhados",
        TextKey::Show => "Série",
        TextKey::ShowNotFound => "Série não encontrada",
        TextKey::Shows => "Séries",
        TextKey::Start => "Início",
        TextKey::StartTime => "Hora de início",
        TextKey::StartTimeOnOrAfter => "Hora de início em ou após",
        TextKey::StartType => "Tipo de início",
        TextKey::Strict => "Estrito",
        TextKey::StrictAlwaysWaitExactStartTime => {
            "Estrito - sempre aguardar a hora exata de início"
        }
        TextKey::StrictWaitExactStartTime => "Estrito - aguardar a hora exata de início",
        TextKey::TailFiller => "Preenchimento final",
        TextKey::TailMode => "Modo final",
        TextKey::Tail => "Final",
        TextKey::TelevisionSeason => "Temporada de TV",
        TextKey::TelevisionShow => "Série de TV",
        TextKey::TreatCollectionsAsShows => "Tratar coleções como séries",
        TextKey::TreatCollectionsAsShowsHelp => "Útil para episódios crossover em várias partes",
        TextKey::Type => "Tipo",
        TextKey::TypeToSearch => "Digite para pesquisar...",
        TextKey::Search => "Pesquisar",
        TextKey::Filters => "Filtros",
        TextKey::ActiveFilters => "filtros ativos",
        TextKey::ClearFilters => "Limpar filtros",
        TextKey::CreateSmartCollection => "Criar coleção inteligente",
        TextKey::FromYear => "A partir do ano",
        TextKey::ToYear => "Até o ano",
        TextKey::CollectionComposer => "Editor de coleção inteligente",
        TextKey::CollectionDetails => "Detalhes da coleção",
        TextKey::Rules => "Regras",
        TextKey::Overview => "Visão geral",
        TextKey::MatchAll => "Corresponder a todas",
        TextKey::MatchAllHelp => "Todas as regras deste grupo devem corresponder.",
        TextKey::MatchAny => "Corresponder a qualquer uma",
        TextKey::MatchAnyHelp => "Pelo menos uma regra deste grupo deve corresponder.",
        TextKey::ExcludeAny => "Excluir correspondências",
        TextKey::ExcludeAnyHelp => {
            "Itens que correspondem a qualquer regra deste grupo são excluídos."
        }
        TextKey::AddRule => "Adicionar regra",
        TextKey::EditRule => "Editar regra",
        TextKey::RemoveRule => "Remover regra",
        TextKey::RuleField => "Campo",
        TextKey::Genre => "Gênero",
        TextKey::Studio => "Estúdio",
        TextKey::Network => "Rede",
        TextKey::Actor => "Ator",
        TextKey::Writer => "Roteirista",
        TextKey::SubtitleLanguage => "Idioma da legenda",
        TextKey::ShowTitle => "Título da série",
        TextKey::ContentKindLabel => "Tipo de conteúdo",
        TextKey::SubKindLabel => "Subtipo",
        TextKey::HasCommentary => "Com comentários",
        TextKey::AllFields => "Todos os campos",
        TextKey::MatchType => "Tipo de correspondência",
        TextKey::MatchValue => "Valor",
        TextKey::TextContains => "O texto contém",
        TextKey::ExactPhrase => "Frase exata",
        TextKey::Wildcard => "Curinga",
        TextKey::Fuzzy => "Aproximada",
        TextKey::Range => "Intervalo",
        TextKey::RangeStart => "Início do intervalo",
        TextKey::RangeEnd => "Fim do intervalo",
        TextKey::NestedGroup => "Grupo aninhado",
        TextKey::NestedGroupHelp => {
            "Edite este grupo no modo de foco para mostrar um nível booleano por vez."
        }
        TextKey::AddNestedGroup => "Adicionar grupo aninhado",
        TextKey::EditNestedGroup => "Editar grupo aninhado",
        TextKey::BackToRules => "Voltar às regras",
        TextKey::BackToSearch => "Voltar à pesquisa",
        TextKey::CanonicalQuery => "Consulta canônica",
        TextKey::CanonicalQueryHelp => {
            "Esta consulta é usada pela pesquisa, pelo agendamento e pela coleção salva."
        }
        TextKey::LivePreview => "Prévia ao vivo",
        TextKey::MatchingItems => "itens correspondentes",
        TextKey::QueryValid => "Consulta válida",
        TextKey::QueryInvalid => "A consulta requer atenção",
        TextKey::SearchRuleValueRequired => "Toda regra de pesquisa precisa de um valor.",
        TextKey::SearchGroupRequired => "Adicione pelo menos uma regra a este grupo.",
        TextKey::SearchWildcardRequired => "Uma regra curinga deve conter * ou ?.",
        TextKey::SavedSearches => "Pesquisas salvas",
        TextKey::SavedSearchName => "Nome da pesquisa salva",
        TextKey::SavedSearchHelp => {
            "Salve esta pesquisa complexa para recuperá-la e modificá-la depois."
        }
        TextKey::ChooseSavedSearch => "Escolher uma pesquisa salva",
        TextKey::LoadSavedSearch => "Carregar pesquisa salva",
        TextKey::SaveSavedSearch => "Salvar pesquisa",
        TextKey::UpdateSavedSearch => "Atualizar pesquisa salva",
        TextKey::NewSavedSearch => "Nova pesquisa salva",
        TextKey::DeleteSavedSearch => "Excluir pesquisa salva",
        TextKey::SavedSearchSaved => "Pesquisa salva.",
        TextKey::SavedSearchDeleted => "Pesquisa salva excluída.",
        TextKey::SavedSearchLoaded => "Pesquisa salva carregada.",
        TextKey::SavedSearchNameRequired => "O nome da pesquisa salva é obrigatório.",
        TextKey::SavedSearchNameTooLong => "O nome deve ter no máximo 80 caracteres.",
        TextKey::SavedSearchNameUnique => "O nome da pesquisa salva deve ser único.",
        TextKey::NoSavedSearches => "Ainda não há pesquisas salvas.",
        TextKey::RefreshPreview => "Atualizar prévia",
        TextKey::NoRules => "Não há regras neste grupo.",
        TextKey::SearchFiltersHelp => {
            "Somente filtros comuns. Use o editor da coleção para lógica aninhada."
        }
        TextKey::SearchMedia => "Pesquisar mídia",
        TextKey::SearchMoviesOrShows => "Pesquisar filmes ou séries...",
        TextKey::SearchAddonMediaList => "Pesquisar lista add-on",
        TextKey::SearchingMinimumLogLevel => "Nível mínimo de log da pesquisa",
        TextKey::Settings => "Definições",
        TextKey::SettingsSaveFailed => "Falha ao salvar definições",
        TextKey::ShowMediaInfo => "Mostrar informações da mídia",
        TextKey::ScriptedScheduleTimeout => "Tempo limite de agenda por script",
        TextKey::SkipMissingItems => "Ignorar itens ausentes",
        TextKey::SkipMissingItemsHelp => {
            "Controla se itens com arquivo não encontrado ou indisponíveis devem ser incluídos nas programações"
        }
        TextKey::SmartCollections => "Coleções inteligentes",
        TextKey::SmartCollection => "Coleção inteligente",
        TextKey::SmartCollectionDeleted => "Coleção inteligente excluída.",
        TextKey::SmartCollectionQueryPlaceholder => "ex.: genre:action OR genre:comedy",
        TextKey::SmartCollectionSaved => "Coleção inteligente salva.",
        TextKey::SaveAddonMediaList => "Salvar lista add-on",
        TextKey::Songs => "Músicas",
        TextKey::SplitTimeEvenly => "Dividir tempo uniformemente",
        TextKey::StreamingMinimumLogLevel => "Nível mínimo de log do streaming",
        TextKey::Studios => "Estúdios",
        TextKey::Support => "Suporte",
        TextKey::Template => "Modelo",
        TextKey::TemplateCopied => "Modelo copiado.",
        TextKey::TemplateCreated => "Modelo criado.",
        TextKey::TemplateDeleted => "Modelo excluído.",
        TextKey::TemplateGroup => "Grupo de modelos",
        TextKey::TemplateGroupCreated => "Grupo de modelos criado.",
        TextKey::TemplateGroupDeleted => "Grupo de modelos excluído.",
        TextKey::TemplateGroupName => "Nome do grupo de modelos",
        TextKey::TemplateGroups => "Grupos de modelos",
        TextKey::TemplateName => "Nome do modelo",
        TextKey::TemplateNotFound => "Modelo não encontrado.",
        TextKey::Templates => "Modelos",
        TextKey::Theme => "Tema",
        TextKey::ThisActionCannotBeUndone => "Esta ação não pode ser desfeita.",
        TextKey::ThisCollectionHasNoItemsYet => "Esta coleção ainda não tem itens.",
        TextKey::Thumbnail => "miniatura",
        TextKey::Title => "Título",
        TextKey::ToggleNavigation => "Alternar navegação",
        TextKey::TotalDurationHelp => "duração total do conteúdo, em segundos",
        TextKey::TotalPointsHelp => "número total de pontos mid-roll possíveis",
        TextKey::TotalProgressHelp => "posição normalizada de 0 a 1",
        TextKey::AddonMediaList => "Lista add-on",
        TextKey::AddonMediaListActionFailed => "Falha na ação da lista add-on.",
        TextKey::AddonMediaListAdded => "Lista add-on adicionada.",
        TextKey::AddonMediaListDeleted => "Lista add-on excluída.",
        TextKey::AddonMediaListMatched => "Lista add-on combinada.",
        TextKey::AddonMediaListNotFound => "Lista add-on não encontrada.",
        TextKey::AddonMediaListSaveFailed => "Falha ao salvar lista add-on.",
        TextKey::AddonMediaListSaved => "Lista add-on salva.",
        TextKey::AddonMediaLists => "Listas add-on",
        TextKey::AddonToolMaintenance => "Manutenção de ferramentas",
        TextKey::AddonToolMaintenanceNone => "Nenhuma tentativa de manutenção ainda.",
        TextKey::AddonToolUpdatePermission => "Permitir atualizações nativas dos executáveis configurados",
        TextKey::AddonToolMaintenanceQueued => "Na fila",
        TextKey::AddonToolMaintenanceRunning => "Em execução",
        TextKey::AddonToolMaintenanceWaiting => "Aguardando nova tentativa",
        TextKey::AddonToolUpdated => "Atualizado",
        TextKey::AddonToolAlreadyCurrent => "Já atualizado",
        TextKey::AddonToolManualUpdate => "Atualização manual necessária",
        TextKey::AddonToolManualHelp => "Atualize manualmente a ferramenta configurada pelo método de instalação original e depois verifique o complemento.",
        TextKey::AddonToolFailedHelp => "A atualização falhou. Verifique a disponibilidade da ferramenta e o acesso à rede; a manutenção automática tentará novamente mais tarde.",
        TextKey::AddonToolUpdateFailedStale => "Não foi possível atualizar o programa e há uma versão mais recente disponível. A manutenção automática tentará novamente mais tarde. Para atualizá-lo agora, use o método de instalação original e depois verifique o complemento.",
        TextKey::AddonToolUpdateVersions => "{program}: versão instalada {installed}, versão mais recente {latest}.",
        TextKey::AddonToolVersionHelp => "Não foi possível ler a versão da ferramenta. Verifique o executável configurado e suas permissões.",
        TextKey::AddonToolUnavailableHelp => "A ferramenta configurada está indisponível. Verifique a configuração do executável e a instalação.",
        TextKey::AddonToolBusyHelp => "Aguardando a ferramenta ficar livre; a reprodução ativa não é interrompida.",
        TextKey::AddonToolCleanupHelp => "Pare o servidor, o scanner e todos os processos relacionados do provedor e da ferramenta. No diretório de instalação do servidor, execute a recuperação offline desta ferramenta com confirmação explícita, substituindo <absolute-executable-path> pelo caminho absoluto configurado. No Windows, use ersatzrs-web.exe. Para um perfil personalizado, informe também --state-dir com seu diretório absoluto. Só então reinicie. Isso remove o bloqueio de segurança dos processos; não repara o executável.",
        TextKey::AddonListDiskWarnings => "Avisos de uso do disco",
        TextKey::AddonListDiskWarningGb => "Esta lista: avisar a partir de (GB)",
        TextKey::AddonListsCombinedDiskWarningGb => "Todas as listas de complementos: avisar a partir de (GB)",
        TextKey::AddonListDiskWarningHelp => "GB decimais inteiros (1 GB = 1.000.000.000 bytes). 0 desativa apenas este aviso. Atingir ou ultrapassar o limite apenas exibe um aviso de integridade; não interrompe downloads nem exclui arquivos.",
        TextKey::AddonListDiskWarningFailed => "Não foi possível carregar ou salvar o aviso de uso do disco. Recarregue a página e tente novamente.",
        TextKey::ScanWorkerTitle => "Serviço de verificação",
        TextKey::ScanWorkerUnavailableBrief => "O serviço de verificação está indisponível",
        TextKey::ScanWorkerUnavailableHelp => "O serviço de verificação está indisponível. Novas verificações não podem ser executadas. Confira os logs e confirme que todos os processos locais de FFprobe restantes foram encerrados antes de reiniciar o ErsatzRS.",
        TextKey::PlaybackDataAccessTitle => "Acesso aos dados de reprodução",
        TextKey::LegacyMpegTsCapacityBrief => "O modo MPEG-TS legado requer pelo menos duas vagas de transcodificação ativas",
        TextKey::LegacyMpegTsCapacityHelp => "Um canal habilitado usa o modo MPEG-TS legado nativo. Seu processo de encapsulamento e o codificador do programa requerem duas vagas de transcodificação ativas distintas. Aumente o limite de transcodificações ativas para pelo menos 2 ou selecione outro modo de transmissão.",
        TextKey::TeletextBroadcastCheck => "Verificação de transmissão",
        TextKey::TeletextStatusNotLive => "Fora do ar",
        TextKey::TeletextStatusLive => "No ar",
        TextKey::TeletextStatusPublished => "Publicado",
        TextKey::TeletextStatusUnpublished => "Alterações não publicadas",
        TextKey::TeletextStatusNoSource => "Nenhuma fonte está ativa neste canal ainda. Ative uma abaixo.",
        TextKey::TeletextStatusRouteBlocked => "O modo de streaming deste canal não transporta teletexto. Abra a verificação de transmissão para a solução.",
        TextKey::TeletextStatusFailed => "A última tentativa de transmissão falhou. Abra a verificação de transmissão para ver a causa.",
        TextKey::TeletextStatusNotPublished => "O teletexto está configurado, mas ainda não foi publicado. Verifique e publique abaixo.",
        TextKey::TeletextStatusPendingDetail => "O teletexto está publicado. Uma verificação mais recente aguarda publicação.",
        TextKey::TeletextStatusLiveDetail => "O teletexto está publicado e está sendo enviado com este canal agora.",
        TextKey::TeletextStatusPublishedIdle => "O teletexto está publicado. Ele é enviado sempre que este canal estiver transmitindo.",
        TextKey::TeletextPageSetTitle => "Conjunto de páginas",
        TextKey::TeletextPageSetHelp => "Todas as páginas que os espectadores podem abrir neste canal.",
        TextKey::TeletextPageSetEmpty => "Nenhuma página ainda",
        TextKey::TeletextPageSetEmptyHelp => "Ative um complemento abaixo para adicionar páginas.",
        TextKey::TeletextPageSetPages => "Páginas",
        TextKey::TeletextPageSetSource => "Fonte",
        TextKey::TeletextPageSetStatus => "Status",
        TextKey::TeletextPageSetAddon => "complemento",
        TextKey::TeletextPageSetRangeUnknown => "Definidas pelo complemento",
        TextKey::TeletextPageSetStart => "Página inicial para os espectadores:",
        TextKey::TeletextSourceOn => "Ativado neste canal",
        TextKey::TeletextSourceOff => "Desativado neste canal",
        TextKey::TeletextSourceDisabled => "Complemento indisponível",
        TextKey::TeletextSourceMissing => "Complemento não instalado",
        TextKey::TeletextSourceNeedsAttention => "Requer atenção",
        TextKey::TeletextSourceHelp => "Configurações das páginas que este complemento cria neste canal.",
        TextKey::TeletextSourceUnavailableHelp => "Este complemento não pode ser usado agora e não adiciona páginas. Este canal ainda tem uma seleção antiga para ele.",
        TextKey::TeletextSourceFixAddon => "Gerenciar complementos",
        TextKey::TeletextSourceEnable => "Ativar neste canal",
        TextKey::TeletextSourceEnableHelp => "Começa com as configurações padrão do complemento. Você pode alterá-las depois.",
        TextKey::TeletextSourceSettings => "Configurações",
        TextKey::TeletextSourceAdvanced => "Outros conjuntos de configurações",
        TextKey::TeletextSettingSource => "Valor",
        TextKey::TeletextSettingDefault => "Padrão do complemento",
        TextKey::TeletextSettingCustom => "Valor próprio",
        TextKey::TeletextGuideSourceTitle => "Canais no guia",
        TextKey::TeletextGuideSourceHelp => "Salvo para este canal e usado em cada verificação.",
        TextKey::TeletextGuideSourceSummary => "Canais no guia:",
        TextKey::TeletextGuideSourceNone => "nenhum (páginas sem dados de programação)",
        TextKey::TeletextCustomPagesTitle => "Páginas próprias",
        TextKey::TeletextCustomPagesNone => "Nenhuma ainda",
        TextKey::TeletextCustomPagesHelp => "Páginas que você mesmo cria, por exemplo no QTeletextMaker.",
        TextKey::TeletextCustomPagesEmpty => "Ainda não há páginas próprias neste canal",
        TextKey::TeletextCustomPagesEmptyHelp => "A vinculação das suas páginas TTI chega com a biblioteca Teletext elements.",
        TextKey::TeletextPublishTitle => "Verificar e publicar",
        TextKey::TeletextPublishHelp => "Nada vai ao ar até você publicar.",
        TextKey::TeletextPublishAdvanced => "Avançado: escolha do script Lua e guia atual",
        TextKey::TeletextStepCheck => "Verificar",
        TextKey::TeletextStepPreview => "Prévia",
        TextKey::TeletextStepPublish => "Publicar",
        TextKey::TeletextCheckTechnical => "Detalhes técnicos",
        TextKey::TeletextCheckTechnicalHelp => "Estes valores já estão preenchidos. Altere-os só se souber por quê.",
        TextKey::TeletextScriptNotice => "Um script Lua escolhe agora o teletexto deste canal. A verificação dele fica em Avançado.",
        TextKey::TeletextChannelTitle => "Teletexto do canal",
        TextKey::TeletextChannelBaseHelp => "Escolha quais páginas de teletexto os espectadores recebem neste canal. Salvar não coloca nada no ar; você publica separadamente abaixo.",
        TextKey::TeletextChannelEmpty => "Nenhum complemento de teletexto disponível. Instale ou ative um primeiro.",
        TextKey::TeletextChannelProfiles => "Perfis",
        TextKey::TeletextProfileCreate => "Novo conjunto de configurações",
        TextKey::TeletextProfileSave => "Salvar configurações",
        TextKey::TeletextProfileName => "Nome deste conjunto",
        TextKey::TeletextProfileSharedHelp => "Um conjunto de configurações pode ser compartilhado por vários canais. Uma alteração vale para todos os canais que o usam.",
        TextKey::TeletextBindingProfile => "Conjunto de configurações usado por este canal",
        TextKey::TeletextBindingSave => "Usar este conjunto",
        TextKey::TeletextBindingOff => "Desativar neste canal",
        TextKey::TeletextBindingNone => "Nenhum perfil selecionado",
        TextKey::TeletextProfileInheritHelp => "O padrão segue o valor do próprio complemento. Escolha Valor próprio para defini-lo aqui; um valor próprio vazio é diferente do padrão.",
        TextKey::TeletextProfileOverride => "Substituir",
        TextKey::TeletextProfileValue => "Valor personalizado",
        TextKey::TeletextProfileReference => "Local do segredo",
        TextKey::TeletextProfileConfigured => "Configuração válida",
        TextKey::TeletextProfileMissingInstallation => "O pacote não está instalado",
        TextKey::TeletextProfileDisabledInstallation => "O pacote está desativado",
        TextKey::TeletextProfileIncompatibleInstallation => "O pacote é incompatível",
        TextKey::TeletextProfileInvalidSettings => "As configurações do perfil são inválidas",
        TextKey::TeletextProfileDeleted => "O perfil foi excluído",
        TextKey::TeletextProfileStaleInstallation => "O pacote mudou; revise e salve este perfil novamente",
        TextKey::TeletextAdminConflict => "Esta configuração mudou. Revise os valores atuais antes de salvar novamente.",
        TextKey::TeletextAdminInvalid => "Não foi possível salvar estas configurações. Revise o perfil, a seleção e os valores dos campos.",
        TextKey::TeletextAdminUnavailable => "A configuração de teletexto está indisponível. Verifique a aplicação e o banco de dados e tente novamente.",
        TextKey::TeletextAdminSaved => "Configuração de base salva. A validação e a publicação de páginas continuam sendo etapas separadas.",
        TextKey::TeletextAdminWrongAddon => "O perfil não pertence a este complemento. Revise a seleção.",
        TextKey::TeletextAdminLimit => "Estas configurações excedem os limites de configuração suportados. Reduza os valores do perfil ou as seleções.",
        TextKey::TeletextProfileRedacted => "Um valor secreto salvo não pode ser exibido aqui. Herde uma configuração válida ou substitua-o pelo local de um segredo.",
        TextKey::TeletextProfileRepairHelp => "Este perfil contém configurações que não podem ser editadas com segurança no complemento atual. Crie um novo perfil com valores revisados e selecione-o explicitamente. A seleção atual do canal ainda pode ser desativada.",
        TextKey::TeletextEffectiveAuthoringTitle => "Páginas da seleção Scripted ativa",
        TextKey::TeletextEffectiveAuthoringHelp => "Valide os perfis selecionados pelo script atual e salve um novo rascunho. Você pode incluir os canais da programação selecionados abaixo. Salvar não publica páginas.",
        TextKey::TeletextEffectiveAuthoringUnavailable => "Uma seleção Scripted ativa e configurações de criação válidas são necessárias.",
        TextKey::TeletextEffectiveAuthoringAbsent => "O script atual desativa todo o teletexto. Salve um rascunho sem teletexto, sem endereço de página.",
        TextKey::TeletextEffectiveScopeExpiry => "Esta seleção Scripted é válida até:",
        TextKey::TeletextPreviewAbsent => "Este rascunho não seleciona páginas de teletexto. Não há página para exibir.",
        TextKey::TeletextEffectiveEditorUnavailable => "A exportação e importação de rascunhos Scripted no editor ainda não estão conectadas. O rascunho salvo permanece disponível para prévia e publicação validada.",
        TextKey::TeletextAuthoringTitle => "Projetos de páginas",
        TextKey::TeletextAuthoringHelp => "Verificar monta todas as páginas com as configurações atuais e salva como rascunho com prévia. A transmissão não muda. O nível 2.5 é experimental.",
        TextKey::TeletextAuthoringStaticHelp => "Escolha os canais da programação abaixo ou deixe todos desmarcados para páginas estáticas.",
        TextKey::TeletextGuideChannels => "Canais da programação de TV (opcional)",
        TextKey::TeletextGuideChannelsHelp => "Selecione os canais cuja programação de TV deve aparecer neste rascunho. Sem seleção, as páginas serão estáticas. Salvar não altera a transmissão. A publicação verifica separadamente se a programação salva ainda está atualizada.",
        TextKey::TeletextGuideChannelsUnavailable => "A preparação da programação não está disponível. Projetos de páginas estáticas continuam disponíveis.",
        TextKey::TeletextAuthoringUnavailable => "A validação de rascunhos exige uma política de criação válida e configurada e uma seleção de perfis ativada.",
        TextKey::TeletextDesignName => "Nome do rascunho",
        TextKey::TeletextEntryProducer => "Complemento para a página inicial",
        TextKey::TeletextEntryPage => "Página inicial (100–899)",
        TextKey::TeletextEntrySubpage => "Subpágina inicial (hex)",
        TextKey::TeletextV1Region => "Região de caracteres de nível 1",
        TextKey::TeletextV1WesternLatin => "Latim ocidental",
        TextKey::TeletextResourceLifecycle => "Registros de atualização de recursos (JSON)",
        TextKey::TeletextResourceLifecycleHelp => "Forneça um registro de atualização explícito para cada página de recursos. [] é válido quando não há recursos.",
        TextKey::TeletextValidateSaveDraft => "Verificar",
        TextKey::TeletextDraftSaved => "Rascunho salvo. A prévia e a publicação são etapas separadas.",
        TextKey::TeletextEditorTitle => "Editar com QTeletextMaker",
        TextKey::TeletextEditorHelp => "Baixe um ZIP, extraia-o e edite as páginas TTI no QTeletextMaker. A exportação inclui os metadados originais e os arquivos de recursos necessários.",
        TextKey::TeletextEditorOwnershipHelp => "Apenas o conteúdo livre e as zonas personalizadas declaradas podem mudar. O guia gerado, as identidades das páginas e as declarações de recursos são protegidos. Alterações não compatíveis são rejeitadas.",
        TextKey::TeletextEditorTemplateHelp => "Alterações nos modelos do addon exigem uma atualização explícita da versão; importar um rascunho não modifica os pacotes instalados.",
        TextKey::TeletextEditorImportHelp => "Selecione os metadados .ersatzrs.json sem alterações e todos os arquivos .tti e .dvb da mesma exportação, inclusive os inalterados. Mantenha os nomes originais.",
        TextKey::TeletextEditorNoPublicationHelp => "A importação valida e salva um rascunho não publicado para a prévia. Solicitar e confirmar a publicação continuam sendo ações separadas.",
        TextKey::TeletextEditorUnavailable => "A exportação e a importação do editor estão indisponíveis porque não foi possível verificar o estado atual do projeto. Recarregue a prévia.",
        TextKey::TeletextEditorExportPage => "Baixar esta página e subpágina",
        TextKey::TeletextEditorExportSet => "Baixar todas as páginas",
        TextKey::TeletextEditorSidecar => "Metadados da exportação (.ersatzrs.json)",
        TextKey::TeletextEditorFiles => "Arquivos de páginas e recursos (.tti, .dvb)",
        TextKey::TeletextEditorImport => "Validar e importar como rascunho",
        TextKey::TeletextObjectCopyTitle => "Copiar definição de objeto",
        TextKey::TeletextObjectCopyHelp => "Crie uma cópia não publicada no mesmo recurso. Edite uma nova exportação no QTeletextMaker e publique separadamente.",
        TextKey::TeletextObjectCopySource => "Objeto de origem",
        TextKey::TeletextObjectCopyNumber => "Número de objeto livre",
        TextKey::TeletextObjectCopyAction => "Criar cópia de rascunho",
        TextKey::TeletextObjectCopyCurrentHelp => "Abra a revisão atual do rascunho para copiar um objeto.",
        TextKey::TeletextObjectCopyInvalid => "Selecione um objeto de origem existente e um número de objeto livre no mesmo recurso.",
        TextKey::TeletextSecondaryDefaultTitle => "Acordo do segundo conjunto de caracteres",
        TextKey::TeletextSecondaryDefaultHelp => "Aplique uma escolha explícita às páginas herdadas deste complemento. As fontes e configurações explícitas de página/revista são preservadas. Salve um rascunho; publique separadamente.",
        TextKey::TeletextSecondaryDefaultChoice => "Acordo",
        TextKey::TeletextSecondaryDefaultOriginal => "Original (depende do receptor)",
        TextKey::TeletextSecondaryDefaultNone => "Sem segundo conjunto G0",
        TextKey::TeletextSecondaryDefaultDesignated => "Segundo conjunto G0 explícito",
        TextKey::TeletextSecondaryDefaultCode => "Código do conjunto (EN 300 706 tabela 33, dois dígitos hexadecimais)",
        TextKey::TeletextSecondaryDefaultSave => "Salvar acordo como rascunho",
        TextKey::TeletextSecondaryDefaultCurrentHelp => "Abra a versão atual do rascunho para alterar este acordo.",
        TextKey::TeletextSecondaryDefaultInvalid => "Não foi possível salvar o acordo. Verifique o código e o rascunho atual; as páginas protegidas permanecem inalteradas.",
        TextKey::TeletextObjectRemoveTitle => "Remover objeto não utilizado",
        TextKey::TeletextObjectRemoveHelp => "Remova deste rascunho uma definição adicionada não utilizada. As definições originais e os objetos usados por uma chamada ou um valor padrão estão protegidos. A transmissão só muda após a publicação explícita.",
        TextKey::TeletextObjectRemoveTarget => "Objeto adicionado não utilizado",
        TextKey::TeletextObjectRemoveAction => "Remover do rascunho",
        TextKey::TeletextObjectRemoveCurrentHelp => "Abra a revisão atual do rascunho para remover um objeto não utilizado.",
        TextKey::TeletextObjectRemoveInvalid => "Não é possível remover este objeto. Escolha uma definição adicionada não utilizada no rascunho atual; os objetos originais, referenciados e padrão continuam protegidos.",
        TextKey::TeletextObjectAddTitle => "Adicionar definição de objeto",
        TextKey::TeletextObjectAddHelp => "Crie uma definição vazia em um recurso POP/GPOP existente. Ela se torna um novo rascunho; a publicação continua sendo uma etapa separada.",
        TextKey::TeletextObjectAddResource => "Recurso",
        TextKey::TeletextObjectAddType => "Tipo de objeto",
        TextKey::TeletextObjectAddAction => "Criar definição vazia",
        TextKey::TeletextObjectAddInvalid => "O recurso, o tipo ou o novo número de objeto é inválido, pertence a outro projeto ou já está em uso.",
        TextKey::TeletextResourceBankTitle => "Criar banco de recursos",
        TextKey::TeletextResourceBankHelp => "Crie um banco POP/GPOP vazio em um novo rascunho. Informe explicitamente todos os valores de atualização. As páginas de origem e os pacotes instalados não mudam; publique separadamente.",
        TextKey::TeletextResourceDisplay => "Página exibida / subpágina",
        TextKey::TeletextResourceBankKind => "Tipo de recurso",
        TextKey::TeletextResourceBankPage => "Página de recurso (três dígitos hexadecimais)",
        TextKey::TeletextResourceBankSubpage => "Subpágina lógica (S1)",
        TextKey::TeletextResourceBankCounter => "Contador de atualização",
        TextKey::TeletextResourceBankUpdate => "Estado de atualização",
        TextKey::TeletextResourceBankUnchanged => "Sem alterações",
        TextKey::TeletextResourceBankChanged => "Alterado",
        TextKey::TeletextResourceBankMoreSubpages => "Há mais subpáginas a seguir",
        TextKey::TeletextResourceBankFinalSubpage => "Última subpágina",
        TextKey::TeletextResourceBankLastSubpage => "Última subpágina lógica",
        TextKey::TeletextResourceBankAction => "Criar banco em rascunho",
        TextKey::TeletextResourceBankInvalid => "Verifique a página exibida, o endereço POP/GPOP e os valores explícitos de atualização. O banco deve ser novo e válido.",
        TextKey::TeletextSubpageAnnouncementTitle => "Anunciar subpágina de recurso",
        TextKey::TeletextSubpageAnnouncementHelp => "Selecione uma ou mais páginas exibidas e uma subpágina de recurso existente. Somente uma página de recurso já selecionada pode ser ampliada. Um rascunho é criado; publique separadamente.",
        TextKey::TeletextSubpageAnnouncementDisplays => "Páginas exibidas / subpáginas (uma ou mais)",
        TextKey::TeletextSubpageAnnouncementAction => "Salvar anúncio em rascunho",
        TextKey::TeletextSubpageAnnouncementInvalid => "Selecione páginas exibidas válidas e uma subpágina existente do recurso POP/GPOP selecionado. Nenhuma alteração foi salva.",
        TextKey::TeletextObjectAttachmentTitle => "Vincular uma página de recurso existente",
        TextKey::TeletextObjectAttachmentHelp => "Escolha uma página existente para uma função sem página selecionada. As chamadas de objetos existentes permanecem inalteradas. Isso salva um rascunho. Publique separadamente.",
        TextKey::TeletextObjectAttachmentAction => "Salvar vínculo como rascunho",
        TextKey::TeletextObjectAttachmentRestoreTitle => "Desfazer um vínculo de página de recurso",
        TextKey::TeletextObjectAttachmentRestoreHelp => "Restaure esta função ao estado original sem página selecionada. As outras alterações do rascunho são mantidas. Isso salva um rascunho. Publique separadamente.",
        TextKey::TeletextObjectAttachmentRestoreAction => "Salvar reversão como rascunho",
        TextKey::TeletextObjectSelectionTitle => "Escolher páginas de recursos e chamadas de objetos",
        TextKey::TeletextObjectSelectionHelp => "Escolha páginas POP/GPOP e DRCS/GDRCS já presentes e atualize explicitamente as chamadas de recursos existentes em conjunto. As chamadas nunca mudam automaticamente; coordene todos os usos afetados. Isso salva um rascunho. Publique separadamente.",
        TextKey::TeletextObjectSelectionPageTarget => "Página do recurso",
        TextKey::TeletextObjectSelectionMask => "Subpáginas necessárias (máscara hexadecimal)",
        TextKey::TeletextObjectSelectionMaskHelp => "Digite de 1 a 4 dígitos hexadecimais. O bit 0 anuncia S1=0, até o bit 15 para S1=15. Anuncie todas as subpáginas necessárias da página escolhida.",
        TextKey::TeletextObjectSelectionCalls => "Chamada de recurso",
        TextKey::TeletextObjectSelectionMainCalls => "Chamada Main explícita",
        TextKey::TeletextObjectSelectionCaller => "Objeto chamador",
        TextKey::TeletextObjectSelectionCommand => "Índice do comando (começa em 0)",
        TextKey::TeletextObjectSelectionCurrentTarget => "Destino atual",
        TextKey::TeletextObjectSelectionOffset => "Deslocamento da origem (linha, coluna)",
        TextKey::TeletextObjectSelectionNoOffset => "Sem deslocamento explícito",
        TextKey::TeletextObjectSelectionCallTarget => "Destino da chamada",
        TextKey::TeletextObjectSelectionAction => "Salvar seleção como rascunho",
        TextKey::TeletextObjectSelectionInvalid => "As páginas, máscaras de subpáginas ou chamadas coordenadas são inválidas ou não estão mais atualizadas. Nenhuma alteração foi salva.",
        TextKey::TeletextResourceEditCurrentHelp => "Abra a revisão atual do rascunho para alterar bancos ou anúncios de recursos.",
        TextKey::TeletextObjectAddCurrentHelp => "Abra a revisão atual do rascunho para adicionar uma definição de objeto.",
        TextKey::TeletextObjectTypeActive => "Ativo",
        TextKey::TeletextObjectTypeAdaptive => "Adaptativo",
        TextKey::TeletextObjectTypePassive => "Passivo",
        TextKey::TeletextEditorLimit => "Os arquivos do editor excedem os limites configurados de tamanho ou processamento. Reduza o conjunto de páginas compatíveis ou verifique os limites configurados. A transmissão permanece inalterada.",
        TextKey::TeletextEditorMismatch => "Importação rejeitada: os arquivos ou metadados não correspondem à exportação original ou ao projeto atual. Recarregue a prévia e use arquivos da mesma exportação.",
        TextKey::TeletextEditorProtected => "Importação rejeitada: uma parte protegida ou não compatível foi alterada. Edite apenas conteúdo livre ou zonas personalizadas declaradas.",
        TextKey::TeletextEditorNoChanges => "A importação não contém alterações de página compatíveis. Edite conteúdo livre ou uma zona personalizada antes de importar.",
        TextKey::TeletextEditorSyntax => "Importação rejeitada: uma página TTI é inválida ou usa sintaxe não compatível. Verifique a página e a posição indicadas.",
        TextKey::TeletextEditorMissingReceipt => "O comprovante da exportação original está indisponível. Exporte o projeto novamente antes de importar.",
        TextKey::TeletextDeliveryTitle => "Saída das transmissões ativas",
        TextKey::TeletextDeliveryHelp => "Cada entrada descreve uma saída observada. A entrega não confirma a exibição no cliente nem para outros espectadores. Recarregue para atualizar; a última entrega confirmada é histórica.",
        TextKey::TeletextDeliveryUnavailable => "Observação da saída indisponível.",
        TextKey::TeletextDeliveryEmpty => "Nenhuma saída ativa observada.",
        TextKey::TeletextDeliveryIncomplete => "Não foi possível registrar algumas saídas. Esta lista está incompleta.",
        TextKey::TeletextDeliveryLabel => "Saída",
        TextKey::TeletextDeliveryRequested => "Saída solicitada",
        TextKey::TeletextDeliveryConfirmed => "Última entrega confirmada",
        TextKey::TeletextDeliveryWaiting => "Aguardando a entrega completa.",
        TextKey::TeletextDeliveryApplied => "Saída solicitada entregue.",
        TextKey::TeletextDeliveryRejected => "Alteração solicitada rejeitada.",
        TextKey::TeletextDeliveryUnconfirmed => "Nenhuma entrega confirmada.",
        TextKey::TeletextDeliveryAbsent => "Sem páginas criadas",
        TextKey::TeletextDeliveryGeneration => "Geração",
        TextKey::TeletextGuideRuntimeSnapshot => "Versão do guia",
        TextKey::TeletextGuideInitialSeed => "Guia inicial",
        TextKey::TeletextGuideFresh => "Atualizado na observação",
        TextKey::TeletextGuideStale => "Guia desatualizado mantido",
        TextKey::TeletextGuideCapturedAt => "Guia capturado em",
        TextKey::TeletextGuideCurrentPreview => "Guia de TV atual",
        TextKey::TeletextGuideCurrentPreviewHelp => "Consulte o guia selecionado por página e subpágina. Esta visualização somente leitura não altera projetos salvos nem publica páginas. Uma prévia não confirma a entrega ao espectador.",
        TextKey::TeletextGuideMaintenanceTitle => "Manutenção do guia de TV",
        TextKey::TeletextGuideMaintenanceDisabled => "A atualização automática não está ativada. Esta verificação não a ativa.",
        TextKey::TeletextGuideMaintenanceEmpty => "Nenhum guia registrado para atualização automática.",
        TextKey::TeletextGuideMaintenanceSummary => "Guias salvos: atualizados {fresh}, desatualizados {stale}, indisponíveis {unavailable}, revogados {revoked}. Tarefas: revisão necessária {needs-review}, responsabilidade ativa {owned}. Dados inválidos {invalid metadata}; visão incompleta: {incomplete}. Estas observações não confirmam entrega do fluxo nem exibição no cliente.",
        TextKey::TeletextGuideMaintenanceUnavailable => "Não foi possível avaliar a manutenção do guia. Verifique a configuração e o estado salvo.",
        TextKey::TeletextGuideMissingAuthorization => "{count} guias base selecionados não têm autorização de atualização. Prepare o guia novamente, solicite a publicação e publique explicitamente. A reprodução normal de áudio e vídeo continua disponível.",
        TextKey::TeletextPublicationTitle => "Publicar este projeto salvo",
        TextKey::TeletextPublicationHelp => "Solicite a publicação desta revisão salva e depois publique a solicitação validada. Salvar ou visualizar uma prévia de um rascunho não o publica.",
        TextKey::TeletextPublicationUnrequested => "Esta revisão salva não tem uma solicitação de publicação pendente. As páginas selecionadas permanecem inalteradas.",
        TextKey::TeletextPublicationPending => "A publicação desta revisão salva foi solicitada. Ela ainda não está selecionada para os fluxos.",
        TextKey::TeletextPublicationSelected => "Esta revisão salva está selecionada. Isso não confirma que um fluxo a aplicou; cada saída compatível valida as alterações antes de aplicá-las.",
        TextKey::TeletextPublicationUnavailable => "A publicação não está disponível para esta revisão salva. Revise as configurações atuais e a validação antes de solicitá-la novamente.",
        TextKey::TeletextPublicationRequest => "Solicitar publicação",
        TextKey::TeletextPublicationPublish => "Publicar as páginas solicitadas",
        TextKey::TeletextPublicationConflict => "O projeto, a seleção ou o estado da publicação mudou. Recarregue esta revisão exata e revise-a antes de tentar novamente.",
        TextKey::TeletextPublicationRequestSaved => "Solicitação de publicação salva. As páginas selecionadas para os fluxos não mudaram.",
        TextKey::TeletextPublicationSelectedSaved => "As páginas solicitadas estão selecionadas. A aplicação real é verificada separadamente para cada saída compatível.",
        TextKey::TeletextPublicationReplacePending => "Solicitar esta revisão substitui a solicitação pendente de outro projeto. As páginas atualmente selecionadas para os fluxos permanecem inalteradas.",
        TextKey::TeletextPreviewTitle => "Prévia",
        TextKey::TeletextPreviewHelp => "Esta prévia usa o modelo validado. O cabeçalho do receptor, o vídeo e as áreas não resolvidas são marcados separadamente. O nível 2.5 é experimental; o suporte dos clientes pode ser incompleto.",
        TextKey::TeletextPreviewPage => "Página",
        TextKey::TeletextPreviewSubpage => "Subpágina",
        TextKey::TeletextPreviewLoad => "Mostrar prévia",
        TextKey::TeletextPreviewDiagnostics => "Problemas de validação e exibição",
        TextKey::TeletextPreviewNoDiagnostics => "Não há problemas de exibição não resolvidos nesta página.",
        TextKey::TeletextPreviewReceiverHeader => "Cabeçalho do receptor",
        TextKey::TeletextPreviewVideo => "Vídeo",
        TextKey::TeletextPreviewUnresolved => "Não resolvido",
        TextKey::TeletextDraftMissing => "O rascunho ou a versão solicitada não está disponível para este canal.",
        TextKey::TeletextWorkBusy => "O processamento de teletexto está ocupado. Tente novamente.",
        TextKey::TeletextWorkExpired => "O limite de tempo original expirou. Um rascunho já salvo continua armazenado.",
        TextKey::TeletextAuthoringRejected => "Não foi possível validar os dados da página selecionada. A transmissão em andamento permanece inalterada.",
        TextKey::TeletextPreviewPosition => "Posição",
        TextKey::TeletextPreviewSource => "Fonte original",
        TextKey::TeletextPreviewReload => "Reabrir rascunho salvo",
        TextKey::TeletextPreviewEditorialTitle => "Limites de edição",
        TextKey::TeletextPreviewEditorialHelp => "Estes limites vêm deste original salvo. As células fora das regiões declaradas permanecem protegidas; importar um rascunho não altera o modelo.",
        TextKey::TeletextPreviewRegion => "Região",
        TextKey::TeletextPreviewArea => "Área da página",
        TextKey::TeletextPreviewExtent => "Linhas e colunas (numeradas a partir de zero)",
        TextKey::TeletextPreviewPermission => "Edição",
        TextKey::TeletextPreviewFree => "Conteúdo livre",
        TextKey::TeletextPreviewZone => "Zona preenchível",
        TextKey::TeletextPreviewGenerated => "Gerado automaticamente",
        TextKey::TeletextPreviewDynamicField => "Campo dinâmico",
        TextKey::TeletextPreviewBinding => "Vinculação",
        TextKey::TeletextPreviewNoEditorialMetadata => "Nenhuma região de edição foi declarada para esta página salva.",
        TextKey::TeletextPreviewPanelLeft => "Painel esquerdo",
        TextKey::TeletextPreviewPanelMain => "Página principal",
        TextKey::TeletextPreviewPanelRight => "Painel direito",
        TextKey::TeletextNavIndex => "Índice",
        TextKey::TeletextNavRed => "Vermelho",
        TextKey::TeletextNavGreen => "Verde",
        TextKey::TeletextNavYellow => "Amarelo",
        TextKey::TeletextNavCyan => "Ciano",
        TextKey::TeletextAuthoringHealthTitle => "Configuração de criação de teletexto",
        TextKey::TeletextAuthoringHealthAbsentBrief => "A criação de teletexto não está configurada",
        TextKey::TeletextAuthoringHealthAbsentDetail => "Nenhuma política de criação de teletexto está configurada. A preparação de rascunhos e a prévia estão indisponíveis; a reprodução normal não é afetada.",
        TextKey::TeletextAuthoringHealthValidBrief => "As configurações de criação de teletexto são válidas",
        TextKey::TeletextAuthoringHealthValidDetail => "As configurações de criação de teletexto são válidas. Cada rascunho e prévia ainda exige validação com limites definidos. Esta verificação não valida as páginas criadas, a publicação, o suporte completo ao nível 2.5 nem os limites de memória.",
        TextKey::TeletextAuthoringHealthInvalidBrief => "As configurações de criação de teletexto são inválidas",
        TextKey::TeletextAuthoringHealthInvalidDetail => "As configurações de criação de teletexto são inválidas. A preparação de rascunhos e a prévia estão indisponíveis; a reprodução normal não é afetada. Corrija as configurações de criação de teletexto.",
        TextKey::TeletextAuthoringHealthUnreadableBrief => "Não foi possível ler as configurações de criação de teletexto",
        TextKey::TeletextAuthoringHealthUnreadableDetail => "Não foi possível ler as configurações de criação de teletexto. A preparação de rascunhos e a prévia estão indisponíveis; a reprodução normal não é afetada. Verifique a disponibilidade e o acesso ao banco de dados.",
        TextKey::TeletextWorkTitle => "Configuração das tarefas de teletexto",
        TextKey::TeletextWorkDefault => "Por padrão, as tarefas de teletexto são executadas uma por vez. Isso não é um limite de memória nem confirma que as páginas criadas estejam prontas para transmissão.",
        TextKey::TeletextWorkConfigured => "O limite configurado é de {count} tarefas simultâneas de teletexto. Isso não é um limite de memória nem confirma que as páginas criadas estejam prontas para transmissão.",
        TextKey::TeletextWorkInvalid => "O número de tarefas simultâneas de teletexto deve ser um inteiro positivo. As tarefas ficarão indisponíveis até que essa configuração seja corrigida.",
        TextKey::TeletextSourceStorageInvalid => "A configuração de armazenamento das fontes do provedor é inválida ou está indisponível. As legendas opcionais de teletexto do provedor estão indisponíveis; a reprodução normal de áudio e vídeo continua disponível.",
        TextKey::TeletextOutputTitle => "Configuração da saída de teletexto",
        TextKey::TeletextOutputAbsentBrief => "A saída de teletexto não está configurada",
        TextKey::TeletextOutputAbsentHelp => "Nenhuma política de saída de teletexto está configurada. Os canais continuam a reprodução normal sem adicionar teletexto.",
        TextKey::TeletextOutputConfiguredBrief => "As configurações da saída de teletexto são válidas",
        TextKey::TeletextOutputConfiguredHelp => "As configurações da saída de teletexto são válidas. A reprodução ainda verifica as páginas aceitas e a compatibilidade do fluxo de cada canal. Esta verificação não comprova a transmissão de teletexto, o suporte completo ao nível 2.5 nem os limites de memória.",
        TextKey::TeletextOutputInvalidBrief => "As configurações da saída de teletexto são inválidas",
        TextKey::TeletextOutputInvalidHelp => "As configurações da saída de teletexto são inválidas. A reprodução normal continua sem adicionar teletexto. Corrija as configurações para ativar a saída de teletexto.",
        TextKey::TeletextOutputUnreadableBrief => "Não foi possível ler as configurações da saída de teletexto",
        TextKey::TeletextOutputUnreadableHelp => "Não foi possível ler as configurações da saída de teletexto. A reprodução normal continua sem adicionar teletexto. Verifique a disponibilidade e o acesso ao banco de dados.",
        TextKey::TeletextReadinessTitle => "Preparação do teletexto",
        TextKey::TeletextReadinessHelp => "Cada etapa é verificada separadamente. Ativado, validado, publicado e transmitido são estados diferentes. A exibição em um cliente não é verificada aqui.",
        TextKey::TeletextReadinessDone => "Concluído",
        TextKey::TeletextReadinessMissing => "Ação necessária",
        TextKey::TeletextReadinessAddon => "Complemento de teletexto ativado para este canal",
        TextKey::TeletextReadinessAddonHelp => "Ative um complemento e escolha um perfil válido na seção de complementos abaixo.",
        TextKey::TeletextReadinessCapacity => "Capacidade do teletexto (políticas de criação e saída)",
        TextKey::TeletextCapacityProfileTvguide16 => "Guia de TV (nível 1/1.5), até 16 páginas por geração",
        TextKey::TeletextCapacityActive => "Ambas as políticas correspondem a este perfil de capacidade:",
        TextKey::TeletextCapacityCustom => "Políticas válidas ajustadas manualmente estão em uso. Elas são mantidas, a menos que você as substitua explicitamente.",
        TextKey::TeletextCapacityAbsent => "Uma ou ambas as políticas de teletexto estão ausentes. Os canais são reproduzidos sem teletexto até que você aplique um perfil de capacidade.",
        TextKey::TeletextCapacityInvalid => "Uma ou ambas as políticas de teletexto são inválidas ou ilegíveis. A reprodução normal continua sem teletexto.",
        TextKey::TeletextCapacityApplyHelp => "Aplicar grava as duas políticas globais após a validação do servidor. Cada limite é um teto: um guia maior é rejeitado com um diagnóstico, nunca cortado. Isso afeta todos os canais com teletexto.",
        TextKey::TeletextCapacityConfirmScope => "Entendo que isso altera as configurações de teletexto de todos os canais",
        TextKey::TeletextCapacityReplace => "Substituir também as políticas existentes ajustadas manualmente",
        TextKey::TeletextCapacityApply => "Aplicar perfil de capacidade",
        TextKey::TeletextCapacitySaved => "O perfil de capacidade foi aplicado. Valide e publique as páginas para iniciar o teletexto.",
        TextKey::TeletextCapacityExists => "Já existem políticas de teletexto válidas ajustadas manualmente. Marque “Substituir também” para sobrescrevê-las.",
        TextKey::TeletextCapacityScopeUnconfirmed => "Confirme que a alteração se aplica a todos os canais antes de aplicar o perfil.",
        TextKey::TeletextReadinessRoute => "A rota de saída transporta teletexto",
        TextKey::TeletextRouteLegacy => "MPEG-TS: o servidor adiciona teletexto ao fluxo deste canal.",
        TextKey::TeletextRouteHybridBuiltin => "MPEG-TS Hybrid com o wrapper integrado: o servidor adiciona teletexto ao fluxo final.",
        TextKey::TeletextRouteHybridScript => "O MPEG-TS Hybrid usa o script MPEG-TS global abaixo, que cria o fluxo final por conta própria. O servidor não consegue adicionar teletexto a ele.",
        TextKey::TeletextRouteHls => "HLS (Direct ou Segmenter) não transporta teletexto DVB. Clientes de playlist não recebem teletexto.",
        TextKey::TeletextRouteFixChannel => "Correção somente para este canal: defina o modo de streaming como MPEG-TS no editor do canal (motor Legacy). Os outros canais não mudam.",
        TextKey::TeletextRouteFixGlobal => "Alternativa de alcance global: não escolha nenhum script MPEG-TS (wrapper integrado) nas configurações do FFmpeg. Isso altera o fluxo Hybrid de todos os canais.",
        TextKey::TeletextRouteScript => "Script MPEG-TS",
        TextKey::TeletextReadinessPublished => "Páginas validadas publicadas",
        TextKey::TeletextReadinessPending => "Uma revisão validada aguarda publicação explícita.",
        TextKey::TeletextReadinessUnpublished => "Nada foi publicado ainda. Prepare e valide um rascunho abaixo e depois publique-o explicitamente.",
        TextKey::TeletextReadinessBroadcast => "Transmissão confirmada por uma saída ativa",
        TextKey::TeletextReadinessNoOutputs => "Nenhuma saída ativa no momento. Inicie o fluxo e recarregue esta página para confirmar a entrega.",
        TextKey::TeletextReadinessDelivering => "Saídas ativas que confirmam a entrega do teletexto",
        TextKey::TeletextReadinessFailed => "A última saída de teletexto deste canal parou em",
        TextKey::TeletextFailureCapacity => "A saída de taxa constante deixou pouco espaço ao lado dos picos de vídeo. Aumente moderadamente a margem de taxa de bits do perfil FFmpeg ou reduza seu buffer de vídeo (VBV) e reinicie o canal. A reprodução normal sem teletexto continua funcionando.",
        TextKey::TeletextFailureDeadline => "A saída perdeu o prazo de entrega, geralmente porque o codificador ou o cliente ficou para trás. Verifique a carga do servidor e o perfil FFmpeg do canal e reinicie o canal.",
        TextKey::TeletextFailureSource => "A saída do codificador terminou ou falhou antes que o teletexto pudesse ser transportado. Verifique o log do FFmpeg deste canal.",
        TextKey::TeletextFailureTransport => "A saída MPEG-TS foi rejeitada pelas verificações de transporte do teletexto (PSI, relógio ou enquadramento). Verifique se a rota de saída acima suporta teletexto e reinicie o canal.",
        TextKey::TeletextDecoderTitle => "Decodificação de teletexto",
        TextKey::TeletextDecoderReady => "O runtime FFmpeg selecionado passou nas verificações de seleção de páginas de teletexto, texto de legendas e decodificação de imagens coloridas.",
        TextKey::TeletextDecoderMissing => "A decodificação de teletexto não está disponível. Selecione um runtime FFmpeg 9 com suporte a teletexto via libzvbi.",
        TextKey::TeletextDecoderOptions => "O decodificador de teletexto não possui as opções necessárias de seleção de páginas ou renderização. Selecione um runtime FFmpeg 9 compatível.",
        TextKey::TeletextDecoderHelpFailed => "Não foi possível concluir a verificação do decodificador de teletexto. Verifique a disponibilidade e as permissões do runtime FFmpeg.",
        TextKey::TeletextDecoderTextFailed => "A decodificação de páginas de teletexto e texto de legendas falhou. Selecione um runtime FFmpeg 9 compatível.",
        TextKey::TeletextDecoderOriginalPtsUnavailable => "O ambiente pode decodificar teletexto, mas não preservar os carimbos de tempo originais das legendas ao vivo. As legendas de teletexto ao vivo estão indisponíveis; a reprodução normal continua disponível.",
        TextKey::TeletextDecoderBitmapFailed => "A decodificação de imagens coloridas de teletexto falhou. Selecione um runtime FFmpeg 9 compatível.",
        TextKey::NextRuntimeAvailability => "Disponibilidade do mecanismo Next",
        TextKey::NextOverlayInvalid => "A sobreposição de configuração do Next está indisponível ou é inválida",
        TextKey::NextOverlaySchema => "As sobreposições de configuração do Next exigem um URI de versão compatível (0.1.0 ou 0.1.1). Revise as configurações de cópia antes de atualizar uma sobreposição antiga.",
        TextKey::NextOverlayIncomplete => "A validação da sobreposição de configuração do Next está incompleta",
        TextKey::JavascriptAudioSelectionTitle => "Seleção de áudio JavaScript",
        TextKey::JavascriptAudioSelectionHealthy => "Nenhum erro de seleção registrado desde a inicialização. Scripts não executados não foram validados.",
        TextKey::JavascriptAudioSelectionFailures => "{count} erros de seleção desde a inicialização; foi usada a seleção de áudio integrada. Confira scripts/audio-stream-selector e os limites da ADR 0096. O contador é zerado ao reiniciar.",
        TextKey::AddonListDiskUsageTitle => "Uso do disco das listas de complementos",
        TextKey::AddonListDiskUsageBrief => "Aviso de uso do disco das listas de complementos",
        TextKey::AddonListDiskUsageUnavailable => "Não foi possível medir completamente o uso do disco das listas de complementos. Verifique o acesso ao armazenamento e repita a verificação de integridade.",
        TextKey::AddonListDiskUsageUnavailableBrief => "Não foi possível medir o uso do disco das listas de complementos.",
        TextKey::AddonListDiskUsageLine => "Lista nº {id}: {used} GB usados; aviso a partir de {limit} GB.",
        TextKey::AddonListsCombinedDiskUsageLine => "Total: {used} GB usados; aviso a partir de {limit} GB.",
        TextKey::AddonMediaListUrl => "URL da lista add-on",
        TextKey::AddonMediaListFolder => "Pasta de armazenamento",
        TextKey::AddonMediaListFolderHelp => "Opcional; caso contrário, derivada do nome da lista",
        TextKey::DestructiveActions => "Ações destrutivas",
        TextKey::ReconcileEntireSource => "Reconciliar toda a fonte",
        TextKey::ReconcileEntireSourceHelp => {
            "Compara um instantâneo completo da fonte sem alterar a publicação atual nem as configurações dos itens."
        }
        TextKey::ReconcileQueued => "Reconciliação completa da fonte enfileirada.",
        TextKey::ReconcileFailed => "Falha na reconciliação completa da fonte.",
        TextKey::ResetAndReload => "Redefinir e recarregar",
        TextKey::ResetAddonMediaListHelp => {
            "Remove todos os resultados gerados para esta lista e depois enfileira uma importação completa."
        }
        TextKey::SourceHost => "Host de origem",
        TextKey::ResetRemoves => "Esta redefinição remove",
        TextKey::ResetRemovesItems => {
            "Itens importados, metadados do provedor, alterações do operador, numeração de episódios e substituições."
        }
        TextKey::ResetRemovesArtifacts => {
            "Mídia gerada, arquivos NFO, imagens, legendas, caches e arquivos guardados."
        }
        TextKey::ResetRemovesRuntime => {
            "Tarefas, pontos de controle, artefatos de revisão, itens da playlist gerada e mídia projetada."
        }
        TextKey::ResetRetains => "Esta redefinição mantém",
        TextKey::ResetRetainsSettings => {
            "O nome da lista, os vínculos com add-on, origem e biblioteca, a identidade da playlist gerada, as políticas de atualização e publicação, a cota de artefatos e outras configurações do operador."
        }
        TextKey::ResetReloadFailureWarning => {
            "Se a recarga falhar, os canais e a playlist gerada permanecerão vazios."
        }
        TextKey::TypeListNameToConfirm => "Digite o nome atual da lista para confirmar",
        TextKey::ResetQueued => "Redefinição e recarga enfileiradas.",
        TextKey::ResetAddonMediaListFailed => "Falha ao redefinir e recarregar.",
        TextKey::ChapterInput => "Capítulos",
        TextKey::ChapterInputHelp => "Opcional, um por linha: 0:00 Introdução",
        TextKey::RestoreProviderChapters => "Restaurar capítulos do provedor",
        TextKey::ManagedByAddon => "Gerenciado por um complemento",
        TextKey::Trash => "Lixo",
        TextKey::TrashAlreadyEmpty => "O lixo já está vazio.",
        TextKey::TrashEmpty => "O lixo está vazio.",
        TextKey::TrashEmptyConfirm => {
            "Excluir todos os itens com arquivo não encontrado do banco de dados?"
        }
        TextKey::TrashEmptyConfirmHelp => {
            "Isso remove permanentemente os itens de mídia ausentes no momento e suas linhas de metadados dependentes."
        }
        TextKey::TrashSubtitle => {
            "Itens cujos arquivos de origem estão ausentes no disco ou no provedor."
        }
        TextKey::Troubleshooting => "Solução de problemas",
        TextKey::TunerCount => "Quantidade de sintonizadores",
        TextKey::HdhrDisabledByJwt => "Os endpoints HDHomeRun estão desativados porque a proteção JWT para IPTV está ativada. Estas configurações não têm efeito.",
        TextKey::TvShows => "Séries",
        TextKey::Ui => "Interface",
        TextKey::UnavailableLocalMediaMessage => "Há {count} itens indisponíveis porque o ErsatzRS não consegue encontrá-los no disco, incluindo: {items}",
        TextKey::UnavailableRemoteStreamsMessage => "Há {count} Remote Streams indisponíveis porque o provedor informa que não podem ser reproduzidos, incluindo: {items}",
        TextKey::Unavailable => "Indisponível",
        TextKey::UseActualTimes => "Usar horários reais",
        TextKey::UseCustomPlaybackOrder => "Usar ordem de reprodução personalizada",
        TextKey::UseChaptersAsMediaItems => "Usar capítulos como itens de mídia",
        TextKey::UseChaptersAsMediaItemsHelp => {
            "Agendar capítulos individuais em vez de arquivos inteiros"
        }
        TextKey::UseEmbeddedSubtitles => "Usar legendas embutidas",
        TextKey::Uuid => "UUID",
        TextKey::UntitledItem => "Item sem título",
        TextKey::WindowsStartup => "Inicialização com o Windows",
        TextKey::WindowsStartupNeedsRepair => {
            "O comando de inicialização salvo aponta para outro local. Aplique esta configuração para corrigi-lo."
        }
        TextKey::WindowsStartupUnavailable => {
            "A inicialização com o Windows está indisponível porque o iniciador não foi encontrado ou o registro não pôde ser lido."
        }
        TextKey::ActiveTranscodeLimit => "Máximo de transcodificações de streaming simultâneas",
        TextKey::ActiveTranscodeLimitHelp => {
            "Limite rígido para transcodificações FFmpeg de streaming. Quando todos os slots estão ocupados, um novo espectador recebe uma resposta de todos os streams em uso até que um slot seja liberado; streams ativos não ultrapassam o limite."
        }
        TextKey::ActiveTranscodeSuggestionHardware => {
            "Codificador de hardware detectado; este é um valor sugerido, não uma alteração automática."
        }
        TextKey::ActiveTranscodeSuggestionSoftware => {
            "Apenas codificação por software; este é um valor sugerido, não uma alteração automática."
        }
        TextKey::Peak => "pico",
        TextKey::StreamsInUse => "Streams em uso",
        TextKey::SuggestedTranscodeLimit => "Valor sugerido",
        TextKey::Watermarks => "Marcas d'água",
        TextKey::Width => "Largura",
        TextKey::WorkAheadSegmenterLimit => "Transcodificações HLS antecipadas",
        TextKey::WorkAheadSegmenterLimitHelp => {
            "Quantos slots de transcodificação HLS podem avançar na velocidade máxima para criar buffer. O avanço sempre cede lugar a streams assistidos."
        }
        TextKey::AccelerationKind => "Tipo de aceleração",
        TextKey::AddFfmpegProfile => "Adicionar perfil FFmpeg",
        TextKey::AddProfile => "Adicionar perfil",
        TextKey::AllowBFrames => "Permitir B-frames",
        TextKey::Audio => "Áudio",
        TextKey::AudioBitrate => "Bitrate de áudio (kBit/s)",
        TextKey::AudioBufferSize => "Tamanho do buffer de áudio (kBit)",
        TextKey::AudioChannels => "Canais",
        TextKey::AudioFormat => "Formato de áudio",
        TextKey::BitDepth => "Profundidade de bits",
        TextKey::Copy => "Copiar",
        TextKey::CopyBlock => "Copiar bloco",
        TextKey::CopyFailed => "Falha ao copiar",
        TextKey::CopyNameSuffix => "Cópia",
        TextKey::CopyProfile => "Copiar perfil",
        TextKey::CopyTemplate => "Copiar modelo",
        TextKey::Crop => "Cortar",
        TextKey::AllowWatermarks => "Permitir marcas d'água",
        TextKey::DeinterlaceVideo => "Desentrelaçar vídeo",
        TextKey::Edit => "Editar",
        TextKey::EditBlock => "Editar bloco",
        TextKey::EditCollection => "Editar coleção",
        TextKey::EditFfmpegProfile => "Editar perfil FFmpeg",
        TextKey::EditMultiCollection => "Editar multi-coleção",
        TextKey::EditRerunCollection => "Editar coleção de reprises",
        TextKey::EditTemplate => "Editar modelo",
        TextKey::Expression => "Expressão",
        TextKey::Fallback => "Fallback",
        TextKey::FillerKind => "Tipo de preenchimento",
        TextKey::General => "Geral",
        TextKey::HardwareAcceleration => "Aceleração de hardware",
        TextKey::HardwareIfPossible => "Hardware se possível",
        TextKey::ProfileDeleteDefault => "O perfil FFmpeg padrão não pode ser excluído.",
        TextKey::ProfileDeleteInUse => "Este perfil FFmpeg é usado por um canal e não pode ser excluído.",
        TextKey::ProfileDeleteFailed => "Não foi possível excluir o perfil FFmpeg.",
        TextKey::NewProfileName => "Novo nome do perfil",
        TextKey::NoneOption => "(Nenhum)",
        TextKey::NumHelp => "o número do ponto mid-roll, começando em 1",
        TextKey::OptionalDisplayTitle => "Título de exibição opcional...",
        TextKey::TranscodeAudio => "Transcodificar áudio",
        TextKey::NormalizeColors => "Normalizar cores",
        TextKey::NormalizeFramerate => "Normalizar taxa de quadros",
        TextKey::NormalizeLoudness => "Normalizar loudness",
        TextKey::TranscodeVideo => "Transcodificar vídeo",
        TextKey::Off => "Desligado",
        TextKey::Pad => "Preencher",
        TextKey::PadNearestFifteenMinutes => "15 (:00, :15, :30, :45)",
        TextKey::PadNearestFiveMinutes => "5 (:00, :05, :10, :15, :20 etc.)",
        TextKey::PadNearestTenMinutes => "10 (:00, :10, :20, :30, :40, :50)",
        TextKey::PadNearestThirtyMinutes => "30 (:00, :30)",
        TextKey::PadMode => "Modo de preenchimento",
        TextKey::PadToNearestMinute => "Preencher até o minuto mais próximo",
        TextKey::PointHelp => "a posição do ponto mid-roll, em segundos",
        TextKey::ProfileCopied => "Perfil copiado.",
        TextKey::ProfileNotFound => "Perfil não encontrado.",
        TextKey::QsvExtraHardwareFrames => "Frames extras de hardware QSV",
        TextKey::Resolution => "Resolução",
        TextKey::SampleRateKhz => "Taxa de amostragem (kHz)",
        TextKey::SaveDecoTemplate => "Salvar modelo de decoração",
        TextKey::SaveProfile => "Salvar perfil",
        TextKey::SaveFillerPreset => "Salvar predefinição de preenchimento",
        TextKey::SaveRerunCollection => "Salvar coleção de reprises",
        TextKey::SaveSequentialSchedule => "Salvar programação sequencial",
        TextKey::SaveTemplate => "Salvar modelo",
        TextKey::SaveBlock => "Salvar bloco",
        TextKey::ScaleAndPad => "Escalar e preencher",
        TextKey::ScalingBehavior => "Comportamento de escala",
        TextKey::Software => "Software",
        TextKey::Stretch => "Esticar",
        TextKey::TargetLoudnessLufs => "Loudness alvo (LUFS)",
        TextKey::ThreadCountAuto => "Quantidade de threads (0 = auto)",
        TextKey::NativeEngineOnlyHelp => "Somente o mecanismo nativo do ErsatzRS usa esta configuração. O ErsatzTV Next a ignora.",
        TextKey::OutputDynamicRange => "Faixa dinâmica de saída",
        TextKey::OutputDynamicRangeCopyHint => {
            "O vídeo é copiado, portanto a saída mantém a faixa dinâmica da origem."
        }
        TextKey::HdrStaticMetadataHint => {
            "Deixe em branco para usar os padrões HDR10 documentados. Eles se aplicam apenas quando a origem não traz metadados HDR próprios; o monitor de masterização e os níveis de luz da origem sempre prevalecem."
        }
        TextKey::HdrMasteringDisplay => "Monitor de masterização",
        TextKey::HdrMaxCll => "MaxCLL (cd/m²)",
        TextKey::HdrMaxFall => "MaxFALL (cd/m²)",
        TextKey::TonemapAlgorithm => "Algoritmo de tonemapping",
        TextKey::VaapiDevicePath => "Caminho do dispositivo VAAPI",
        TextKey::VaapiDisplay => "Display VAAPI",
        TextKey::VaapiDriver => "Driver VAAPI",
        TextKey::Video => "Vídeo",
        TextKey::VideoBitrate => "Bitrate de vídeo (kBit/s)",
        TextKey::VideoBufferSize => "Tamanho do buffer de vídeo (kBit)",
        TextKey::VideoFormat => "Formato de vídeo",
        TextKey::VideoPreset => "Predefinição de vídeo",
        TextKey::VideoTune => "Ajuste de vídeo",
        TextKey::VideoProfile => "Perfil de vídeo",
        TextKey::ViewCollection => "Ver coleção",
        TextKey::ViewPlaylist => "Ver playlist",
        TextKey::AddAll => "Adicionar tudo",
        TextKey::AddAllToCollection => "Adicionar tudo à coleção",
        TextKey::AddAllToPlaylist => "Adicionar tudo à playlist",
        TextKey::AddedItemToPlaylist => "Item adicionado à playlist.",
        TextKey::AddedArtistToSchedule => "Artista adicionado ao horário",
        TextKey::AddedMovieToSchedule => "Filme adicionado ao horário",
        TextKey::AddedSeasonToSchedule => "Temporada adicionada ao horário",
        TextKey::AddedShowToSchedule => "Série adicionada ao horário",
        TextKey::AddingItem => "Adicionando item...",
        TextKey::AddingItemToPlaylist => "Adicionando item à playlist...",
        TextKey::Artists => "Artistas",
        TextKey::ClearSelection => "Limpar seleção",
        TextKey::Collection => "Coleção",
        TextKey::CollectionNameRequired => "Nome da coleção é obrigatório",
        TextKey::CollectionNotFound => "Coleção não encontrada",
        TextKey::CollectionRenamed => "Coleção renomeada.",
        TextKey::EditAddonMediaListProperties => "Editar propriedades da lista add-on",
        TextKey::EnterAddonMediaListUrl => "Insira a URL da lista add-on",
        TextKey::EnterSearchQuery => "Digite uma consulta de pesquisa.",
        TextKey::Episodes => "Episódios",
        TextKey::LoadingCollections => "Carregando coleções...",
        TextKey::LoadingPlaylistGroups => "Carregando grupos de playlists...",
        TextKey::LoadingPlaylists => "Carregando playlists...",
        TextKey::GeneratePlaylist => "Criar playlist na ordem original",
        TextKey::GeneratePlaylistFromSortedAddonMediaListItems => {
            "Manter a ordem original do provedor após cada publicação"
        }
        TextKey::MusicVideos => "Vídeos musicais",
        TextKey::NewCollectionName => "Nome da nova coleção",
        TextKey::NoMediaMatched => "Nenhuma mídia encontrada.",
        TextKey::NoItemsToAdd => "Nenhum item para adicionar.",
        TextKey::NoItemsToPreview => "Nenhum item para pré-visualizar.",
        TextKey::NoPlaylistsInGroup => "Nenhuma playlist neste grupo.",
        TextKey::PleaseChoosePlaylist => "Escolha uma playlist.",
        TextKey::RequestFailed => "Falha na solicitação",
        TextKey::Released => "Lançado",
        TextKey::SaveAs => "Salvar como",
        TextKey::Seasons => "Temporadas",
        TextKey::SeeAll => "Ver tudo >>",
        TextKey::SelectAll => "Selecionar tudo",
        TextKey::SelectGroup => "(Selecione um grupo)",
        TextKey::SelectPlaylist => "(Selecione uma playlist)",
        TextKey::SelectSearchCollectionHelp => {
            "Selecione a coleção para adicionar os resultados da pesquisa."
        }
        TextKey::SelectSearchPlaylistHelp => {
            "Selecione a playlist para adicionar os resultados da pesquisa."
        }
        TextKey::Selected => "Selecionado",
        TextKey::SelectCollectionToAddThisMovie => {
            "Selecione uma coleção para adicionar este filme."
        }
        TextKey::SelectPlaylistToAddThisMovie => {
            "Selecione uma playlist para adicionar este filme."
        }
        TextKey::Add => "Adicionar",
        TextKey::AddLibrary => "Adicionar biblioteca",
        TextKey::Address => "Endereço",
        TextKey::AddressRequired => "Endereço é obrigatório.",
        TextKey::ApiKey => "Chave de API",
        TextKey::ApiKeyRequired => "Chave de API é obrigatória.",
        TextKey::AuthorizedRedirecting => "Autorizado - redirecionando...",
        TextKey::CheckingEveryTwoSeconds => "Verificando a cada 2 segundos...",
        TextKey::ClickAuthorizePlex => "Clique aqui para autorizar no Plex.tv",
        TextKey::ClickPlayoutItemHelp => {
            "Clique em um item da programação para inspecionar título, horários e contexto de agendamento."
        }
        TextKey::ConfigureConnection => "Configurar conexão",
        TextKey::Connection => "Conexão",
        TextKey::DeepScanCollections => "Varredura profunda de coleções",
        TextKey::DeepScanLibrary => "Varredura profunda da biblioteca",
        TextKey::DeepScanConfirmationQuestion => "Deseja realmente fazer uma varredura profunda de {name}?",
        TextKey::DeepScanConfirmationWarning => "Uma varredura profunda atualiza tudo, até os itens que parecem inalterados, e pode demorar muito mais que uma varredura normal.",
        TextKey::LibraryScanJavascriptRequired => "JavaScript é necessário para verificar bibliotecas e coleções externas. Ative JavaScript para revisar e confirmar uma varredura profunda.",
        TextKey::DeepScan => "Varredura profunda",
        TextKey::EditLibrary => "Editar biblioteca",
        TextKey::EditPlaylist => "Editar playlist",
        TextKey::EnterOnePathPerLine => "Digite um caminho por linha.",
        TextKey::Error => "Erro",
        TextKey::ExternalCollections => "Coleções externas",
        TextKey::Information => "Informação",
        TextKey::Kind => "Tipo",
        TextKey::Library => "Biblioteca",
        TextKey::LibraryKind => "Tipo de biblioteca",
        TextKey::LibraryPaths => "Caminhos da biblioteca",
        TextKey::FolderPreview => "Examinar pastas de séries",
        TextKey::FolderPreviewHelp => "Para séries de TV: examine até oito caminhos absolutos antes de salvar. A prévia lê no máximo 10.000 entradas, 32 níveis e dois segundos de trabalho no sistema de arquivos; não altera arquivos nem configurações.",
        TextKey::FolderPreviewCandidates => "São arquivos candidatos, não mídias decodificadas. São exibidos até 40 exemplos de diagnóstico por pasta. Estrutura esperada: biblioteca/série/temporada/arquivos; pastas intermediárias não são exploradas automaticamente.",
        TextKey::FolderPreviewPartial => "Prévia parcial: um limite foi atingido. Os números não abrangem toda a pasta.",
        TextKey::FolderPreviewUnavailable => "Prévia indisponível. Selecione séries de TV e informe de um a oito caminhos absolutos (máximo de 4.096 bytes cada). Verifique o acesso e tente novamente; outra prévia pode estar em andamento.",
        TextKey::FolderIgnoredExtension => "Extensão ignorada",
        TextKey::FolderUnsupportedNesting => "Aninhamento não suportado ou vídeo fora de lugar",
        TextKey::FolderEmptyShow => "Séries sem mídia encontrada",
        TextKey::FolderUnreadable => "Entrada ilegível",
        TextKey::FolderRepeated => "Pasta repetida ou ciclo de links",
        TextKey::FolderDepthLimit => "Limite de profundidade de pastas",
        TextKey::FolderInspected => "Entradas examinadas",
        TextKey::FolderExtras => "Vídeos extras candidatos",
        TextKey::FolderHidden => "Entradas ocultas",
        TextKey::LocalLibraryPathAbsolute => "O caminho [{path}] deve ser absoluto.",
        TextKey::LocalLibraryPathExists => "O caminho [{path}] deve ser uma pasta existente.",
        TextKey::LocalLibraryPathConflict => "O caminho [{path}] se sobrepõe ao caminho de outra biblioteca.",
        TextKey::LocalLibraries => "Bibliotecas locais",
        TextKey::LocalPath => "Caminho local",
        TextKey::MediaKind => "Tipo de mídia",
        TextKey::NoLibrariesConfigured => {
            "Nenhuma biblioteca configurada. Adicione uma fonte de mídia primeiro."
        }
        TextKey::NoSchedulingContext => "Nenhum contexto de agendamento",
        TextKey::NoPlayoutItemsInWindow => "Nenhum item de programação nesta janela de 24 horas",
        TextKey::NoPlayoutsFound => "Nenhuma programação encontrada.",
        TextKey::NoPathReplacements => "Nenhuma substituição de caminho.",
        TextKey::Paths => "Caminhos",
        TextKey::PathReplacements => "Substituições de caminho",
        TextKey::PlexSignInHelp => "Entre no Plex para adicionar uma fonte de mídia.",
        TextKey::Remote => "Remoto",
        TextKey::RemotePath => "Caminho remoto",
        TextKey::Remove => "Remover",
        TextKey::RemoveBlockFromTemplate => "Remover bloco do modelo",
        TextKey::RemoveContent => "Remover conteúdo",
        TextKey::Retry => "Tentar novamente",
        TextKey::Saved => "Salvo.",
        TextKey::Saving => "Salvando...",
        TextKey::QuickScan => "Varredura rápida",
        TextKey::ScanCollections => "Escanear coleções",
        TextKey::ScanFailed => "Falha na varredura",
        TextKey::ScanLibrary => "Escanear biblioteca",
        TextKey::ScanQueued => "Varredura enfileirada",
        TextKey::Scanning => "Escaneando...",
        TextKey::Server => "Servidor",
        TextKey::ServerAddress => "Endereço do servidor",
        TextKey::PlexRefreshFailed => "Não foi possível atualizar as bibliotecas Plex. Verifique a conexão com o servidor e tente novamente.",
        TextKey::ReauthenticatePlex => "Entrar novamente no Plex",
        TextKey::PlexMissingServer => "Servidor ausente no Plex",
        TextKey::PlexMissingHelp => "Este servidor não aparece mais na sua conta Plex. Reivindique-o novamente no Plex ou remova-o aqui. O conteúdo é mantido e as verificações estão pausadas.",
        TextKey::PlexSignedOutHelp => "Entre novamente para restaurar o acesso ao Plex. Suas fontes de mídia e o conteúdo sincronizado são mantidos.",
        TextKey::PlexRemoveServerHelp => "Remover este servidor exclui suas bibliotecas e o conteúdo sincronizado do ErsatzRS. Os arquivos no servidor Plex não são excluídos.",
        TextKey::PlexSignInFailed => "O login no Plex falhou ou expirou. Tente novamente.",
        TextKey::SignInToPlex => "Entrar no Plex",
        TextKey::SignOutOfPlex => "Sair do Plex",
        TextKey::Sync => "Sincronizar",
        TextKey::WaitingForPlexAuthorization => "Aguardando autorização do Plex.",
        TextKey::AddChannel => "Adicionar canal",
        TextKey::Any => "Qualquer",
        TextKey::BlockPlayoutTroubleshooting => "Solução de problemas da programação por blocos",
        TextKey::BlockPlayoutHistory => "Histórico da programação por blocos",
        TextKey::Categories => "Categorias",
        TextKey::CategoriesHelp => "Lista de categorias separadas por vírgula",
        TextKey::Channel => "Canal",
        TextKey::ChannelCouldNotBeSaved => {
            "Não foi possível salvar o canal. Verifique os valores do formulário e tente novamente."
        }
        TextKey::ChannelDisabled => "Canal desativado",
        TextKey::ChannelHiddenFromEpg => "Canal oculto do EPG",
        TextKey::ChannelLogo => "Logotipo do canal",
        TextKey::ChannelLogoPreview => "Prévia do logotipo do canal",
        TextKey::ChannelLogoUploadFailed => {
            "Falha ao enviar o logotipo do canal. Verifique o arquivo e tente novamente."
        }
        TextKey::Check => "Verificação",
        TextKey::Continuous => "Contínuo",
        TextKey::DateAndTime => "Data e hora",
        TextKey::DefaultOption => "Padrão",
        TextKey::DefaultSchedule => "Horário padrão",
        TextKey::DeleteChannel => "Excluir canal",
        TextKey::DownloadMediaSample => "Baixar amostra de mídia",
        TextKey::DownloadResults => "Baixar resultados",
        TextKey::DisableWatermarks => "Desativar marcas d'água",
        TextKey::EditChannel => "Editar canal",
        TextKey::EditChannelNumbers => "Editar números dos canais",
        TextKey::EditNumbers => "Editar números",
        TextKey::ExternalLogoUrl => "URL externa do logotipo",
        TextKey::Fail => "Falha",
        TextKey::FallbackFiller => "Preenchimento de fallback",
        TextKey::FfmpegProfile => "Perfil FFmpeg",
        TextKey::FirstPage => "Primeira página",
        TextKey::FirstRunPlaybackOrder => "Ordem de reprodução da primeira exibição",
        TextKey::Forced => "Forçada",
        TextKey::GenerateSubtitles => "Gerar legendas",
        TextKey::Generated => "Gerado",
        TextKey::Details => "Detalhes",
        TextKey::Full => "Completo",
        TextKey::Group => "Grupo",
        TextKey::FfmpegRuntime => "Runtime FFmpeg",
        TextKey::FfmpegRuntimeDownloaded => "Runtime FFmpeg baixado",
        TextKey::FfmpegRuntimeLatestRelease => "Versão mais recente",
        TextKey::FfmpegRuntimeRelease => "Versão instalada",
        TextKey::FfmpegRuntimeSource => "Origem",
        TextKey::FfmpegRuntimeSourceDownloaded => "Baixado",
        TextKey::FfmpegRuntimeSourceManaged => "Gerenciado",
        TextKey::FfmpegRuntimeSourceOverride => "Substituição",
        TextKey::FfmpegRuntimeSourcePath => "PATH",
        TextKey::FfmpegRuntimeSourceUnknown => "Desconhecido",
        TextKey::FfmpegRuntimeUpdateAvailable => "Atualização disponível",
        TextKey::FfmpegRuntimeUpdateCheckFailed => "Falha ao verificar atualização",
        TextKey::FfmpegRuntimeUpdateFailed => "Falha ao atualizar runtime FFmpeg",
        TextKey::FfmpegRuntimeUpToDate => "Atualizado",
        TextKey::FfmpegRuntimeUpdated => "Runtime FFmpeg atualizado",
        TextKey::FfmpegRuntimeVersion => "Versão",
        TextKey::HealthNoPersistedRun => "Nenhuma verificação de integridade se aplica agora.",
        TextKey::HealthPersistedRunHelp => {
            "Os resultados são armazenados por cinco minutos. Atualize para verificar novamente agora. Esta página também é atualizada quando as varreduras de mídia alteram o estado dos itens."
        }
        TextKey::HistoryDetails => "Detalhes do histórico",
        TextKey::HlsDirect => "HLS direto",
        TextKey::HlsSegmenter => "Segmentador HLS",
        TextKey::Hours => "horas",
        TextKey::IdleBehavior => "Comportamento ocioso",
        TextKey::IdleBehaviorHelp => {
            "Controla como a transcodificação se comporta depois que todos os clientes se desconectam"
        }
        TextKey::Info => "Informação",
        TextKey::Import => "Importação",
        TextKey::IsEnabled => "Está ativado",
        TextKey::Json => "JSON",
        TextKey::KeepRunning => "Continuar executando",
        TextKey::Key => "Chave",
        TextKey::LastMidFillerHelp => "segundos desde o último preenchimento mid-roll",
        TextKey::LastPage => "Última página",
        TextKey::Level => "Nível",
        TextKey::LoadingMediaInfo => "Carregando informações da mídia...",
        TextKey::Logs => "Logs",
        TextKey::PlaybackSpeed => "Velocidade",
        TextKey::Logo => "Logotipo",
        TextKey::MediaItem => "Item de mídia",
        TextKey::Message => "Mensagem",
        TextKey::MissingMediaItemQueryParameter => "Parâmetro de consulta mediaItem ausente",
        TextKey::Mirror => "Espelho",
        TextKey::MirrorOptionsHelp => {
            "Disponível somente quando a origem da programação está definida como Espelho."
        }
        TextKey::MirrorSourceChannel => "Canal de origem do espelho",
        TextKey::MusicVideoCreditsMode => "Modo de créditos de videoclipes",
        TextKey::MusicVideoCreditsTemplate => "Modelo de créditos de videoclipes",
        TextKey::NoMatchingRecordsFound => "Nenhum registro correspondente encontrado",
        TextKey::NoFfmpegProfiles => "Nenhum perfil FFmpeg encontrado. Crie um primeiro.",
        TextKey::NoBlockPlayoutSelected => {
            "Selecione uma programação por blocos para inspecionar seus blocos."
        }
        TextKey::NoBlocks => "Nenhum bloco encontrado.",
        TextKey::NoHistory => "Nenhum histórico encontrado.",
        TextKey::NoMessages => "Nenhuma mensagem.",
        TextKey::NvidiaCapabilitiesNotCollected => "Os recursos NVIDIA ainda não foram coletados.",
        TextKey::Number => "Número",
        TextKey::OnDemand => "Sob demanda",
        TextKey::MatchedPointsHelp => "número de pontos mid-roll que já corresponderam à expressão",
        TextKey::Pass => "Aprovado",
        TextKey::ParityDebt => "ParityDebt",
        TextKey::PlaybackTroubleshooting => "Solução de problemas de reprodução",
        TextKey::Play => "Reproduzir",
        TextKey::PlayoutTimeline => "Linha do tempo da programação",
        TextKey::PlayoutMode => "Modo de programação",
        TextKey::PlayoutModeHelp => "Controla a progressão da programação do canal",
        TextKey::PlayoutOffset => "Deslocamento da programação",
        TextKey::PlayoutSource => "Origem da programação",
        TextKey::PlayoutSourceHelp => {
            "Controla a origem do conteúdo do canal. Isso não pode ser alterado quando o canal já tem uma programação."
        }
        TextKey::Preview => "Prévia",
        TextKey::PreviewBlockPlayout => "Prévia da programação do bloco",
        TextKey::PreviewChannel => "Prévia do canal",
        TextKey::ChannelPreview => "Prévia do canal",
        TextKey::ReloadStream => "Recarregar transmissão",
        TextKey::StopPreview => "Parar prévia",
        TextKey::OpenFullPlayer => "Abrir player completo",
        TextKey::BackToChannels => "Voltar aos canais",
        TextKey::NowPlaying => "Em reprodução",
        TextKey::UpNext => "A seguir",
        TextKey::ProgrammeGuide => "Guia de programação",
        TextKey::EpgPreview => "Prévia do EPG",
        TextKey::EpgPreviewDescription => {
            "Consulte o guia publicado com duas horas de histórico e 24 horas à frente."
        }
        TextKey::EpgZoom => "Zoom",
        TextKey::EpgNow => "Agora",
        TextKey::NoEpgChannels => "Nenhum canal habilitado está visível no EPG.",
        TextKey::ScheduleGap => "Lacuna na programação",
        TextKey::Filler => "Preenchimento",
        TextKey::NoProgramming => "Não há programação agendada neste período.",
        TextKey::ChannelDisabledPreview => "Este canal está desativado.",
        TextKey::ChannelNotAiring => "Este canal não está transmitindo agora.",
        TextKey::ChannelNoPlayout => "Este canal não tem programação.",
        TextKey::PreviewIdle => "A prévia inicia automaticamente sem som.",
        TextKey::PreviewConnecting => "Conectando à transmissão ao vivo…",
        TextKey::PreviewErrorSlate => "Tela de erro: a reprodução do programa falhou.",
        TextKey::PreviewOutputUnknown => "O estado de saída do servidor não está disponível.",
        TextKey::PreviewLive => "A transmissão ao vivo está em reprodução.",
        TextKey::PreviewPaused => "Prévia pausada.",
        TextKey::PreviewRecovering => "Recuperando reprodução…",
        TextKey::PreviewStopped => "Prévia interrompida.",
        TextKey::PreviewUnsupported => "Este navegador não reproduz vídeo HLS.",
        TextKey::PreviewAuthorizationFailed => "Falha na autorização da prévia.",
        TextKey::PreviewQueued => "Prévia na fila",
        TextKey::PreviewRunning => "Renderizando a prévia",
        TextKey::PreviewCompleted => "Renderização concluída",
        TextKey::PreviewCancelling => "Parando a prévia; limpeza em andamento",
        TextKey::PreviewProgrammeSeek => "Segundos desde o início do programa",
        TextKey::PreviewProgrammeSeekHelp => "Deixe vazio para iniciar no horário UTC escolhido. A busca permanece no programa selecionado.",
        TextKey::PreviewStartHelp => "O momento da programação do canal a visualizar. A data e a hora em UTC são obrigatórias.",
        TextKey::PreviewStartRequired => "A data e a hora são obrigatórias.",
        TextKey::PreviewStartInvalid => "Escolha uma data e hora válidas em UTC.",
        TextKey::PreviewTimestamp => "Data e hora da prévia (UTC):",
        TextKey::PreviewAlreadyRunning => "Uma prévia de diagnóstico já está em andamento. Aguarde sua conclusão antes de iniciar outra.",
        TextKey::PreviewCleanupUnconfirmed => "Não foi possível confirmar a limpeza da prévia. Reinicie o servidor antes de tentar novamente.",
        TextKey::PreviewConfigurationInvalid => "O modo de transmissão do canal é inválido. Salve um modo compatível no editor do canal e tente novamente.",
        TextKey::PreviewStartupFailed => "Não foi possível iniciar a prévia. Consulte o erro de inicialização no log do servidor e verifique a mídia e o perfil FFmpeg.",
        TextKey::PreviewUnavailable => "A transmissão ao vivo está indisponível.",
        TextKey::PreviewCapacityReached => "Todas as vagas de transcodificação estão em uso.",
        TextKey::PreviewNetworkError => "A prévia perdeu a conexão de rede.",
        TextKey::PreviewMediaError => {
            "O navegador não conseguiu decodificar a saída configurada deste canal."
        }
        TextKey::PreviewDerivativeNote => {
            "A prévia administrativa usa uma versão H.264/AAC compatível com o navegador; a saída configurada do canal não é alterada."
        }
        TextKey::JavascriptRequiredPlayback => {
            "JavaScript é necessário para reprodução interativa."
        }
        TextKey::ConfirmStopTranscodeSession => {
            "Parar a sessão de transcodificação compartilhada deste canal?"
        }
        TextKey::PreviewPlaylistPlayout => "Prévia da programação da playlist",
        TextKey::PreferredAudioTitle => "Título de áudio preferido",
        TextKey::PreferredSubtitleLanguage => "Idioma de legenda preferido",
        TextKey::QsvCapabilitiesNotCollected => "Os recursos QSV ainda não foram coletados.",
        TextKey::RemoveStoredLogo => "Remover logotipo armazenado",
        TextKey::SaveChannel => "Salvar canal",
        TextKey::SaveChannels => "Salvar canais",
        TextKey::Second => "segundo",
        TextKey::Seconds => "segundos",
        TextKey::SelectProfile => "- Selecione um perfil -",
        TextKey::SequentialSchedule => "Programação sequencial",
        TextKey::SequentialScheduleValidation => "Validação de programação sequencial",
        TextKey::SeekSeconds => "Segundos de busca",
        TextKey::Schema => "Esquema",
        TextKey::ShowDisabled => "Mostrar desativados",
        TextKey::ShowInEpg => "Mostrar no EPG",
        TextKey::SlugSeconds => "Segundos de intervalo",
        TextKey::SlugSecondsHelp => {
            "Segundos de vídeo preto e áudio silencioso inseridos entre cada item da programação"
        }
        TextKey::Song => "Música",
        TextKey::SongVideoMode => "Modo de vídeo de música",
        TextKey::SortById => "Ordenar por ID",
        TextKey::SortByLevel => "Ordenar por nível",
        TextKey::SortByName => "Ordenar por nome",
        TextKey::SortByTimestamp => "Ordenar por data/hora",
        TextKey::StopOnDisconnect => "Parar ao desconectar",
        TextKey::StopScheduling => "Parar agendamento",
        TextKey::StopSchedulingBlockItems => "Parar agendamento dos itens do bloco",
        TextKey::StopTranscodeSession => "Parar sessão de transcodificação",
        TextKey::StartFromBeginning => "Iniciar do começo",
        TextKey::StreamSelector => "Seletor de stream",
        TextKey::StreamSelectorMode => "Modo do seletor de stream",
        TextKey::InvalidNextEngineTextSubtitleMode => "Modo de legendas de texto do mecanismo Next inválido",
        TextKey::SourceTeletextDelivery => "Legendas do teletexto da fonte",
        TextKey::SourceTeletextDeliveryHelp => "Escolha como transmitir as legendas de teletexto selecionadas na fonte. As faixas DVB exigem atualmente saída MPEG-TS nativa ou Hybrid. Isso não altera a seleção de legendas nem as páginas de teletexto criadas.",
        TextKey::SourceTeletextDeliveryBurn => "Incorporar ao vídeo",
        TextKey::SourceTeletextDeliveryDvb => "Faixa de legendas DVB",
        TextKey::InvalidSourceTeletextDelivery => "Transmissão de legendas do teletexto da fonte inválida",
        TextKey::SourceTeletextDeliveryUnsupported => "Legendas DVB do teletexto exigem saída MPEG-TS nativa ou Hybrid",
        TextKey::TeletextCarriageReady => "Os requisitos para transmitir legendas DVB estão disponíveis. A preparação por fonte e a exibição no cliente ainda precisam ser validadas.",
        TextKey::TeletextCarriageMissing => "A transmissão de legendas DVB está configurada, mas falta o suporte necessário ao FFmpeg. Selecione um ambiente compatível; a reprodução normal continua disponível.",
        TextKey::TeletextCarriageInvalid => "Um canal tem configurações inválidas para transmitir teletexto da fonte. Revise a opção de transmissão de legendas; a reprodução normal continua disponível.",
        TextKey::TeletextCarriageOutput => "Um canal usa legendas DVB com um mecanismo ou uma saída sem suporte. Selecione MPEG-TS nativo ou altere a transmissão de legendas; a reprodução normal continua disponível.",
        TextKey::NextEngineTextSubtitleModeHelp => "As legendas de texto convertidas estão disponíveis apenas via HLS. Selecione Incorporar no vídeo para legendas em MPEG-TS.",
        TextKey::NextEngineTextSubtitleMode => "Modo de legendas de texto do mecanismo Next",
        TextKey::SubtitleBurn => "Incorporar no vídeo",
        TextKey::SubtitleConvertIfPossible => "Converter se possível",
        TextKey::PreviewCaptions => "Legendas",
        TextKey::StreamingEngine => "Mecanismo de streaming",
        TextKey::InvalidStreamingEngine => "Mecanismo de streaming inválido",
        TextKey::StreamingMode => "Modo de streaming",
        TextKey::Styles => "Estilos",
        TextKey::Subtitle => "Legenda",
        TextKey::SubtitleMode => "Modo de legenda",
        TextKey::SubtitleModeHelp => {
            "Forçada mostra apenas faixas de narrativa forçada (forced narrative). Escolha \
             Qualquer para sempre exibir legendas no seu idioma preferido."
        }
        TextKey::Timestamp => "Data/hora",
        TextKey::Timeline => "Linha do tempo",
        TextKey::Target => "Destino",
        TextKey::Tags => "Tags",
        TextKey::Tools => "Ferramentas",
        TextKey::TranscodeMode => "Modo de transcodificação",
        TextKey::TranscodeModeHelp => "Controla quando o processo de transcodificação fica ativo",
        TextKey::TroubleshootPlayback => "Solucionar problemas de reprodução",
        TextKey::TroubleshootChannel => "Solucionar problemas do canal",
        TextKey::TroubleshootingReports => "Relatórios de solução de problemas",
        TextKey::MpegTsTroubleshooting => "Solução de problemas de MPEG-TS",
        TextKey::InternalMpegTs => "MPEG-TS interno",
        TextKey::PublicMpegTs => "MPEG-TS público",
        TextKey::RetainRawSample => "Manter amostra bruta",
        TextKey::TsdDuckVersion => "Versão do TSDuck",
        TextKey::Report => "Relatório",
        TextKey::RunId => "ID da execução",
        TextKey::Status => "Status",
        TextKey::Summary => "Resumo",
        TextKey::SampleBytes => "Bytes da amostra",
        TextKey::RawSampleArchived => "Amostra bruta arquivada",
        TextKey::ArchivePath => "Caminho do arquivo",
        TextKey::SampleUrl => "URL da amostra",
        TextKey::StabilityMeter => "Medidor de estabilidade",
        TextKey::Stable => "Estável",
        TextKey::Packets => "pacotes",
        TextKey::Services => "Serviços",
        TextKey::Pids => "PIDs",
        TextKey::ContinuityFindings => "Achados de continuidade",
        TextKey::PcrFindings => "Achados de PCR",
        TextKey::Bitrate => "Taxa de bits",
        TextKey::AnalysisJson => "JSON da análise",
        TextKey::StandardOutput => "Saída padrão",
        TextKey::StandardError => "Erro padrão",
        TextKey::NotAvailable => "Não disponível",
        TextKey::NotApplicable => "Não aplicável",
        TextKey::Completed => "Concluído",
        TextKey::Failed => "Falhou",
        TextKey::TroubleshootingPlaybackStartDebt => {
            "A prévia de reprodução de item de mídia e canal agora grava relatórios FFmpeg, logs, media_info.json, troubleshooting_info.json, o YAML do seletor de stream escolhido e arquivos de recursos, executa regras do seletor de stream e redireciona para /iptv/session/.troubleshooting/live.m3u8."
        }
        TextKey::UnableToLoadMediaInfo => "Não foi possível carregar as informações de mídia",
        TextKey::UnableToLoadPlaybackTroubleshooting => {
            "Não foi possível carregar o contexto de solução de problemas de reprodução"
        }
        TextKey::UploadLogo => "Enviar logotipo",
        TextKey::UploadLogoHelp => {
            "Escolha uma imagem local para substituir o logotipo do canal armazenado ao salvar"
        }
        TextKey::Warning => "Aviso",
        TextKey::Watermark => "Marca d'água",
        TextKey::WatermarkIds => "IDs de marcas d'água",
        TextKey::Window => "Janela",
        TextKey::WithProgress => "Com progresso",
        TextKey::Writers => "Roteiristas",
        TextKey::Validate => "Validar",
        TextKey::Yaml => "YAML",
        TextKey::YamlFile => "Arquivo YAML",
        TextKey::Messages => "Mensagens",
        TextKey::CollectionName => "Nome da coleção",
        TextKey::MediaItemType => "Tipo de item de mídia",
        TextKey::MediaItemTitle => "Título do item de mídia",
        TextKey::ActualSize => "Tamanho real",
        TextKey::AddWatermark => "Adicionar marca d'água",
        TextKey::AspectRatioGuides => "Guias de proporção",
        TextKey::BottomLeft => "Inferior esquerdo",
        TextKey::BottomMiddle => "Inferior central",
        TextKey::BottomRight => "Inferior direito",
        TextKey::CopyWatermark => "Copiar marca d'água",
        TextKey::CopyOf => "Cópia de",
        TextKey::CurrentImage => "Imagem atual",
        TextKey::Custom => "Personalizado",
        TextKey::ChannelLogoPreviewUnavailable => {
            "A prévia do logotipo do canal requer o contexto de um canal. Escolha uma imagem personalizada aqui."
        }
        TextKey::DeleteWatermark => "Excluir marca d'água",
        TextKey::DurationSeconds => "Duração (segundos)",
        TextKey::EditWatermark => "Editar marca d'água",
        TextKey::EnableWatermarkToPreview => "Ative a marca d'água para ver seu posicionamento.",
        TextKey::Frequency => "Frequência",
        TextKey::GeneratingPreview => "Gerando prévia...",
        TextKey::HorizontalMarginPercent => "Margem horizontal (%)",
        TextKey::GuideFourThree => "Quadro seguro 4:3",
        TextKey::Identity => "Identidade",
        TextKey::Image => "Imagem",
        TextKey::ImageSource => "Fonte da imagem",
        TextKey::Intermittent => "Intermitente",
        TextKey::IntermittentTiming => "Temporização intermitente",
        TextKey::LeftMiddle => "Meio esquerdo",
        TextKey::Location => "Local",
        TextKey::MiddleCenter => "Centro",
        TextKey::Mode => "Modo",
        TextKey::Minutes => "minutos",
        TextKey::Moods => "Humores",
        TextKey::NewName => "Novo nome",
        TextKey::NativeAssetDimensions => "Tamanho original e renderizado",
        TextKey::NextWatermark => "Próxima marca d'água",
        TextKey::Opacity => "Opacidade",
        TextKey::OpacityExpression => "Expressão de opacidade",
        TextKey::OpacityOneToHundred => "Opacidade (1-100)",
        TextKey::Permanent => "Permanente",
        TextKey::PerPass => "por passagem",
        TextKey::PlaceWithinSourceContent => "Posicionar dentro do conteúdo de origem",
        TextKey::Placement => "Posicionamento",
        TextKey::PlacementPreview => "Prévia do posicionamento",
        TextKey::PreviewFailed => "Falha na prévia",
        TextKey::PreviewReferenceFullHd => "Quadro de referência Full HD · 1920×1080",
        TextKey::PreviousWatermark => "Marca d'água anterior",
        TextKey::RightMiddle => "Meio direito",
        TextKey::SaveWatermark => "Salvar marca d'água",
        TextKey::Scaled => "Escalado",
        TextKey::SelectCustomImageToPreview => {
            "Selecione ou envie uma imagem personalizada para visualizar."
        }
        TextKey::Size => "Tamanho",
        TextKey::TopLeft => "Superior esquerdo",
        TextKey::TopMiddle => "Superior central",
        TextKey::TopRight => "Superior direito",
        TextKey::UploadFailed => "Falha no upload.",
        TextKey::UploadImage => "Enviar imagem",
        TextKey::UnsavedChanges => "Alterações não salvas",
        TextKey::UnsavedWatermarkChangesMessage => {
            "Descartar as alterações da marca d'água e abrir a marca d'água selecionada?"
        }
        TextKey::VerticalMarginPercent => "Margem vertical (%)",
        TextKey::WatermarkCopied => "Marca d'água copiada.",
        TextKey::WatermarkNotFound => "Marca d'água não encontrada.",
        TextKey::WatermarkPlacementPreview => "prévia do posicionamento da marca d'água",
        TextKey::WatermarkPreview => "prévia da marca d'água",
        TextKey::WatermarkNavigation => "Navegação entre marcas d'água",
        TextKey::WidthPercent => "Largura (%)",
        TextKey::ZIndex => "Índice Z",
        TextKey::DiscardChanges => "Descartar alterações",
        TextKey::XmltvBlockBehavior => "Comportamento de blocos XMLTV",
        TextKey::XmltvTimeZone => "Fuso horário XMLTV",
        TextKey::Xmltv => "XMLTV",
        TextKey::Badge => "Badge",
        TextKey::Buttons => "Botões",
        TextKey::CheckValue => "Marcação",
        TextKey::ChannelName => "Nome do canal",
        TextKey::ChannelNumber => "Número do canal",
        TextKey::ComponentGallery => "Galeria de componentes ErsatzRS",
        TextKey::DangerButton => "Perigo",
        TextKey::DeleteDialog => "Diálogo de exclusão",
        TextKey::Decrement => "Diminuir",
        TextKey::Dialogs => "Diálogos",
        TextKey::DisabledButton => "Desativado",
        TextKey::EnterName => "Digite o nome",
        TextKey::ErrorOccurred => "Ocorreu um erro.",
        TextKey::ErrorToast => "Toast de erro",
        TextKey::ExampleModal => "Modal de exemplo",
        TextKey::ExampleModalBody => "Este é um exemplo de corpo de diálogo modal.",
        TextKey::FormInputs => "Campos de formulário",
        TextKey::Icons => "Ícones",
        TextKey::IncrementBadge => "Aumentar badge",
        TextKey::InfoToast => "Toast informativo",
        TextKey::JustFyi => "Apenas para sua informação.",
        TextKey::LinkButton => "Botão de link",
        TextKey::ModalCancelled => "Cancelado.",
        TextKey::ModalConfirmed => "Confirmado!",
        TextKey::NumValue => "Número",
        TextKey::OpenModal => "Abrir modal",
        TextKey::OperationSucceeded => "Operação concluída com sucesso.",
        TextKey::PagedTableDemo => "PagedTable (5 linhas por página, 7 no total)",
        TextKey::PrimaryButton => "Primário",
        TextKey::SecondaryButton => "Secundário",
        TextKey::SelectValue => "Seleção",
        TextKey::SomethingMightBeWrong => "Algo pode estar errado.",
        TextKey::SuccessToast => "Toast de sucesso",
        TextKey::TextButton => "Texto",
        TextKey::TextValue => "Texto",
        TextKey::Toasts => "Toasts",
        TextKey::WarningToast => "Toast de aviso",
        TextKey::YamlScript => "Script YAML",
        TextKey::Yes => "Sim",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::i18n::{
        addon_media_list_error_text, episode_display_subtitle_text, ffmpeg_loudness_mode_text,
        ffmpeg_profile_error_text, ffmpeg_profile_video_format_text, ffmpeg_tonemap_algorithm_text,
        filler_error_text, hardware_acceleration_option_text, log_level_text,
        media_action_error_text, media_browser_error_text, media_kind_text,
        media_source_error_text, playout_schedule_kind_text, schedule_error_text,
        scheduling_content_error_text, settings_error_text, upload_error_text,
        watermark_error_text, UiLanguage,
    };

    #[test]
    fn season_display_names() {
        assert_eq!(STRINGS.season_display_name(2), "Temporada 2");
    }

    #[test]
    fn episode_display_subtitles() {
        assert_eq!(
            episode_display_subtitle_text(UiLanguage::PortugueseBrazil, None, Some(1), Some(3))
                .as_deref(),
            Some("S01E03")
        );
    }

    #[test]
    fn fallback_item_labels() {
        assert_eq!(STRINGS.media_item_fallback_label(42), "Item de mídia #42");
    }

    #[test]
    fn log_level_labels() {
        assert_eq!(
            log_level_text(UiLanguage::PortugueseBrazil, "Debug"),
            "Depuração"
        );
    }

    #[test]
    fn channel_labels() {
        assert_eq!(STRINGS.channel_language_label("deu"), "Alemão");
    }

    #[test]
    fn media_kind_labels() {
        assert_eq!(
            media_kind_text(UiLanguage::PortugueseBrazil, 4),
            "Outros vídeos"
        );
    }

    #[test]
    fn playout_schedule_kind_labels() {
        assert_eq!(
            playout_schedule_kind_text(UiLanguage::PortugueseBrazil, 4),
            "Por script"
        );
    }

    #[test]
    fn hardware_acceleration_labels() {
        assert_eq!(
            hardware_acceleration_option_text(UiLanguage::PortugueseBrazil, "NVENC", false),
            "NVENC (não disponível neste sistema)"
        );
    }

    #[test]
    fn ffmpeg_option_labels() {
        assert_eq!(
            ffmpeg_tonemap_algorithm_text(UiLanguage::PortugueseBrazil, 2),
            "Gama"
        );
        assert_eq!(
            ffmpeg_loudness_mode_text(UiLanguage::PortugueseBrazil, 1),
            "loudnorm"
        );
        assert_eq!(
            ffmpeg_profile_video_format_text(UiLanguage::PortugueseBrazil, 0, 0),
            "Nenhum"
        );
    }

    #[test]
    fn deco_template_action_labels() {
        assert_eq!(
            STRINGS.text(TextKey::SaveDecoTemplate),
            "Salvar modelo de decoração"
        );
    }

    #[test]
    fn server_errors_follow_language() {
        assert_eq!(
            ffmpeg_profile_error_text(
                UiLanguage::PortugueseBrazil,
                "An FFmpeg profile named \"main\" already exists."
            ),
            "Já existe um perfil FFmpeg chamado \"main\"."
        );
        assert_eq!(
            schedule_error_text(UiLanguage::PortugueseBrazil, "Search Query is required."),
            "A consulta de pesquisa é obrigatória."
        );
        assert_eq!(
            scheduling_content_error_text(
                UiLanguage::PortugueseBrazil,
                "Template items cannot cross midnight."
            ),
            "Os itens do modelo não podem atravessar a meia-noite."
        );
        assert_eq!(
            scheduling_content_error_text(
                UiLanguage::PortugueseBrazil,
                "Deco from 01:00:00 to 01:30:00 intersects deco from 01:15:00 to 01:45:00"
            ),
            "A decoração se sobrepõe a outra decoração."
        );
        assert_eq!(
            watermark_error_text(
                UiLanguage::PortugueseBrazil,
                "Width must be between 0 and 100%"
            ),
            "A largura deve estar entre 0 e 100%"
        );
        assert_eq!(
            settings_error_text(
                UiLanguage::PortugueseBrazil,
                "XMLTV days to build (5) cannot be greater than Playout days to build (2)"
            ),
            "Dias de XMLTV para construir (5) não podem ser maiores que dias de playout para construir (2)"
        );
        assert_eq!(
            media_action_error_text(UiLanguage::PortugueseBrazil, "Playlist does not exist."),
            "A playlist não existe."
        );
        assert_eq!(
            filler_error_text(
                UiLanguage::PortugueseBrazil,
                "Use Chapters As Media Items must not be enabled for fallback filler"
            ),
            "Usar capítulos como itens de mídia não pode estar ativado para preenchimento de fallback."
        );
        assert_eq!(
            media_browser_error_text(
                UiLanguage::PortugueseBrazil,
                "Playlist name \"example\" is invalid"
            ),
            "O nome da playlist é inválido."
        );
        assert_eq!(
            media_source_error_text(
                UiLanguage::PortugueseBrazil,
                "Could not reach Jellyfin: connection refused"
            ),
            "Não foi possível acessar o Jellyfin: connection refused"
        );
        assert_eq!(
            addon_media_list_error_text(
                UiLanguage::PortugueseBrazil,
                "failed to decode media-list add-on output: status 404 Not Found"
            ),
            "O add-on retornou dados de lista inválidos: status 404 Not Found"
        );
        assert_eq!(
            upload_error_text(
                UiLanguage::PortugueseBrazil,
                "Write failed: permission denied"
            ),
            "Falha ao gravar: permission denied"
        );
    }
}
