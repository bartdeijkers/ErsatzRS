pub(in crate::scheduling::scripted) mod content;
pub(in crate::scheduling::scripted) mod graphics;
pub(in crate::scheduling::scripted) mod guide;
pub(in crate::scheduling::scripted) mod policy;
pub(in crate::scheduling::scripted) mod pre_roll;
pub(in crate::scheduling::scripted) mod skipping;
pub(in crate::scheduling::scripted) mod sources;
pub(in crate::scheduling::scripted) mod state_queries;
pub(in crate::scheduling::scripted) mod teletext;
pub(in crate::scheduling::scripted) mod timing;
pub(in crate::scheduling::scripted) mod watermark;

#[cfg(test)]
pub(super) const REGISTERED_GLOBALS: [&str; 41] = [
    "build_mode",
    "start_time",
    "set_smart_shuffle_policy",
    "set_cooldown_policy",
    "schedule_count",
    "schedule_duration",
    "set_watermark",
    "clear_watermarks",
    "graphics_on",
    "graphics_off",
    "wait_until",
    "add_count",
    "add_all",
    "add_duration",
    "pad_to_next",
    "pad_until",
    "skip",
    "skip_to",
    "rewind",
    "pre_roll_on",
    "pre_roll_off",
    "lock_guide_group",
    "unlock_guide_group",
    "is_done",
    "current_time",
    "finish_time",
    "watermark_on",
    "watermark_off",
    "graphics_on_path",
    "graphics_off_path",
    "add_collection",
    "add_multi_collection",
    "add_smart_collection",
    "add_search",
    "add_show",
    "add_playlist",
    "add_key",
    "add_marathon",
    "create_playlist",
    "teletext_profile",
    "teletext_enabled",
];
