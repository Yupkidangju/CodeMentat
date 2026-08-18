use crate::hotkeys::{focused_shortcut_action, FocusedShortcutAction, GlobalShortcutController};
use crate::provider_setup::ProviderSetupState;
use crate::theme::MentatTheme;
use crate::widgets::pill_bar::PillBar;
use crate::widgets::settings_panel::SettingsPanel;
use egui::{
    vec2, CentralPanel, Color32, Context, Frame, RichText, Rounding, Stroke, ViewportCommand,
};
use futures_util::StreamExt;
use mentat_analysis::{
    AnswerBundleNormalizer, ApprovedInferenceRequest, ConsentAssemblyState, EgressFilter,
    EgressPacket, EgressReceipt, ProjectDetector, ProjectStructureSummary, SemanticKernel,
    SemanticKernelBuilder,
};
use mentat_core::error::MentatError;
use mentat_core::models::{
    AnswerBundle, Claim, ClaimClassification, ConflictItem, EvidenceRef, FileRecord,
    Recommendation, RepositoryProfile, RepositorySnapshot, SnapshotStatus,
};
use mentat_core::ports::RepositoryReader;
use mentat_inference::{
    BackendProfile, InferenceBackend, InferenceEvent, ModelCatalog, ModelVerification,
};
use mentat_inference_openai::MultiProviderAdapter;
use mentat_persona::{PersonaKind, PersonaRenderer};
use mentat_platform::PlatformManager;
use mentat_repository::{
    ReadOnlySession, RepositoryWatcher, ScanLimits, ScanOmission, ScanOmitReason, ScanOutcome,
};
use mentat_storage::SqliteStorage;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpansionTier {
    Tier1Pill,
    Tier2Card,
    Tier3Inspector,
}

pub const TIER1_SIZE: [f32; 2] = [580.0, 52.0];
pub const TIER2_SIZE: [f32; 2] = [580.0, 300.0];
pub const TIER3_SIZE: [f32; 2] = [660.0, 480.0];
pub const SETTINGS_SIZE: [f32; 2] = [660.0, 420.0];

pub fn viewport_size_for(tier: ExpansionTier, settings_open: bool) -> egui::Vec2 {
    let size = if settings_open && tier != ExpansionTier::Tier3Inspector {
        SETTINGS_SIZE
    } else {
        match tier {
            ExpansionTier::Tier1Pill if !settings_open => TIER1_SIZE,
            ExpansionTier::Tier1Pill | ExpansionTier::Tier2Card => TIER2_SIZE,
            ExpansionTier::Tier3Inspector => TIER3_SIZE,
        }
    };
    vec2(size[0], size[1])
}

fn snapshot_allows_analysis(status: SnapshotStatus) -> bool {
    status == SnapshotStatus::Ready
}

fn install_scan_token(slot: &mut Option<CancellationToken>, next: CancellationToken) {
    if let Some(previous) = slot.replace(next) {
        previous.cancel();
    }
}

pub type ScanChannel = Receiver<Result<(ScanOutcome, RepositorySnapshot), MentatError>>;
pub type ModelDiscoveryChannel = Receiver<(BackendProfile, Result<ModelCatalog, MentatError>)>;
pub type ModelVerificationChannel =
    Receiver<(BackendProfile, Result<ModelVerification, MentatError>)>;

pub struct MentatApp {
    pub session: Option<Arc<ReadOnlySession>>,
    pub snapshot: Option<RepositorySnapshot>,
    pub files: Vec<FileRecord>,
    pub summary: Option<ProjectStructureSummary>,
    pub kernel: Option<SemanticKernel>,
    pub query_text: String,
    pub expansion_tier: ExpansionTier,
    pub is_pinned: bool,
    pub status_text: String,
    pub rt: Arc<Runtime>,

    // Multi-Provider & Inference Backend
    pub backend: Arc<MultiProviderAdapter>,
    pub provider_setup: ProviderSetupState,
    pub persona: PersonaKind,
    pub settings_open: bool,
    pub provider_status: String,
    pub is_provider_busy: bool,

    // Storage persistence
    pub storage: Option<Arc<SqliteStorage>>,
    pub recent_repos: Vec<RepositoryProfile>,

    // Streaming state
    pub is_streaming: bool,
    pub streaming_cancel: Option<CancellationToken>,
    pub stream_rx: Option<Receiver<InferenceEvent>>,

    // Async task channels (Non-blocking UI loop DBG-F001 & DBG-F007)
    pub scan_rx: Option<ScanChannel>,
    pub model_discovery_rx: Option<ModelDiscoveryChannel>,
    pub model_verification_rx: Option<ModelVerificationChannel>,
    pub local_query_rx: Option<Receiver<Result<AnswerBundle, MentatError>>>,
    pub egress_packet_rx: Option<Receiver<(u64, Result<EgressPacket, MentatError>)>>,
    pub preview_rx: Option<Receiver<Result<String, MentatError>>>,

    // Egress Consent Sheet state (SEC-F001 / SEC-F011 generation-guarded exclusions)
    pub repo_consent_given: bool,
    pub consent: ConsentAssemblyState,
    pub restored_snapshot: Option<RepositorySnapshot>,

    // Analysis results
    pub recent_claims: Vec<Claim>,
    pub recent_recommendations: Vec<Recommendation>,
    pub recent_conflicts: Vec<ConflictItem>,
    pub evidence_map: Vec<EvidenceRef>,
    pub answer_preview: Option<String>,
    pub watcher: Option<RepositoryWatcher>,
    pub selected_file_idx: Option<usize>,
    pub selected_file_content: Option<String>,
    pub scan_cancel: Option<CancellationToken>,
    pub scan_omissions: Vec<ScanOmission>,
    pub citation_file_texts: HashMap<PathBuf, String>,
    pub focus_query: bool,
    pub window_visible: bool,
    pub global_shortcuts: GlobalShortcutController,
}

