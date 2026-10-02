# 저장소 멘토 사용성·실행 루프 개선

- 작성일: 2026-10-02
- 근거: 사용자의 현재 목표, `chat_app.rs`, `agent_loop.rs`, production inference adapters
- 상태: 두 사용자 목표의 Windows 실제 실행 검증 통과 (2026-10-02)

## 제품 계약

Code Mentat의 기본 흐름은 **질의 → 지정 저장소에서 탐색/읽기 → 조언 답변 → 같은 대화의 맥락 유지**다. 자연어 Markdown을 기본 답변으로 사용한다. 모델의 실행 권한은 읽기 전용 도구 6개로 한정하고 저장소 쓰기, 셸 실행, Git 변경은 제공하지 않는다. API 키 보호, 저장소 루트 경계, 민감 파일 제외, 전송 동의와 취소·시간·바이트 한도는 유지한다.

기존 baseline·CR의 상세 관리 기능은 역사적 요구사항이다. 사용자의 최신 요청에 따라 기본 화면에서는 Audit 분류, prompt revision, receipt ID 등 내부 관리 정보를 숨기고 필요할 때만 상세 화면에서 확인한다. 새 관리 기능이나 자동 compaction 시스템은 추가하지 않는다.

## 화면 설계

```text
창 제목 / 새 대화 / 설정 / 항상 위 / 닫기
AI 상태와 연결·변경 / 저장소 상태와 연결·다시 읽기
─────────────────────────────────────────
온보딩(준비 단계 + 질문 예시) 또는 대화 목록
질문과 답변을 충분한 폭으로 표시, 답변 복사/근거
─────────────────────────────────────────
질문 입력 (여러 줄, 현재 전송 단축키 안내)
탐색·응답 상태 / 중지 / 전송
```

- 기본 새 창: 560×760pt, 최소 360×480pt. 저장된 사용자 창 크기는 최소 크기 범위 내에서 유지한다. OS 제목/resize 경계를 사용한다.
- 흰 바탕, body 16pt, 의미 있는 텍스트 버튼과 상태 레이블을 사용한다. 모델·저장소 긴 이름은 줄바꿈/ellipsis하고 대화 폭을 침범하지 않는다.
- AI 미활성/저장소 미연결 시 각 단계의 실행 버튼과 질문 예시를 표시한다. 질문 예시는 입력란만 채우고 자동 전송하지 않는다.
- 이전에 활성화해 저장된 AI와 복원된 키가 있으면 시작 시 모델 목록과 호환성을 다시 확인하고 자동으로 연결한다. 목록에 없거나 실패하면 연결하지 않고 설정에서 원인을 표시한다. 확인 도중 사용자가 공급자/모델을 편집하면 자동 연결 결과를 폐기한다. 모델 목록은 이름 검색을 제공한다.
- 입력란은 사용 가능한 폭 전체를 쓰며 기본 4줄、최대 높이 안에서 스크롤한다. 전송 후 입력 focus를 복원한다. 설정은 명시적 대화 복귀 버튼을 제공한다.
- 키보드 전송은 해당 Enter 이벤트의 modifier와 IME를 판정한다. 같은 프레임에서 Shift/Ctrl이 해제되어도 잘못 전송하지 않는다. OS 닫기와 Ctrl+Q도 기존 작업 취소·설정 보존·편집 확인 경로를 사용한다.
- 저장소 연결·재인덱싱 상태는 대화 스크롤 밖에 둔다. 동의와 제한 상태는 사용자가 실제 전송 가능 여부를 이해할 수 있는 문장으로 표시한다.
- 폴더 선택은 worker에서 실행해 UI 입력을 막지 않는다. 선택 중 메인 창의 always-on-top을 해제하고 종료 시 사용자 핀 상태를 복원한다. 선택 결과도 conversation/generation에 결속한다.
- Markdown 강조·inline code·링크를 같은 문단 흐름에서 렌더링한다. 이미지/HTML 자동 실행·외부 fetch는 제공하지 않는다.

## 구현 순서와 검증

