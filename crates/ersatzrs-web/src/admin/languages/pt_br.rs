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
            "O estado da programação clássica não permite uma atualização segura para: {playouts}. Use Redefinir programação para estados antigos ou incompatíveis; Atualizar nunca limpa uma janela sem um ponto de controle completo."
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
        TextKey::AddonAccessProfileUnavailable => "Perfil do navegador indisponível",
        TextKey::AddonAccessProfileLocked => "Perfil do navegador bloqueado",
        TextKey::AddonAccessDecryptionFailed => {
            "Não foi possível descriptografar as credenciais do navegador"
        }
        TextKey::AddonAccessRestricted => "Acesso restrito pelo provedor",
        TextKey::AddonAccessNextAttempt => "Próxima tentativa:",
        TextKey::AddonAccessExpires => "O resultado expira:",
        TextKey::AddonAccessRefresh => "Atualizar status",
        TextKey::AddonOptionsCurrentFilter => "Filtro atual",
        TextKey::AddonOptions => "Opções do provedor",
        TextKey::AddonOptionsDraftHelp => {
            "As alterações são salvas no rascunho. A reprodução publicada mantém suas opções até a publicação."
        }
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
        TextKey::NeedReview => "Precisa de revisão",
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
        TextKey::ProcessedMediaRequired => "Arquivos de mídia a preparar",
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
        TextKey::PublishAgainAfterDetails => "Revise os detalhes concluídos e publique novamente.",
        TextKey::FetchDetails => "Buscar detalhes",
        TextKey::DetailsQueued => "Detalhes na fila",
        TextKey::DetailsLoading => "Carregando detalhes",
        TextKey::RetryDetails => "Tentar detalhes novamente",
        TextKey::DetailsComplete => "Detalhes completos",
        TextKey::DetailsSkipped => "Detalhes ignorados",
        TextKey::FindingProgrammes => "Procurando programas",
        TextKey::CheckingOverview => "Verificando a visão geral",
        TextKey::ReadyToReview => "Pronto para revisão",
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
        TextKey::ItemsNeedingReview => "Itens que precisam de revisão",
        TextKey::KeepCurrentValue => "Manter o valor atual",
        TextKey::UndoLastChange => "Desfazer a última alteração",
        TextKey::Undo => "Desfazer",
        TextKey::ChangeUndone => "Alteração desfeita",
        TextKey::MetadataConflicts => "Conflitos de metadados",
        TextKey::MetadataConflictsHelp => {
            "O provedor alterou campos substituídos pelo operador. Revise os valores antes de publicar."
        }
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
        TextKey::MetadataDraftNotice => {
            "Revise os metadados e os itens incluídos antes de publicar esta lista."
        }
        TextKey::ApplyToAllItemsPrompt => {
            "Cada item é reclassificado com o tipo de mídia selecionado, substituindo valores derivados pelo complemento ou definidos por você."
        }
        TextKey::TelevisionEpisode => "Episódio de televisão",
        TextKey::CollectionTypeAuto => "Determinar pelos itens",
        TextKey::CurrentLibraryType => "Tipo atual da biblioteca",
        TextKey::CollectionTypeChangeHelp => {
            "Você pode alterar isso após a importação. Salve e publique para reclassificar os itens existentes sem importar o complemento novamente; a ordem, a inclusão e as edições de metadados são preservadas."
        }
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
        TextKey::AdaptiveRefreshUnavailable => {
            "A atualização adaptativa está indisponível porque este complemento não oferece mais verificações da origem. Escolha outra frequência para retomar a atualização automática."
        }
        TextKey::RefreshPublicationPolicy => "Publicação após atualização",
        TextKey::RefreshPublicationReview => "Revisar cada atualização",
        TextKey::RefreshPublicationAutomatic => "Publicar automaticamente adições seguras",
        TextKey::RefreshPublicationPolicyHelp => {
            "Somente para atualizações agendadas após a primeira publicação. Atualizações manuais, reconciliações e redefinições sempre exigem revisão. Alterações ou adições não seguras ficam para revisão."
        }
        TextKey::AutoPublicationHeld => "A publicação automática precisa de revisão",
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
        TextKey::DefaultMpegTsScriptHelp => "O script MPEG-TS usado durante o streaming",
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
        TextKey::ArtworkCardinality => "Cardinalidade das imagens",
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
        TextKey::TvShows => "Séries",
        TextKey::Ui => "Interface",
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
            "Resultados atuais das verificações de integridade de paridade. Esta página é atualizada depois que varreduras de mídia alteram o estado dos itens."
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
        TextKey::PreviewLive => "A transmissão ao vivo está em reprodução.",
        TextKey::PreviewPaused => "Prévia pausada.",
        TextKey::PreviewRecovering => "Recuperando reprodução…",
        TextKey::PreviewStopped => "Prévia interrompida.",
        TextKey::PreviewUnsupported => "Este navegador não reproduz vídeo HLS.",
        TextKey::PreviewAuthorizationFailed => "Falha na autorização da prévia.",
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