impl MentatApp {
    pub fn new(cc: &eframe::CreationContext<'_>, rt: Arc<Runtime>) -> Self {
        MentatTheme::apply(&cc.egui_ctx);

        let storage = match PlatformManager::get_app_data_dir() {
            Ok(dir) => {
                let db_path = dir.join("mentat.db");
                SqliteStorage::open(&db_path).ok().map(Arc::new)
            }
            Err(_) => None,
        };

        let recent_repos = storage
            .as_ref()
            .and_then(|s| s.list_recent_repos().ok())
            .unwrap_or_default();

        // [IMP-F005] Restore saved backend profile if available
        let profile = storage
            .as_ref()
            .and_then(|s| s.load_backend_profile().ok().flatten())
            .unwrap_or_default();

        let global_shortcuts = GlobalShortcutController::register(&cc.egui_ctx);
        let initial_status = global_shortcuts.status().to_string();

        Self {
            session: None,
            snapshot: None,
            files: Vec::new(),
            summary: None,
            kernel: None,
            query_text: String::new(),
            expansion_tier: ExpansionTier::Tier1Pill,
            is_pinned: true,
            status_text: initial_status,
            rt,
            backend: Arc::new(MultiProviderAdapter::new()),
            provider_setup: ProviderSetupState::new(profile),
            persona: PersonaKind::DefaultAnalyst,
            settings_open: false,
            provider_status: String::new(),
            is_provider_busy: false,
            storage,
            recent_repos,
            is_streaming: false,
            streaming_cancel: None,
            stream_rx: None,
            scan_rx: None,
            model_discovery_rx: None,
            model_verification_rx: None,
            local_query_rx: None,
            egress_packet_rx: None,
            preview_rx: None,
            repo_consent_given: false,
            consent: ConsentAssemblyState::default(),
            restored_snapshot: None,
            recent_claims: Vec::new(),
            recent_recommendations: Vec::new(),
            recent_conflicts: Vec::new(),
            evidence_map: Vec::new(),
            answer_preview: None,
            watcher: None,
            selected_file_idx: None,
            selected_file_content: None,
            scan_cancel: None,
            scan_omissions: Vec::new(),
            citation_file_texts: HashMap::new(),
            focus_query: false,
            window_visible: true,
            global_shortcuts,
        }
    }

