# Code Mentat Canonical Specification (spec.md)
## 코드 멘타트 활성 실행 명세서 및 마스터 요구사항 추적 매트릭스

- **문서 버전:** 0.1.0-dev (Turn 13 / Re-audit #10 Remediation)
- **패키지 버전 (Cargo workspace SemVer):** `0.1.0`
- **버전 정책:** Cargo `0.1.0`이 패키지/바이너리 버전이다. `0.1.0-dev`는 동일 0.1.0 라인의 미릴리스 문서·개발 상태 표기이며 별도 제품 버전이 아니다.
- **문서 권위:** `CODE_MENTAT_SPEC.md`는 마스터 요구사항 베이스라인(Baseline)이며, 본 `spec.md`는 baseline 요구사항의 정의, 수용 기준 및 ID를 1:1 원본 그대로 보존하고 현재 구현/검증 상태를 추적하는 활성 실행 명세서임 (DEC-ARCH-003).
- **표준 규격:** D3D Protocol v1.3 / AI Implementation Documentation Standard
- **기준 작성일:** 2026-08-18 (최종 갱신: 2026-08-19)

---

## 1. 요구사항 추적 매트릭스 (Requirements Traceability Matrix)

### 1.1 기능 요구사항 (Functional Requirements - Baseline FR-001 ~ FR-026 원본 보존)

| Baseline ID | 출처 | Baseline 요구사항 정의 (원본 그대로 보존) | 수용 기준 (Baseline Acceptance Criteria) | 담당 모듈/파일 | 구현 상태 | 검증 증거 / 회귀 테스트 |
|---|---|---|---|---|---|---|
| **FR-001** | EXPLICIT | 사용자는 로컬 디렉터리 또는 Git 저장소를 열 수 있다. | 파일 선택기에서 경로를 고르면 정규화된 루트, 저장소 유형, 접근 가능 여부가 표시된다. | `mentat-repository/src/session.rs` | Implemented | `test_read_only_session_scan_and_snapshot` |
| **FR-002** | EXPLICIT | 저장소 접근은 항상 읽기 전용이어야 한다. | 세션 전후 파일 내용 해시·디렉터리 엔트리·권한·수정시각·`git status` 등가 상태가 같고 앱 코드 경로에서 저장소 쓰기 작업이 호출되지 않는다. OS가 읽기 시 갱신할 수 있는 접근시각은 판정에서 분리한다. | `mentat-repository/src/session.rs` | Partial | `ReadOnlySession` 읽기 전용 포트 강제; 3-pass 해시/권한 불변 회귀 테스트 진행 중 |
| **FR-003** | DERIVED | 저장소 경계를 벗어나는 심볼릭 링크와 재분석 경로를 차단한다. | 루트 밖으로 해석되는 링크는 읽지 않고 `EXTERNAL_PATH_BLOCKED`로 표시한다. | `mentat-repository/src/session.rs`, `scanner.rs` | Partial | `test_external_path_blocked`, `test_sec_f006_inspect_file_canonical_safety`; OS 심볼릭 픽스처 보강 중 |
| **FR-004** | DERIVED | 저장소 안에서는 셸·프로세스·빌드·테스트를 실행하지 않는다. | UI와 코어 공개 API에 실행 명령이 없고 악성 저장소 픽스처가 프로세스를 시작하지 못한다. | `mentat-core/src/ports.rs` | Implemented | `ReadOnlySession` 실행 포트 원천 부재 |
| **FR-005** | EXPLICIT | 파일 트리와 파일 내용을 읽고 탐색할 수 있다. | 텍스트 파일을 행 번호와 함께 열고 경로/내용 검색 결과에서 해당 행으로 이동한다. | `mentat-app/src/app.rs`, `mentat-repository` | Implemented | Tier 3 Inspector Panel & `read_file_lines` (비동기 프리뷰) |
| **FR-006** | DERIVED | 제외 규칙과 자원 한도를 적용한다. | `.gitignore`, 앱 기본 제외, 사용자 제외가 병합되고 파일 크기·총 바이트·파일 수 한도 초과 항목이 이유와 함께 건너뛰어진다. | `mentat-repository/src/scanner.rs`, `mentat-app` | Partial | 앱이 `ScanOutcome` omissions와 취소 토큰을 표시; 사용자 한도 설정 UI 후속 |
| **FR-007** | EXPLICIT | 프로젝트 구조와 기술 스택을 요약한다. | 언어 분포, 주요 매니페스트, 빌드/테스트/문서 후보, 진입점 후보를 증거와 함께 보여준다. | `mentat-analysis/src/detector.rs` | Implemented | `test_detector_rust_project` |
| **FR-008** | DERIVED | 저장소의 문서·코드·구성·테스트 사이 관계를 분석한다. | 적어도 참조 경로, 파일명/심볼 언급, 매니페스트 관계를 통해 연결 그래프를 생성하고 근거를 열 수 있다. | `mentat-analysis/src/semantic_kernel.rs` | Partial | `SemanticKernelBuilder::run_local_workflow` (/where, /structure, /risks) |
| **FR-009** | EXPLICIT | 사용자는 저장소와 프로젝트에 대해 자연어로 질문할 수 있다. | 질문이 컨텍스트 계획→근거 선택→추론→구조화 답변 상태를 거치며 취소 가능하다. | `mentat-app/src/app.rs` | Partial | 클라우드 요청에 AnswerBundle JSON schema·snapshot·file hash/range를 포함; `from_model_text_with_contents` 검증 |
| **FR-010** | DERIVED | 답변은 관찰·추론·제안·충돌을 구분한다. | 모든 핵심 주장이 `OBSERVED`, `INFERRED`, `PROPOSED`, `CONFLICT` 중 하나와 신뢰도를 가진다. | `mentat-core/src/models.rs` | Partial | `ClaimClassification` & `test_imp_f004_unstructured_response_does_not_invent_claims`; 비구조 응답은 Unknown으로 강등 |
| **FR-011** | DERIVED | 주장에 저장소 증거를 연결한다. | 증거가 있는 주장은 상대 경로, 행 범위, 콘텐츠 해시, 인덱스 스냅샷 ID를 제공하고 클릭 시 파일로 이동한다. | `mentat-core/src/models.rs`, `mentat-analysis/src/evidence.rs`, `answer_bundle.rs` | Partial | `test_imp_f004_adapter_loopback_valid_and_invalid_citations`; 클릭 점프 UI 후속 |
| **FR-012** | DERIVED | 일반 권장사항과 프로젝트 의도 정렬 권장사항을 구분한다. | 추천마다 `GENERAL_PRACTICE`, `PROJECT_INTENT_ALIGNED`, `NEEDS_USER_DECISION` 중 하나가 표시된다. | `mentat-core/src/models.rs` | Implemented | `RecommendationBasis` 엔티티 |
| **FR-013** | EXPLICIT | OpenAI 호환 API 프로필을 구성할 수 있다. | base URL, 프로토콜, 모델명, 선택 헤더, 시간 제한을 저장하고 연결 시험 결과를 구조화 표시한다. 비밀은 설정 DB에 평문 저장하지 않는다. | `mentat-inference`, `mentat-inference-openai`, `mentat-app`, `mentat-storage` | Partial | base URL/model/timeout과 동적 연결 시험은 구현, protocol·선택 헤더 구성과 키체인 영속화는 미구현 |
| **FR-014** | EXPLICIT | API 응답을 스트리밍하고 사용자가 중단할 수 있다. | 첫 텍스트 조각이 도착 즉시 UI에 표시되고 취소 후 네트워크 작업과 스트림 소비가 제한 시간 내 종료된다. | `mentat-inference/src/fake.rs`, `mentat-app` | Implemented | `test_fake_inference_cancellation`, `test_fake_inference_stream_completes`, `test_pre_response_cancellation_aborts_immediately` |
| **FR-015** | DERIVED | 공급자 오류를 안정된 내부 오류로 변환한다. | 인증, 한도, 속도 제한, 네트워크, 시간 초과, 프로토콜 불일치, 서버 오류가 서로 다른 오류 코드와 복구 지침을 가진다. | `mentat-core/src/error.rs` | Partial | `MentatError::InferenceError`, `BackendError` 변환; 공급자별 복구 지침 정교화 진행 중 |
| **FR-016** | DERIVED | 외부 송신 전 범위와 민감정보를 통제한다. | 저장소별 동의가 없으면 API 요청이 전송되지 않으며 파일 목록·예상 문자량·민감정보 제외 결과를 미리 확인할 수 있다. | `mentat-analysis/src/egress.rs`, `consent.rs`, `mentat-app` | Partial | question/validation/snapshot/ref/profile canonical seal, 단일 전송, generation guard 구현; 저장소별 consent 영속 정책은 후속 |
| **FR-017** | EXPLICIT | 추론 백엔드를 교체할 수 있다. | 테스트 더블과 OpenAI 백엔드가 동일 계약 테스트를 통과하고 UI 재컴파일 외 변경 없이 프로필로 선택된다. | `mentat-inference/src/lib.rs`, `mentat-inference-openai`, `mentat-app` | Partial | 공통 `InferenceBackend`와 프로필 선택은 구현, 전체 backend 적합성 suite와 네이티브 로컬 구현은 미완료 |
| **FR-018** | EXPLICIT | 향후 네이티브 llama.cpp 백엔드를 위한 구조가 준비되어야 한다. | `NativeLlama` 기능이 비활성 상태로 명시되고 모델/컨텍스트/스트림/취소/능력 인터페이스와 테스트 더블이 존재하되 초기 패키지는 llama.cpp를 링크하지 않는다. | `mentat-inference-llama/src/contract.rs` | Implemented | `test_native_llama_contract_isolated_context_and_kv_cleanup` |
| **FR-019** | EXPLICIT | 페르소나를 선택·구성할 수 있다. | 이름, 말투, 호칭, 간결성, 응답 언어를 변경해도 동일 분석 입력의 증거·분류·위험 값이 변하지 않는다. | `mentat-persona/src/persona.rs` | Implemented | `test_persona_rendering_preserves_facts_and_evidence` |
| **FR-020** | DERIVED | 아나운서는 중요도 기반으로 조용하게 동작한다. | 중요도 0~2는 흐름을 끊지 않고, 3은 세션 피드, 4는 배너, 5만 확인 모달을 허용한다. 임계값은 설정 가능하다. | `mentat-persona/src/announcer.rs` | Partial | `test_announcement_policy_levels`; 사용자 임계값 설정 UI 진행 중 |
| **FR-021** | DERIVED | 기본 분석 워크플로를 제공한다. | `프로젝트 온보딩`, `구조 설명`, `작업 위치 안내`, `문서-구현 불일치`, `위험 및 미확정 결정` 워크플로가 사전 정의 질문과 결과 스키마를 가진다. | `mentat-analysis/src/semantic_kernel.rs` | Partial | `test_imp_f004_doc_code_language_conflict_fixture`; /conflicts가 문서 주장과 감지 언어를 비교. 스키마 위저드 후속 |
| **FR-022** | DERIVED | 저장소 변경을 감지하고 인덱스 신선도를 표시한다. | 파일 변경 후 세션이 `STALE`로 전환되고 기존 답변의 스냅샷이 유지되며 사용자가 재인덱싱을 선택할 수 있다. | `mentat-repository/src/watcher.rs` | Partial | `notify` OS event + changed-path full hash와 16KiB tail-edit 검증; 원클릭 재인덱싱 UI 후속 |
| **FR-023** | DERIVED | 세션·설정·인덱스 메타데이터를 저장소 밖에 보존한다. | 앱 데이터 경로가 저장소 내부이면 시작을 거부하고 재실행 후 최근 저장소·세션·설정이 복원된다. | `mentat-platform/src/lib.rs`, `mentat-storage`, `mentat-repository/src/session.rs` | Partial | canonical root 기준 안정 repo ID (`test_imp_f005_stable_repo_id_and_snapshot_restore`); 동일 digest면 이전 snapshot ID 재사용. 전체 세션 대화 복원은 후속 |
| **FR-024** | EXPLICIT | UI와 조언 응답 언어를 별도로 설정할 수 있다. | 메뉴 언어와 답변 언어를 독립 변경하고 재시작 후 유지한다. | `mentat-persona/src/persona.rs` | Partial | `PersonaRenderer` 언어 파라미터 적용; UI 다국어 로케일 진행 중 |
| **FR-025** | DERIVED | 답변과 보고서를 저장소를 수정하지 않고 내보낼 수 있다. | 클립보드 복사와 저장소 밖 사용자 선택 경로 저장이 가능하며 저장소 내부 경로는 거부된다. | `mentat-app/src/app.rs`, `mentat-platform` | Partial | `PlatformManager::copy_to_clipboard`; 외부 보고서 파일 내보내기 진행 중 |
| **FR-026** | EXPLICIT | Windows, Linux, macOS용 단일 데스크톱 앱으로 패키징한다. | 각 플랫폼 산출물이 별도 서버 없이 실행되고 동일한 핵심 수용 시나리오를 통과한다. | `mentat-app/src/main.rs` | Partial | Windows x86_64 릴리스 빌드 검증; Linux/macOS CI 매트릭스 구성 완료 |

---

### 1.2 비기능 요구사항 (Non-Functional Requirements - Baseline NFR-001 ~ NFR-013 원본 보존)

| Baseline ID | Baseline 요구사항 정의 (원본 그대로 보존) | 수용 기준 (Baseline Acceptance Criteria) | 구현 상태 | 검증 증거 및 비고 |
|---|---|---|---|---|
| **NFR-001** | 권한 최소화 | 저장소 모듈은 읽기 인터페이스만 공개하고 쓰기·실행 능력을 주입받지 않는다. | Implemented | `test_read_only_session_scan_and_snapshot` |
| **NFR-002** | UI 반응성 | 인덱싱·검색·네트워크·추론이 UI 스레드를 막지 않으며 사용자 입력 p95 처리 지연이 100ms 이하이다. | Partial | 비동기 논블로킹 채널 폴링(`try_recv`), 워처는 UI 스레드 밖 백그라운드 walk (`test_dbg_f008_background_watcher_poll_is_nonblocking`) |
| **NFR-003** | 대형 저장소 대응 | 기본 벤치 저장소(100,000파일, 텍스트 2GiB, 제외 디렉터리 포함)에서 메모리 상한과 취소 가능성을 측정하고 전체 내용을 메모리에 동시에 보관하지 않는다. 구체 상한은 P2에서 기준 장비와 함께 기록한다. | Partial | 기준 Windows 장비의 peak working set 상한 128MiB, global preview 8MiB, Incomplete gate를 ignored 100k/2GiB profile에서 강제; 다른 OS 기준은 후속 |
| **NFR-004** | 증거 추적성 | 답변 스냅샷의 모든 증거 참조는 원본 행 또는 `STALE/CHANGED` 상태로 해석 가능하다. | Partial | cloud citation은 snapshot/hash/excerpt/range 검증; STALE 재해석 및 클릭 점프는 후속 |
| **NFR-005** | 프라이버시 | 외부 API 전송은 명시 동의·송신 범위 표시·민감정보 필터를 통과해야 하며 로그에 API 키와 원문 코드가 기본 기록되지 않는다. | Partial | 패턴+고엔트로피 마스킹, relevance threshold, consent generation 가드. 신규 provider 전용 포맷은 엔트로피 경로로 처리 |
| **NFR-006** | 백엔드 격리 | 구체 API/모델 객체는 추론 어댑터 밖으로 노출되지 않고 백엔드 실패가 저장소 세션을 손상시키지 않는다. | Implemented | `InferenceBackend` trait 경계 |
| **NFR-007** | 요청 격리 | 각 추론 요청은 독립 취소 토큰·시간 제한·컨텍스트를 가지며 공유 가능한 것은 읽기 전용 모델 가중치와 불변 설정뿐이다. | Implemented | `test_fake_inference_cancellation`, `test_pre_response_cancellation_aborts_immediately` |
| **NFR-008** | 시간 제한 | 네트워크 및 미래 네이티브 추론 요청은 기본 제한 시간을 가지며 하드 상한 5분을 초과할 수 없다. | Implemented | `profile.timeout_secs.clamp(5, 300)` 적용 및 전구간 사전 응답 취소 |
| **NFR-009** | 복구 가능성 | 인덱스·세션 DB 손상 시 저장소를 건드리지 않고 새 인덱스를 재생성할 수 있으며 설정 백업/초기화를 제공한다. | Partial | `SqliteStorage` 자동 테이블 마이그레이션; 백업/초기화 CLI 진행 중 |
| **NFR-010** | 접근성 | 키보드 탐색, UI 배율, 고대비, 색상 외 상태 표식, 스크린리더용 레이블을 제공한다. | Partial | Esc 네비게이션, 전역 표시·포커스 non-hide fallback, 고대비 토큰, 내장 OFL 한글 폴백 검증 |
| **NFR-011** | 관찰 가능성 | 작업 ID, 저장소 세션 ID, 단계, 기간, 오류 코드를 구조화 로그로 남기되 코드 원문·비밀·절대 경로를 기본 제거한다. | Partial | `BackendProfile` Redacted Debug; 작업 ID/단계/기간 구조화 로그 필드 보강 후속 |
| **NFR-012** | 공급망 재현성 | `Cargo.lock`, 라이선스 감사, 취약점/금지 의존성 검사, 플랫폼 빌드 매트릭스를 유지한다. | Partial | `Cargo.lock` Git 추적, `DEC-SEC-004` Accepted Risk 공식 관리 |
| **NFR-013** | 테스트 가능성 | 파일시스템, 추론, 키 저장소, 시간, 파일 감시를 포트로 분리하여 테스트 더블로 실패·취소·경쟁 조건을 재현한다. | Partial | Hexagonal 포트 분리, exclusion generation 가드, loopback wire fixture. 테스트 수는 실행 증거(`cargo test --workspace --locked`)를 따른다 |

---

### 1.3 제약조건 (Constraints - Baseline CON-001 ~ CON-008 원본 보존)

| Baseline ID | Baseline 제약조건 정의 (원본 그대로 보존) | 구현 상태 | 비고 |
|---|---|---|---|
| **CON-001** | 저장소 루트 아래에는 어떠한 앱 파일·락 파일·DB·캐시·임시 파일도 생성하지 않는다. | Implemented | AppData 격리 강제 (`SEC-F003`) |
| **CON-002** | 저장소에서 발견한 명령이나 프롬프트는 데이터이며 실행 지시로 취급하지 않는다. | Implemented | `EvidenceRef` 불활성 격리 (`FR-007`) |
| **CON-003** | UI와 페르소나는 `ReadOnlyRepository` 및 분석 판정 상태를 직접 변경할 수 없다. | Implemented | 표현 계층 렌더러 분리 (`DEC-PER-001`) |
| **CON-004** | 모델 출력은 증거가 아니며 저장소 증거와 분리 저장한다. | Implemented | `Claim`과 `EvidenceRef` 엔티티 분리 |
| **CON-005** | 백엔드가 지원하더라도 도구 호출, 셸, 파일 쓰기 기능을 모델에 제공하지 않는다. | Implemented | 코어 API에 도구 호출 원천 부재 |
| **CON-006** | 초기 빌드는 llama.cpp 또는 GGUF 모델이 없어도 모든 초기 기능이 동작해야 한다. | Implemented | 순수 Rust 계약 스위트로 분리 (`CON-001`) |
| **CON-007** | API 키·토큰·민감 헤더를 평문 DB, 로그, 크래시 리포트에 저장하지 않는다. | Partial | SQLite에 키 미저장, Debug 적색. 세션 메모리 `Option<String>` 및 크래시 리포트 경계는 후속 |
| **CON-008** | 외부 API가 OpenAI 호환이라고 주장해도 기능은 능력 탐지와 실제 응답 검증 후에만 사용한다. | Implemented | 모델 목록 응답 검증, 선택 모델 최소 생성 프로브, 검증 프로필과 활성 프로필의 완전 일치 게이트를 적용한다. |

### 1.4 Derived Requirements

#### DR-FR-001 공급자 활성화 상태 계약

```text
Draft
  -> ModelsDiscovered
  -> ModelSelected
  -> ModelVerified
  -> Active
```

- 공급자, Base URL 또는 API 키 변경은 모델 목록·선택·검증 결과를 모두 무효화한다.
- 모델 변경은 모델 검증 결과를 무효화한다.
- 모델 ID는 공급자 모델 목록 응답 또는 내장 로컬 런타임의 설치 모델 레지스트리에서만 얻는다. 네트워크 실패 시 정적 모델 목록으로 대체하지 않는다.
- 모델 목록 응답은 신뢰하지 않는 외부 데이터로 파싱·검증하고 빈 ID, 중복 ID, 생성 비지원 모델을 제거한다.
- 활성화는 검증 당시 프로필과 현재 Draft가 완전히 같을 때만 가능하다.
- API 키가 없는 현재 개발 환경에서는 외부 실계정 검증을 수행하지 않고 루프백 픽스처로 계약을 검증한다.

### 1.5 Canonical Egress Seal 보강 요구사항

- 승인 봉인은 `prompt_context`뿐 아니라 redacted question, snapshot ID, 포함/제외 파일, 행 범위, redaction/token 계수, citation validation text digest와 활성 provider endpoint/model identity를 포함한다.
- canonical field는 길이-prefix encoding과 정렬된 path/map 순서를 사용해 연결 모호성과 `HashMap` 순서 비결정성을 제거한다.
- packet 생성, receipt 발급, 승인 요청 생성, consume-time 검증은 동일 digest 함수를 공유한다.
- question, validation map, snapshot/ref, endpoint/model tamper matrix 중 하나라도 변경되면 외부 전송 전에 `EgressViolation`으로 차단한다.

### 1.6 Incomplete Snapshot 및 Watcher 보강 요구사항

- 취소 또는 scan 한도 omission이 있는 결과는 `SnapshotStatus::Incomplete`이며 DB 저장과 로컬/클라우드 분석에 사용할 수 없다.
- 새 저장소를 열기 전에 이전 scan token을 취소하고 receiver를 폐기한다.
- text preview의 scan 전체 메모리 상한은 8MiB이며, 초과 파일은 preview를 지연 로드한다.
- OS watcher event를 primary로 사용하고 scanner와 동일 ignore 범위를 적용한다. Access-only는 제외하지만 `need_rescan`, Any, Other, `.gitignore`, `.git/info/exclude` 변경은 전체 event loss/scope 변경으로 간주해 즉시 STALE로 전환한다.
- 신규 로컬/클라우드 분석은 `Ready` snapshot에서만 허용한다. `Stale`은 기존 결과 열람만 가능하다.
- Egress live read의 SHA-256이 scan 시점 `FileRecord.content_hash`와 다르면 snapshot lineage 위반으로 packet assembly를 중단한다.

### 1.7 Verified Answer Projection 보강 요구사항

- cloud `direct_answer`는 신뢰 입력이며 UI 주요 본문에 직접 표시하지 않는다.
- citation/confidence invariant를 통과하고 `Unknown`이 아닌 claim만 canonical answer 합성에 참여한다.
- Cloud `Observed`/`Inferred`/`Proposed`/`Conflict` claim은 모두 최소 1개의 유효 evidence를 요구한다.
- Cloud `ConflictItem`은 evidence가 비어 있거나 missing/invalid/duplicate이면 검증된 conflicts와 `[CONFLICT]` UI에서 제거한다.
- 검증 가능한 claim이 없으면 고정된 “검증된 근거 기반 답변 없음” 상태를 표시하고 모델 원문은 `raw_model_response`로만 보존한다.

### 1.8 고대비 스위스 UI 및 종료 수명주기 요구사항

- 프레임리스 창은 Tier 1 우측 끝에 항상 보이는 `종료 ×` 버튼을 제공한다. 설정 패널의 `패널 닫기`와 프로그램 종료를 용어와 동작으로 구분한다.
- `종료 ×` 또는 `Ctrl+Q`를 받으면 추론·인덱싱 취소 토큰을 먼저 취소하고 비동기 수신 채널과 동의 조립 상태를 폐기한 뒤 `ViewportCommand::Close`를 요청한다. 전역 단축키와 watcher는 소유 객체의 Drop 수명주기로 해제한다.
- 창은 투명 합성을 사용하지 않는 불투명 흰색 표면이어야 한다. 기본/카드/입력 배경은 각각 `#FFFFFF`, `#F5F5F2`, `#FFFFFF`이고 기본/보조 글자는 `#111111`, `#525252`이다.
- 모든 egui widget state(`noninteractive`, `inactive`, `hovered`, `active`, `open`)의 foreground stroke를 명시해 운영체제나 기본 dark visual에 따라 글자가 사라지지 않게 한다.
- 정상 크기 텍스트는 흰색 배경에서 WCAG AA 대비율 4.5:1 이상이어야 하며, 상태는 색상만으로 전달하지 않고 `R/O`, `상태:`, `오류:` 등의 텍스트를 함께 표시한다.
- Tier 1/2/3/설정 뷰포트는 각각 `760x56`, `760x360`, `900x620`, `760x480`으로 고정한다. Tier 1 입력창은 최소 200pt를 확보하고 좁아지면 `/onboard` 보조 칩부터 숨긴다.
- Tier 1의 `고정`·`설정`·`종료 ×`는 우측 180pt trailing 영역에 먼저 배치해 동적 문자열이 침범할 수 없게 한다. 저장소 버튼은 최대 120pt이며 초과하는 ASCII/CJK 이름은 한 줄 ellipsis로 줄이고 전체 이름을 tooltip으로 제공한다.
- 최소 창 폭 640px과 기본 폭 760px에서 긴 ASCII/CJK 저장소명을 사용해도 질문 입력 폭은 200pt 이상이고 trailing 세 버튼의 hit rect는 viewport 안에 있어야 한다.