실제 Gemini 3.8 호출에서 텍스트 생성은 통과하지만 function declaration 요청이 HTTP 400으로 거부되는 것을 확인했다. Gemini의 OpenAPI subset `parameters`에 일반 JSON Schema를 그대로 전달한 경로를 `parametersJsonSchema`로 변경한다. capability probe 오류는 chat-only로 숨기지 않고 반환한다. 이어지는 function calling round에서 provider의 원본 call ID와 thought signature를 inference 내부 메시지에 보존하며, 도구 실행 인자·저장소 권한은 기존 typed parser로 독립 검증한다. opaque metadata는 사용자 답변/근거/DB 기록에 저장하지 않고 해당 요청 history에서만 전달한다.

1. 화면 구조와 문구, 창·타이포그래피, onboarding·composer를 정리한다.
2. Markdown inline 레이아웃을 수정하고 긴 한국어·ASCII·코드·링크 회귀를 실행한다.
3. loopback HTTP 서버와 production adapter + AgentLoop + RepositoryToolGateway로 질의→도구→답변→후속 질의를 실제 실행한다. 요청에 이전 대화가 포함되는지, read-only 도구 결과가 provider에 돌아가는지 확인한다. root escape·민감 파일·취소·잘못된 도구는 거부 회귀로 검증한다.
4. Windows 바이너리를 빌드·실행하여 onboarding, 설정 복귀, 입력, Markdown 답변, 근거 조작·resize를 직접 확인한다. 실제 cloud model 실행은 사용 가능한 사용자 credential이 있어야 완료 증거로 인정한다. loopback을 실제 cloud model 추론으로 표기하지 않는다.
5. 실제 실행 근거와 미확인 항목을 이 문서에 갱신한다. 모든 목표가 실제 동작으로 확인되기 전 목표 완료를 선언하지 않는다.

## 실행 증거 (2026-10-02)

- Markdown 단일 문장 회귀: 수정 전 105pt로 여러 줄에 분리되어 실패, 수정 후 40pt 미만 통과.
- 실제 API: 사용자가 지정한 `gemini-3.8-flash`를 모델 목록에서 확인하고 생성·도구 호환성 프로브 통과.
- `mentat-app --mentor-smoke --model gemini-3.8-flash`: first turn tools=3 / sources=1 / receipts=6, second turn tools=0이며 앞선 실제 랜덤 숫자를 유지한 답변 통과. root escape와 민감 파일 거부, 파일 hash 불변, 4개 메시지 terminal 저장·복원 통과.
- 로컬 증거 DB: `target/mentor-smoke-1a2bb2fb-6e17-43d5-9f3d-95f03cd7b60e/harness.db`. 사용자 API 키는 포함하지 않는다.
- Gemini 실패 재현: 기존 function declaration HTTP400, parametersJsonSchema 수정 후 probe 통과, 이어지는 tool round HTTP400. 모델의 원본 functionCall part/thoughtSignature 보존 후 전체 2-turn 루프 통과.
- provider signature/ID/JSON Schema/debug 비노출 회귀 통과. workspace tests, strict Clippy, fmt 및 Windows optimized release build 통과.
- Windows UI 실행: 360pt 저장 창에서 한글·온보딩·설정 진입/복귀·키 mask·44개 모델 목록·3.8 Flash 선택·호환성 확인·활성화 검증. Enter 전송 후 스트리밍/본문/목록/굵은 글씨가 실제로 표시됨. 후속 질문에서 사용자가 앞서 지정한 '청록멘토'를 그대로 답함. OS 창 최대화도 정상 조작됨.
- 최종 optimized release smoke 재실행: `target/mentor-smoke-25b4edc3-783f-4c83-803f-9bfb9a4b7c88`에서 첫 질문 도구 3회/근거 1개/receipt 6개와 두 번째 질문 맥락 유지, 저장·복원, 경계·불변성 모두 PASS.
- 실제 release UI 재시작에서 Gemini 3.8 자동 연결 확인. 비동기 폴더 선택 창이 핀 창 앞에 정상 표시되고 전용 fixture 선택 결과 Ready/1 file로 연결됨. UI 입력에서 실제 도구 3회/근거 1개를 거쳐 파일의 숫자 `620605`를 답하고, 코드 근거를 클릭하면 `engine.rs:1–3` 원문을 확인할 수 있음.
- 실제 답변의 `details/summary/strong` HTML 원문 표시 문제를 실패 회귀로 재현해 inert 참고 문구로 변환. 외부 fetch/HTML 실행은 추가하지 않음. app 40개 테스트 및 strict Clippy 통과.

