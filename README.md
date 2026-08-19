# Code Mentat (코드 멘타트)

> **"Evidence Before Advice"** — 엄격한 읽기 전용 경계에서 작동하는 스마트/컴팩트 데스크톱 소프트웨어 저장소 조언자

패키지 버전은 Cargo workspace `0.1.0`이다. 문서의 `0.1.0-dev`는 같은 라인의 미릴리스 개발 상태를 뜻한다.

Code Mentat는 로컬 소프트웨어 저장소(Repository)를 **완전한 읽기 전용(Read-Only)** 경계에서 조사하고, 프로젝트의 실제 구현, 설계 의도, 문서 간 정합성을 증거 기반으로 분석하여 조언을 제공하는 독립형 데스크톱 위젯 애플리케이션입니다.

### CR-UX-001 구현 상태 / Implementation Status / 実装状況 / 實作狀態 / 实现状态

- **한국어:** 세로형 자유 대화 UI가 production AgentLoop와 연결되었습니다. 검증된 모델은 6개 읽기 전용 도구를 사용하며, redaction→사용자 동의→exact-body canonical receipt→Grounding/Audit UI 경계를 통과한 결과만 외부 공급자에 전송합니다.
- **English:** The vertical chat UI now uses the production AgentLoop. Verified models can use six read-only tools, and repository excerpts reach external providers only through redaction, explicit consent, an exact-body canonical receipt, and Grounding/Audit UI boundaries.
- **日本語:** 縦型チャット UI を production AgentLoop に接続しました。検証済みモデルは 6 個の読み取り専用ツールを利用でき、リポジトリ抜粋は redaction、明示的同意、exact-body canonical receipt、Grounding/Audit UI を通過した場合のみ外部プロバイダーへ送信されます。
- **繁體中文:** 直向聊天 UI 已連接 production AgentLoop。經驗證的模型可使用六個唯讀工具；儲存庫摘錄僅能經過遮罩、明確同意、exact-body canonical receipt 與 Grounding/Audit UI 邊界後傳送給外部供應商。
- **简体中文:** 纵向聊天 UI 已连接 production AgentLoop。经验证的模型可使用六个只读工具；仓库摘录仅能经过脱敏、明确同意、exact-body canonical receipt 与 Grounding/Audit UI 边界后发送给外部供应商。

상세 계획 / Details: [ROADMAP.md](ROADMAP.md), [CR-UX-001_TRACEABILITY.md](CR-UX-001_TRACEABILITY.md)

---

## 🌟 주요 특징 (Key Features)

1. **세로형 자유 대화 멘토 UI**
   - 최초 `312.5×660`, 최소 `240×360`이며 사용자가 조절한 크기·핀·전송 키 설정을 AppData에 복원합니다.
   - 닫기·설정·핀·새 대화는 상단 고정 영역에 유지되고 상태 전환이 창 크기를 강제로 바꾸지 않습니다.
   - 다중 턴 timeline, 3행 composer, Shift+Enter 줄바꿈, 스트리밍 취소, CommonMark/code block 복사를 제공합니다.
2. **엄격한 읽기 전용 불변조건 (Strict Read-Only Guarantee)**
   - 저장소 파일을 수정하거나 셸/빌드/Git 변경 명령을 실행하지 않습니다.
   - 상위 경로 탈출 및 악성 프롬프트 인젝션 텍스트는 순수 데이터로만 안전하게 격리됩니다.
3. **다중 AI 프로바이더 지원 (Multi-Provider Support)**
   - **Google Gemini (AI Studio):** 계정에 활성화된 생성 모델의 REST/SSE 스트리밍
   - **OpenRouter:** 현재 API 키로 접근 가능한 멀티 모델 카탈로그 연동
   - **OpenAI & Local Compatible:** OpenAI 공식 API와 호환 서버 또는 내장 로컬 런타임
   - 모델 ID는 코드 프리셋이 아니라 현재 API 키/로컬 런타임이 반환한 목록에서 동적으로 선택합니다.
   - `API 확인 및 모델 불러오기 → 선택 모델 호환성 확인 → 활성화`를 통과한 동일 프로필만 프로그램 AI로 사용합니다.
   - API key 저장을 선택하면 Windows Credential Manager/macOS Keychain/Linux Secret Service에 보관하고 SQLite에는 profile-scoped reference만 기록합니다.
   - 호환성 확인은 모델 ID 추정 없이 실제 native `repo_status` tool probe를 실행하며, 실패한 모델은 chat-only로 정직하게 활성화합니다.
   - 내장 로컬은 항상 선택 가능하지만 실행 엔진 또는 설치 모델이 없으면 이유를 표시하고 활성화를 차단합니다.
4. **오프라인 로컬 분석 워크플로 (Offline Workflows)**
   - 외부 AI 없이도 `/onboard`, `/structure`, `/conflicts`, `/where` 명령으로 프로젝트 구조, 매니페스트, 권위 문서를 로컬에서 즉시 분석
5. **Prompt Profile과 응답 스타일**
   - 읽기 전용 Kernel, 4개 System 숙련도 preset, 3개 Persona와 사용자 편집을 지원합니다.
   - Edit/Reset/과거 version 선택은 draft만 바꾸며 Apply한 atomic revision이 다음 턴부터 적용됩니다.
