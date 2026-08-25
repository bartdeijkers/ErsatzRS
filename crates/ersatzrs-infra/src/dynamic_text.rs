//! Runtime controller for file-backed `FFmpeg` `drawtext` overlays.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{DateTime, SecondsFormat, Utc};
use minijinja::{Environment, UndefinedBehavior};
use serde_json::{json, Map, Number, Value};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

const MAX_DYNAMIC_TEXT_BYTES: usize = 64 * 1024;
const MIN_REFRESH_INTERVAL: Duration = Duration::from_millis(250);
const MAX_REFRESH_INTERVAL: Duration = Duration::from_mins(1);
const TEMPLATE_NAME: &str = "dynamic-text";

#[derive(Debug, thiserror::Error)]
pub enum DynamicTextError {
    #[error("dynamic text template exceeds the 64 KiB limit")]
    TemplateTooLarge,
    #[error("dynamic text output exceeds the 64 KiB limit")]
    OutputTooLarge,
    #[error("dynamic text {kind} contains a NUL character")]
    ContainsNul { kind: &'static str },
    #[error("dynamic text {kind} starts with a UTF-8 byte-order mark")]
    ContainsBom { kind: &'static str },
    #[error("dynamic text variables must be a JSON object")]
    InvalidVariables,
    #[error("dynamic text template error: {0}")]
    Template(#[from] minijinja::Error),
    #[error("dynamic text I/O error for {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Dynamic values added to the existing graphics variable tree for one render.
#[derive(Debug, Clone, Copy)]
pub struct DynamicTextRenderContext {
    pub now: DateTime<Utc>,
    pub playback_elapsed_seconds: f64,
    pub playback_duration_seconds: f64,
}

/// A validated minijinja template used by a dynamic text controller.
pub struct DynamicTextTemplate {
    environment: Environment<'static>,
}

impl DynamicTextTemplate {
    /// Compile and validate a bounded template.
    ///
    /// # Errors
    ///
    /// Returns an error when the source exceeds its bounds, contains unsafe
    /// characters, or is not valid minijinja syntax.
    pub fn compile(source: String) -> Result<Self, DynamicTextError> {
        validate_text("template", &source, MAX_DYNAMIC_TEXT_BYTES)?;
        let mut environment = Environment::new();
        environment.set_undefined_behavior(UndefinedBehavior::Strict);
        environment.add_template_owned(TEMPLATE_NAME, source)?;
        // Force syntax validation now rather than on the first controller tick.
        let _ = environment.get_template(TEMPLATE_NAME)?;
        Ok(Self { environment })
    }

    /// Render against the existing serde-json graphics tree plus live values.
    ///
    /// # Errors
    ///
    /// Returns an error when variables are not an object, rendering fails, or
    /// the rendered output violates the dynamic-text bounds.
    pub fn render(
        &self,
        variables: &Value,
        context: DynamicTextRenderContext,
    ) -> Result<String, DynamicTextError> {
        let mut variables = variables
            .as_object()
            .cloned()
            .ok_or(DynamicTextError::InvalidVariables)?;
        insert_dynamic_context(&mut variables, context);
        let rendered = self
            .environment
            .get_template(TEMPLATE_NAME)?
            .render(Value::Object(variables))?;
        validate_text("output", &rendered, MAX_DYNAMIC_TEXT_BYTES)?;
        Ok(rendered)
    }
}

/// Parameters needed to prepare and refresh one `FFmpeg` text file.
pub struct DynamicTextControllerRequest {
    pub element_id: i64,
    pub template: String,
    pub variables: Value,
    pub text_file: PathBuf,
    pub refresh_interval_milliseconds: u64,
    pub playback_offset_seconds: f64,
    pub playback_duration_seconds: f64,
}

/// Owns the refresh task for one reloadable `FFmpeg` `drawtext` text file.
pub struct DynamicTextController {
    element_id: i64,
    text_file: PathBuf,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl DynamicTextController {
    /// Validate and render the initial file before starting the refresh task.
    ///
    /// # Errors
    ///
    /// Returns an error when template validation or initial rendering fails,
    /// or when the controller cannot create and atomically write its text file.
    pub async fn start(request: DynamicTextControllerRequest) -> Result<Self, DynamicTextError> {
        let DynamicTextControllerRequest {
            element_id,
            template,
            variables,
            text_file,
            refresh_interval_milliseconds,
            playback_offset_seconds,
            playback_duration_seconds,
        } = request;
        let template = DynamicTextTemplate::compile(template)?;
        if let Some(parent) = text_file.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|source| DynamicTextError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
        }

        let started_at = Instant::now();
        let initial = template.render(
            &variables,
            DynamicTextRenderContext {
                now: Utc::now(),
                playback_elapsed_seconds: playback_offset_seconds,
                playback_duration_seconds,
            },
        )?;
        atomic_replace_if_changed(&text_file, initial.as_bytes())?;

        let refresh_interval = clamp_refresh_interval(refresh_interval_milliseconds);
        let (shutdown, mut shutdown_rx) = oneshot::channel();
        let task_text_file = text_file.clone();
        let task = tokio::spawn(async move {
            let first_tick = tokio::time::Instant::now() + refresh_interval;
            let mut interval = tokio::time::interval_at(first_tick, refresh_interval);
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    _ = interval.tick() => {
                        let elapsed = playback_offset_seconds + started_at.elapsed().as_secs_f64();
                        match template
                            .render(
                                &variables,
                                DynamicTextRenderContext {
                                    now: Utc::now(),
                                    playback_elapsed_seconds: elapsed,
                                    playback_duration_seconds,
                                },
                            )
                            .and_then(|rendered| {
                                atomic_replace_if_changed(&task_text_file, rendered.as_bytes())
                            })
                        {
                            Ok(_) => {}
                            Err(error) => tracing::warn!(
                                %error,
                                element_id,
                                text_file = %task_text_file.display(),
                                "failed to refresh dynamic text; keeping the last good file"
                            ),
                        }
                    }
                }
            }
        });

        Ok(Self {
            element_id,
            text_file,
            shutdown: Some(shutdown),
            task: Some(task),
        })
    }

    #[must_use]
    pub fn text_file(&self) -> &Path {
        &self.text_file
    }

    /// Stop refreshing and wait until the task can no longer touch its file.
    pub async fn shutdown(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = self.task.take() {
            if let Err(error) = task.await {
                if !error.is_cancelled() {
                    tracing::debug!(
                        %error,
                        element_id = self.element_id,
                        text_file = %self.text_file.display(),
                        "dynamic text controller join failed"
                    );
                }
            }
        }
    }
}

impl Drop for DynamicTextController {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// Gracefully stop a set of controllers before deleting their graphics directory.
pub async fn shutdown_dynamic_text_controllers(controllers: Vec<DynamicTextController>) {
    for controller in controllers {
        controller.shutdown().await;
    }
}

fn insert_dynamic_context(root: &mut Map<String, Value>, context: DynamicTextRenderContext) {
    let now = context.now;
    root.insert(
        "Now".to_owned(),
        json!({
            "Utc": now.to_rfc3339_opts(SecondsFormat::Millis, true),
            "Rfc3339": now.to_rfc3339_opts(SecondsFormat::Millis, true),
            "Date": now.format("%Y-%m-%d").to_string(),
            "Time": now.format("%H:%M:%S%.3fZ").to_string(),
            "Unix": now.timestamp(),
            "UnixMilliseconds": now.timestamp_millis(),
        }),
    );

    let elapsed = finite_nonnegative(context.playback_elapsed_seconds);
    let duration = finite_nonnegative(context.playback_duration_seconds);
    root.insert(
        "Playback".to_owned(),
        json!({
            "ElapsedSeconds": number_value(elapsed),
            "ElapsedMilliseconds": number_value(elapsed * 1_000.0),
            "ElapsedMinutes": number_value(elapsed / 60.0),
            "ElapsedHours": number_value(elapsed / 3_600.0),
            "DurationSeconds": number_value(duration),
            "RemainingSeconds": number_value((duration - elapsed).max(0.0)),
        }),
    );
}

fn finite_nonnegative(value: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn number_value(value: f64) -> Value {
    Value::Number(Number::from_f64(value).unwrap_or_else(|| Number::from(0)))
}

fn validate_text(
    kind: &'static str,
    value: &str,
    max_bytes: usize,
) -> Result<(), DynamicTextError> {
    if value.len() > max_bytes {
        return Err(if kind == "template" {
            DynamicTextError::TemplateTooLarge
        } else {
            DynamicTextError::OutputTooLarge
        });
    }
    if value.contains('\0') {
        return Err(DynamicTextError::ContainsNul { kind });
    }
    if value.starts_with('\u{feff}') {
        return Err(DynamicTextError::ContainsBom { kind });
    }
    Ok(())
}

fn clamp_refresh_interval(milliseconds: u64) -> Duration {
    Duration::from_millis(milliseconds).clamp(MIN_REFRESH_INTERVAL, MAX_REFRESH_INTERVAL)
}

/// Atomically replace a file through a same-directory temporary, unless bytes are unchanged.
fn atomic_replace_if_changed(path: &Path, bytes: &[u8]) -> Result<bool, DynamicTextError> {
    match std::fs::read(path) {
        Ok(current) if current == bytes => return Ok(false),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(DynamicTextError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|source| DynamicTextError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.flush())
        .map_err(|source| DynamicTextError::Io {
            path: temporary.path().to_path_buf(),
            source,
        })?;
    temporary
        .persist(path)
        .map_err(|error| DynamicTextError::Io {
            path: path.to_path_buf(),
            source: error.error,
        })?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;
    use serde_json::json;

    use super::*;

    #[test]
    fn renders_static_and_dynamic_graphics_variables_deterministically() {
        let template = DynamicTextTemplate::compile(
            "{{ MediaItem_Title }}|{{ Now.Utc }}|{{ Now.Date }}|{{ Now.Time }}|{{ Now.Unix }}|{{ Playback.ElapsedSeconds }}|{{ Playback.RemainingSeconds }}".to_owned(),
        )
        .unwrap();
        let now = Utc.with_ymd_and_hms(2026, 8, 25, 12, 34, 56).unwrap();

        let rendered = template
            .render(
                &json!({ "MediaItem_Title": "Live" }),
                DynamicTextRenderContext {
                    now,
                    playback_elapsed_seconds: 12.5,
                    playback_duration_seconds: 30.0,
                },
            )
            .unwrap();

        assert_eq!(
            rendered,
            "Live|2026-08-25T12:34:56.000Z|2026-08-25|12:34:56.000Z|1787661296|12.5|17.5"
        );
        assert!(!rendered.starts_with('\u{feff}'));
        assert!(!rendered.ends_with('\n'));
    }

    #[test]
    fn rejects_unbounded_or_unsafe_text() {
        assert!(matches!(
            DynamicTextTemplate::compile("x".repeat(MAX_DYNAMIC_TEXT_BYTES + 1)),
            Err(DynamicTextError::TemplateTooLarge)
        ));
        assert!(matches!(
            DynamicTextTemplate::compile("bad\0text".to_owned()),
            Err(DynamicTextError::ContainsNul { kind: "template" })
        ));
        assert!(matches!(
            DynamicTextTemplate::compile("\u{feff}text".to_owned()),
            Err(DynamicTextError::ContainsBom { kind: "template" })
        ));
    }

    #[test]
    fn atomic_replace_skips_unchanged_bytes_and_leaves_no_sibling_tempfile() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("current.txt");

        assert!(atomic_replace_if_changed(&path, b"first").unwrap());
        assert!(!atomic_replace_if_changed(&path, b"first").unwrap());
        assert!(atomic_replace_if_changed(&path, b"second").unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn controller_refreshes_and_drop_aborts_future_writes() {
        let directory = tempfile::tempdir().unwrap();
        let text_file = directory.path().join("current.txt");
        let controller = DynamicTextController::start(DynamicTextControllerRequest {
            element_id: 7,
            template: "{{ Playback.ElapsedMilliseconds }}".to_owned(),
            variables: json!({}),
            text_file: text_file.clone(),
            refresh_interval_milliseconds: 1,
            playback_offset_seconds: 0.0,
            playback_duration_seconds: 10.0,
        })
        .await
        .unwrap();
        let initial = tokio::fs::read_to_string(&text_file).await.unwrap();

        tokio::time::sleep(Duration::from_millis(350)).await;
        let refreshed = tokio::fs::read_to_string(&text_file).await.unwrap();
        assert_ne!(refreshed, initial);

        drop(controller);
        let after_drop = tokio::fs::read_to_string(&text_file).await.unwrap();
        tokio::time::sleep(Duration::from_millis(350)).await;
        assert_eq!(
            tokio::fs::read_to_string(&text_file).await.unwrap(),
            after_drop
        );
    }
}
