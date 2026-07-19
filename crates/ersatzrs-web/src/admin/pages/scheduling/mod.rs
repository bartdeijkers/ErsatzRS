pub mod block_editor;
pub mod blocks;
pub mod decos;
pub mod playout_editor;
pub mod playout_kind_editor;
pub mod playout_nested_editor;
pub mod playouts;
pub mod schedules;
pub mod templates;
pub mod yaml_editor;

use crate::admin::{
    i18n::{tr, TextKey},
    server_fns::schedules::SmartShuffleOverviewWarning,
};

fn smart_shuffle_overview_warning_text(warning: SmartShuffleOverviewWarning) -> &'static str {
    tr(match warning {
        SmartShuffleOverviewWarning::InactivePlaybackOrder => TextKey::SmartShuffleInactive,
        SmartShuffleOverviewWarning::Off => TextKey::SmartShuffleOverviewOff,
        SmartShuffleOverviewWarning::EmptySource => TextKey::SmartShuffleOverviewEmptySource,
        SmartShuffleOverviewWarning::NoMatchingRules => {
            TextKey::SmartShuffleOverviewNoMatchingRules
        }
    })
}