## 완료 판정 근거

| 사용자 요구 | 현재 상태의 증거 |
|---|---|
| 메시지 가시성과 대화 입력 개선 | 실제 360pt 창의 한글·본문·목록·inline code·전체 폭 입력·Enter 전송·스트리밍 확인. HTML 참고 문구는 태그 없이 복원 표시됨 |
| 옵션·온보딩을 간단하게 조작 | 실제 공급자 모델 목록/선택/검증/활성화, 3.8 이름 검색 결과, AI 자동 재연결, 파일 선택 창 가림 해소, 근거 원문 열기, 창 확대 및 Ctrl+Q 정상 종료 확인 |
| API AI의 안전한 로컬 탐색/읽기 | 실제 Gemini 3.8 Flash production 하네스 및 UI에서 tool 3회/source 1개/receipt 6개로 `engine.rs`를 읽음. escape·민감 파일 거부와 파일 hash 불변성 확인 |
| 질의→탐색→답변→맥락 유지 | 하네스와 UI 두 번째 질문 모두 앞선 실제 숫자를 유지. UI는 `620605`를 재답변. 재실행 후 저장된 답변과 근거 복원 확인 |
| 불필요한 기능·관리 줄이기 | 기본 Advisor 대화와 읽기 도구만 노출. 상세 prompt 관리 접기, Audit 선택을 기본 대화에서 제거. 새 compaction/agent framework 도입 없음 |

현재 상태는 Windows 사용자 시나리오의 완료 판정이다. 기존 전체 보안 감사나 비Windows 배포 검증을 새로 PASS로 바꾸지 않는다. 검증 당시 작업 트리에서 전체 workspace tests, strict Clippy, fmt, optimized release build를 통과했다. 폴더 선택/스캔 중 입력 draft는 보존되고 turn을 시작하지 않는 회귀도 추가했다. 이후 Git 커밋으로 해당 변경과 이 실행 근거를 함께 고정한다.

- 종료 회귀: native Close 이벤트를 편집 중인 경우에만 보류하도록 수정. clean close의 작업 취소/CancelClose 부재와 dirty close 보류를 테스트했고, 최종 GUI Ctrl+Q의 process 종료와 실행 세션 exit 0을 확인했다.
- 최종 모델 검색: 목록 바깥에서 '3.8' 입력 후 해당 모델 3개만 표시되는 것을 직접 확인했다.
- 키 파일 경로: target/release를 작업 디렉터리로 한 `--mentor-check`에서도 프로젝트 루트의 키를 찾아 `credential_present=true`로 확인했다. 키 값은 출력하지 않았다.
- 기존 감사 manifest의 원본 8개 SHA-256 변경 0건, `.env.local`은 Git ignore 확인.

## 개발용 키 파일 형식

사용자가 승인한 개발 파일은 `C:\LocalDev\rust\CodeMentat\.env.local`이다. API 키만 한 줄 또는 `GEMINI_API_KEY=값`으로 저장한다. Gemini native credential 복원이 불가능할 때 시작 작업 디렉터리의 파일을 읽으며, Cargo의 target/debug 또는 target/release에서 실행한 개발 바이너리는 Cargo.toml이 있는 프로젝트 루트도 확인한다. 탐색기 실행에서 작업 디렉터리가 달라도 이 개발 파일을 찾을 수 있다. 임의의 상위 디렉터리를 계속 탐색하지 않는다. 모델 이름은 코드에 기본값으로 고정하지 않고 API 목록에서 선택한다. 파일은 Git ignore 대상이며 gateway에서도 민감 파일로 제외된다. 이 파일 자체는 평문이므로 로컬 파일 접근자가 읽을 수 있다. 배포 기본 저장 방식은 기존 OS credential store다.
