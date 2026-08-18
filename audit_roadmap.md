# Code Mentat Audit Roadmap (audit_roadmap.md)
## 코드 멘타트 감사 로드맵

- **문서 버전:** 1.0.0
- **참조 표준:** AI Audit Documentation Standard (`AI_AUDIT_DOC_STANDARD.md`)
- **적용 대상:** Code Mentat 10-Crate Workspace

---

## 1. 감사 프레임워크 (3-Pass Audit Model)

Code Mentat 감사는 세 가지 독립된 관점(Pass)으로 수행되며, 모든 게이트 조건을 충족해야 최종 PASS로 판정합니다.

| Pass | 핵심 질문 | 주요 입력물 | 게이트 통과 기준 |
|---|---|---|---|
| **Pass 1: Implementation Compliance** | `spec.md` 및 `designs.md`의 기능, 타입, API, UI가 코드와 테스트에 정확히 반영되었는가? | `spec.md`, `designs.md`, `crates/*` | 문서-구현 드리프트 없음, 고아(Orphan) 코드 없음 |
| **Pass 2: Debug / Engineering Quality** | 코드가 결정적이고 안정적이며 부팅/빌드 재현성을 보장하는가? | `Cargo.toml`, `Cargo.lock`, `BUILD_GUIDE.md`, tests | `cargo test --workspace` 100% PASS, 0 warnings |
| **Pass 3: Security & Privacy** | 읽기 전용 경계, 프롬프트 인젝션 차단, Egress 필터, AppData 격리가 완벽한가? | `mentat-repository`, `mentat-analysis/egress.rs`, `mentat-platform` | 파일 쓰기 시도 원천 차단, 외부 전송 시 민감정보 자동 배제 |

---

## 2. Phase별 게이트 통과 기준 및 검증 결과

### 🏁 Phase 1: Workspace Scaffolding & Strict Read-Only Boundary
- **검증 항목:**
  - `mentat-repository`: `.gitignore` 존중 스캐닝 및 상위 디렉터리 탈출(`..`) 차단 (`test_external_path_blocked`)
  - `mentat-platform`: AppData 경로가 저장소 내부가 아님을 검증 (`test_storage_isolation_detection`)
- **게이트 판정:** **PASS (Verified)**

### 🏁 Phase 2: Universal Detection, Evidence Model & Semantic Kernel
- **검증 항목:**
  - `mentat-analysis`: 다중 언어 기술스택 및 매니페스트 정확 탐지 (`test_detector_rust_project`)
  - `mentat-analysis`: 악성 프롬프트 인젝션 텍스트가 실행되지 않고 데이터로 격리됨 (`test_evidence_and_prompt_injection_safety`)
  - `mentat-repository`: Mtime 변경 감시자 (`RepositoryWatcher`) 작동
- **게이트 판정:** **PASS (Verified)**

### 🏁 Phase 3: Multi-Provider Inference & Streaming
- **검증 항목:**
  - `mentat-inference`: `FakeInferenceBackend` 스트리밍 및 취소 토큰(`CancellationToken`) 100ms 내 중단 검증
  - `mentat-analysis`: `.env`, `*.pem`, `*.key`, `id_rsa` 민감정보 자동 필터링 (`test_sensitive_filtering`)
  - `mentat-inference-openai`: Google Gemini (AI Studio) 및 OpenRouter/OpenAI SSE 파싱
- **게이트 판정:** **PASS (Verified)**

### 🏁 Phase 4: Persona Engine & Storage Persistence
- **검증 항목:**
  - `mentat-persona`: 3종 페르소나 전환 시 `Claim` ID, 신뢰도, `EvidenceRef` 팩트 100% 일치 (`test_persona_rendering_preserves_facts_and_evidence`)
  - `mentat-storage`: SQLite `recent_repositories` CRUD 및 정렬 검증 (`test_sqlite_storage_save_and_list_recent_repos`)
- **게이트 판정:** **PASS (Verified)**

### 🏁 Phase 5: Native Llama Contract & Final Stabilization
- **검증 항목:**
  - `mentat-inference-llama`: 무거운 외부 C 링크 없이 모델 수명주기, 격리 컨텍스트, KV 캐시 명시적 해제, 세마포어 동시성 제한기 계약 검증 (`test_native_llama_contract_isolated_context_and_kv_cleanup`)
  - 전체 워크스페이스 10개 크레이트 빌드 및 테스트 완전 통과
- **게이트 판정:** **PASS (Verified)**

---

## 3. 잔여 리스크 및 모니터링 (Accepted Risks)

1. **대용량 저장소(100,000+ 파일) 스캐닝 메모리 사용량:**
   - *위험 수준:* Minor
   - *대응책:* 현재 `.gitignore` 준수 및 바이너리/미디어 파일 자동 제외를 적용하고 있으며, 향후 가상화 페이징 인덱서 도입 검토.
2. **클라우드 API 네트워크 지연:**
   - *위험 수준:* Minor
   - *대응책:* `Esc` 또는 `[⏹️ 취소]` 버튼을 통한 즉각적인 비동기 취소(`CancellationToken`) 메커니즘이 확립되어 있음.
