# Code Mentat Canonical Specification (spec.md)
## 코드 멘타트 활성 실행 명세서 및 마스터 요구사항 추적 매트릭스

- **문서 버전:** 0.1.0-dev (Turn 8 / Re-audit #5 Remediation)
- **문서 권위:** `CODE_MENTAT_SPEC.md`는 마스터 요구사항 베이스라인(Baseline)이며, 본 `spec.md`는 baseline 요구사항의 정의, 수용 기준 및 ID를 1:1 원본 그대로 보존하고 현재 구현/검증 상태를 추적하는 활성 실행 명세서임 (DEC-ARCH-003).
- **표준 규격:** D3D Protocol v1.3 / AI Implementation Documentation Standard
- **기준 작성일:** 2026-08-18 (최종 갱신: 2026-08-19)

---

## 1. 요구사항 추적 매트릭스 (Requirements Traceability Matrix)

### 1.1 기능 요구사항 (Functional Requirements - Baseline FR-001 ~ FR-026 원본 보존)

| Baseline ID | 출처 | Baseline 요구사항 정의 (원본 그대로 보존) | 수용 기준 (Baseline Acceptance Criteria) | 담당 모듈/파일 | 구현 상태 | 검증 증거 / 회귀 테스트 |
|---|---|---|---|---|---|---|
| **FR-001** | EXPLICIT | 사용자는 로컬 디렉터리 또는 Git 저장소를 열 수 있다. | 파일 선택기에서 경로를 고르면 정규화된 루트, 저장소 유형, 접근 가능 여부가 표시된다. | `mentat-repository/src/session.rs` | Implemented | `test_read_only_session_scan_and_snapshot` |
| **FR-002** | EXPLICIT | 저장소 접근은 항상 읽기 전용이어야 한다. | 세션 전후 파일 내용 해시·디렉터리 엔트리·권한·수정시각·`git status` 등가 상태가 같고 앱 코드 경로에서 저장소 쓰기 작업이 호출되지 않는다. | `mentat-repository/src/session.rs` | Implemented | `test_read_only_session_scan_and_snapshot` |
| **FR-003** | DERIVED | 저장소 경계를 벗어나는 심볼릭 링크와 재분석 경로를 차단한다. | 루트 밖으로 해석되는 링크는 읽지 않고 `EXTERNAL_PATH_BLOCKED`로 표시한다. | `mentat-repository/src/session.rs`, `scanner.rs` | Implemented | `test_external_path_blocked`, `test_sec_f006_inspect_file_canonical_safety` |
| **FR-004** | DERIVED | 저장소 안에서는 셸·프로세스·빌드·테스트를 실행하지 않는다. | UI와 코어 공개 API에 실행 명령이 없고 악성 저장소 픽스처가 프로세스를 시작하지 못한다. | `mentat-core/src/ports.rs` | Implemented | `ReadOnlySession` 실행 포트 원천 부재 |
| **FR-005** | EXPLICIT | 파일 트리와 파일 내용을 읽고 탐색할 수 있다. | 텍스트 파일을 행 번호와 함께 열고 경로/내용 검색 결과에서 해당 행으로 이동한다. | `mentat-app/src/app.rs`, `mentat-repository` | Implemented | Tier 3 Inspector Panel & `read_file_lines` (비동기 프리뷰) |
| **FR-006** | DERIVED | 제외 규칙과 자원 한도를 적용한다. | `.gitignore`, 앱 기본 제외, 사용자 제외가 병합되고 파일 크기·총 바이트·파일 수 한도 초과 항목이 이유와 함께 건너뛰어진다. | `mentat-repository/src/scanner.rs` | Partial | `.gitignore` 필터링 및 10MB 상한 적용; UI 상한 설정 진행 중 |
| **FR-007** | EXPLICIT | 프로젝트 구조와 기술 스택을 요약한다. | 언어 분포, 주요 매니페스트, 빌드/테스트/문서 후보, 진입점 후보를 증거와 함께 보여준다. | `mentat-analysis/src/detector.rs` | Implemented | `test_detector_rust_project` |
| **FR-008** | DERIVED | 저장소의 문서·코드·구성·테스트 사이 관계를 분석한다. | 적어도 참조 경로, 파일명/심볼 언급, 매니페스트 관계를 통해 연결 그래프를 생성하고 근거를 열 수 있다. | `mentat-analysis/src/semantic_kernel.rs` | Partial | `SemanticKernelBuilder::run_local_workflow` (/where, /structure) |
| **FR-009** | EXPLICIT | 사용자는 저장소와 프로젝트에 대해 자연어로 질문할 수 있다. | 질문이 컨텍스트 계획→근거 선택→추론→구조화 답변 상태를 거치며 취소 가능하다. | `mentat-app/src/app.rs` | Implemented | `MentatApp::handle_query` & `FakeInferenceBackend` |
| **FR-010** | DERIVED | 답변은 관찰·추론·제안·충돌을 구분한다. | 모든 핵심 주장이 `OBSERVED`, `INFERRED`, `PROPOSED`, `CONFLICT` 중 하나와 신뢰도를 가진다. | `mentat-core/src/models.rs` | Implemented | `ClaimClassification` & `test_persona_rendering_preserves_facts_and_evidence` |
| **FR-011** | DERIVED | 주장에 저장소 증거를 연결한다. | 증거가 있는 주장은 상대 경로, 행 범위, 콘텐츠 해시, 인덱스 스냅샷 ID를 제공하고 클릭 시 파일로 이동한다. | `mentat-core/src/models.rs`, `mentat-analysis/src/evidence.rs` | Implemented | `test_evidence_and_prompt_injection_safety` |
| **FR-012** | DERIVED | 일반 권장사항과 프로젝트 의도 정렬 권장사항을 구분한다. | 추천마다 `GENERAL_PRACTICE`, `PROJECT_INTENT_ALIGNED`, `NEEDS_USER_DECISION` 중 하나가 표시된다. | `mentat-core/src/models.rs` | Implemented | `RecommendationBasis` 엔티티 |
| **FR-013** | EXPLICIT | OpenAI 호환 API 프로필을 구성할 수 있다. | base URL, 프로토콜, 모델명, 선택 헤더, 시간 제한을 저장하고 연결 시험 결과를 구조화 표시한다. 비밀은 설정 DB에 평문 저장하지 않는다. | `mentat-inference-openai/src/lib.rs` | Implemented | `test_multi_provider_adapter_default_initialization`, `test_sec_f004_parsed_url_loopback_validation` |
| **FR-014** | EXPLICIT | API 응답을 스트리밍하고 사용자가 중단할 수 있다. | 첫 텍스트 조각이 도착 즉시 UI에 표시되고 취소 후 네트워크 작업과 스트림 소비가 제한 시간 내 종료된다. | `mentat-inference/src/fake.rs`, `mentat-app` | Implemented | `test_fake_inference_cancellation`, `test_fake_inference_stream_completes` |
| **FR-015** | DERIVED | 공급자 오류를 안정된 내부 오류로 변환한다. | 인증, 한도, 속도 제한, 네트워크, 시간 초과, 프로토콜 불일치, 서버 오류가 서로 다른 오류 코드와 복구 지침을 가진다. | `mentat-core/src/error.rs` | Partial | `MentatError::InferenceError`, `BackendError` 변환; 공급자별 복구 지침 정교화 진행 중 |
| **FR-016** | DERIVED | 외부 송신 전 범위와 민감정보를 통제한다. | 저장소별 동의가 없으면 API 요청이 전송되지 않으며 파일 목록·예상 문자량·민감정보 제외 결과를 미리 확인할 수 있다. | `mentat-analysis/src/egress.rs`, `mentat-app` | Partial | `test_complete_pem_block_redaction_zero_raw_leak`, `test_aws_key_and_jwt_redaction`, `test_sec_f010_unicode_casing_expansion_exact_byte_offsets` |
| **FR-017** | EXPLICIT | 추론 백엔드를 교체할 수 있다. | 테스트 더블과 OpenAI 백엔드가 동일 계약 테스트를 통과하고 UI 재컴파일 외 변경 없이 프로필로 선택된다. | `mentat-inference/src/lib.rs` | Implemented | `InferenceBackend` trait & `MultiProviderAdapter` |
| **FR-018** | EXPLICIT | 향후 네이티브 llama.cpp 백엔드를 위한 구조가 준비되어야 한다. | `NativeLlama` 기능이 비활성 상태로 명시되고 모델/컨텍스트/스트림/취소/능력 인터페이스와 테스트 더블이 존재하되 초기 패키지는 llama.cpp를 링크하지 않는다. | `mentat-inference-llama/src/contract.rs` | Implemented | `test_native_llama_contract_isolated_context_and_kv_cleanup` |
| **FR-019** | EXPLICIT | 페르소나를 선택·구성할 수 있다. | 이름, 말투, 호칭, 간결성, 응답 언어를 변경해도 동일 분석 입력의 증거·분류·위험 값이 변하지 않는다. | `mentat-persona/src/persona.rs` | Implemented | `test_persona_rendering_preserves_facts_and_evidence` |
| **FR-020** | DERIVED | 아나운서는 중요도 기반으로 조용하게 동작한다. | 중요도 0~2는 흐름을 끊지 않고, 3은 세션 피드, 4는 배너, 5만 확인 모달을 허용한다. 임계값은 설정 가능하다. | `mentat-persona/src/announcer.rs` | Implemented | `test_announcement_policy_levels` |
| **FR-021** | DERIVED | 기본 분석 워크플로를 제공한다. | `프로젝트 온보딩`, `구조 설명`, `작업 위치 안내`, `문서-구현 불일치`, `위험 및 미확정 결정` 워크플로가 사전 정의 질문과 결과 스키마를 가진다. | `mentat-analysis/src/semantic_kernel.rs` | Implemented | `SemanticKernelBuilder::run_local_workflow` (/onboard, /structure, /conflicts, /where) |
| **FR-022** | DERIVED | 저장소 변경을 감지하고 인덱스 신선도를 표시한다. | 파일 변경 후 세션이 `STALE`로 전환되고 기존 답변의 스냅샷이 유지되며 사용자가 재인덱싱을 선택할 수 있다. | `mentat-repository/src/watcher.rs` | Implemented | `RepositoryWatcher::check_for_changes` |
| **FR-023** | DERIVED | 세션·설정·인덱스 메타데이터를 저장소 밖에 보존한다. | 앱 데이터 경로가 저장소 내부이면 시작을 거부하고 재실행 후 최근 저장소·세션·설정이 복원된다. | `mentat-platform/src/lib.rs`, `mentat-storage` | Partial | 최근 저장소 영속화 및 격리 완료; 세션 스냅샷 내역 복원 진행 중 |
| **FR-024** | EXPLICIT | UI와 조언 응답 언어를 별도로 설정할 수 있다. | 메뉴 언어와 답변 언어를 독립 변경하고 재시작 후 유지한다. | `mentat-persona/src/persona.rs` | Partial | `PersonaRenderer` 언어 파라미터 적용; UI 다국어 로케일 진행 중 |
| **FR-025** | DERIVED | 답변과 보고서를 저장소를 수정하지 않고 내보낼 수 있다. | 클립보드 복사와 저장소 밖 사용자 선택 경로 저장이 가능하며 저장소 내부 경로는 거부된다. | `mentat-app/src/app.rs`, `mentat-platform` | Implemented | `PlatformManager::copy_to_clipboard` |
| **FR-026** | EXPLICIT | Windows, Linux, macOS용 단일 데스크톱 앱으로 패키징한다. | 각 플랫폼 산출물이 별도 서버 없이 실행되고 동일한 핵심 수용 시나리오를 통과한다. | `mentat-app/src/main.rs` | Partial | Windows x86_64 릴리스 빌드 검증; Linux/macOS CI 매트릭스 구성 완료 |

---

### 1.2 비기능 요구사항 (Non-Functional Requirements - Baseline NFR-001 ~ NFR-013 원본 보존)

| Baseline ID | Baseline 요구사항 정의 (원본 그대로 보존) | 수용 기준 (Baseline Acceptance Criteria) | 구현 상태 | 검증 증거 및 비고 |
|---|---|---|---|---|
| **NFR-001** | 권한 최소화 | 저장소 모듈은 읽기 인터페이스만 공개하고 쓰기·실행 능력을 주입받지 않는다. | Implemented | `test_read_only_session_scan_and_snapshot` |
| **NFR-002** | UI 반응성 | 인덱싱·검색·네트워크·추론이 UI 스레드를 막지 않으며 사용자 입력 p95 처리 지연이 100ms 이하이다. | Partial | 비동기 논블로킹 채널 폴링(`try_recv`) 및 프리뷰 비동기화; 레이턴시 벤치마크 진행 중 |
| **NFR-003** | 대형 저장소 대응 | 기본 벤치 저장소(100,000파일, 텍스트 2GiB, 제외 디렉터리 포함)에서 메모리 상한과 취소 가능성을 측정하고 전체 내용을 메모리에 동시에 보관하지 않는다. | Partial | 제외 패턴 및 64KB 스트리밍 해시 버퍼 적용; 10만 파일 벤치마크 진행 중 |
| **NFR-004** | 증거 추적성 | 답변 스냅샷의 모든 증거 참조는 원본 행 또는 `STALE/CHANGED` 상태로 해석 가능하다. | Implemented | `EvidenceRef` 경로+행범위+내용 SHA-256 해시 검증 |
| **NFR-005** | 프라이버시 | 외부 API 전송은 명시 동의·송신 범위 표시·민감정보 필터를 통과해야 하며 로그에 API 키와 원문 코드가 기본 기록되지 않는다. | Implemented | `test_complete_pem_block_redaction_zero_raw_leak`, `test_tampered_egress_request_rejection_fail_closed` |
| **NFR-006** | 백엔드 격리 | 구체 API/모델 객체는 추론 어댑터 밖으로 노출되지 않고 백엔드 실패가 저장소 세션을 손상시키지 않는다. | Implemented | `InferenceBackend` trait 경계 |
| **NFR-007** | 요청 격리 | 각 추론 요청은 독립 취소 토큰·시간 제한·컨텍스트를 가지며 공유 가능한 것은 읽기 전용 모델 가중치와 불변 설정뿐이다. | Implemented | `test_fake_inference_cancellation` |
| **NFR-008** | 시간 제한 | 네트워크 및 미래 네이티브 추론 요청은 기본 제한 시간을 가지며 하드 상한 5분을 초과할 수 없다. | Implemented | `profile.timeout_secs.clamp(5, 300)` 적용 및 전구간 사전 응답 취소 |
| **NFR-009** | 복구 가능성 | 인덱스·세션 DB 손상 시 저장소를 건드리지 않고 새 인덱스를 재생성할 수 있으며 설정 백업/초기화를 제공한다. | Partial | `SqliteStorage` 자동 테이블 마이그레이션; 백업/초기화 CLI 진행 중 |
| **NFR-010** | 접근성 | 키보드 탐색, UI 배율, 고대비, 색상 외 상태 표식, 스크린리더용 레이블을 제공한다. | Partial | Esc 네비게이션, 고대비 테마 토큰 |
| **NFR-011** | 관찰 가능성 | 작업 ID, 저장소 세션 ID, 단계, 기간, 오류 코드를 구조화 로그로 남기되 코드 원문·비밀·절대 경로를 기본 제거한다. | Implemented | `BackendProfile` Redacted Debug & `tracing` 구조화 로깅 |
| **NFR-012** | 공급망 재현성 | `Cargo.lock`, 라이선스 감사, 취약점/금지 의존성 검사, 플랫폼 빌드 매트릭스를 유지한다. | Partial | `Cargo.lock` Git 추적, `DEC-SEC-004` Accepted Risk 공식 관리 |
| **NFR-013** | 테스트 가능성 | 파일시스템, 추론, 키 저장소, 시간, 파일 감시를 포트로 분리하여 테스트 더블로 실패·취소·경쟁 조건을 재현한다. | Implemented | Hexagonal 포트 분리 & 29개 단위/회귀 테스트 PASS |

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
| **CON-007** | API 키·토큰·민감 헤더를 평문 DB, 로그, 크래시 리포트에 저장하지 않는다. | Implemented | `BackendProfile` Redacted Debug 및 `EgressFilter` 마스킹 |
| **CON-008** | 외부 API가 OpenAI 호환이라고 주장해도 기능은 능력 탐지와 실제 응답 검증 후에만 사용한다. | Implemented | `health_check` 연결 시험 및 스트림 검증 |
