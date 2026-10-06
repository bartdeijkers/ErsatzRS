//! One native form serves hydrated and no-JavaScript clients.
use super::editor::{escape_html as escape, style};
use crate::admin::{
    i18n::{text, TextKey, UiLanguage},
    server_fns::{
        channels::ChannelNumberEditorRow,
        graphics_authoring::{from_fields, to_fields, Fields},
    },
};
use ersatzrs_domain::GraphicsElement;
use std::fmt::Write as _;

pub(crate) fn error_text(language: UiLanguage, code: &str) -> &'static str {
    text(
        language,
        match code {
            "graphics-authoring-missing-font" => TextKey::GraphicsMissingFont,
            "graphics-authoring-missing-media" => TextKey::GraphicsMissingMedia,
            "graphics-authoring-missing-variable" => TextKey::GraphicsMissingVariable,
            "graphics-authoring-preview-failed" => TextKey::GraphicsAuthoringPreviewFailed,
            _ => TextKey::GraphicsAuthoringInvalid,
        },
    )
}

#[allow(
    clippy::too_many_lines,
    reason = "shared form deliberately mounts every typed kind for no-JS clients"
)]
pub(super) fn html(
    element: &GraphicsElement,
    channels: &[ChannelNumberEditorRow],
    language: UiLanguage,
    error: &str,
    saved: bool,
    preview_fields: &Fields,
) -> Result<String, &'static str> {
    let fields = to_fields(element)?;
    let label = |key| escape(text(language, key));
    let mut html = format!(
        r#"<form method="post" action="/admin-api/graphics-elements/author" class="{}" data-graphics-element-editor data-graphics-element-id="{}"><input type="hidden" name="id" value="{}"><div class="{}"><a class="{}" href="/graphics-elements">← {}</a><h1 class="{}">{}</h1></div><div class="{}"><button class="{}" name="intent" value="save" data-save-graphics-element>{}</button><a class="{}" href="/graphics-elements">{}</a></div>"#,
        style::page,
        element.id,
        element.id,
        style::breadcrumb,
        style::back_link,
        label(TextKey::GraphicsElements),
        style::page_title,
        label(if element.id == 0 {
            TextKey::AddGraphicsElement
        } else {
            TextKey::EditGraphicsElement
        }),
        style::action_bar,
        style::btn_primary,
        label(if element.id == 0 {
            TextKey::AddGraphicsElement
        } else {
            TextKey::SaveGraphicsElement
        }),
        style::btn_secondary,
        label(TextKey::Cancel)
    );
    if saved {
        let _ = write!(
            html,
            r#"<p class="{}" role="status" data-graphics-element-saved="true">{}</p>"#,
            style::success,
            label(TextKey::GraphicsElementSaved)
        );
    }
    if !error.is_empty() {
        let _ = write!(
            html,
            r#"<p class="{}" role="alert" data-graphics-element-error>{}</p>"#,
            style::error,
            escape(error_text(language, error))
        );
    }
    let _ = write!(html, r#"<section class="{}">"#, style::section);
    input(
        &mut html,
        &fields,
        language,
        "name",
        TextKey::Name,
        "text",
        "required maxlength=\"50\" data-graphics-element-name",
    );
    select(
        &mut html,
        &fields,
        language,
        "kind",
        TextKey::GraphicsElementType,
        &[
            ("Image", TextKey::GraphicsElementKindImage),
            ("Text", TextKey::GraphicsElementKindText),
            ("Subtitle", TextKey::GraphicsElementKindSubtitle),
            ("Motion", TextKey::GraphicsElementKindMotion),
            ("Script", TextKey::GraphicsElementKindScript),
        ],
        "data-graphics-element-kind-select",
    );
    html.push_str("</section>");
    let _ = write!(
        html,
        r#"<section class="{}"><h2 class="{}">{}</h2>"#,
        style::section,
        style::section_title,
        label(TextKey::GraphicsGeometry)
    );
    select(
        &mut html,
        &fields,
        language,
        "location",
        TextKey::Location,
        &[
            ("TopLeft", TextKey::TopLeft),
            ("TopMiddle", TextKey::TopMiddle),
            ("TopRight", TextKey::TopRight),
            ("LeftMiddle", TextKey::LeftMiddle),
            ("MiddleCenter", TextKey::MiddleCenter),
            ("RightMiddle", TextKey::RightMiddle),
            ("BottomLeft", TextKey::BottomLeft),
            ("BottomMiddle", TextKey::BottomMiddle),
            ("BottomRight", TextKey::BottomRight),
        ],
        "",
    );
    for (name, key, attrs) in [
        (
            "location_x",
            TextKey::GraphicsPixelX,
            "step=\"any\" min=\"-8192\" max=\"8192\"",
        ),
        (
            "location_y",
            TextKey::GraphicsPixelY,
            "step=\"any\" min=\"-8192\" max=\"8192\"",
        ),
        (
            "horizontal_margin_percent",
            TextKey::HorizontalMarginPercent,
            "step=\"any\" min=\"-100\" max=\"100\"",
        ),
        (
            "vertical_margin_percent",
            TextKey::VerticalMarginPercent,
            "step=\"any\" min=\"-100\" max=\"100\"",
        ),
        ("z_index", TextKey::ZIndex, "min=\"-10000\" max=\"10000\""),
        ("opacity_percent", TextKey::Opacity, "min=\"0\" max=\"100\""),
        (
            "epg_entries",
            TextKey::GraphicsEpgEntries,
            "min=\"0\" max=\"256\"",
        ),
        (
            "scale_width_percent",
            TextKey::GraphicsScaleWidth,
            "step=\"any\" min=\"0.01\" max=\"1000\"",
        ),
    ] {
        input(&mut html, &fields, language, name, key, "number", attrs);
    }
    input(
        &mut html,
        &fields,
        language,
        "opacity_expression",
        TextKey::OpacityExpression,
        "text",
        "",
    );
    select(
        &mut html,
        &fields,
        language,
        "scale",
        TextKey::Scaled,
        &[("false", TextKey::No), ("true", TextKey::Yes)],
        "",
    );
    select(
        &mut html,
        &fields,
        language,
        "place_within_source_content",
        TextKey::GraphicsSourceContent,
        &[("false", TextKey::No), ("true", TextKey::Yes)],
        "",
    );
    let _ = write!(
        html,
        r#"<p class="{}">{}</p></section>"#,
        style::help,
        label(TextKey::GraphicsGeometryHelp)
    );
    for (kind, key) in [
        ("Image", TextKey::GraphicsElementKindImage),
        ("Text", TextKey::GraphicsElementKindText),
        ("Subtitle", TextKey::GraphicsElementKindSubtitle),
        ("Motion", TextKey::GraphicsElementKindMotion),
        ("Script", TextKey::GraphicsElementKindScript),
    ] {
        let _ = write!(
            html,
            r#"<details class="{}" data-kind-fields="{}"{}><summary>{}</summary>"#,
            style::section,
            kind,
            if fields.get("kind").is_some_and(|k| k == kind) {
                " open"
            } else {
                ""
            },
            label(key)
        );
        // Closed native details still submit their controls. Supply each kind's
        // own defaults so a later kind switch does not submit empty required
        // controller values from the previously selected configuration.
        let fields = if fields.get("kind").is_some_and(|selected| selected == kind) {
            fields.clone()
        } else {
            let draft = from_fields(
                &Fields::from([("kind".into(), kind.into())]),
                element.clone(),
            )?;
            to_fields(&draft)?
        };
        match kind {
            "Image" => input(
                &mut html,
                &fields,
                language,
                "image",
                TextKey::FilePath,
                "text",
                "",
            ),
            "Subtitle" => input(
                &mut html,
                &fields,
                language,
                "template",
                TextKey::Template,
                "text",
                "",
            ),
            "Motion" => {
                input(
                    &mut html,
                    &fields,
                    language,
                    "video_path",
                    TextKey::FilePath,
                    "text",
                    "",
                );
                input(
                    &mut html,
                    &fields,
                    language,
                    "start_seconds",
                    TextKey::GraphicsStartSeconds,
                    "number",
                    "step=\"any\" min=\"0\" max=\"86400\"",
                );
                input(
                    &mut html,
                    &fields,
                    language,
                    "hold_seconds",
                    TextKey::GraphicsHoldSeconds,
                    "number",
                    "step=\"any\" min=\"0\" max=\"86400\"",
                );
                select(
                    &mut html,
                    &fields,
                    language,
                    "end_behavior",
                    TextKey::GraphicsEndBehavior,
                    &[
                        ("Disappear", TextKey::GraphicsDisappear),
                        ("Loop", TextKey::GraphicsLoop),
                        ("Hold", TextKey::GraphicsHold),
                    ],
                    "",
                );
            }
            "Script" => {
                textarea(
                    &mut html,
                    &fields,
                    language,
                    "command",
                    TextKey::GraphicsLuaCommand,
                    "",
                );
                textarea(
                    &mut html,
                    &fields,
                    language,
                    "args",
                    TextKey::GraphicsScriptArgs,
                    "",
                );
                for (name, key) in [
                    ("script_start_seconds", TextKey::GraphicsStartSeconds),
                    ("duration_seconds", TextKey::DurationSeconds),
                ] {
                    input(
                        &mut html,
                        &fields,
                        language,
                        name,
                        key,
                        "number",
                        "step=\"any\" min=\"0\" max=\"86400\"",
                    );
                }
                input(
                    &mut html,
                    &fields,
                    language,
                    "format",
                    TextKey::Format,
                    "text",
                    "",
                );
                input(
                    &mut html,
                    &fields,
                    language,
                    "pixel_format",
                    TextKey::PixelFormat,
                    "text",
                    "",
                );
                let _ = write!(
                    html,
                    r#"<p class="{}">{}</p>"#,
                    style::help,
                    label(TextKey::GraphicsLuaHelp)
                );
            }
            _ => {
                select(
                    &mut html,
                    &fields,
                    language,
                    "renderer",
                    TextKey::Renderer,
                    &[
                        ("drawvg", TextKey::DrawvgRendererOption),
                        ("drawtext", TextKey::DrawtextRendererOption),
                    ],
                    "data-graphics-element-renderer-select",
                );
                textarea(
                    &mut html,
                    &fields,
                    language,
                    "text",
                    TextKey::TextTemplate,
                    "data-graphics-element-text",
                );
                for (name, key) in [
                    ("font_file", TextKey::FontFile),
                    ("base_style", TextKey::GraphicsBaseStyle),
                    ("font_family", TextKey::GraphicsFontFamily),
                    ("text_color", TextKey::GraphicsTextColor),
                    ("include_fonts_from", TextKey::GraphicsFontDirectory),
                ] {
                    input(
                        &mut html,
                        &fields,
                        language,
                        name,
                        key,
                        "text",
                        if name == "font_file" {
                            "data-drawtext-font-file"
                        } else {
                            ""
                        },
                    );
                }
                for (name, key, attrs) in [
                    (
                        "font_size",
                        TextKey::GraphicsFontSize,
                        "step=\"any\" min=\"1\" max=\"2048\"",
                    ),
                    ("line_height", TextKey::GraphicsLineHeight, "step=\"any\""),
                    (
                        "width_percent",
                        TextKey::GraphicsTextWidth,
                        "step=\"any\" min=\"0.01\" max=\"100\"",
                    ),
                    (
                        "refresh_interval_milliseconds",
                        TextKey::RefreshPeriodMilliseconds,
                        "min=\"250\" max=\"60000\" data-drawtext-refresh-period",
                    ),
                    (
                        "reload_interval_frames",
                        TextKey::ReloadFrameInterval,
                        "min=\"1\" data-drawtext-reload-frames",
                    ),
                    (
                        "metadata_refresh_interval_seconds",
                        TextKey::GraphicsMetadataRefreshSeconds,
                        "min=\"0\" max=\"60\" data-graphics-metadata-refresh",
                    ),
                ] {
                    input(&mut html, &fields, language, name, key, "number", attrs);
                }
                select(
                    &mut html,
                    &fields,
                    language,
                    "text_fit",
                    TextKey::GraphicsTextFit,
                    &[
                        ("None", TextKey::GraphicsFixedSize),
                        ("Wrap", TextKey::GraphicsWrap),
                        ("Scale", TextKey::Scaled),
                    ],
                    "",
                );
                select(
                    &mut html,
                    &fields,
                    language,
                    "text_align",
                    TextKey::GraphicsTextAlign,
                    &[
                        ("Left", TextKey::Left),
                        ("Center", TextKey::Center),
                        ("Right", TextKey::Right),
                    ],
                    "",
                );
                for key in [
                    TextKey::GraphicsDrawvgTypography,
                    TextKey::GraphicsDrawtextTypography,
                    TextKey::GraphicsMetadataRefreshHelp,
                    TextKey::ProgrammeTemplateHelp,
                ] {
                    let _ = write!(html, r#"<p class="{}">{}</p>"#, style::help, label(key));
                }
            }
        }
        if matches!(kind, "Image" | "Motion") {
            let _ = write!(
                html,
                r#"<p class="{}" data-jpeg-xl-help>{}</p>"#,
                style::help,
                label(TextKey::GraphicsJpegXlHelp)
            );
        }
        html.push_str("</details>");
    }
    let _ = write!(
        html,
        r#"<section class="{}" data-graphics-real-preview><h2 class="{}">{}</h2><p class="{}">{}</p>"#,
        style::section,
        style::section_title,
        label(TextKey::GraphicsRealPreview),
        style::help,
        label(TextKey::GraphicsRealPreviewHelp)
    );
    let _ = write!(
        html,
        r#"<label class="{}">{}<select name="preview_channel" class="{}"><option value="">{}</option>"#,
        style::field,
        label(TextKey::Channel),
        style::input,
        label(TextKey::None)
    );
    for channel in channels {
        let _ = write!(
            html,
            r#"<option value="{}"{}>{}</option>"#,
            channel.id,
            if preview_fields.get("preview_channel") == Some(&channel.id.to_string()) {
                " selected"
            } else {
                ""
            },
            escape(&format!("{} · {}", channel.number, channel.name))
        );
    }
    html.push_str("</select></label>");
    let mut preview = Fields::from([
        ("preview_width".into(), "1920".into()),
        ("preview_height".into(), "1080".into()),
        ("preview_seek".into(), "0".into()),
    ]);
    #[cfg(feature = "ssr")]
    preview.insert("preview_time".into(), chrono::Utc::now().to_rfc3339());
    preview.extend(preview_fields.clone());
    for (name, key, input_type, attrs) in [
        ("preview_time", TextKey::PreviewTimestamp, "text", ""),
        (
            "preview_seek",
            TextKey::ProgrammeSeekSeconds,
            "number",
            "step=\"any\" min=\"0\" max=\"86400\"",
        ),
        (
            "preview_width",
            TextKey::Width,
            "number",
            "min=\"16\" max=\"8192\"",
        ),
        (
            "preview_height",
            TextKey::Height,
            "number",
            "min=\"16\" max=\"8192\"",
        ),
    ] {
        input(&mut html, &preview, language, name, key, input_type, attrs);
    }
    textarea(
        &mut html,
        &Fields::new(),
        language,
        "preview_variables",
        TextKey::GraphicsPreviewVariables,
        "",
    );
    let _ = write!(
        html,
        r#"<button class="{}" name="intent" value="preview" formtarget="graphics-render-preview" data-graphics-render-submit>{}</button><iframe name="graphics-render-preview" title="{}" style="width:100%;height:24rem;border:0" data-graphics-render-frame></iframe></section><section class="{}"><h2 class="{}">{}</h2><p class="{}">{}</p>"#,
        style::btn_secondary,
        label(TextKey::Preview),
        label(TextKey::GraphicsRealPreview),
        style::section,
        style::section_title,
        label(TextKey::GraphicsYaml),
        style::help,
        label(TextKey::GraphicsYamlHelp)
    );
    textarea(
        &mut html,
        &Fields::new(),
        language,
        "yaml",
        TextKey::GraphicsYaml,
        "",
    );
    let _ = write!(
        html,
        r#"<div class="{}"><button class="{}" name="intent" value="import" formnovalidate data-graphics-yaml-import>{}</button><button class="{}" name="intent" value="export" data-graphics-yaml-export>{}</button></div></section></form>"#,
        style::action_bar,
        style::btn_secondary,
        label(TextKey::Import),
        style::btn_secondary,
        label(TextKey::GraphicsExport)
    );
    Ok(html)
}

fn input(
    html: &mut String,
    fields: &Fields,
    language: UiLanguage,
    name: &str,
    key: TextKey,
    kind: &str,
    attrs: &str,
) {
    let _ = write!(
        html,
        r#"<label class="{}"><span class="{}">{}</span><input class="{}" name="{}" type="{}" value="{}" {}></label>"#,
        style::field,
        style::label,
        escape(text(language, key)),
        style::input,
        name,
        kind,
        escape(fields.get(name).map_or("", String::as_str)),
        attrs
    );
}
fn textarea(
    html: &mut String,
    fields: &Fields,
    language: UiLanguage,
    name: &str,
    key: TextKey,
    attrs: &str,
) {
    let _ = write!(
        html,
        r#"<label class="{}"><span class="{}">{}</span><textarea class="{}" name="{}" maxlength="262144" {}>{}</textarea></label>"#,
        style::field,
        style::label,
        escape(text(language, key)),
        style::textarea,
        name,
        attrs,
        escape(fields.get(name).map_or("", String::as_str))
    );
}
fn select(
    html: &mut String,
    fields: &Fields,
    language: UiLanguage,
    name: &str,
    key: TextKey,
    options: &[(&str, TextKey)],
    attrs: &str,
) {
    let _ = write!(
        html,
        r#"<label class="{}"><span class="{}">{}</span><select class="{}" name="{}" {}>"#,
        style::field,
        style::label,
        escape(text(language, key)),
        style::input,
        name,
        attrs
    );
    for (value, key) in options {
        let _ = write!(
            html,
            r#"<option value="{}"{}>{}</option>"#,
            value,
            if fields.get(name).is_some_and(|v| v == value) {
                " selected"
            } else {
                ""
            },
            escape(text(language, *key))
        );
    }
    html.push_str("</select></label>");
}
