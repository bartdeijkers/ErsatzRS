//! Ordinary one-shot Copy recovery and the published prepared-caption alternative.
//! Uses the existing pinned page889 source, callback recorder and HTTP oracles.
use super::*;

#[derive(Clone, Copy)]
enum Policy {
    Off,
    Burn,
    Convert,
}

impl Policy {
    fn convert(self) -> bool {
        matches!(self, Self::Convert)
    }
}

use super::one_shot::install_source;

async fn configure(
    state: &AppState,
    item: i64,
    subtitle: i64,
    policy: Policy,
    base: &FFmpegProfile,
) {
    state
        .repositories
        .ffmpeg_profile
        .upsert(base)
        .await
        .unwrap();
    state
        .pool
        .execute(r#"DELETE FROM "PlayoutItem" WHERE "PlayoutId"=1"#)
        .await
        .unwrap();
    if matches!(policy, Policy::Burn) {
        policies::schedule(state, item, policies::Policy::Burn).await;
    } else {
        next_schedule(state, item).await;
    }
    sqlx::query(r#"UPDATE "RemoteStream" SET "Liveness"=1,"IsLive"=0 WHERE "Id"=?"#)
        .bind(item)
        .execute(&state.pool)
        .await
        .unwrap();
    let independent = !matches!(policy, Policy::Off);
    sqlx::query(r#"UPDATE "Subtitle" SET "SubtitleKind"=?,"StreamIndex"=?,"Codec"=?,"IsExtracted"=? WHERE "Id"=?"#)
        .bind(i32::from(independent)).bind(if independent {0} else {2})
        .bind(if independent {"subrip"} else {"dvb_teletext"}).bind(!independent)
        .bind(subtitle).execute(&state.pool).await.unwrap();
    if independent {
        sqlx::query(r#"UPDATE "Subtitle" SET "Title"='Prepared page889',"Language"='nld',"Default"=1 WHERE "Id"=?"#)
            .bind(subtitle).execute(&state.pool).await.unwrap();
    }
    let mut channel = state.repositories.channel.get_by_number("1").await.unwrap();
    channel.subtitle_mode = if matches!(policy, Policy::Off) {
        ChannelSubtitleMode::None
    } else {
        ChannelSubtitleMode::Any
    };
    channel.next_engine_text_subtitle_mode = if policy.convert() {
        NextEngineTextSubtitleMode::Convert
    } else {
        NextEngineTextSubtitleMode::Burn
    };
    state.repositories.channel.upsert(&channel).await.unwrap();
}

fn selected(
    callback: &PlayoutItem,
    source: &Path,
    sidecar: &Path,
    captioned: bool,
) -> Option<(u64, u64)> {
    let tracks = callback.tracks.as_ref().unwrap();
    let video = tracks.video.as_ref().unwrap();
    let audio = tracks.audio.as_ref().unwrap();
    assert_eq!(video.stream_index, Some(0));
    assert_eq!(audio.stream_index, Some(3));
    let same_source =
        serde_json::to_vec(&video.source).unwrap() == serde_json::to_vec(&audio.source).unwrap();
    assert!(same_source, "selected AV must share one physical source");
    if captioned {
        let Some(Source::Local {
            path,
            in_point_ms,
            out_point_ms,
            ..
        }) = &video.source
        else {
            panic!("prepared AV must use its published immutable artifact");
        };
        assert_eq!(Path::new(path), source);
        assert!(in_point_ms.unwrap() >= 2000);
        assert_eq!(*out_point_ms, Some(9200));
        let subtitle = tracks.subtitle.as_ref().unwrap();
        assert_eq!(subtitle.stream_index, Some(0));
        let Some(Source::Local {
            path,
            in_point_ms: seek,
            out_point_ms: end,
            ..
        }) = &subtitle.source
        else {
            panic!("prepared captions must use the independently verified sidecar");
        };
        assert_eq!(Path::new(path), sidecar);
        assert_eq!(seek, in_point_ms);
        assert_eq!(end, out_point_ms);
        return Some((seek.unwrap(), end.unwrap()));
    }
    let Some(Source::Http {
        uri,
        keep_alive,
        reconnect,
        probe_hint,
        in_point_ms,
        ..
    }) = &video.source
    else {
        panic!("one-shot AV must use the registered consuming HTTP source");
    };
    assert!(uri.contains("/ffmpeg/next-prepared/"));
    assert_eq!(*keep_alive, Some(true));
    assert!(probe_hint.as_ref().unwrap().duration_ms.is_none());
    let single_use = probe_hint.as_ref().unwrap().single_use.as_ref().unwrap();
    assert_eq!(single_use.version, 1);
    assert_eq!(
        single_use
            .first_video_packets
            .iter()
            .find(|packet| packet.stream_index == 0)
            .map(|packet| packet.is_keyframe),
        Some(true)
    );
    assert_eq!(*reconnect, Some(false));
    assert!(tracks.subtitle.is_none());
    assert!(in_point_ms.is_none_or(|value| value == 0));
    assert!(sidecar.is_file());
    None
}

#[derive(Clone)]
struct Delivery {
    state: AppState,
    callbacks: Callbacks,
    client: reqwest::Client,
    public: String,
    root: PathBuf,
    source: PathBuf,
    sidecar: PathBuf,
    tools: [String; 3],
    policy: Policy,
}

impl Delivery {
    async fn check(self) {
        let cues = if self.policy.convert() {
            timeout(
                Duration::from_secs(35),
                delivered(&self.client, &self.public, &self.root),
            )
            .await
            .expect("ordinary one-shot Convert delivery")
        } else {
            timeout(
                Duration::from_secs(35),
                policies::capture(&self.client, &self.public, &self.root),
            )
            .await
            .expect("ordinary one-shot Burn/Off delivery");
            Vec::new()
        };
        let callbacks = self.callbacks.lock().await;
        assert_eq!(callbacks.len(), 1, "one original programme callback");
        let interval = selected(
            &callbacks[0],
            &self.source,
            &self.sidecar,
            !matches!(self.policy, Policy::Off),
        );
        drop(callbacks);
        let bytes = tokio::fs::read(self.root.join("captured.ts"))
            .await
            .unwrap();
        if let Some((seek, end)) = interval.filter(|_| self.policy.convert()) {
            let text = tokio::fs::read_to_string(&self.sidecar).await.unwrap();
            Box::pin(check(
                (Path::new(&self.tools[0]), Path::new(&self.tools[1])),
                &self.source,
                &self.root.join("captured.ts"),
                &cues,
                &text,
                (seek, end, true),
                &self.root,
            ))
            .await;
            let inventory = stream_inventory(&self.tools[1], &bytes).await;
            assert_eq!(inventory.len(), 2);
            assert_eq!(
                inventory
                    .iter()
                    .find(|s| s["codec_type"] == "audio")
                    .unwrap()["tags"]["language"],
                "nld"
            );
        } else {
            if matches!(self.policy, Policy::Off) {
                let captured = self.root.join("captured.ts");
                copied_video(Path::new(&self.tools[0]), &self.source, &captured).await;
                copied_audio(Path::new(&self.tools[1]), &self.source, &captured).await;
                let original = packets(Path::new(&self.tools[1]), &self.source, "v:0", true).await;
                let actual = packets(Path::new(&self.tools[1]), &captured, "v:0", true).await;
                let shift = video_shift(&original, &actual);
                tokio::fs::write(self.root.join("copy-payload-clock.json"),
                    serde_json::to_vec_pretty(&json!({
                        "coded_video_pts_dts_association":true,
                        "alternate_aac_payload_identity":true,
                        "video_copy_shift_ticks":shift,
                        "scope":"one-shot Off ownership and coded AV; no source-script caption-clock claim"
                    })).unwrap()).await.unwrap();
            }
            Box::pin(policies::output(
                &self.tools,
                &self.root,
                &bytes,
                if matches!(self.policy, Policy::Burn) {
                    policies::Policy::Burn
                } else {
                    policies::Policy::Off
                },
            ))
            .await;
        }
        assert_eq!(
            self.state
                .application
                .operations()
                .entity_locker()
                .active_teletext_work(),
            0
        );
    }
}

async fn producer_count(root: &Path) -> usize {
    match tokio::fs::read_to_string(root.join("producer-starts")).await {
        Ok(calls) => {
            assert!(calls.lines().all(|line| line == "play"));
            calls.lines().count()
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => panic!("read fixture producer invocations: {error}"),
    }
}

async fn assert_reaped(root: &Path) {
    if let Ok(pid) = tokio::fs::read_to_string(root.join("producer-pid")).await {
        let pid = pid.trim();
        assert!(pid.parse::<u32>().unwrap() > 0);
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "kill -0 \"$1\" 2>/dev/null",
            "synthetic-producer",
            pid,
        ]);
        command.kill_on_drop(true);
        assert!(!timeout(Duration::from_secs(2), command.status())
            .await
            .unwrap()
            .unwrap()
            .success());
    }
}

const CASES: [(&str, Policy); 3] = [
    ("off-recovery", Policy::Off),
    ("prepared-burn", Policy::Burn),
    ("prepared-convert", Policy::Convert),
];

async fn run_cases(template: Delivery, item: i64, subtitle: i64, base_profile: FFmpegProfile) {
    let mut prepared = None;
    for (label, policy) in CASES {
        template.callbacks.lock().await.clear();
        configure(&template.state, item, subtitle, policy, &base_profile).await;
        let case = template.root.join(label);
        tokio::fs::create_dir(&case).await.unwrap();
        let mut delivery = template.clone();
        if !matches!(policy, Policy::Off) {
            if prepared.is_none() {
                prepared = Some(
                    super::one_shot_prepared::publish(
                        &template.state,
                        item,
                        subtitle,
                        &template.source,
                        &template.sidecar,
                        &template.tools[1],
                    )
                    .await,
                );
            }
            let (source, sidecar) = prepared.as_ref().unwrap();
            delivery.source = source.clone();
            delivery.sidecar = sidecar.clone();
        }
        delivery.root = case.clone();
        delivery.policy = policy;
        let outcome = tokio::spawn(Box::pin(delivery.check())).await;
        stop_next(&template.state).await;
        assert_reaped(&template.root).await;
        assert_eq!(
            producer_count(&template.root).await,
            1,
            "one ordinary Copy producer; published prepared playback never reopens it"
        );
        assert!(
            !template.root.join("playout/burn-subtitles").exists(),
            "never extract the consuming producer"
        );
        outcome.expect("ordinary Next one-shot caption policy");
        tokio::fs::write(
            case.join("completion.json"),
            serde_json::to_vec_pretty(&json!({
                "case":label,"producer_starts":1,"joined_cleanup":true,"default_capacity":2,
                "refused":false,"ordinary_public_route":true,
                "published_prepared_alternative":!matches!(policy,Policy::Off)
            }))
            .unwrap(),
        )
        .await
        .unwrap();
        println!("ordinary Next source-caption policy completed: {label}");
    }
    let seeks = tokio::fs::read_to_string(template.root.join("producer-seeks"))
        .await
        .unwrap();
    assert_eq!(seeks.lines().count(), 1);
    assert!(
        seeks.lines().all(|value| {
            let parts: Vec<_> = value.split(':').collect();
            assert_eq!(parts.len(), 3);
            parts[0].parse::<u64>().unwrap() == 0
                && parts[1].parse::<u64>().unwrap() == 0
                && parts[2].parse::<f64>().unwrap() >= 2.0
        }),
        "finite producers receive the selected source-owned seek"
    );
}

async fn record_route(
    State(callbacks): State<Callbacks>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let prepared = request.uri().path().starts_with("/ffmpeg/next-prepared/");
    let method = request.method().clone();
    let range = request.headers().get("range").cloned();
    let response = record_callback(State(callbacks), request, next).await;
    if prepared {
        println!(
            "ordinary one-shot prepared request: method={method} range={range:?} status={}",
            response.status()
        );
    }
    response
}

async fn run(tools: &[String; 3]) {
    let root = tempfile::tempdir().unwrap().keep();
    println!(
        "ordinary Next one-shot caption policy evidence: {}",
        root.display()
    );
    let library = root.join("library");
    let source = retained_source(&tools[0], &library).await;
    let sidecar = library.join("independent-889.srt");
    let unused = Server::start(Router::new()).await;
    let mut state = application(&root, &unused).await;
    state.next_engine = Arc::new(NextHost::new(
        state.pool.clone(),
        state.application.operations().entity_locker().clone(),
        root.join("playout"),
    ));
    let (item, version, provider) = Box::pin(scan_caption_owner(
        &state,
        &library,
        &source,
        &tools[1],
        MediaSourceKind::Local,
    ))
    .await;
    assert!(provider.is_none());
    let subtitle = install_source(&state, &root, item, version).await;
    let base_profile = state.repositories.ffmpeg_profile.get(1).await.unwrap();
    let callbacks = Callbacks::default();
    let internal = Server::start(build_internal_app(state.clone()).layer(
        axum::middleware::from_fn_with_state(callbacks.clone(), record_route),
    ))
    .await;
    state.internal_http_addr.set(internal.address).unwrap();
    state.next_engine.media.configure(internal.address).unwrap();
    let public = Server::start(build_app(state.clone())).await;
    state.streaming_http_addr.set(public.address).unwrap();
    let template = Delivery {
        state: state.clone(),
        callbacks: callbacks.clone(),
        client: reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(25))
            .build()
            .unwrap(),
        public: public.url(""),
        root: root.clone(),
        source,
        sidecar,
        tools: tools.clone(),
        policy: Policy::Off,
    };
    let outcome = tokio::spawn(Box::pin(run_cases(template, item, subtitle, base_profile))).await;
    finish_caption_owner(
        &state,
        provider,
        MediaSourceKind::Local,
        public,
        internal,
        unused,
    )
    .await;
    outcome.expect("complete grouped ordinary one-shot caption policy proof");
}

#[test]
#[ignore = "requires pinned Next/FFmpeg9, retained page889 source/SRT, TSDuck and isolated Unix app-data; ordinary route policy proof"]
fn normal_next_one_shot_copy_and_prepared_captions_preserve_delivery_and_cleanup() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    assert_eq!(DEFAULT_ACTIVE_TRANSCODE_LIMIT, 2);
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(async {
            let tools = initialize_source_caption_runtime().await;
            run(&tools).await;
        }));
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        0
    );
}
