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
        Note over Analysis: 프롬프트 바이트 직접 재해시 + 모든 메타데이터 봉인
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
| `mentat-repository` | 읽기 전용 저장소 엔진 | `ReadOnlySession` (`ScanOutcome` 취소/omission, metadata preflight), `RepositoryWatcher` (생성자 non-walk) | `test_dbg_f003_mid_scan_cancel`, `test_dbg_f003_giant_file_omitted_without_full_hash`, `test_dbg_f008_constructor_does_not_walk_tree` |
| `mentat-storage` | AppData SQLite 영속화 | `SqliteStorage`, 최근 저장소, 프로필, 스냅샷, canonical root 조회 | `test_sqlite_storage_save_and_list_recent_repos`, `test_imp_f005_find_repo_by_canonical_root` |
| `mentat-analysis` | 정적 분석 및 유출 통제 | `AnswerBundleNormalizer` (snapshot/hash/excerpt/range), `/conflicts` 문서-코드 비교, 고엔트로피/relevance egress | `test_imp_f004_wrong_snapshot_hash_excerpt_range_and_mixed_evidence`, `test_imp_f004_doc_code_language_conflict_fixture`, `test_sec_f002_high_entropy_redaction` |
| `mentat-inference` | 추론 도메인 인터페이스 | `InferenceBackend` trait, `BackendProfile` Redacted Debug & parsed URL loopback validation, `FakeInferenceBackend` | `test_sec_f004_redacted_debug_formatting`, `test_sec_f004_parsed_url_loopback_validation`, `test_fake_inference_cancellation`, `test_fake_inference_stream_completes` |
| `mentat-inference-openai` | 멀티 프로바이더 스트리밍 | Gemini/OpenAI SSE, 사전 응답 취소, 바이트 버퍼 SSE, loopback wire fixture | `test_openai_wire_http_error_codes`, `test_openai_wire_cancel_during_send`, `test_openai_wire_split_sse_chunks` |
| `mentat-inference-llama` | 미래 온디바이스 계약 | `NativeLlamaContract`, 하드웨어 탐지 스텁 | `test_native_llama_contract_isolated_context_and_kv_cleanup` 등 3개 |
| `mentat-persona` | 페르소나 및 아나운서 | 3가지 페르소나 프리셋, 사실 보존 렌더러, 중요도 기반 알림 정책 | `test_persona_rendering_preserves_facts_and_evidence`, `test_announcement_policy_levels` |
| `mentat-app` | eframe 데스크톱 UI | Consent generation 가드, 복원된 snapshot을 Indexing/STALE 상태에 연결, 백그라운드 워처 폴링, 검증된 AnswerBundle 렌더 | `test_tampered_egress_request_rejection_fail_closed` |

---

## 3. 품질 게이트 검증 결과 (Verification Results)

- **단위/회귀 테스트:** `cargo test --workspace --locked` 실행 증거 (65 passed, 1 ignored 100k profile)
- **포맷팅 검사:** `cargo fmt --all -- --check` (0 diffs)
- **정적 분석:** `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` (0 errors, 0 warnings)
- **릴리스 바이너리 빌드:** `cargo build --release -p mentat-app` (완료)
- **의존성 보안 감사:** `cargo audit --file Cargo.lock` (`DEC-SEC-004` Accepted Risk 공식 관리 및 명문화)
