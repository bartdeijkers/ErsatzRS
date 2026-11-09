use super::*;
use std::sync::Mutex;

#[derive(Default)]
struct Runtime {
    events: Mutex<Vec<&'static str>>,
    fail_lookup: bool,
    empty: bool,
}

#[async_trait::async_trait]
impl ChannelRuntime for Runtime {
    async fn active_channel_numbers(
        &self,
    ) -> std::result::Result<Vec<String>, crate::ChannelRuntimeError> {
        panic!("no session enumeration")
    }
    async fn session_models(
        &self,
    ) -> std::result::Result<Vec<ersatzrs_core::hls::HlsSessionModel>, crate::ChannelRuntimeError>
    {
        panic!("no session inspection")
    }
    async fn stop_channel_session(
        &self,
        _: &str,
    ) -> std::result::Result<bool, crate::ChannelRuntimeError> {
        panic!("no stopping playback")
    }
    fn request_subtitle_extraction(&self, _: Vec<i64>) {
        panic!("no extraction")
    }
    async fn refresh_profile_playouts(
        &self,
        _: i64,
    ) -> std::result::Result<(), crate::ChannelRuntimeError> {
        panic!("not a profile change")
    }

    async fn playout_projection_dependents(
        &self,
        change: PlayoutProjectionChange,
    ) -> std::result::Result<Vec<String>, crate::ChannelRuntimeError> {
        assert_eq!(change, PlayoutProjectionChange::Watermark(7));
        self.events.lock().unwrap().push("capture references");
        if self.fail_lookup {
            return Err(crate::ChannelRuntimeError::unavailable(
                "synthetic lookup failure",
            ));
        }
        Ok(if self.empty {
            Vec::new()
        } else {
            vec!["1".into(), "6".into()]
        })
    }

    async fn refresh_playout_projections(
        &self,
        channels: Vec<String>,
    ) -> std::result::Result<(), crate::ChannelRuntimeError> {
        assert_eq!(channels, ["1", "6"]);
        self.events.lock().unwrap().push("notify dependents");
        Ok(())
    }
}

#[tokio::test]
async fn dependency_capture_precedes_delete_and_notification_follows_success_only() {
    for fails in [false, true] {
        let runtime = Runtime::default();
        let result = refresh_after(&runtime, PlayoutProjectionChange::Watermark(7), async {
            runtime.events.lock().unwrap().push("delete references");
            if fails {
                Err(ApplicationError::Validation {
                    field: "synthetic",
                    message: "failed mutation".into(),
                })
            } else {
                Ok(())
            }
        })
        .await;
        assert_eq!(result.is_err(), fails);
        let expected = if fails {
            vec!["capture references", "delete references"]
        } else {
            vec![
                "capture references",
                "delete references",
                "notify dependents",
            ]
        };
        assert_eq!(*runtime.events.lock().unwrap(), expected);
    }
}

#[tokio::test]
async fn failed_dependency_capture_never_starts_the_mutation() {
    let runtime = Runtime {
        fail_lookup: true,
        ..Default::default()
    };
    let result: Result<()> =
        refresh_after(&runtime, PlayoutProjectionChange::Watermark(7), async {
            panic!("a failed dependency read must not mutate persisted state")
        })
        .await;
    assert!(matches!(result, Err(ApplicationError::Unavailable { .. })));
    assert_eq!(*runtime.events.lock().unwrap(), ["capture references"]);
}

#[tokio::test]
async fn an_unreferenced_mutation_succeeds_without_notifying_any_session() {
    let runtime = Runtime {
        empty: true,
        ..Default::default()
    };
    let result = refresh_after(&runtime, PlayoutProjectionChange::Watermark(7), async {
        runtime
            .events
            .lock()
            .unwrap()
            .push("save unreferenced object");
        Ok(42)
    })
    .await
    .unwrap();
    assert_eq!(result, 42);
    assert_eq!(
        *runtime.events.lock().unwrap(),
        ["capture references", "save unreferenced object"]
    );
}
