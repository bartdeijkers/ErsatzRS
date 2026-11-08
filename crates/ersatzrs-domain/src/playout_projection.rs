//! Dependency roots for refreshing derived playouts without rebuilding schedules.
use crate::EntityId;

/// An edited persisted object whose serialized playback projection may change.
/// Resolve references before deletion; this value never authorizes a Reset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayoutProjectionChange {
    /// Global `FFmpeg` settings affect every Next channel.
    All,
    /// Channel, item, deco and global watermark references.
    Watermark(EntityId),
    /// Direct and template-based deco references.
    Deco(EntityId),
    /// Every deco removed by group deletion.
    DecoGroup(EntityId),
    /// Playouts using this deco template.
    DecoTemplate(EntityId),
    /// Every deco template removed by group deletion.
    DecoTemplateGroup(EntityId),
    /// One playout's default deco or template attachment.
    Playout(EntityId),
    /// Media-source IDs share the parent `MediaSource` identity across providers.
    MediaSource(EntityId),
}