6. **Egress Consent & 프라이버시 쉴드**
   - 클라우드 API 전송 전 `.env`, 키 파일, 인증서 등 민감정보 파일 자동 배제 및 1회 승인 시트 제공
   - 일반 Advisor 답변은 모델의 자유 Markdown을 보존하고, 구조화 claim/evidence는 명시적 Audit validator에서만 사용합니다.
   - Cloud Inferred/Proposed/Conflict와 ConflictItem도 유효하고 중복 없는 evidence를 요구
   - 저장소가 `STALE`이면 신규 분석을 차단하고 재인덱싱 전 live file과 이전 hash 혼합을 허용하지 않음
   - 파일 watcher의 event-loss/unknown 신호와 ignore 규칙 변경은 즉시 STALE로 실패 폐쇄
   - OpenAI 호환/Gemini tool result는 provider가 직렬화한 정확한 body와 durable receipt가 일치해야만 송신되며 redirect에는 재전송하지 않습니다.
7. **Grounding 및 Audit UI**
   - Advisor 답변은 자유 Markdown을 유지하고, 별도 Grounding drawer에서 tool call, receipt, SourceRef 경로·줄·redacted excerpt를 확인합니다.
   - Audit mode는 Ready 저장소와 tool-capable 모델에서만 선택되며 validated AnswerBundle만 구조화 UI와 SQLite에 저장·복원합니다.

---

## ⌨️ 단축키 일람 (Shortcuts)

| 단축키 | 기능 |
|---|---|
| `Alt + Space` / `Ctrl + Alt + M` | OS 전역 표시·포커스 요청 |
| `Esc` | 스트리밍·인덱싱 취소 또는 설정 닫기 |
| `Ctrl + Q` | 진행 작업을 취소하고 Code Mentat 종료 |
| `Enter` | 메시지 전송 |
| `Shift + Enter` | composer 줄바꿈 |

---

## 📦 빠른 시작 (Quick Start)

### 필수 요구사항
- [Rust](https://www.rust-lang.org/) (1.88+ 필요)

### 빌드 및 실행
```bash
# 워크스페이스 전체 단위 테스트 실행
cargo test --workspace

# 컴팩트 위젯 애플리케이션 실행
cargo run -p mentat-app
```

### 메뉴형/명령형 멀티 플랫폼 빌드

```bash
# 대화형 메뉴
cargo mentat-build

# 현재 OS release + 품질 게이트
cargo mentat-build build --platform current --profile release --gates

# 명시적 타깃 dry-run
cargo mentat-build build --platform macos --arch aarch64 --profile release --dry-run
```

Windows는 `./scripts/build.ps1`, Linux/macOS는 `sh ./scripts/build.sh`로 같은 메뉴와 인자를 사용할 수 있습니다. 명시적 크로스 타깃은 해당 Rust target과 linker/SDK가 준비되어야 하며, 준비되지 않으면 다른 산출물로 대체하지 않습니다.

### UI 문제 해결 / UI Troubleshooting / UI トラブルシューティング / UI 疑難排解 / UI 故障排除

- **한국어:** 내장 한글 폴백과 흰색 고대비 테마로 네모 글리프와 흐린 글자를 방지합니다. 창은 자유롭게 조절되며 상태 변경 후에도 크기를 유지하고 `×`로 닫을 수 있습니다.
- **English:** The embedded Korean fallback and high-contrast white theme prevent missing-glyph squares and faint text. The window is freely resizable, keeps its size across state changes, and closes with `×`.
- **日本語:** 内蔵韓国語フォールバックと高コントラスト白テーマで文字化けを防ぎます。ウィンドウは自由にリサイズでき、状態変更後もサイズを保ち、`×`で閉じられます。
- **繁體中文:** 內嵌韓文字型與高對比白色主題可避免方框字元。視窗可自由調整大小、狀態切換後保持尺寸，並可用 `×` 關閉。
- **简体中文:** 内置韩文字体与高对比白色主题可避免方框字符。窗口可自由调整大小、状态切换后保持尺寸，并可用 `×` 关闭。

### Gemini 활성화 문제 해결 / Gemini Activation Troubleshooting / Gemini 有効化 / Gemini 啟用 / Gemini 激活

- **한국어:** 모델 목록 조회 후 호환성 확인은 thinking 응답의 모든 candidate/part를 검사합니다. text가 없으면 `finishReason`과 thinking token 진단을 표시하며, 정상 검증 후에만 활성화됩니다.
- **English:** Model verification scans every candidate/part in thinking responses. If visible text is absent, the UI reports `finishReason` and thinking-token diagnostics; activation remains fail-closed.
- **日本語:** モデル検証では thinking 応答の全 candidate/part を確認します。表示テキストがない場合は `finishReason` と thinking token 診断を表示し、検証成功後のみ有効化します。
- **繁體中文:** 模型驗證會檢查 thinking 回應中的所有 candidate/part。若沒有可見文字，介面會顯示 `finishReason` 與 thinking token 診斷，僅在驗證成功後啟用。
- **简体中文:** 模型验证会检查 thinking 响应中的所有 candidate/part。若没有可见文本，界面会显示 `finishReason` 与 thinking token 诊断，仅在验证成功后激活。

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
