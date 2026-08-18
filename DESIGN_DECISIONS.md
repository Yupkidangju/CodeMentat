# Code Mentat Architecture Decision Records (DESIGN_DECISIONS.md)
## 아키텍처 및 디자인 결정 기록

- **문서 버전:** 0.1.0-dev (Turn 2 Remediation)
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
- **Tier 1 (Smart Pill, `580x48px`):** 상단 상주 슬림 검색 바
- **Tier 2 (Smart Card, `580x280px`):** 질문 시 스트리밍 답변과 Claim 태그 표출
- **Tier 3 (Detailed Inspector, `640x460px`):** 소스코드 행 뷰어 및 파일 트리

### [DEC-INF-001] 다중 공급자 공통 계약 및 Google Gemini / OpenRouter 지원
- 단일 `InferenceBackend` 인터페이스 하에 Google Gemini REST/SSE 및 OpenRouter / OpenAI SSE 어댑터 지원.

### [DEC-PER-001] 사실 불변조건을 준수하는 표현 계층 페르소나 분리
- 페르소나 전환 시에도 `Claim`, `EvidenceRef`, `Conflict` 등 사실 판단은 100% 보존.
