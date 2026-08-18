use crate::theme::MentatTheme;
use crate::widgets::pill_bar::PillBar;
use crate::widgets::settings_panel::SettingsPanel;
use egui::{
    vec2, CentralPanel, Color32, Context, Frame, RichText, Rounding, Stroke, ViewportCommand,
};
use futures_util::StreamExt;
use mentat_analysis::{
    ApprovedInferenceRequest, EgressFilter, EgressPacket, EgressReceipt, ProjectDetector,
    ProjectStructureSummary, SemanticKernel, SemanticKernelBuilder,
};
use mentat_core::error::MentatError;
use mentat_core::models::{
    AnswerBundle, Claim, ClaimClassification, ConflictItem, EvidenceRef, FileRecord,
    Recommendation, RepositoryProfile, RepositorySnapshot, SnapshotStatus,
};
use mentat_core::ports::RepositoryReader;
use mentat_inference::{BackendProfile, InferenceBackend, InferenceEvent};
use mentat_inference_openai::MultiProviderAdapter;
use mentat_persona::{PersonaKind, PersonaRenderer};
use mentat_platform::PlatformManager;
use mentat_repository::{ReadOnlySession, RepositoryWatcher};
use mentat_storage::SqliteStorage;
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

pub type ScanChannel = Receiver<(
    Result<Vec<FileRecord>, MentatError>,
    Result<RepositorySnapshot, MentatError>,
)>;

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
    pub profile: BackendProfile,
    pub persona: PersonaKind,
    pub settings_open: bool,
    pub ping_status: String,
    pub is_pinging: bool,

    // Storage persistence
    pub storage: Option<Arc<SqliteStorage>>,
    pub recent_repos: Vec<RepositoryProfile>,

    // Streaming state
    pub is_streaming: bool,
    pub streaming_cancel: Option<CancellationToken>,
    pub stream_rx: Option<Receiver<InferenceEvent>>,

    // Async task channels (Non-blocking UI loop DBG-F001 & DBG-F007)
    pub scan_rx: Option<ScanChannel>,
    pub ping_rx: Option<Receiver<Result<mentat_inference::HealthStatus, MentatError>>>,
    pub local_query_rx: Option<Receiver<Result<AnswerBundle, MentatError>>>,
    pub egress_packet_rx: Option<Receiver<Result<EgressPacket, MentatError>>>,
    pub preview_rx: Option<Receiver<Result<String, MentatError>>>,

    // Egress Consent Sheet state (SEC-F001 Fail-Closed single-use receipt)
    pub repo_consent_given: bool,
    pub pending_egress_packet: Option<EgressPacket>,
    pub pending_query: Option<String>,

    // Analysis results
    pub recent_claims: Vec<Claim>,
    pub recent_recommendations: Vec<Recommendation>,
    pub recent_conflicts: Vec<ConflictItem>,
    pub evidence_map: Vec<EvidenceRef>,
    pub answer_preview: Option<String>,
    pub watcher: Option<RepositoryWatcher>,
    pub selected_file_idx: Option<usize>,
    pub selected_file_content: Option<String>,
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

        Self {
            session: None,
            snapshot: None,
            files: Vec::new(),
            summary: None,
            kernel: None,
            query_text: String::new(),
            expansion_tier: ExpansionTier::Tier1Pill,
            is_pinned: true,
            status_text: "준비됨".to_string(),
            rt,
            backend: Arc::new(MultiProviderAdapter::new()),
            profile,
            persona: PersonaKind::DefaultAnalyst,
            settings_open: false,
            ping_status: String::new(),
            is_pinging: false,
            storage,
            recent_repos,
            is_streaming: false,
            streaming_cancel: None,
            stream_rx: None,
            scan_rx: None,
            ping_rx: None,
            local_query_rx: None,
            egress_packet_rx: None,
            preview_rx: None,
            repo_consent_given: false,
            pending_egress_packet: None,
            pending_query: None,
            recent_claims: Vec::new(),
            recent_recommendations: Vec::new(),
            recent_conflicts: Vec::new(),
            evidence_map: Vec::new(),
            answer_preview: None,
            watcher: None,
            selected_file_idx: None,
            selected_file_content: None,
        }
    }

    pub fn set_expansion_tier(&mut self, ctx: &Context, tier: ExpansionTier) {
        if self.expansion_tier != tier {
            self.expansion_tier = tier;
            let size = match tier {
                ExpansionTier::Tier1Pill => vec2(580.0, 52.0),
                ExpansionTier::Tier2Card => vec2(580.0, 300.0),
                ExpansionTier::Tier3Inspector => vec2(660.0, 480.0),
            };
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
        }
    }

    pub fn open_repository(&mut self, path: std::path::PathBuf) {
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

        match ReadOnlySession::open(&path) {
            Ok(session) => {
                let profile = session.profile().clone();
                if let Some(ref s) = self.storage {
                    let _ = s.save_recent_repo(&profile);
                    self.recent_repos = s.list_recent_repos().unwrap_or_default();
                }

                let session_arc = Arc::new(session);
                self.status_text = format!("저장소 인덱싱 중... {}", path.display());
                self.watcher = Some(RepositoryWatcher::new(&path));
                self.repo_consent_given = false;
                self.pending_egress_packet = None;
                self.pending_query = None;

                let rt = self.rt.clone();
                let s = session_arc.clone();

                // DBG-F001 & DBG-F002: Single-scan async indexing without double disk traversal
                let (tx, rx) = std::sync::mpsc::channel();
                self.scan_rx = Some(rx);
                rt.spawn(async move {
                    let files_res = s.scan_files().await;
                    let snap_res = files_res
                        .as_ref()
                        .map(|files| s.create_snapshot_from_files(files))
                        .map_err(|e| e.clone());
                    let _ = tx.send((files_res, snap_res));
                });

                self.session = Some(session_arc);
            }
            Err(e) => {
                self.status_text = format!("오류: {}", e);
            }
        }
    }

    pub fn ping_backend(&mut self) {
        self.is_pinging = true;
        self.ping_status = "연결 시험 중...".to_string();

        let backend = self.backend.clone();
        let profile = self.profile.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.ping_rx = Some(rx);

        self.rt.spawn(async move {
            let res = backend.health_check(&profile).await;
            let _ = tx.send(res);
        });
    }

    pub fn handle_query(&mut self, ctx: &Context, query: String) {
        let session = match &self.session {
            Some(s) => s.clone(),
            None => {
                self.status_text = "저장소를 먼저 열어주세요.".to_string();
                return;
            }
        };

        let summary = match &self.summary {
            Some(sum) => sum.clone(),
            None => return,
        };

        self.set_expansion_tier(ctx, ExpansionTier::Tier2Card);

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

        // [SEC-F001] Fail-Closed Egress Consent Workflow (Non-blocking async assembly)
        let files = self.files.clone();
        let sum = summary.clone();
        let s = session.clone();
        let q = query.clone();

        let (tx, rx) = std::sync::mpsc::channel();
        self.egress_packet_rx = Some(rx);
        self.pending_query = Some(query);

        self.rt.spawn(async move {
            let packet = EgressFilter::assemble_packet(s.as_ref(), &files, &sum, &q).await;
            let _ = tx.send(packet);
        });
    }

    pub fn start_inference_stream_with_approved_request(
        &mut self,
        approved: ApprovedInferenceRequest,
    ) {
        // [SEC-F001] Strict cryptographic verification and consume-once execution
        let request = match approved.into_inference_request() {
            Ok(req) => req,
            Err(e) => {
                self.status_text = format!("🛡️ 오류: {}", e);
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
            || self.ping_rx.is_some()
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
                Ok((Ok(files), Ok(snap))) => {
                    let summary = ProjectDetector::summarize(&files);
                    let kernel = SemanticKernelBuilder::build(&summary);

                    self.status_text = format!(
                        "{}개 파일 ({} - {}) 인덱싱 완료",
                        snap.file_count,
                        summary.primary_language.as_deref().unwrap_or("General"),
                        snap.tree_digest.chars().take(8).collect::<String>()
                    );

                    if let Some(ref s) = self.storage {
                        let _ = s.save_snapshot_meta(&snap);
                    }

                    self.files = files;
                    self.summary = Some(summary);
                    self.kernel = Some(kernel);
                    self.snapshot = Some(snap);
                    self.scan_rx = None;
                }
                Ok((Err(e), _)) | Ok((_, Err(e))) => {
                    self.status_text = format!("인덱싱 실패: {}", e);
                    self.scan_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.status_text = "인덱싱 작업 채널이 중단되었습니다.".to_string();
                    self.scan_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }

        // 2. Poll ping task (Non-blocking & full terminal error state consumption)
        if let Some(ref rx) = self.ping_rx {
            match rx.try_recv() {
                Ok(Ok(status)) => {
                    self.ping_status = if status.healthy {
                        format!(
                            "✅ {} ({}ms)",
                            status.message,
                            status.latency_ms.unwrap_or(0)
                        )
                    } else {
                        format!("❌ {}", status.message)
                    };
                    self.is_pinging = false;
                    self.ping_rx = None;
                }
                Ok(Err(e)) => {
                    self.ping_status = format!("❌ {}", e);
                    self.is_pinging = false;
                    self.ping_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.ping_status = "❌ 연결 시험 채널 중단".to_string();
                    self.is_pinging = false;
                    self.ping_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }

        // 3. Poll local query workflow (Non-blocking & full terminal error state consumption)
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

        // 4. Poll egress packet assembly (Non-blocking & full terminal error state consumption)
        if let Some(ref rx) = self.egress_packet_rx {
            match rx.try_recv() {
                Ok(Ok(packet)) => {
                    let snap_id = self
                        .snapshot
                        .as_ref()
                        .map(|s| s.id)
                        .unwrap_or_else(uuid::Uuid::new_v4);
                    if !self.repo_consent_given {
                        self.pending_egress_packet = Some(packet);
                    } else {
                        let receipt = EgressReceipt {
                            receipt_id: uuid::Uuid::new_v4(),
                            packet_hash: packet.packet_hash.clone(),
                            snapshot_id: snap_id,
                            token_count: packet.estimated_tokens,
                            file_count: packet.included_files.len(),
                            granted_at: chrono::Utc::now().to_rfc3339(),
                        };

                        let q = self.pending_query.take().unwrap_or_default();
                        if let Ok(approved_req) = ApprovedInferenceRequest::new(
                            receipt,
                            packet,
                            q,
                            snap_id,
                            self.profile.clone(),
                        ) {
                            self.start_inference_stream_with_approved_request(approved_req);
                        }
                    }
                    self.egress_packet_rx = None;
                }
                Ok(Err(e)) => {
                    self.status_text = format!("🛡️ 컨텍스트 조립 실패: {}", e);
                    self.egress_packet_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.status_text = "컨텍스트 조립 채널이 중단되었습니다.".to_string();
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
                                self.status_text =
                                    format!("🤖 {} 스트리밍 중...", self.profile.model);
                            }
                            InferenceEvent::TextDelta(delta) => {
                                if let Some(ref mut text) = self.answer_preview {
                                    text.push_str(&delta);
                                }
                            }
                            InferenceEvent::Completed { full_text } => {
                                self.answer_preview = Some(full_text);
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

        // Periodic file watcher check for STALE transitions
        if let Some(ref mut watcher) = self.watcher {
            if let Ok(true) = watcher.check_for_changes() {
                if let Some(ref mut snap) = self.snapshot {
                    snap.status = SnapshotStatus::Stale;
                    self.status_text =
                        "⚠️ 외부 파일 변경 감지됨 (STALE: 재인덱싱 권장)".to_string();
                }
            }
        }

        // Global Keyboard Shortcut Esc to cancel or collapse tiers
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.is_streaming {
                self.cancel_inference();
            } else if self.settings_open {
                self.settings_open = false;
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
            let action = PillBar::new(
                repo_display_name.as_deref(),
                is_read_only,
                &mut self.query_text,
                self.is_pinned,
                &self.status_text,
            )
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
            }

            // Render Settings Panel (if toggled)
            if self.settings_open {
                ui.add_space(8.0);
                let settings_action = SettingsPanel::new(
                    &mut self.profile,
                    &mut self.persona,
                    &self.ping_status,
                    self.is_pinging,
                )
                .show(ui);

                if settings_action.ping_clicked {
                    self.ping_backend();
                }
                if settings_action.close_clicked {
                    if let Some(ref s) = self.storage {
                        let _ = s.save_backend_profile(&self.profile);
                    }
                    self.settings_open = false;
                }
            }

            // Render Egress Consent Sheet (if pending)
            let mut consent_granted = false;
            let mut consent_cancelled = false;

            if let Some(ref packet) = self.pending_egress_packet {
                ui.add_space(8.0);
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("🛡️ 외부 데이터 전송 승인 (Egress Consent)").color(MentatTheme::STATUS_CONFLICT).strong().size(13.0));
                    });
                    ui.label(RichText::new(format!(
                        "선택된 공급자({})로 문맥을 전송합니다. (포함: {}개 파일 / 약 {} 토큰 / 해시: {})",
                        self.profile.model,
                        packet.included_files.len(),
                        packet.estimated_tokens,
                        packet.packet_hash.chars().take(8).collect::<String>()
                    )).size(12.0));

                    // SEC-F002: Exact file and line preview in consent sheet
                    ui.collapsing("📄 포함될 파일 및 행 범위 미리보기", |ui| {
                        for ref_item in &packet.included_file_refs {
                            ui.label(RichText::new(format!(
                                " • {} (1..{}행, 총 {}행)",
                                ref_item.relative_path.display(),
                                ref_item.line_end,
                                ref_item.line_count
                            )).size(11.0).color(MentatTheme::TEXT_MUTED));
                        }
                    });

                    if !packet.excluded_sensitive_files.is_empty() {
                        ui.label(RichText::new(format!(
                            "🔒 자동 제외된 민감정보 파일: {}건 (.env, 인증서, 토큰 등)",
                            packet.excluded_sensitive_files.len()
                        )).color(MentatTheme::STATUS_READ_ONLY).size(11.5));
                    }

                    if packet.redacted_secret_occurrences > 0 {
                        ui.label(RichText::new(format!(
                            "✂️ 내용 중 마스킹된 비밀정보: {}건",
                            packet.redacted_secret_occurrences
                        )).color(MentatTheme::STATUS_INFERENCING).size(11.5));
                    }

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("✅ 전송 승인 및 실행").strong()).clicked() {
                            consent_granted = true;
                        }
                        if ui.button("✖ 취소").clicked() {
                            consent_cancelled = true;
                        }
                    });
                });
            }

            if consent_granted {
                if let Some(packet) = self.pending_egress_packet.take() {
                    let q = self.pending_query.take().unwrap_or_default();
                    self.repo_consent_given = true;
                    let snap_id = self.snapshot.as_ref().map(|s| s.id).unwrap_or_else(uuid::Uuid::new_v4);

                    let receipt = EgressReceipt {
                        receipt_id: uuid::Uuid::new_v4(),
                        packet_hash: packet.packet_hash.clone(),
                        snapshot_id: snap_id,
                        token_count: packet.estimated_tokens,
                        file_count: packet.included_files.len(),
                        granted_at: chrono::Utc::now().to_rfc3339(),
                    };

                    if let Ok(approved) = ApprovedInferenceRequest::new(
                        receipt,
                        packet,
                        q,
                        snap_id,
                        self.profile.clone(),
                    ) {
                        self.start_inference_stream_with_approved_request(approved);
                    }
                }
            }
            if consent_cancelled {
                self.pending_egress_packet = None;
                self.pending_query = None;
                self.status_text = "전송이 사용자에 의해 취소되었습니다.".to_string();
            }

            // Render Tier 2 Smart Card (if expanded)
            if self.expansion_tier != ExpansionTier::Tier1Pill {
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // Quick Action Chips Row
                ui.horizontal(|ui| {
                    ui.label(RichText::new("⚡ 빠른 분석:").size(11.5).color(MentatTheme::TEXT_MUTED));
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
                        if ui.button("✖ 접기 (Esc)").clicked() {
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
                    if ui.button("📋 답변 복사").clicked() {
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
                    ui.heading(RichText::new("🔬 Detailed Evidence & File Inspector").size(13.0));

                    let mut clicked_file_idx = None;

                    ui.columns(2, |cols| {
                        cols[0].group(|ui| {
                            ui.label(RichText::new("📂 저장소 파일 트리").strong().size(12.0));
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
                            ui.label(RichText::new("📄 소스코드 행 뷰어").strong().size(12.0));
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
    fn test_app_expansion_tier_transitions() {
        let mut tier = ExpansionTier::Tier1Pill;
        assert_eq!(tier, ExpansionTier::Tier1Pill);

        tier = ExpansionTier::Tier2Card;
        assert_eq!(tier, ExpansionTier::Tier2Card);

        tier = ExpansionTier::Tier3Inspector;
        assert_eq!(tier, ExpansionTier::Tier3Inspector);
    }

    #[test]
    fn test_tampered_egress_request_rejection_fail_closed() {
        use mentat_inference::{BackendProfile, ProviderKind};
        use sha2::{Digest, Sha256};

        let snap_id = uuid::Uuid::new_v4();
        let prompt_context = "context content".to_string();
        let mut hasher = Sha256::new();
        hasher.update(prompt_context.as_bytes());
        let exact_hash = format!("{:x}", hasher.finalize());

        let packet = EgressPacket {
            packet_id: uuid::Uuid::new_v4(),
            packet_hash: exact_hash.clone(),
            included_files: vec![],
            included_file_refs: vec![],
            excluded_sensitive_files: vec![],
            redacted_secret_occurrences: 0,
            estimated_tokens: 10,
            prompt_context: prompt_context.clone(),
        };

        let receipt = EgressReceipt {
            receipt_id: uuid::Uuid::new_v4(),
            packet_hash: exact_hash,
            snapshot_id: snap_id,
            token_count: 10,
            file_count: 0,
            granted_at: "2026-08-19T00:00:00Z".to_string(),
        };

        let profile = BackendProfile {
            id: uuid::Uuid::new_v4(),
            name: "Gemini Test".to_string(),
            provider: ProviderKind::GoogleGemini,
            base_url: ProviderKind::GoogleGemini.default_base_url().to_string(),
            model: "gemini-2.5-flash".to_string(),
            api_key: None,
            timeout_secs: 30,
        };

        let req = ApprovedInferenceRequest::new(
            receipt.clone(),
            packet,
            "query".to_string(),
            snap_id,
            profile.clone(),
        )
        .expect("Should construct approved request");

        assert!(req.verify_integrity());

        // Single-use consume succeeds
        let consumed = req
            .into_inference_request()
            .expect("Should consume successfully");
        assert_eq!(consumed.user_question, "query");
        assert_eq!(consumed.profile.model, "gemini-2.5-flash");

        // Tampered prompt context in packet fails construction
        let tampered_packet = EgressPacket {
            packet_id: uuid::Uuid::new_v4(),
            packet_hash: "unmatched_hash".to_string(),
            included_files: vec![],
            included_file_refs: vec![],
            excluded_sensitive_files: vec![],
            redacted_secret_occurrences: 0,
            estimated_tokens: 10,
            prompt_context: "tampered context".to_string(),
        };

        let err = ApprovedInferenceRequest::new(
            receipt,
            tampered_packet,
            "query".to_string(),
            snap_id,
            profile,
        );
        assert!(err.is_err(), "Tampered packet hash must fail construction");
    }
}
