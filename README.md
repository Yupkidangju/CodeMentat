# Code Mentat (코드 멘타트)

> **"Evidence Before Advice"** — 엄격한 읽기 전용 경계에서 작동하는 스마트/컴팩트 데스크톱 소프트웨어 저장소 조언자

패키지 버전은 Cargo workspace `0.1.0`이다. 문서의 `0.1.0-dev`는 같은 라인의 미릴리스 개발 상태를 뜻한다.

Code Mentat는 로컬 소프트웨어 저장소(Repository)를 **완전한 읽기 전용(Read-Only)** 경계에서 조사하고, 프로젝트의 실제 구현, 설계 의도, 문서 간 정합성을 증거 기반으로 분석하여 조언을 제공하는 독립형 데스크톱 위젯 애플리케이션입니다.

---

## 🌟 주요 특징 (Key Features)

1. **컴팩트 스마트 알약 위젯 (3-Tier Progressive Disclosure)**
   - **Tier 1 (Smart Pill, 760×56):** 흰색 고대비 스위스 스타일의 검색 바. 긴 저장소명은 ellipsis/tooltip으로 제한하고 고정·설정·종료를 우측에 보존합니다. `종료 ×`와 `Ctrl+Q`로 언제든 안전하게 종료 가능 (`Alt+Space` / `Ctrl+Alt+M`은 전역 표시·포커스 및 안전 접기)
   - **Tier 2 (Smart Card):** 질문 시 스트리밍 답변과 핵심 주장 태그(`[OBSERVED]`, `[INFERRED]`, `[CONFLICT]`)를 요약 노출
   - **Tier 3 (Detailed Inspector):** 클릭 시 펼쳐지는 실제 소스코드 행 번호 뷰어 및 관련 파일 트리
2. **엄격한 읽기 전용 불변조건 (Strict Read-Only Guarantee)**
   - 저장소 파일을 수정하거나 셸/빌드/Git 변경 명령을 실행하지 않습니다.
   - 상위 경로 탈출 및 악성 프롬프트 인젝션 텍스트는 순수 데이터로만 안전하게 격리됩니다.
3. **다중 AI 프로바이더 지원 (Multi-Provider Support)**
   - **Google Gemini (AI Studio):** 계정에 활성화된 생성 모델의 REST/SSE 스트리밍
   - **OpenRouter:** 현재 API 키로 접근 가능한 멀티 모델 카탈로그 연동
   - **OpenAI & Local Compatible:** OpenAI 공식 API와 호환 서버 또는 내장 로컬 런타임
   - 모델 ID는 코드 프리셋이 아니라 현재 API 키/로컬 런타임이 반환한 목록에서 동적으로 선택합니다.
   - `API 확인 및 모델 불러오기 → 선택 모델 호환성 확인 → 활성화`를 통과한 동일 프로필만 프로그램 AI로 사용합니다.
   - 내장 로컬은 항상 선택 가능하지만 실행 엔진 또는 설치 모델이 없으면 이유를 표시하고 활성화를 차단합니다.
4. **오프라인 로컬 분석 워크플로 (Offline Workflows)**
   - 외부 AI 없이도 `/onboard`, `/structure`, `/conflicts`, `/where` 명령으로 프로젝트 구조, 매니페스트, 권위 문서를 로컬에서 즉시 분석
5. **페르소나 레이어 & 조용한 아나운서 (Persona & Quiet Announcer)**
   - 기본 분석가, 메스카키 아나운서(Mesugaki), 간결한 감사자 페르소나 전환 지원
   - 페르소나를 변경해도 **모든 주장과 증거 팩트는 100% 동일하게 보존**
   - 중요도 0~3은 무간섭, 중요도 5(외부 전송)만 승인 시트로 노출
6. **Egress Consent & 프라이버시 쉴드**
   - 클라우드 API 전송 전 `.env`, 키 파일, 인증서 등 민감정보 파일 자동 배제 및 1회 승인 시트 제공
   - 모델 narrative는 진단 데이터로만 보존하고, UI 주요 답변은 검증된 claim/evidence에서만 합성
   - Cloud Inferred/Proposed/Conflict와 ConflictItem도 유효하고 중복 없는 evidence를 요구
   - 저장소가 `STALE`이면 신규 분석을 차단하고 재인덱싱 전 live file과 이전 hash 혼합을 허용하지 않음
   - 파일 watcher의 event-loss/unknown 신호와 ignore 규칙 변경은 즉시 STALE로 실패 폐쇄

---

## ⌨️ 단축키 일람 (Shortcuts)

| 단축키 | 기능 |
|---|---|
| `Alt + Space` / `Ctrl + Alt + M` | OS 전역 표시·포커스 및 Tier 1 접기. self-unhide 불능을 막기 위해 창을 숨기지 않음 |
| `/` 또는 `Ctrl + K` | 질문 입력창 포커스 |
| `Esc` | 단계별 축소 (Inspector → Card → Pill), 스트리밍·인덱싱 취소 |
| `Ctrl + P` | Always-on-Top 최상위 핀 고정 토글 |
| `Ctrl + Q` | 진행 작업을 취소하고 Code Mentat 종료 |
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

### UI 문제 해결 / UI Troubleshooting / UI トラブルシューティング / UI 疑難排解 / UI 故障排除

- **한국어:** 내장 한글 폴백 글꼴과 불투명 흰색 고대비 테마로 사각형 글리프와 흐린 글자를 방지합니다. 설정·질문·증거 패널은 자동 확장되며 상단의 `종료 ×`로 앱을 닫을 수 있습니다.
- **English:** The embedded Korean fallback font and opaque high-contrast white theme prevent square glyphs and faint text. Settings, chat, and evidence panels resize automatically, and `Close ×` exits the app.
- **日本語:** 内蔵の韓国語フォールバックフォントと不透明な高コントラスト白テーマで、文字化けと読みにくい文字を防ぎます。各パネルは自動拡張され、`終了 ×`でアプリを閉じられます。
- **繁體中文:** 內嵌韓文字型與不透明的高對比白色主題可避免方框字元及文字過淡。設定、對話與證據面板會自動展開，並可用 `結束 ×` 關閉程式。
- **简体中文:** 内置韩文字体和不透明高对比白色主题可避免方框字符与文字过淡。设置、对话和证据面板会自动展开，并可用 `退出 ×` 关闭程序。

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

MIT OR Apache-2.0 License. 내장 NanumGothic 글꼴은 `crates/mentat-app/assets/fonts/OFL-NanumGothic.txt`의 SIL Open Font License 1.1을 따릅니다.