    pub fn set_expansion_tier(&mut self, ctx: &Context, tier: ExpansionTier) {
        if self.expansion_tier != tier {
            self.expansion_tier = tier;
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(viewport_size_for(
                tier,
                self.settings_open,
            )));
        }
    }

    fn sync_viewport_size(&self, ctx: &Context) {
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(viewport_size_for(
            self.expansion_tier,
            self.settings_open,
        )));
    }

    pub fn open_repository(&mut self, path: std::path::PathBuf) {
        if let Some(previous) = self.scan_cancel.take() {
            previous.cancel();
        }
        self.scan_rx = None;
        let app_data_dir = match PlatformManager::get_app_data_dir() {
            Ok(dir) => dir,
            Err(e) => {
                self.status_text = format!("보안 오류: {}", e);
                return;
            }
        };

        if let Err(e) = PlatformManager::validate_storage_isolation(&app_data_dir, &path) {
            self.status_text = format!("저장소 격리 위반 차단: {}", e);
            return;
        }

        let known_id = self
            .storage
            .as_ref()
            .and_then(|s| s.find_repo_by_root(&path).ok().flatten())
            .map(|p| p.id);

        match ReadOnlySession::open_with_known_id(&path, known_id) {
            Ok(session) => {
                let profile = session.profile().clone();
                if let Some(ref s) = self.storage {
                    let _ = s.save_recent_repo(&profile);
                    self.recent_repos = s.list_recent_repos().unwrap_or_default();
                    // [IMP-F005] Restore latest snapshot under the stable repo ID
                    if let Ok(Some(mut last_snap)) = s.load_latest_snapshot(profile.id) {
                        last_snap.status = SnapshotStatus::Indexing;
                        self.restored_snapshot = Some(last_snap.clone());
                        self.snapshot = Some(last_snap);
                    }
                }

                let session_arc = Arc::new(session);
                self.status_text = if self.restored_snapshot.is_some() {
                    format!("이전 스냅샷 복원됨, 인덱싱 중... {}", path.display())
                } else {
                    format!("저장소 인덱싱 중... {}", path.display())
                };
                let mut watcher = RepositoryWatcher::new(&path);
                watcher.spawn_background();
                self.watcher = Some(watcher);
                self.repo_consent_given = false;
                self.consent.reset();

                let rt = self.rt.clone();
                let s = session_arc.clone();
                let cancel = CancellationToken::new();
                install_scan_token(&mut self.scan_cancel, cancel.clone());
                self.scan_omissions.clear();

                // DBG-F003: Cancellable ScanOutcome path
                let (tx, rx) = std::sync::mpsc::channel();
                self.scan_rx = Some(rx);
                rt.spawn(async move {
                    let result = s
                        .scan_files_with_limits(ScanLimits::default(), cancel)
                        .await
                        .map(|outcome| {
                            let snap = s.create_snapshot_from_outcome(&outcome);
                            (outcome, snap)
                        });
                    let _ = tx.send(result);
                });

                self.session = Some(session_arc);
            }
            Err(e) => {
                self.status_text = format!("오류: {}", e);
            }
        }
    }

    pub fn discover_provider_models(&mut self) {
        self.is_provider_busy = true;
        self.provider_status = "API 확인 및 모델 목록 조회 중...".to_string();
        let backend = self.backend.clone();
        let profile = self.provider_setup.begin_discovery();
        let (tx, rx) = std::sync::mpsc::channel();
        self.model_discovery_rx = Some(rx);

        self.rt.spawn(async move {
            let result = backend.discover_models(&profile).await;
            let _ = tx.send((profile, result));
        });
    }

    pub fn verify_draft_model(&mut self) {
        let profile = match self.provider_setup.verification_request() {
            Ok(profile) => profile,
            Err(message) => {
                self.provider_status = message;
                return;
            }
        };
        self.is_provider_busy = true;
        self.provider_status = "선택 모델의 실제 생성 호환성 확인 중...".to_string();
        let backend = self.backend.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.model_verification_rx = Some(rx);
        self.rt.spawn(async move {
            let result = backend.verify_model(&profile).await;
            let _ = tx.send((profile, result));
        });
    }

    pub fn activate_draft_profile(&mut self) {
        match self.provider_setup.activate() {
            Ok(()) => {
                self.provider_status = "검증된 모델을 프로그램 AI로 활성화했습니다.".to_string();
                self.consent.cancel();
                if let (Some(storage), Some(active)) =
                    (&self.storage, self.provider_setup.active_profile())
                {
                    let _ = storage.save_backend_profile(active);
                }
            }
            Err(message) => self.provider_status = message,
        }
    }

    pub fn handle_query(&mut self, ctx: &Context, query: String) {
        // 선행조건 오류도 카드 안에서 보여야 하므로 검증보다 먼저 대화 영역을 연다.
        self.set_expansion_tier(ctx, ExpansionTier::Tier2Card);

        let session = match &self.session {
            Some(s) => s.clone(),
            None => {
                self.status_text = "저장소를 먼저 열어주세요.".to_string();
                self.answer_preview = Some(self.status_text.clone());
                return;
            }
        };

        let snapshot_status = self
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.status.clone());
        if !snapshot_status.is_some_and(snapshot_allows_analysis) {
            self.status_text = "불완전하거나 인덱싱 중인 스냅샷은 분석할 수 없습니다.".to_string();
            self.answer_preview = Some(self.status_text.clone());
            return;
        }

        let summary = match &self.summary {
            Some(sum) => sum.clone(),
            None => {
                self.status_text = "저장소 인덱싱이 끝날 때까지 기다려주세요.".to_string();
                self.answer_preview = Some(self.status_text.clone());
                return;
            }
        };

        // [IMP-F004] Cleanly reset previous query results for request-scoped correctness
        self.recent_claims.clear();
        self.recent_recommendations.clear();
        self.recent_conflicts.clear();
        self.evidence_map.clear();
        self.answer_preview = None;

        // If local slash command, run instant async local workflow
        if query.starts_with('/') {
            let snap_id = self
                .snapshot
                .as_ref()
                .map(|s| s.id)
                .unwrap_or_else(uuid::Uuid::new_v4);
            let files = self.files.clone();
            let query_clone = query.clone();

            let (tx, rx) = std::sync::mpsc::channel();
            self.local_query_rx = Some(rx);
            let rt = self.rt.clone();
            rt.spawn(async move {
                let answer = SemanticKernelBuilder::run_local_workflow(
                    &query_clone,
                    session.as_ref(),
                    &files,
                    &summary,
                    snap_id,
                )
                .await;
                let _ = tx.send(answer);
            });
            return;
        }

        if self.provider_setup.active_profile().is_none() {
            self.status_text =
                "설정에서 모델 목록 조회, 호환성 확인, 활성화를 먼저 완료해 주세요.".to_string();
            self.answer_preview = Some(self.status_text.clone());
            return;
        }

        // [SEC-F001 & SEC-F011] Fail-closed egress consent with generation-guarded exclusions
        let generation = self.consent.begin_assembly(query);
        self.spawn_egress_assembly(session, summary, generation);
    }

    fn spawn_egress_assembly(
        &mut self,
        session: Arc<ReadOnlySession>,
        summary: ProjectStructureSummary,
        generation: u64,
    ) {
        let files = self.files.clone();
        let exclusions = self.consent.user_excluded_files.clone();
        let q = self.consent.pending_query.clone().unwrap_or_default();
        let snap_id = self
            .snapshot
            .as_ref()
            .map(|s| s.id)
            .unwrap_or_else(uuid::Uuid::new_v4);
        let Some(profile) = self.provider_setup.active_profile().cloned() else {
            self.status_text = "활성 공급자 프로필이 없어 전송 준비를 중단했습니다.".to_string();
            return;
        };

        let (tx, rx) = std::sync::mpsc::channel();
        self.egress_packet_rx = Some(rx);

        self.rt.spawn(async move {
            let packet = EgressFilter::assemble_packet_with_user_exclusions(
                session.as_ref(),
                &files,
                &summary,
                &q,
                &exclusions,
                snap_id,
                &profile,
            )
            .await;
            let _ = tx.send((generation, packet));
        });
    }

    pub fn start_inference_stream_with_approved_request(
        &mut self,
        approved: ApprovedInferenceRequest,
    ) {
        // [SEC-F001] Strict cryptographic verification and consume-once execution
        self.citation_file_texts = approved.citation_file_texts().clone();
        let request = match approved.into_inference_request() {
            Ok(req) => req,
            Err(e) => {
                self.status_text = format!("보호 오류: {}", e);
                return;
            }
        };

        let cancel_token = CancellationToken::new();
        self.streaming_cancel = Some(cancel_token.clone());
        self.is_streaming = true;
        self.answer_preview = Some(String::new());

        let (stream_tx, stream_rx) = std::sync::mpsc::channel();
        self.stream_rx = Some(stream_rx);

        let backend = self.backend.clone();

        self.rt.spawn(async move {
            match backend.infer_stream(request, cancel_token).await {
                Ok(mut stream) => {
                    while let Some(event) = stream.next().await {
                        let _ = stream_tx.send(event);
                    }
                }
                Err(e) => {
                    let _ = stream_tx.send(InferenceEvent::Failed {
                        error_code: "INFERENCE_FAILED".to_string(),
                        message: e.to_string(),
                    });
                }
            }
        });
    }

    pub fn cancel_inference(&mut self) {
        if let Some(cancel) = self.streaming_cancel.take() {
            cancel.cancel();
        }
        self.is_streaming = false;
        self.status_text = "추론이 취소되었습니다.".to_string();
    }

    pub fn cancel_scan(&mut self) {
        if let Some(cancel) = self.scan_cancel.take() {
            cancel.cancel();
        }
        self.status_text = "인덱싱 취소를 요청했습니다.".to_string();
    }

    /// [DBG-F001] Fully asynchronous preview loading with zero blocking on the UI thread
    pub fn load_file_preview(&mut self, idx: usize) {
        if let Some(file) = self.files.get(idx) {
            self.selected_file_idx = Some(idx);
            self.selected_file_content = Some("파일을 불러오는 중...".to_string());
            if let Some(session) = &self.session {
                let rt = self.rt.clone();
                let s = session.clone();
                let rel = file.relative_path.clone();

                let (tx, rx) = std::sync::mpsc::channel();
                self.preview_rx = Some(rx);
                rt.spawn(async move {
                    let content = s.read_file_content(&rel).await;
                    let _ = tx.send(content);
                });
            }
        }
    }
}

