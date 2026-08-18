# Code Mentat Implementation Summary (IMPLEMENTATION_SUMMARY.md)
## 코드 멘타트 구현 요약서

- **문서 버전:** 0.1.0-dev (Turn 13 / Re-audit #10 Remediation)
- **패키지 버전:** Cargo workspace `0.1.0` (`0.1.0-dev`는 미릴리스 문서 상태)
- **표준 규격:** AI Implementation Documentation Standard Section 6
- **기준 작성일:** 2026-08-18 (최종 수정: 2026-08-19)

---

## 1. 전체 런타임 흐름 (Runtime Execution Flow)

```mermaid
sequenceDiagram
    autonumber
    actor Dev as Developer
    participant App as mentat-app (UI Widget)
    participant Platform as mentat-platform (Isolation Guard)
    participant Repo as mentat-repository (Read-Only)
    participant Analysis as mentat-analysis (Evidence & Egress)
    participant Storage as mentat-storage (SQLite in AppData)
    participant Infer as mentat-inference-openai (Gemini/OpenRouter)

    Dev->>App: 앱 실행 (Pill Bar 표시)
    App->>Storage: 최근 저장소 및 저장된 프로필 로드 (load_backend_profile)
    Storage-->>App: Recent Repos & BackendProfile

    Dev->>App: 저장소 폴더 선택 (또는 최근 저장소 클릭)
    App->>Platform: validate_storage_isolation(app_data_dir, repo_root)
    alt 격리 위반 (저장소와 AppData 상호 포함)
        Platform-->>App: Err(StorageIsolationViolation)
        App->>Dev: [저장소 격리 위반 차단] 알림 표시 및 세션 오픈 거부
    else 격리 확인 성공
        App->>Repo: ReadOnlySession::open(path)
        Repo->>Repo: Single-Scan 파일 스캔 및 결정적 정렬 스냅샷 다이제스트 생성 (100k/2GiB 예산 통제)
        Repo-->>App: FileRecords & RepositorySnapshot
        App->>Storage: save_recent_repo(profile) & save_snapshot_meta(snapshot)
    end

    Dev->>App: 질문 입력 ("프로젝트 구조 분석")
    alt 로컬 슬래시 명령 (/onboard, /structure, /conflicts, /where, /risks)
        App->>Analysis: run_local_workflow(query) (논블로킹 비동기)
        Analysis-->>App: AnswerBundle (Direct Answer + Claims with EvidenceRef + Evidence)
    else 클라우드 AI 질의 (Gemini / OpenRouter)
        App->>Analysis: ConsentAssemblyState.begin_assembly + EgressFilter::assemble_packet_with_user_exclusions
        Analysis-->>App: (generation, EgressPacket)
        App->>Dev: Egress Consent Sheet (재조립 중이면 승인 비활성)
        Note over App: 제외 토글 시 old packet 즉시 None, generation 증가, stale 결과 폐기
        Dev->>App: [✅ 전송 승인 및 실행] (can_approve일 때만)
        App->>Analysis: ApprovedInferenceRequest::new(receipt, packet, query, snapshot, profile)
        Note over Analysis: question/validation/snapshot/ref/profile canonical seal 재검증
        Analysis-->>App: approved_req (Private Fields)
        App->>Infer: infer_stream(approved_req.into_inference_request(), cancel_token)
        Infer-->>App: SSE Stream (TextDelta, Started, Completed, Cancelled)
        App->>Analysis: AnswerBundleNormalizer::from_model_text (snapshot/hash/excerpt/range)
    end
    App->>Dev: 답변 렌더링 (관찰/추론/제안/충돌 태그 + 소스코드 증거 링크)
```

---

## 2. 모듈별 구현 현황 (Hexagonal Architecture)

| Crate | 역할 | 주요 구현 내용 | 검증 테스트 |
|---|---|---|---|
| `mentat-core` | 도메인 모델 & 포트 | `Claim`, `EvidenceRef`, `RepositoryProfile`, `RepositorySnapshot`, `MentatError` 정의 | - |
| `mentat-platform` | OS 플랫폼 & 격리 가드 | 폴더 선택기, 클립보드 복사, `validate_storage_isolation` (AppData 상호 침범 방지) | `test_storage_isolation_detection` |
| `mentat-repository` | 읽기 전용 저장소 엔진 | Incomplete snapshot, 8MiB preview budget, repository switch cancel, OS event watcher + changed-path full hash | `test_dbg_f003_mid_scan_cancel`, `test_dbg_f003_preview_memory_has_a_global_budget`, `test_dbg_f002_preserved_mtime_same_size_content_change` |
| `mentat-storage` | AppData SQLite 영속화 | `SqliteStorage`, 최근 저장소, 프로필, 스냅샷, canonical root 조회 | `test_sqlite_storage_save_and_list_recent_repos`, `test_imp_f005_find_repo_by_canonical_root` |
| `mentat-analysis` | 정적 분석 및 유출 통제 | canonical egress seal, claim evidence/confidence invariant, AnswerBundle citation 검증, 고엔트로피/relevance egress | `test_approved_inference_request_binding_integrity_and_consume_once`, `test_imp_f004_claim_invariants_reject_empty_duplicate_and_invalid_confidence` |
| `mentat-inference` | 추론 도메인 인터페이스 | 동적 `ModelCatalog`, `ModelVerification`, 하드코딩 없는 `BackendProfile`, URL 검증, 테스트 더블 | `production_profile_does_not_choose_a_hardcoded_model`, `model_catalog_rejects_empty_ids_and_deduplicates_provider_data` |
| `mentat-inference-openai` | 멀티 프로바이더 스트리밍 | Gemini/OpenAI 동적 모델 검색, 최소 생성 프로브, redirect key 차단, SSE | `gemini_cross_origin_redirect_never_receives_api_key`, `openai_model_discovery_uses_provider_response_without_presets` |
| `mentat-inference-llama` | 미래 온디바이스 계약 | `NativeLlamaContract`, 하드웨어 탐지 스텁 | `test_native_llama_contract_isolated_context_and_kv_cleanup` 등 3개 |
| `mentat-persona` | 페르소나 및 아나운서 | 이모지 글꼴에 의존하지 않는 3가지 페르소나 표시명, 사실 보존 렌더러, 중요도 기반 알림 정책 | `test_persona_rendering_preserves_facts_and_evidence`, `test_announcement_policy_levels` |
| `mentat-app` | eframe 데스크톱 UI | Draft/Verified/Active 공급자 상태, incomplete query gate, global hotkey 표시·포커스와 non-hide fallback | `incomplete_or_indexing_snapshot_cannot_enter_analysis`, `failed_or_colliding_registration_never_hides_the_window` |

### UI 복구 실행 증거 (2026-08-19)

- 수정 전 실제 실행: 580×52 Tier 1에서 한국어·이모지가 사각형으로 표시되고, 설정 및 저장소 미선택 질문은 창 높이를 늘리지 않아 보조 UI가 보이지 않았다.
- 수정 후 실제 실행: 초기 한글 레이블이 정상 표시되고, 설정과 질문 카드는 580×300, Detailed Inspector는 660×480으로 확장됐다.
- 폰트 자산: `crates/mentat-app/assets/fonts/NanumGothic-Regular.ttf`와 OFL 1.1 라이선스를 앱 바이너리에 포함한다.

---

## 3. 품질 게이트 검증 결과 (Verification Results)

- **단위/회귀 테스트:** `cargo test --workspace --locked` 실행 증거 (88 passed, 1 ignored 100k/2GiB profile)
- **100k/2GiB profile:** 100,000 files / 2,147,483,648 bytes / preview 3,358,720 bytes / scan 106,477ms / Windows peak working set 46,096,384 bytes (별도 ignored gate PASS)
- **포맷팅 검사:** `cargo fmt --all -- --check` (0 diffs)
- **정적 분석:** `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` (0 errors, 0 warnings)
- **릴리스 바이너리 빌드:** `cargo build --release -p mentat-app` (완료)
- **Windows global shortcut runtime:** 다른 앱 포커스에서 `Ctrl+Alt+M` 전역 event 수신과 Code Mentat 표시 유지/Tier 1 non-hide fallback 확인
- **Baseline 원문 대조:** `CODE_MENTAT_SPEC.md`와 `spec.md`의 FR/NFR/CON 47개 정의·수용 기준, mismatch 0
- **의존성 보안 감사:** `cargo audit --file Cargo.lock` FAIL — `quick-xml 0.30.0` High 2건(`RUSTSEC-2026-0194`, `RUSTSEC-2026-0195`)과 unmaintained 2건. High 2건은 `DEC-SEC-004`의 Windows 비도달 Accepted Risk이며 audit 실패를 PASS로 표기하지 않음
