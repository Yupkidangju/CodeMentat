# Code Mentat Architecture Decision Records (DESIGN_DECISIONS.md)
## 아키텍처 및 디자인 결정 기록

- **문서 버전:** 0.1.0-dev (Turn 13 / Re-audit #10 Remediation)
- **패키지 버전:** 0.1.0 (`0.1.0-dev`는 문서 상태)
- **표준 규격:** AI Implementation Documentation Standard Section 7 / D3D Protocol v1.3
- **기준 작성일:** 2026-08-18

---

## 1. 아키텍처 및 도메인 경계 결정

### [DEC-ARCH-001] 10개 독립 크레이트로의 엄격한 모듈 분해 (Hexagonal Cargo Workspace)
- **배경:** 저장소 파일 시스템 쓰기, 빌드 실행 등 위험한 동작이 조언 및 분석 엔진 내부로 침투하는 것을 원천 차단해야 함.
- **결정:** `mentat-core`를 중심으로 10개의 독립 crate(`mentat-repository`, `mentat-analysis`, `mentat-inference`, `mentat-inference-openai`, `mentat-inference-llama`, `mentat-persona`, `mentat-storage`, `mentat-platform`, `mentat-app`)로 분리함.
- **대안 및 기각 사유:**
  - *단일 거대 모듈(Monolith):* crate 내부 가시성(`pub(crate)`) 누수로 인해 실수로 쓰기 기능이 노출될 위험이 있어 기각.
- **결과:** 컴파일 타임에 읽기 전용 경계가 강제됨.

### [DEC-ARCH-002] AppData 경로로의 SQLite 저장소 격리
- **배경:** 최근 저장소 목록, 공급자 프로필, 세션 데이터를 보관해야 하지만 대상 저장소 내부에 `.mentat/` 폴더를 생성하는 것은 읽기 전용 불변조건을 위배함.
- **결정:** OS 표준 AppData(`PlatformManager::get_app_data_dir()`)에만 SQLite DB를 생성하고 영속화함.
- **대안 및 기각 사유:**
  - *저장소 내 `.mentat/` 생성:* Git status를 더럽히고 읽기 전용 원칙을 훼손하여 기각.
- **결과:** 저장소 파일 시스템은 완벽히 무오염 상태 유지.

### [DEC-ARCH-003] 마스터 사양서 권위 및 추적성 모델 (IMP-F001 권위 확립)
- **배경:** `CODE_MENTAT_SPEC.md`와 `spec.md` 간의 권위 충돌 및 요구사항 ID 단절 문제 해소 필요.
- **결정:**
  - `CODE_MENTAT_SPEC.md`: 기능(FR-001~026), 비기능(NFR-001~013), 제약조건(CON-001~008)의 **마스터 요구사항 베이스라인(Master Baseline)**으로 유지.
  - `spec.md`: 현재 검증된 구현 상태, 모듈 계약, 실데이터 및 요구사항 추적 매트릭스를 제공하는 **활성 실행 명세서(Canonical Implementation Spec)**로 유지.
- **대안 및 기각 사유:**
  - *기존 요구사항 ID 일괄 삭제:* 감사 추적성이 단절되므로 기각.
- **결과:** 두 문서의 역할이 명확히 정의되고 양방향 요구사항 추적성 확보.

---

## 2. 보안 및 프라이버시 결정 (Security ADRs)

### [DEC-SEC-001] Fail-Closed Egress Consent & Single-Use Immutable EgressReceipt (SEC-F001)
- **배경:** 컨텍스트 조립 지연 시 무단 전송(Fail-Open) 및 전송 전후 문맥 불일치(TOCTOU) 위험 차단.
- **결정:**
  - 컨텍스트 조립 실패/시간초과 시 외부 전송을 전면 차단(Fail-Closed).
  - 승인된 패킷의 SHA-256 해시를 고정한 단일 사용 `EgressReceipt`를 발급하고, 승인된 패킷을 재조립 없이 그대로 소비.
- **결과:** 무승인 전송 및 문맥 변조 위험 0건 달성.

### [DEC-SEC-008] 승인 대상 전체를 결속하는 canonical egress seal (SEC-F001)
- **배경:** `prompt_context` 단독 해시는 승인 후 질문, citation 검증 텍스트, 스냅샷, 파일 참조 또는 공급자 엔드포인트 교체를 식별하지 못한다.
- **결정:** 하나의 길이-prefix canonical SHA-256 함수가 packet ID, snapshot, prompt, redacted question, 포함/제외 경로, 정렬된 파일 참조, 정렬된 validation text digest, redaction/token 계수와 활성 프로필 ID·provider·endpoint·model·timeout을 결속한다.
- **불변조건:** packet 생성, receipt 발급, `ApprovedInferenceRequest::new`, consume-time 검증은 동일 함수만 사용한다. 별도 부분 해시나 필드별 fallback은 허용하지 않는다.
- **결과:** 같은 길이 question, validation map, snapshot/ref, profile endpoint/model 중 하나라도 바뀌면 승인 요청 생성 또는 소비가 실패 폐쇄된다.

### [DEC-SEC-009] 검증된 claim에서만 주요 답변 합성 (IMP-F004)
- **배경:** 모델의 `direct_answer`와 schema 위반 원문은 claim/evidence validator를 통과하지 않고도 UI의 가장 눈에 띄는 본문에 표시될 수 있다.
- **결정:** cloud 응답의 주요 답변은 validation 이후 최소 1개의 유효 evidence를 가진 `Unknown`이 아닌 claim의 classification, statement, confidence와 evidence cardinality로만 결정적으로 합성한다. `Observed`, `Inferred`, `Proposed`, `Conflict` 모두 같은 evidence cardinality를 적용한다.
- **Cloud conflict:** `ConflictItem`도 non-empty·존재·유효·중복 없음 evidence 조건을 모두 통과할 때만 `[CONFLICT]` UI로 전달한다. 실패 항목은 raw response에만 남기고 검증 결과에서 제거한다.
- **격리:** 모델 원문과 제출한 `direct_answer`는 `raw_model_response`에만 보존하며 주요 UI 본문으로 전달하지 않는다.
- **결과:** schema 위반 또는 claims와 불일치하는 모델 narrative가 evidence product의 검증된 답변으로 보이지 않는다.

### [DEC-SEC-002] 내용 인식 비밀정보 스캐너 및 다층 필터링 (SEC-F002)
- **배경:** 파일명 블랙리스트만으로는 일반 파일(README, 코드) 내 API 키, 인증서, 토큰 유출을 막을 수 없음.
- **결정:**
  - 확장자/파일명 블랙리스트(`token*`, `*.pem`, `*.key`, `id_rsa*`, `credentials*` 등) 대폭 강화.
  - 내용 인식 스캐너(`AIzaSy...`, `sk-...`, `ghp_...`, `BEGIN PRIVATE KEY`, `PASSWORD=`)를 도입하여 자동 마스킹(`[REDACTED_SECRET]`) 및 민감 파일 제외.
- **결과:** 외부 유출 위험 원천 차단.

### [DEC-SEC-003] 저장소 열기 시점의 강제적 AppData 격리 검증 (SEC-F003)
- **배경:** 사용자 홈 디렉터리 등 AppData의 상위 경로를 저장소로 열 때 DB 파일이 저장소 내부에 생성되는 위험 방지.
- **결정:** `ReadOnlySession` 오픈 및 SQLite DB 쓰기 전에 `PlatformManager::validate_storage_isolation`을 필수로 실행하여 상호 포함 관계 시 열기 즉시 거부.
- **결과:** 읽기 전용 불변조건의 완벽한 런타임 강제.

### [DEC-SEC-004] 공급망 보안 감사 및 `quick-xml 0.30.0` 위험 수용 (Accepted Risk - SEC-F007)
- **책임 관리자 (Owner):** Security Lead (`@Yupkidangju`)
- **만료 기한 (Expiry Date):** 2026-11-30 (차기 분기별 의존성 릴리스 사이클)
- **정기 재검토 조건:** `eframe 0.31.0` 릴리스 또는 상위 `accesskit_unix` 패치 릴리스 시 즉시 업그레이드
- **배경:** `eframe 0.30.0`의 전이적 의존성(`accesskit_unix` -> `atspi` -> `quick-xml 0.30.0`)에서 `RUSTSEC-2026-0194`, `RUSTSEC-2026-0195` (CVSS 7.5 High) 발견.
- **도달 가능성(Reachability) 분석:**
  - `CodeMentat`는 Windows 데스크톱 타깃 애플리케이션이며, `quick-xml`은 오직 Linux `atspi` 접근성 경로에서만 조건부 컴파일됨.
  - 외부 또는 신뢰할 수 없는 XML 문서를 파싱하는 런타임 실행 경로가 존재하지 않음.
- **결정 및 완화 조치:**
  - 상위 GUI 프레임워크(`eframe`)의 패치 릴리스 시 즉각 업그레이드하기로 하며, 만료일(2026-11-30) 이전까지는 도달 불가능(Unreachable)한 위험 수용(Accepted Risk)으로 공식 승인함.
- **결과:** 프로덕션 보안 상태 명문화 및 추적성 확보.

---

## 3. UI/UX 및 추론 엔진 결정

### [DEC-UI-001] 3단계 점진적 공개(3-Tier Progressive Disclosure) 위젯 채택
- **Tier 1 (Smart Pill, `760x56px`):** 상단 상주 슬림 검색 바와 명시적 종료 동작
- **Tier 2 (Smart Card, `760x360px`):** 질문 시 스트리밍 답변과 Claim 태그 표출
- **Tier 3 (Detailed Inspector, `900x620px`):** 소스코드 행 뷰어 및 파일 트리

### [DEC-INF-001] 다중 공급자 공통 계약 및 Google Gemini / OpenRouter 지원
- 단일 `InferenceBackend` 인터페이스 하에 Google Gemini REST/SSE 및 OpenRouter / OpenAI SSE 어댑터 지원.

### [DEC-INF-005] 동적 모델 검색과 검증 후 활성화
- **배경:** 정적 모델 프리셋은 계정 권한, 모델 폐기, 로컬 설치 상태와 어긋나며 연결 성공만으로 선택 모델의 실제 추론 호환성을 보장할 수 없다.
- **결정:** 운영 모델 ID 하드코딩을 제거하고 `InferenceBackend`에 모델 검색과 선택 모델 생성 프로브 계약을 둔다. UI는 Draft와 Active 프로필을 분리하고 `Draft → ModelsDiscovered → ModelVerified → Active` 순서만 허용한다.
- **보안 경계:** 공급자 응답은 신뢰하지 않는 데이터로 검증한다. API 키는 메모리의 Draft/Active에만 존재하며 SQLite에는 저장하지 않는다. 프로필 편집은 기존 Active를 변경하지 않는다.
- **내장 로컬:** 선택지는 제공하되 네이티브 실행 엔진 또는 설치 모델이 없으면 빈 카탈로그와 구조화 오류를 반환하여 활성화를 차단한다.
- **대안 및 기각 사유:** 실패 시 코드 내 기본 모델 목록을 표시하는 방식은 존재하지 않거나 권한 없는 모델을 활성화할 수 있어 기각한다.

### [DEC-INF-006] Gemini 호환성 probe는 thinking-aware 응답 전체를 검증한다
- **배경:** `maxOutputTokens: 1`은 thinking 모델이 visible text 전에 `MAX_TOKENS`로 끝나게 할 수 있고, 첫 part 고정 포인터는 thought/signature 뒤의 text를 놓친다.
- **결정:** exact-OK 지시 자체는 유지하되 임의 output token cap을 제거한다. 모든 candidate의 모든 part에서 non-empty text를 검색하며, text가 없으면 block/finish/thought token metadata를 노출한다.
- **보안 경계:** 응답은 1MiB 제한과 timeout을 유지하고 provider 원문 전체나 자격 증명을 진단 메시지에 포함하지 않는다.
- **결과:** 모델 ID 하드코딩이나 fail-open 없이 Gemini 2.5/3 계열의 실제 text 생성 능력을 판정한다.

### [DEC-SEC-005] 사용자 제외 토글은 generation 가드로 이전 패킷을 즉시 무효화한다 (SEC-F011)
- **배경:** 제외 체크박스 변경 후 비동기 재조립이 끝나기 전 기존 `pending_egress_packet`을 승인하면, UI에는 제외된 파일이 전송 payload에는 남는 consent TOCTOU가 발생한다.
- **결정:**
  - 제외 집합이 바뀌는 즉시 `pending_egress_packet = None`, `rebuilding = true`, `generation += 1`.
  - 승인은 `!rebuilding && pending_packet.is_some()`일 때만 가능하다.
  - 조립 결과는 현재 generation과 일치할 때만 수락하고, 이전 generation 결과는 폐기한다.
- **대안 및 기각 사유:**
  - *표시만 바꾸고 승인 payload를 재조립 완료 후 교체:* 느린 조립 동안 승인 창이 열려 위험하므로 기각.
- **결과:** 제외된 파일이 포함된 old packet은 전송 경로에 도달할 수 없다.

### [DEC-ARCH-004] 안정 저장소 ID는 canonical root 조회 결과이다 (IMP-F005)
- **배경:** `ReadOnlySession::open()`이 매번 새 UUID를 만들면 `snapshot_history`가 orphan되고 재실행 복원이 불가능하다.
- **결정:** 정규화된 루트 경로로 `recent_repositories`를 조회하고, 기존 ID가 있으면 세션에 재사용한다. 복원된 snapshot digest가 새 scan과 같으면 snapshot ID도 유지한다.
- **대안 및 기각 사유:**
  - *경로 문자열 그대로 비교:* Windows에서 상대/UNC/`\\?\` 표기 불일치로 실패하므로 기각.
- **결과:** 같은 루트를 다시 열면 이전 snapshot/session 메타데이터를 찾을 수 있다.

### [DEC-INF-002] 클라우드 응답은 AnswerBundle 검증 후에만 Claim이 된다 (IMP-F004)
- **배경:** markdown bullet을 임의로 높은 신뢰도 `Inferred` Claim으로 바꾸면 근거 없는 문장이 구조화 답변처럼 보인다.
- **결정:** JSON `AnswerBundle`만 주장으로 승격한다. citation은 현재 snapshot 파일/행 범위와 대조하고 실패 시 `[INVALID_CITATION]`과 `Unknown`으로 강등한다. 비구조 텍스트는 `UNSTRUCTURED_RESPONSE`로 표시하고 자동 주장을 만들지 않는다.
- **결과:** 모델 원문과 검증된 Claim이 분리된다.

### [DEC-INF-004] Cloud 요청은 AnswerBundle schema와 citation catalog를 포함한다 (IMP-F004)
- **배경:** validator만 강화하면 모델이 유효 citation을 만들 메타데이터를 받지 못한다.
- **결정:** `ApprovedInferenceRequest` system contract에 JSON schema를 넣고, packet context에 snapshot ID와 파일별 path/hash/허용 행 범위를 제공한다. 앱은 packet에 실린 실제 파일 본문으로 `from_model_text_with_contents`를 호출한다.
- **결과:** 입력 계약과 출력 검증이 같은 citation metadata를 공유한다.

### [DEC-SEC-007] 최종 outbound 텍스트는 질문 포함 단일 경로로만 나간다 (SEC-F002)
- **결정:** `user_question`도 `scan_and_redact_secrets`를 통과한다. 질문은 packet의 `redacted_user_question` 한 곳에만 두고 어댑터는 `prompt_context`에 다시 붙이지 않는다.

### [DEC-UI-002] 뷰포트·테마·단축키는 구현값을 제품 기준으로 동결한다 (IMP-F003)
- **상태:** 아래 초기 동결값은 `DEC-UI-003`으로 대체됐다.
- **결정:** Tier 크기는 `580x52` / `580x300` / `660x480`, conflict 색은 amber `#F59E0B`이다. `Ctrl+K`/`Ctrl+P`는 앱 단축키, `Alt+Space`/`Ctrl+Alt+M`은 OS 전역 표시·포커스/접기로 등록한다. hidden winit 창의 self-unhide dead-end를 피하기 위해 창 숨김은 금지한다.

### [DEC-UI-003] 불투명 고대비 스위스 UI와 명시적 종료 경계
- **배경:** 프레임리스 투명 창에는 운영체제 종료 버튼이 없었고, dark visual의 암묵적 foreground와 반투명 표면 때문에 일부 글자와 경계가 표시 환경에 따라 흐려졌다.
- **결정:** `DEC-UI-002`의 색상과 크기를 대체한다. 창은 불투명 흰색으로 렌더링하고 8pt 그리드, 검정 타이포그래피, 1px 선, 최소 모서리 반경과 제한적인 Swiss red 강조를 사용한다. Tier 크기는 `760x56` / `760x360` / `900x620`, 설정은 `760x480`이다. Tier 1에는 항상 `종료 ×`를 두고 `Ctrl+Q`와 같은 종료 수명주기를 공유한다.
- **Trailing 불변조건:** `고정`·`설정`·`종료 ×`는 우측 180pt 고정 영역에 먼저 배치한다. 저장소 버튼은 최대 120pt에서 ellipsis 처리하며 640/760px, 긴 ASCII/CJK 이름에서도 질문 입력 200pt와 종료 hit target을 보존한다.
- **대안 및 기각 사유:** 운영체제 장식을 복구하면 컴팩트 상주 위젯의 화면 점유가 커지므로 기각했다. 투명 흰색 프레임은 Windows 합성 결과가 달라질 수 있어 기각했다.
- **결과:** 종료 경로를 발견할 수 있고, 글자·포커스·상태가 흰 배경에서 일관되게 식별된다.

### [DEC-DBG-003] OS watcher event + changed-path full hash (DBG-F002)
- **배경:** 3초마다 전 파일 첫 8KiB를 읽는 방식은 tail edit를 놓치면서 100k 파일 I/O를 반복한다.
- **결정:** `notify 8.2.0` RecommendedWatcher를 primary로 사용하고 event를 `Ignore`, `ChangedPaths`, `Rescan`으로 분류한다. Access-only는 Ignore, Create/Modify/Remove는 ignore-aware changed-path full hash, `need_rescan`·Any·Other·`.gitignore`·`.git/info/exclude` 변경은 경로 유무와 무관하게 Rescan/STale로 실패 폐쇄한다.
- **Lineage 경계:** 신규 분석은 `Ready` snapshot에서만 가능하다. Egress live read 직후 SHA-256이 scan 시점 `FileRecord.content_hash`와 다르면 packet assembly를 실패 폐쇄한다.
- **결과:** ignored artifact false STALE, Stale 신규 분석, 이전 hash와 live body 혼합을 동시에 차단한다.

### [DEC-PERF-001] 100k/2GiB Windows peak working set 상한 (DBG-F003)
- **기준 장비:** Windows 11 Pro 10.0.26200 x64, AMD Ryzen 7 8845HS 16 logical processors, RAM 27.8GiB, rustc 1.96.0, debug test profile.
- **fixture:** 100,000 files, 실제 text 2,147,483,648 bytes, global preview budget 8MiB.
- **관측값:** peak working set 약 46.1MiB.
- **결정:** 동일 Windows profile의 허용 peak working set 상한을 128MiB로 고정한다. 계측 불가 또는 `PeakWorkingSetSize > 128MiB`이면 ignored performance gate를 실패시킨다.
- **근거:** 현재 관측값의 약 2.7배 여유를 두되, FileRecord/preview 무제한 회귀는 자동 차단한다.

### [DEC-REL-001] Cargo 0.1.0과 문서 0.1.0-dev (IMP-F002)
- **결정:** 패키지 SemVer는 `0.1.0`이다. `0.1.0-dev`는 미릴리스 문서 상태이며 다른 제품을 가리키지 않는다.

### [DEC-BUILD-001] Rust xtask 기반 멀티 플랫폼 빌드 진입점
- **배경:** 플랫폼별 독립 스크립트에 빌드 로직을 복제하면 target triple, 품질 게이트와 산출물 경로가 쉽게 어긋난다.
- **결정:** 의존성 없는 workspace `xtask`가 메뉴/CLI 파싱, 타깃 매핑, 게이트 및 Cargo 실행을 소유한다. PowerShell과 POSIX shell 파일은 인자를 그대로 `cargo mentat-build`에 전달하는 wrapper로만 유지한다.
- **실패 경계:** 명시적 target이 rustup에 설치되지 않았거나 linker/SDK가 없으면 다른 플랫폼 또는 host build로 fallback하지 않는다.
- **결과:** Windows/Linux/macOS에서 같은 명령 계약과 테스트된 build plan을 사용한다.

### [DEC-INF-003] Cloud citation은 current snapshot의 hash/excerpt/range와 일치해야 한다 (IMP-F004)
- **배경:** path만 맞으면 존재하는 파일을 가리키는 가짜 citation이 Observed로 남을 수 있다.
- **결정:** `AnswerBundleNormalizer`는 모델 snapshot ID를 현재 ID로 강제하고, FileRecord content hash, 실제 excerpt, line_end를 검증한다. 하나라도 무효인 evidence를 가진 claim은 Unknown으로 강등한다. `/conflicts`는 문서가 주장한 언어/경로와 스캔 결과를 비교한다.
- **대안 및 기각 사유:** markdown bullet 정규화는 근거 없는 주장을 만들어 기각.
- **결과:** 그럴듯한 가짜 citation과 문서-코드 불일치가 구조화 결과에 남는다.

### [DEC-SEC-006] 고엔트로피 토큰 마스킹과 최소 relevance threshold (SEC-F002)
- **배경:** 알려진 공급자 패턴만으로는 신규 credential 형식과 질의와 무관한 문서를 막지 못한다.
- **결정:** 길이 24 이상이고 Shannon entropy가 임계값을 넘는 토큰을 마스킹한다. path relevance가 최소 점수 미만인 파일은 내용을 읽지 않는다.
- **결과:** 미등록 비밀 형식과 점수 0 문서의 기본 송신 경로가 닫힌다.

### [DEC-DBG-002] Scan은 취소 가능하고 한도 초과는 omission으로 남긴다 (DBG-F003)
- **배경:** 파일 수/바이트 상한만으로는 거대 파일 해시와 장시간 walk를 멈추지 못한다.
- **결정:** metadata preflight로 단일 파일 상한을 해시 전에 적용하고, `CancellationToken`과 `ScanOutcome.omissions`을 도입한다. 대표 규모는 주입 가능한 `ScanLimits`로 검증한다.
- **결과:** 거대 파일/취소/한도가 명시적 결과로 남는다.

### [DEC-DBG-001] 저장소 워처 walk는 UI 스레드 밖에서만 수행한다 (DBG-F008)
- **배경:** 1초 throttle만으로는 큰 트리의 동기 walk가 프레임을 막을 수 있다.
- **결정:** `RepositoryWatcher::new()`는 walk하지 않는다. 최초 signature와 이후 walk는 worker에서만 수행하고 Drop은 join하지 않는다. UI는 `poll_changes()`로 결과만 읽는다.
- **결과:** 저장소 열기·전환 시 UI 스레드가 전체 트리를 순회하지 않는다.

### [DEC-PER-001] 사실 불변조건을 준수하는 표현 계층 페르소나 분리
- 페르소나 전환 시에도 `Claim`, `EvidenceRef`, `Conflict` 등 사실 판단은 100% 보존.