impl eframe::App for MentatApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // [DBG-F007] Keep UI awake and active whenever background tasks are running
        let has_pending_tasks = self.scan_rx.is_some()
            || self.model_discovery_rx.is_some()
            || self.model_verification_rx.is_some()
            || self.local_query_rx.is_some()
            || self.egress_packet_rx.is_some()
            || self.preview_rx.is_some()
            || self.is_streaming;

        if has_pending_tasks {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }

        // 1. Poll scanning task (Non-blocking & full terminal error state consumption)
        if let Some(ref rx) = self.scan_rx {
            match rx.try_recv() {
                Ok(Ok((outcome, snap))) => {
                    let files = outcome.files;
                    self.scan_omissions = outcome.omissions.clone();

                    let omit_note = if self.scan_omissions.is_empty() {
                        String::new()
                    } else {
                        format!(" / 누락 {}건", self.scan_omissions.len())
                    };
                    let complete = snap.status == SnapshotStatus::Ready;
                    self.status_text = if !complete {
                        format!(
                            "불완전한 인덱싱 결과 차단됨 ({}개 파일{})",
                            files.len(),
                            omit_note
                        )
                    } else {
                        let summary = ProjectDetector::summarize(&files);
                        format!(
                            "{}개 파일 ({} - {}) 인덱싱 완료{}",
                            snap.file_count,
                            summary.primary_language.as_deref().unwrap_or("General"),
                            snap.tree_digest.chars().take(8).collect::<String>(),
                            omit_note
                        )
                    };

                    let snap = if complete {
                        if let Some(restored) = self.restored_snapshot.take() {
                            if restored.tree_digest == snap.tree_digest
                                && restored.repo_id == snap.repo_id
                            {
                                let mut reused = restored;
                                reused.status = SnapshotStatus::Ready;
                                reused.file_count = snap.file_count;
                                reused.total_bytes = snap.total_bytes;
                                reused
                            } else {
                                snap
                            }
                        } else {
                            snap
                        }
                    } else {
                        self.restored_snapshot = None;
                        snap
                    };

                    if complete {
                        if let Some(ref s) = self.storage {
                            let _ = s.save_snapshot_meta(&snap);
                        }
                    }

                    self.files = files;
                    if complete {
                        let summary = ProjectDetector::summarize(&self.files);
                        self.kernel = Some(SemanticKernelBuilder::build(&summary));
                        self.summary = Some(summary);
                    } else {
                        self.summary = None;
                        self.kernel = None;
                    }
                    self.snapshot = Some(snap);
                    self.scan_rx = None;
                    self.scan_cancel = None;
                }
                Ok(Err(e)) => {
                    self.status_text = format!("인덱싱 실패: {}", e);
                    self.scan_rx = None;
                    self.scan_cancel = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.status_text = "인덱싱 작업 채널이 중단되었습니다.".to_string();
                    self.scan_rx = None;
                    self.scan_cancel = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }

        // 2. 공급자 모델 검색 결과는 요청 당시 Draft와 일치할 때만 수락한다.
        if let Some(ref rx) = self.model_discovery_rx {
            match rx.try_recv() {
                Ok((requested, Ok(catalog))) => {
                    let count = catalog.models.len();
                    self.provider_status =
                        match self.provider_setup.accept_catalog(&requested, catalog) {
                            Ok(()) => {
                                format!("API 확인 성공: 사용 가능한 모델 {count}개를 불러왔습니다.")
                            }
                            Err(message) => message,
                        };
                    self.is_provider_busy = false;
                    self.model_discovery_rx = None;
                }
                Ok((_, Err(e))) => {
                    self.provider_status = format!("모델 목록 조회 실패: {e}");
                    self.is_provider_busy = false;
                    self.model_discovery_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.provider_status = "모델 목록 조회 채널이 중단되었습니다.".to_string();
                    self.is_provider_busy = false;
                    self.model_discovery_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }

        // 3. 선택 모델 검증도 동일 Draft에 대한 결과만 수락한다.
        if let Some(ref rx) = self.model_verification_rx {
            match rx.try_recv() {
                Ok((requested, Ok(verification))) => {
                    let success_message = verification.message.clone();
                    self.provider_status = match self
                        .provider_setup
                        .accept_verification(&requested, verification)
                    {
                        Ok(()) => success_message,
                        Err(message) => message,
                    };
                    self.is_provider_busy = false;
                    self.model_verification_rx = None;
                }
                Ok((_, Err(e))) => {
                    self.provider_status = format!("선택 모델 검증 실패: {e}");
                    self.is_provider_busy = false;
                    self.model_verification_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.provider_status = "선택 모델 검증 채널이 중단되었습니다.".to_string();
                    self.is_provider_busy = false;
                    self.model_verification_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }

        // 4. Poll local query workflow (Non-blocking & full terminal error state consumption)
        if let Some(ref rx) = self.local_query_rx {
            match rx.try_recv() {
                Ok(Ok(bundle)) => {
                    let rendered = PersonaRenderer::render(&bundle, self.persona);
                    self.answer_preview = Some(rendered.direct_answer);
                    self.recent_claims = rendered.claims;
                    self.recent_recommendations = rendered.recommendations;
                    self.recent_conflicts = rendered.conflicts;
                    self.evidence_map = rendered.evidence_map;
                    self.local_query_rx = None;
                }
                Ok(Err(e)) => {
                    self.status_text = format!("로컬 분석 오류: {}", e);
                    self.local_query_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.status_text = "로컬 분석 채널이 중단되었습니다.".to_string();
                    self.local_query_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }

        // 4. Poll egress packet assembly (generation-guarded; stale results discarded)
        if let Some(ref rx) = self.egress_packet_rx {
            match rx.try_recv() {
                Ok((generation, Ok(packet))) => {
                    let accepted = self.consent.accept_packet(generation, packet);
                    if !accepted {
                        self.status_text =
                            "이전 제외 집합의 패킷을 폐기했습니다. 새 패킷을 기다립니다."
                                .to_string();
                    } else if let Some(p) = self.consent.display_packet() {
                        self.citation_file_texts = p.included_file_texts.clone();
                    }
                    if accepted && self.repo_consent_given {
                        if let Some((packet, q)) = self.consent.take_approved_packet() {
                            let snap_id = self
                                .snapshot
                                .as_ref()
                                .map(|s| s.id)
                                .unwrap_or_else(uuid::Uuid::new_v4);
                            let Some(active_profile) =
                                self.provider_setup.active_profile().cloned()
                            else {
                                self.status_text =
                                    "활성 모델이 없어 승인된 요청을 중단했습니다.".to_string();
                                self.egress_packet_rx = None;
                                return;
                            };
                            let receipt = EgressReceipt::issue(&packet, &active_profile);
                            if let Ok(approved_req) = ApprovedInferenceRequest::new(
                                receipt,
                                packet,
                                q,
                                snap_id,
                                active_profile,
                            ) {
                                self.start_inference_stream_with_approved_request(approved_req);
                            }
                        }
                    }
                    self.egress_packet_rx = None;
                }
                Ok((_, Err(e))) => {
                    self.status_text = format!("보호 오류: 컨텍스트 조립 실패: {}", e);
                    self.consent.rebuilding = false;
                    self.egress_packet_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.status_text = "컨텍스트 조립 채널이 중단되었습니다.".to_string();
                    self.consent.rebuilding = false;
                    self.egress_packet_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }

        // 5. Poll file preview task (Non-blocking & full terminal error state consumption)
        if let Some(ref rx) = self.preview_rx {
            match rx.try_recv() {
                Ok(Ok(content)) => {
                    self.selected_file_content = Some(content);
                    self.preview_rx = None;
                }
                Ok(Err(e)) => {
                    self.selected_file_content = Some(format!("파일 로드 오류: {}", e));
                    self.preview_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.selected_file_content =
                        Some("파일 로드 채널이 중단되었습니다.".to_string());
                    self.preview_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }

        // 6. Poll streaming events (DBG-F007: Handle TryRecvError::Disconnected terminal state)
        if self.is_streaming {
            let mut finished = false;

            if let Some(ref rx) = self.stream_rx {
                loop {
                    match rx.try_recv() {
                        Ok(event) => match event {
                            InferenceEvent::Started { .. } => {
                                let model = self
                                    .provider_setup
                                    .active_profile()
                                    .map(|profile| profile.model.as_str())
                                    .unwrap_or("비활성 모델");
                                self.status_text = format!("{model} 스트리밍 중...");
                            }
                            InferenceEvent::TextDelta(delta) => {
                                if let Some(ref mut text) = self.answer_preview {
                                    text.push_str(&delta);
                                }
                            }
                            InferenceEvent::Completed { full_text } => {
                                let snap_id = self
                                    .snapshot
                                    .as_ref()
                                    .map(|s| s.id)
                                    .unwrap_or_else(uuid::Uuid::new_v4);
                                let bundle = AnswerBundleNormalizer::from_model_text_with_contents(
                                    uuid::Uuid::new_v4(),
                                    snap_id,
                                    &full_text,
                                    &self.files,
                                    &self.citation_file_texts,
                                );
                                let rendered = PersonaRenderer::render(&bundle, self.persona);
                                self.answer_preview = Some(rendered.direct_answer);
                                self.recent_claims = rendered.claims;
                                self.recent_recommendations = rendered.recommendations;
                                self.recent_conflicts = rendered.conflicts;
                                self.evidence_map = rendered.evidence_map;
                                self.status_text = "완료됨".to_string();
                                finished = true;
                                break;
                            }
                            InferenceEvent::Cancelled => {
                                self.status_text = "취소됨".to_string();
                                finished = true;
                                break;
                            }
                            InferenceEvent::Failed {
                                error_code,
                                message,
                            } => {
                                self.status_text = format!("오류 [{}] {}", error_code, message);
                                finished = true;
                                break;
                            }
                            _ => {}
                        },
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            break;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            self.status_text = "스트리밍 연결이 종료되었습니다.".to_string();
                            finished = true;
                            break;
                        }
                    }
                }
            }

            if finished {
                self.is_streaming = false;
                self.streaming_cancel = None;
                self.stream_rx = None;
            }
        }

        // Periodic file watcher check for STALE transitions (background poll only)
        if let Some(ref mut watcher) = self.watcher {
            if let Ok(true) = watcher.poll_changes() {
                if let Some(ref mut snap) = self.snapshot {
                    if snapshot_allows_analysis(snap.status.clone()) {
                        snap.status = SnapshotStatus::Stale;
                        self.status_text =
                            "경고: 외부 파일 변경 감지됨 (STALE: 재인덱싱 권장)".to_string();
                    }
                }
            }
        }

        // In-app shortcuts: Ctrl+K focus, Ctrl+P pin, Alt+Space / Ctrl+Alt+M hide
        let (focus_query, toggle_pin, toggle_visible) = ctx.input(|i| {
            let ctrl = i.modifiers.ctrl;
            let alt = i.modifiers.alt;
            (
                (ctrl && i.key_pressed(egui::Key::K)) || i.key_pressed(egui::Key::Slash),
                ctrl && i.key_pressed(egui::Key::P),
                (alt && i.key_pressed(egui::Key::Space))
                    || (ctrl && alt && i.key_pressed(egui::Key::M)),
            )
        });
        if focus_query {
            self.focus_query = true;
        }
        if toggle_pin {
            self.is_pinned = !self.is_pinned;
            let level = if self.is_pinned {
                egui::viewport::WindowLevel::AlwaysOnTop
            } else {
                egui::viewport::WindowLevel::Normal
            };
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(level));
        }
        if let Some(visible) = self.global_shortcuts.take_visibility_request() {
            self.window_visible = visible;
            self.set_expansion_tier(ctx, ExpansionTier::Tier1Pill);
            self.status_text = self.global_shortcuts.status().to_string();
        } else if toggle_visible {
            match focused_shortcut_action(self.global_shortcuts.is_registered()) {
                FocusedShortcutAction::CollapseOnly => {
                    self.set_expansion_tier(ctx, ExpansionTier::Tier1Pill);
                    self.status_text = format!(
                        "{}; 안전 정책상 창 숨김 대신 Tier 1로 접었습니다.",
                        self.global_shortcuts.status()
                    );
                }
            }
        }

        // Global Keyboard Shortcut Esc to cancel or collapse tiers
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.is_streaming {
                self.cancel_inference();
            } else if self.scan_rx.is_some() {
                self.cancel_scan();
            } else if self.settings_open {
                self.settings_open = false;
                self.sync_viewport_size(ctx);
            } else {
                match self.expansion_tier {
                    ExpansionTier::Tier3Inspector => {
                        self.set_expansion_tier(ctx, ExpansionTier::Tier2Card)
                    }
                    ExpansionTier::Tier2Card => {
                        self.set_expansion_tier(ctx, ExpansionTier::Tier1Pill)
                    }
                    ExpansionTier::Tier1Pill => {}
                }
            }
        }

        let repo_display_name = self
            .session
            .as_ref()
            .map(|s| s.profile().display_name.clone());

        let frame = Frame::none()
            .fill(MentatTheme::BG_BASE)
            .stroke(Stroke::new(1.0, MentatTheme::BORDER_COLOR))
            .rounding(Rounding::same(12.0))
            .inner_margin(egui::Margin::same(8.0));

        CentralPanel::default().frame(frame).show(ctx, |ui| {
            // Render Tier 1 Pill Bar (SEC-F008: dynamic read-only indicator)
            let is_read_only = self.session.is_some();
            let request_focus = self.focus_query;
            self.focus_query = false;
            let action = PillBar::new(
                repo_display_name.as_deref(),
                is_read_only,
                &mut self.query_text,
                self.is_pinned,
                &self.status_text,
            )
            .request_focus(request_focus)
            .show(ui);

            if action.open_repo_clicked {
                if let Some(folder) = PlatformManager::pick_folder() {
                    self.open_repository(folder);
                }
            }

            if let Some(query) = action.query_submitted {
                self.handle_query(ctx, query);
            }

            if let Some(chip) = action.quick_chip_clicked {
                self.handle_query(ctx, chip);
            }

            if action.pin_toggled {
                self.is_pinned = !self.is_pinned;
                let level = if self.is_pinned {
                    egui::viewport::WindowLevel::AlwaysOnTop
                } else {
                    egui::viewport::WindowLevel::Normal
                };
                ctx.send_viewport_cmd(ViewportCommand::WindowLevel(level));
            }

            if action.settings_clicked {
                self.settings_open = !self.settings_open;
                self.sync_viewport_size(ctx);
            }

            // Render Settings Panel (if toggled)
            if self.settings_open {
                ui.add_space(8.0);
                let profile_before_edit = self.provider_setup.draft_profile.clone();
                let setup_stage = self.provider_setup.stage();
                let settings_action = SettingsPanel::new(
                    &mut self.provider_setup.draft_profile,
                    &mut self.persona,
                    &self.provider_setup.catalog.models,
                    setup_stage,
                    &self.provider_status,
                    self.is_provider_busy,
                )
                .show(ui);
                self.provider_setup.reconcile_edit(&profile_before_edit);
                if let Some(model_id) = settings_action.selected_model.as_deref() {
                    if let Err(message) = self.provider_setup.select_model(model_id) {
                        self.provider_status = message;
                    }
                }

                // [IMP-F005] Quick Reopen of Recent Repositories
                if !self.recent_repos.is_empty() {
                    ui.separator();
                    ui.label(RichText::new("최근 저장소 다시 열기:").size(11.5).strong());
                    let mut reopened_root = None;
                    for repo in &self.recent_repos {
                        if ui.button(&repo.display_name).clicked() {
                            reopened_root = Some(repo.root_path.clone());
                        }
                    }
                    if let Some(root) = reopened_root {
                        self.open_repository(root);
                        self.settings_open = false;
                    }
                }

                if settings_action.discover_clicked {
                    self.discover_provider_models();
                }
                if settings_action.verify_clicked {
                    self.verify_draft_model();
                }
                if settings_action.activate_clicked {
                    self.activate_draft_profile();
                }
                if settings_action.close_clicked {
                    self.settings_open = false;
                    self.sync_viewport_size(ctx);
                }
            }

            if self.scan_rx.is_some() {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("인덱싱 진행 중")
                            .color(MentatTheme::STATUS_INFERENCING)
                            .size(12.0),
                    );
                    if ui.button("⏹️ 인덱싱 취소").clicked() {
                        self.cancel_scan();
                    }
                });
            }
            if !self.scan_omissions.is_empty() {
                ui.collapsing(
                    format!("스캔에서 건너뛴 항목 {}건", self.scan_omissions.len()),
                    |ui| {
                        for omission in &self.scan_omissions {
                            let reason = match omission.reason {
                                ScanOmitReason::FileTooLarge => "FileTooLarge",
                                ScanOmitReason::TotalBytesLimit => "TotalBytesLimit",
                                ScanOmitReason::FileCountLimit => "FileCountLimit",
                                ScanOmitReason::Cancelled => "Cancelled",
                            };
                            ui.label(format!("{} — {}", omission.relative_path.display(), reason));
                        }
                    },
                );
            }

            // Render Egress Consent Sheet (shown while assembling or awaiting approval)
            let mut consent_granted = false;
            let mut consent_cancelled = false;
            let mut exclusion_toggled = None;

            if self.consent.should_show_sheet() {
                ui.add_space(8.0);
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("외부 데이터 전송 승인 (Egress Consent)").color(MentatTheme::STATUS_CONFLICT).strong().size(13.0));
                    });

                    if self.consent.rebuilding || !self.consent.can_approve() {
                        ui.label(RichText::new("제외 집합을 반영하는 중입니다. 새 패킷이 도착하기 전까지 승인할 수 없습니다.")
                            .color(MentatTheme::STATUS_INFERENCING)
                            .size(12.0));
                    }

                    if let Some(packet) = self.consent.display_packet() {
                        ui.label(RichText::new(format!(
                            "선택된 공급자({})로 문맥을 전송합니다. (포함: {}개 파일 / 약 {} 토큰 / 해시: {})",
                            self.provider_setup
                                .active_profile()
                                .map(|profile| profile.model.as_str())
                                .unwrap_or("비활성"),
                            packet.included_files.len(),
                            packet.estimated_tokens,
                            packet.packet_hash.chars().take(8).collect::<String>()
                        )).size(12.0));

                        if !packet.excluded_sensitive_files.is_empty() {
                            ui.label(RichText::new(format!(
                                "자동 제외된 파일: {}건 (.env, 인증서, 사용자 제외 파일 등)",
                                packet.excluded_sensitive_files.len()
                            )).color(MentatTheme::STATUS_READ_ONLY).size(11.5));
                        }

                        if packet.redacted_secret_occurrences > 0 {
                            ui.label(RichText::new(format!(
                                "내용 중 마스킹된 비밀정보: {}건",
                                packet.redacted_secret_occurrences
                            )).color(MentatTheme::STATUS_INFERENCING).size(11.5));
                        }
                    }

                    ui.collapsing("포함될 파일 및 행 범위 미리보기 (체크 해제 시 제외)", |ui| {
                        let preview_refs = self.consent.preview_refs().to_vec();
                        for ref_item in &preview_refs {
                            let mut is_included = !self.consent.user_excluded_files.contains(&ref_item.relative_path);
                            let label = format!(
                                "{} (1..{}행, 총 {}행)",
                                ref_item.relative_path.display(),
                                ref_item.line_end,
                                ref_item.line_count
                            );
                            if ui.checkbox(&mut is_included, label).changed() {
                                exclusion_toggled = Some((ref_item.relative_path.clone(), !is_included));
                            }
                        }
                    });

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let approve = ui.add_enabled(
                            self.consent.can_approve(),
                            egui::Button::new(RichText::new("전송 승인 및 실행").strong()),
                        );
                        if approve.clicked() {
                            consent_granted = true;
                        }
                        if ui.button("취소").clicked() {
                            consent_cancelled = true;
                        }
                    });
                });
            }

            // [SEC-F011] Exclusion immediately invalidates the old packet
            if let Some((path, exclude)) = exclusion_toggled {
                let generation = self.consent.apply_exclusion_toggle(path, exclude);
                if let (Some(session), Some(summary)) = (self.session.clone(), self.summary.clone()) {
                    self.spawn_egress_assembly(session, summary, generation);
                }
            }

            if consent_granted {
                if let Some((packet, q)) = self.consent.take_approved_packet() {
                    self.repo_consent_given = true;
                    let snap_id = self.snapshot.as_ref().map(|s| s.id).unwrap_or_else(uuid::Uuid::new_v4);

                    let active_profile = self.provider_setup.active_profile().cloned();
                    if let Some(active_profile) = active_profile {
                    let receipt = EgressReceipt::issue(&packet, &active_profile);
                    if let Ok(approved) = ApprovedInferenceRequest::new(
                        receipt,
                        packet,
                        q,
                        snap_id,
                        active_profile,
                    ) {
                        self.start_inference_stream_with_approved_request(approved);
                    }
                    } else {
                        self.status_text = "활성 모델이 없어 전송을 중단했습니다.".to_string();
                    }
                }
            }
            if consent_cancelled {
                self.consent.cancel();
                self.status_text = "전송이 사용자에 의해 취소되었습니다.".to_string();
            }

            // Render Tier 2 Smart Card (if expanded)
            if self.expansion_tier != ExpansionTier::Tier1Pill {
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // Quick Action Chips Row
                ui.horizontal(|ui| {
                    ui.label(RichText::new("빠른 분석:").size(11.5).color(MentatTheme::TEXT_MUTED));
                    if ui.small_button("/onboard").clicked() {
                        self.handle_query(ctx, "/onboard".to_string());
                    }
                    if ui.small_button("/structure").clicked() {
                        self.handle_query(ctx, "/structure".to_string());
                    }
                    if ui.small_button("/conflicts").clicked() {
                        self.handle_query(ctx, "/conflicts".to_string());
                    }
                    if ui.small_button("/where").clicked() {
                        self.handle_query(ctx, "/where".to_string());
                    }
                    if ui.small_button("/risks").clicked() {
                        self.handle_query(ctx, "/risks".to_string());
                    }

                    if self.is_streaming {
                        ui.add_space(8.0);
                        if ui.button(RichText::new("⏹️ 스트리밍 취소 (Esc)").color(MentatTheme::STATUS_CONFLICT).size(11.5)).clicked() {
                            self.cancel_inference();
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("접기 (Esc)").clicked() {
                            self.set_expansion_tier(ctx, ExpansionTier::Tier1Pill);
                        }
                    });
                });

                if let Some(ref text) = self.answer_preview {
                    ui.add_space(6.0);
                    ui.label(RichText::new(text).color(MentatTheme::TEXT_PRIMARY).size(12.5));
                }

                // Render claims
                ui.add_space(6.0);
                for claim in &self.recent_claims {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            let (badge_text, color) = match claim.classification {
                                ClaimClassification::Observed => ("[OBSERVED]", MentatTheme::STATUS_READ_ONLY),
                                ClaimClassification::Inferred => ("[INFERRED]", MentatTheme::STATUS_INFERENCING),
                                ClaimClassification::Proposed => ("[PROPOSED]", Color32::from_rgb(168, 85, 247)),
                                ClaimClassification::Conflict => ("[CONFLICT]", MentatTheme::STATUS_CONFLICT),
                                ClaimClassification::Unknown => ("[UNKNOWN]", MentatTheme::TEXT_MUTED),
                            };

                            ui.label(RichText::new(badge_text).color(color).strong().size(11.5));
                            ui.label(RichText::new(&claim.statement).color(MentatTheme::TEXT_PRIMARY).size(12.0));
                        });
                        if let Some(ref rationale) = claim.rationale {
                            ui.label(RichText::new(format!("  └─ {}", rationale)).color(MentatTheme::TEXT_MUTED).size(11.0));
                        }
                    });
                }

                // Render conflicts
                for conflict in &self.recent_conflicts {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("[CONFLICT]").color(MentatTheme::STATUS_CONFLICT).strong().size(11.5));
                            ui.label(RichText::new(format!("{} vs {}", conflict.side_a, conflict.side_b)).color(MentatTheme::STATUS_CONFLICT).size(12.0));
                        });
                        ui.label(RichText::new(format!("  └─ 영향: {}", conflict.impact)).color(MentatTheme::TEXT_MUTED).size(11.0));
                    });
                }

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("답변 복사").clicked() {
                        if let Some(ref text) = self.answer_preview {
                            let _ = PlatformManager::copy_to_clipboard(text);
                            self.status_text = "클립보드에 복사되었습니다.".to_string();
                        }
                    }

                    let inspector_label = if self.expansion_tier == ExpansionTier::Tier3Inspector {
                        "▴ 소스 증거 인스펙터 접기"
                    } else {
                        "▾ 소스 증거 인스펙터 열기"
                    };

                    if ui.button(inspector_label).clicked() {
                        let new_tier = if self.expansion_tier == ExpansionTier::Tier3Inspector {
                            ExpansionTier::Tier2Card
                        } else {
                            ExpansionTier::Tier3Inspector
                        };
                        self.set_expansion_tier(ctx, new_tier);
                    }
                });

                // Render Tier 3 Inspector Panel
                if self.expansion_tier == ExpansionTier::Tier3Inspector {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.heading(RichText::new("Detailed Evidence & File Inspector").size(13.0));

                    let mut clicked_file_idx = None;

                    ui.columns(2, |cols| {
                        cols[0].group(|ui| {
                            ui.label(RichText::new("저장소 파일 트리").strong().size(12.0));
                            egui::ScrollArea::vertical().id_salt("file_tree_scroll").max_height(160.0).show(ui, |ui| {
                                for (idx, file) in self.files.iter().enumerate() {
                                    let label = format!("{} ({}행)", file.relative_path.display(), file.line_count.unwrap_or(0));
                                    let is_selected = self.selected_file_idx == Some(idx);
                                    if ui.selectable_label(is_selected, label).clicked() {
                                        clicked_file_idx = Some(idx);
                                    }
                                }
                            });
                        });

                        cols[1].group(|ui| {
                            ui.label(RichText::new("소스코드 행 뷰어").strong().size(12.0));
                            egui::ScrollArea::vertical().id_salt("code_view_scroll").max_height(160.0).show(ui, |ui| {
                                if let Some(ref content) = self.selected_file_content {
                                    for (i, line) in content.lines().enumerate() {
                                        ui.monospace(format!("{:3} | {}", i + 1, line));
                                    }
                                } else {
                                    ui.label(RichText::new("왼쪽에서 파일을 선택하면 소스코드가 표시됩니다.").color(MentatTheme::TEXT_MUTED));
                                }
                            });
                        });
                    });

                    if let Some(idx) = clicked_file_idx {
                        self.load_file_preview(idx);
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_imp_f003_viewport_and_theme_tokens() {
        assert_eq!(TIER1_SIZE, [580.0, 52.0]);
        assert_eq!(TIER2_SIZE, [580.0, 300.0]);
        assert_eq!(TIER3_SIZE, [660.0, 480.0]);
        assert_eq!(MentatTheme::BG_BASE, Color32::from_rgb(18, 21, 26));
        assert_eq!(
            MentatTheme::STATUS_CONFLICT,
            Color32::from_rgb(245, 158, 11)
        );
        assert_eq!(MentatTheme::TEXT_PRIMARY, Color32::from_rgb(243, 244, 246));
    }

    #[test]
    fn test_app_expansion_tier_transitions() {
        let mut tier = ExpansionTier::Tier1Pill;
        assert_eq!(tier, ExpansionTier::Tier1Pill);

        tier = ExpansionTier::Tier2Card;
        assert_eq!(tier, ExpansionTier::Tier2Card);

        tier = ExpansionTier::Tier3Inspector;
        assert_eq!(tier, ExpansionTier::Tier3Inspector);
    }

    #[test]
    fn settings_panel_requests_visible_card_height() {
        assert_eq!(
            viewport_size_for(ExpansionTier::Tier1Pill, true),
            vec2(SETTINGS_SIZE[0], SETTINGS_SIZE[1])
        );
        assert_eq!(
            viewport_size_for(ExpansionTier::Tier3Inspector, true),
            vec2(TIER3_SIZE[0], TIER3_SIZE[1])
        );
    }

    #[test]
    fn incomplete_or_indexing_snapshot_cannot_enter_analysis() {
        assert!(snapshot_allows_analysis(SnapshotStatus::Ready));
        assert!(!snapshot_allows_analysis(SnapshotStatus::Stale));
        assert!(!snapshot_allows_analysis(SnapshotStatus::Indexing));
        assert!(!snapshot_allows_analysis(SnapshotStatus::Incomplete));
    }

    #[test]
    fn replacing_repository_scan_cancels_previous_token() {
        let old = CancellationToken::new();
        let old_observer = old.clone();
        let mut slot = Some(old);
        let next = CancellationToken::new();
        install_scan_token(&mut slot, next.clone());

        assert!(old_observer.is_cancelled());
        assert!(!next.is_cancelled());
    }

    #[test]
    fn unverified_model_narrative_never_becomes_app_answer_preview() {
        let raw = "unverified hallucinated conclusion";
        let bundle = AnswerBundleNormalizer::from_model_text(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            raw,
            &[],
        );
        let rendered = PersonaRenderer::render(&bundle, PersonaKind::DefaultAnalyst);

        assert!(!rendered.direct_answer.contains(raw));
        assert!(rendered
            .direct_answer
            .contains("검증된 근거 기반 답변을 생성할 수 없습니다"));
        assert_eq!(rendered.raw_model_response.as_deref(), Some(raw));
    }

    #[test]
    fn invalid_cloud_conflict_evidence_never_reaches_app_projection() {
        let snapshot = uuid::Uuid::new_v4();
        let conflict = uuid::Uuid::new_v4();
        let missing_evidence = uuid::Uuid::new_v4();
        let json = format!(
            r#"{{
                "request_id":"{}",
                "snapshot_id":"{}",
                "direct_answer":"unverified conflict",
                "claims":[],
                "evidence_map":[],
                "recommendations":[],
                "conflicts":[{{
                    "id":"{}",
                    "side_a":"A",
                    "side_b":"B",
                    "evidence_ids":["{}"],
                    "impact":"impact",
                    "unresolved_question":"question"
                }}],
                "raw_model_response":null
            }}"#,
            uuid::Uuid::new_v4(),
            snapshot,
            conflict,
            missing_evidence,
        );
        let bundle =
            AnswerBundleNormalizer::from_model_text(uuid::Uuid::new_v4(), snapshot, &json, &[]);
        let rendered = PersonaRenderer::render(&bundle, PersonaKind::DefaultAnalyst);

        assert!(rendered.conflicts.is_empty());
        assert!(!rendered.direct_answer.contains("unverified conflict"));
    }

    #[test]
    fn test_tampered_egress_request_rejection_fail_closed() {
        use mentat_inference::{BackendProfile, ProviderKind};

        let snap_id = uuid::Uuid::new_v4();
        let prompt_context = "context content".to_string();
        let profile = BackendProfile {
            id: uuid::Uuid::new_v4(),
            name: "Gemini Test".to_string(),
            provider: ProviderKind::GoogleGemini,
            base_url: ProviderKind::GoogleGemini.default_base_url().to_string(),
            model: "fixture-gemini".to_string(),
            api_key: None,
            timeout_secs: 30,
        };
        let question = EgressFilter::scan_and_redact_secrets("query").0;
        let mut packet = EgressPacket {
            packet_id: uuid::Uuid::new_v4(),
            packet_hash: String::new(),
            included_files: vec![],
            included_file_refs: vec![],
            excluded_sensitive_files: vec![],
            redacted_secret_occurrences: 0,
            estimated_tokens: 10,
            prompt_context: prompt_context.clone(),
            snapshot_id: snap_id,
            redacted_user_question: question.clone(),
            included_file_texts: std::collections::HashMap::new(),
        };
        packet.seal_for_profile(&profile);
        let receipt = EgressReceipt::issue(&packet, &profile);

        let req = ApprovedInferenceRequest::new(
            receipt.clone(),
            packet.clone(),
            question.clone(),
            snap_id,
            profile.clone(),
        )
        .expect("Should construct approved request");

        assert!(req.verify_integrity());

        // Single-use consume succeeds
        let consumed = req
            .into_inference_request()
            .expect("Should consume successfully");
        assert_eq!(consumed.user_question, question);
        assert_eq!(consumed.profile.model, "fixture-gemini");

        // Tampered prompt context in packet fails construction
        let mut tampered_packet = packet;
        tampered_packet.prompt_context = "tampered context".to_string();

        let err =
            ApprovedInferenceRequest::new(receipt, tampered_packet, question, snap_id, profile);
        assert!(err.is_err(), "Tampered packet hash must fail construction");
    }
}
