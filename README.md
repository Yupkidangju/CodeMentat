# Code Mentat (코드 멘타트)

> **"Evidence Before Advice"** — 엄격한 읽기 전용 경계에서 작동하는 스마트/컴팩트 데스크톱 소프트웨어 저장소 조언자

Code Mentat는 로컬 소프트웨어 저장소(Repository)를 **완전한 읽기 전용(Read-Only)** 경계에서 조사하고, 프로젝트의 실제 구현, 설계 의도, 문서 간 정합성을 증거 기반으로 분석하여 조언을 제공하는 독립형 데스크톱 위젯 애플리케이션입니다.

---

## 🌟 주요 특징 (Key Features)

1. **컴팩트 스마트 알약 위젯 (3-Tier Progressive Disclosure)**
   - **Tier 1 (Smart Pill):** 개발자 화면을 가리지 않고 상단/모서리에 상주하는 초슬림 검색 바 (글로벌 단축키 `Alt+Space` 호출)
   - **Tier 2 (Smart Card):** 질문 시 스트리밍 답변과 핵심 주장 태그(`[OBSERVED]`, `[INFERRED]`, `[CONFLICT]`)를 요약 노출
   - **Tier 3 (Detailed Inspector):** 클릭 시 펼쳐지는 실제 소스코드 행 번호 뷰어 및 관련 파일 트리
2. **엄격한 읽기 전용 불변조건 (Strict Read-Only Guarantee)**
   - 저장소 파일을 수정하거나 셸/빌드/Git 변경 명령을 실행하지 않습니다.
   - 상위 경로 탈출 및 악성 프롬프트 인젝션 텍스트는 순수 데이터로만 안전하게 격리됩니다.
3. **다중 AI 프로바이더 지원 (Multi-Provider Support)**
   - **Google Gemini (AI Studio):** `gemini-2.5-flash`, `gemini-2.5-pro` 실시간 REST/SSE 스트리밍
   - **OpenRouter:** Claude 3.7 Sonnet, DeepSeek R1, LLaMA 3.3 등 다양한 모델 연동
   - **OpenAI & Local Compatible:** `gpt-4o`, `gpt-4o-mini` 또는 로컬 vLLM/Ollama
4. **오프라인 로컬 분석 워크플로 (Offline Workflows)**
   - 외부 AI 없이도 `/onboard`, `/structure`, `/conflicts`, `/where` 명령으로 프로젝트 구조, 매니페스트, 권위 문서를 로컬에서 즉시 분석
5. **페르소나 레이어 & 조용한 아나운서 (Persona & Quiet Announcer)**
   - 기본 분석가, 메스카키 아나운서(Mesugaki), 간결한 감사자 페르소나 전환 지원
   - 페르소나를 변경해도 **모든 주장과 증거 팩트는 100% 동일하게 보존**
   - 중요도 0~3은 무간섭, 중요도 5(외부 전송)만 승인 시트로 노출
6. **Egress Consent & 프라이버시 쉴드**
   - 클라우드 API 전송 전 `.env`, 키 파일, 인증서 등 민감정보 파일 자동 배제 및 1회 승인 시트 제공

---

## ⌨️ 단축키 일람 (Shortcuts)

| 단축키 | 기능 |
|---|---|
| `Alt + Space` / `Ctrl + Alt + M` | 위젯 표시 / 숨김 토글 |
| `/` 또는 `Ctrl + K` | 질문 입력창 포커스 |
| `Esc` | 단계별 축소 (Inspector → Card → Pill) 및 스트리밍 즉시 취소 |
| `Ctrl + P` | Always-on-Top 최상위 핀 고정 토글 |
| `Enter` | 질문 전송 및 Egress 승인 |

---

## 📦 빠른 시작 (Quick Start)

### 필수 요구사항
- [Rust](https://www.rust-lang.org/) (1.80+ 권장)

### 빌드 및 실행
```bash
# 워크스페이스 전체 단위 테스트 실행
cargo test --workspace

# 컴팩트 위젯 애플리케이션 실행
cargo run -p mentat-app
```

---

## 🏛️ Cargo Workspace 아키텍처

```text
crates/
├── mentat-core             # 도메인 모델, 불변조건, 포트 (RepositoryReader, StoragePort)
├── mentat-repository       # 읽기 전용 세션(ReadOnlySession), 파일 스캐너, Watcher
├── mentat-analysis         # 기술스택 탐지기, 증거 인덱스(EvidenceRef), 시맨틱 커널, Egress 필터
├── mentat-inference        # 공통 추론 인터페이스(InferenceBackend) 및 Fake 테스트 더블
├── mentat-inference-openai # Google Gemini, OpenRouter, OpenAI 멀티 어댑터
├── mentat-inference-llama  # 미래 llama.cpp 내장을 위한 준비 계약(Contract) 스위트
├── mentat-persona          # 페르소나 3종 렌더러 및 아나운서 정책
├── mentat-storage          # AppData SQLite DB (최근 저장소 및 설정 영속화)
├── mentat-platform         # OS 앱데이터 격리 검증, 다이얼로그, 클립보드
└── mentat-app              # eframe/egui 기반 프레임리스 컴팩트 플로팅 위젯
```

---

## 📄 라이선스 (License)

MIT OR Apache-2.0 License.
