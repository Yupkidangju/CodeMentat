# CR-UX-001 보안 및 프라이버시 계약

- **상태:** `APPROVED — IMPLEMENTATION ACTIVE`
- **보호 대상:** 읽기 전용 저장소, 사용자 대화/프롬프트, API 자격 증명, 외부 전송 범위, 출처 무결성
- **신뢰 모델:** 사용자와 앱의 immutable capability는 신뢰, provider 응답·저장소 콘텐츠·사용자 편집 prompt는 권한 상승 입력으로 신뢰하지 않음

## 1. 보안 목표

1. 자유 대화와 agent loop를 추가해도 저장소 쓰기·실행 capability는 0개다.
2. 사용자 동의 전 저장소 원문이 외부 provider로 전송되지 않는다.
3. Tool result와 SourceRef는 현재 snapshot/live hash에 결속된다.
4. 사용자 prompt와 저장소 prompt injection은 Kernel/capability를 변경하지 못한다.
5. API 키·비밀·원문 코드·대화 원문은 기본 로그와 receipt에 남지 않는다.
6. 취소·timeout·loop/budget 초과가 tool/network/stream 전체에 전파된다.

## 2. 권한 계층

```text
1. Compiled capability boundary (최상위)
   - read-only repository ports
   - no shell/process/write/delete/rename/patch
2. Immutable Kernel Contract
3. Editable System Prompt
4. Editable Persona Prompt
5. Conversation messages
6. UNTRUSTED_REPOSITORY_CONTENT / provider output (최하위)
```

프롬프트 우선순위는 방어 보조 수단이다. 실제 hard boundary는 Rust 타입과 제공하지 않는 capability로 강제한다.

## 3. 위협 및 통제표

| 위협 | 등급 | Hard boundary | 보조 통제 | 필수 테스트 |
|---|---|---|---|---|
| path traversal/symlink escape | 치명 | canonical root + relative path only | omission | traversal/reparse fixtures |
| write/exec tool 조작 | 치명 | tool enum/port에 capability 부재 | Kernel 경고 | compile/API surface 검색 |
| repository prompt injection | 높음 | repository content 권한 최하위 | boundary label | adversarial README fixture |
| editable prompt 권한 상승 | 높음 | immutable Kernel + tool whitelist | settings warning | “write/exec” prompt fixture |
| 동의 전 egress | 치명 | consent state check before body construction | UI scope display | network receiver count 0 |
| receipt TOCTOU | 치명 | canonical digest + consume-time live hash/profile check | generation invalidation | tamper matrix |
| secret/high entropy 유출 | 치명 | redaction/exclusion before receipt | masked preview | known/unknown token fixtures |
| provider tool schema 조작 | 높음 | strict argument enum/parser/bounds | recovery 1회 | malformed/oversized schema |
| 무한 tool loop | 높음 | 8 rounds/24 calls/5분 + fingerprint | partial evidence notice | repeated call fixture |
| stale snapshot 혼합 | 높음 | snapshot ID + live hash equality | STALE UI | edit-during-read fixture |
| 가짜 citation | 높음 | SourceRef gateway ownership | invalid source warning | missing path/range/hash fixture |
| prompt/DB 손상 | 중간 | transactional migration + no delete | backup/quarantine | corrupt DB fixture |
| 대화 삭제 불완전 | 중간 | FK cascade transaction | deletion result | reopen-after-delete test |
| 민감 로그 | 높음 | structured ID-only log schema | redacted Debug | log capture secret fixture |
| Markdown resource fetch | 높음 | parser-only renderer, image/html resource load 없음 | http/https link 명시적 click | no-fetch/scheme fixture |

## 4. RepositoryConsentScope

```rust
enum RepositoryConsentScope {
    None,
    RequestOnce {
        consent_id: Uuid,
        conversation_id: Uuid,
        turn_id: Uuid,
        repository_id: Uuid,
        snapshot_id: Uuid,
        provider_binding: ProviderBinding,
        runtime_capability_nonce: [u8; 32],
    },
    RepositorySession {
        consent_id: Uuid,
        conversation_id: Uuid,
        repository_id: Uuid,
        snapshot_id: Uuid,
        provider_binding: ProviderBinding,
        runtime_capability_nonce: [u8; 32],
        granted_at: DateTime<Utc>,
    },
    Revoked { consent_id: Uuid, revoked_at: DateTime<Utc> },
}

struct ProviderBinding {
    profile_id: Uuid,
    provider: ProviderKind,
    endpoint_identity: String,
    model_id: String,
    target_digest: String,
}
```

- `RequestOnce`는 한 turn 안의 bounded tool batch 전체에 유효하되 각 batch는 별도 receipt를 요구하고 다음 turn에는 재사용하지 않는다.
- `RepositorySession`은 같은 conversation/repository/snapshot과 현재 앱 실행 수명에서만 유효하다.
- repository/profile/model/snapshot 변경과 명시적 revoke는 즉시 `None`과 동등하게 처리한다.
- consent capability는 앱 재시작에서 복원하지 않는다. DB에는 감사 metadata만 남긴다.
- 활성 scope는 process마다 무작위 256-bit `runtime_capability_nonce`를 추가로 요구하며 이 nonce는 직렬화하거나 DB에 저장하지 않는다.
- 일반 대화의 사용자 메시지는 선택 provider에 전송될 수 있음을 모델 활성화 UI에서 고지한다. repository consent는 저장소 원문 추가 전송을 별도 통제한다.

## 5. ToolEgressReceipt

```rust
struct ToolEgressReceipt {
    seal_algorithm_version: String,
    receipt_id: Uuid,
    grounding_trace_id: Uuid,
    consent_id: Uuid,
    conversation_id: Uuid,
    turn_id: Uuid,
    tool_call_id: Uuid,
    repository_id: Uuid,
    snapshot_id: Uuid,
    refs: Vec<CanonicalToolRef>,
    provider_binding: ProviderBinding,
    redaction_count: u32,
    semantic_tool_payload_digest: String,
    exact_provider_body_digest: String,
    canonical_digest: String,
    approved_at: DateTime<Utc>,
    status: ToolEgressStatus,
}

enum ToolEgressStatus { Prepared, Sent, Failed, OutcomeUnknown }

struct CanonicalToolRef {
    relative_path: String,
    line_start: usize,
    line_end: usize,
    source_file_sha256: String,
    redacted_content_sha256: String,
    redacted_bytes: u32,
}
```

Canonical encoding은 `CM_TOOL_EGRESS_V1` length-prefix와 정렬된 ref를 사용한다. endpoint identity는 scheme/host/port와 정규화된 base path를 포함하고 query/userinfo/secret은 제외한다. API key, header value, 원문 전체는 receipt에 저장하지 않는다.

Receipt는 transport 호출 전에 trace와 함께 `Prepared`로 durable 저장한다. 저장 실패 시 transport 호출은 0건이다. canonical digest는 immutable fields만 포함하고 status/attempt timestamps는 제외한다. 전송 성공/실패 후 expected-status CAS로 `Sent`/`Failed`만 갱신하고, process crash 등 결과를 확정할 수 없으면 다음 시작에서 `Prepared → OutcomeUnknown`으로 전환한다.

## 6. 동적 전송 순서

```text
GroundingTrace/turn record prepared
→ Tool call validated
→ repository live read
→ SourceRef/hash 생성
→ secret scan/redaction
→ consent scope 검증
→ canonical packet/Prepared receipt durable 저장
→ consume-time snapshot/hash/profile 재검증
→ provider 전송
→ receipt status를 Sent/Failed/OutcomeUnknown으로 갱신
```

어느 단계든 실패하면 전송하지 않는다. 이전 batch나 이전 generation으로 fallback하지 않는다.

`SourceRef.content_hash`는 scan/live file body의 실제 SHA-256이다. path/range/excerpt 식별 hash와 redacted outbound payload digest는 별도 필드이며 서로 대체하지 않는다.

## 7. Prompt 편집 위협 모델

- Kernel은 바이너리 리소스/코드 상수로 제공하고 DB에서 대체하지 않는다.
- System/Persona에는 자유 텍스트를 허용하지만 tool capability를 추가할 수 없다.
- Effective Prompt preview는 API key, secret ref, 절대 경로, hidden planner schema 원문을 표시하지 않는다.
- factory reset은 내장 리소스를 draft에 로드할 뿐 즉시 저장하지 않는다.
- 적용 시 변경 layer의 immutable content version과 두 layer를 묶는 profile revision을 expected-active CAS transaction으로 append한다. layer별 active/turn 참조 version과 latest 5 unreferenced를 보존한다.
- prompt 길이 기본 상한은 각 32KiB UTF-8이며 초과 입력은 저장/전송 전에 거부한다.

## 8. Provider 및 Agent 경계

- capability는 실제 probe 결과이며 모델 ID allowlist가 아니다.
- `CHAT_CAPABLE`만 true인 모델에는 repository tools를 제공하지 않는다.
- native tool response와 emulated planner JSON은 untrusted schema로 파싱한다.
- schema 복구 1회 이후 실패는 `AGENT_TOOL_SCHEMA_INVALID` terminal error다.
- provider redirect는 기존 cross-origin zero-leak 정책을 유지한다.
- HTTP response/body는 기존 size limit과 timeout을 유지한다.

## 9. 저장·로그·삭제

| 데이터 | 저장 위치 | 로그 허용 | 삭제 |
|---|---|---|---|
| API key | OS native credential store 또는 session memory | 값 금지, profile-scoped reference만 | native put/get/delete + DB byte scan |
| Prompt profile/version | AppData SQLite | profile/version ID만 | profile transaction |
| Conversation/Message | AppData SQLite | IDs/status/length만 | conversation cascade |
| GroundingTrace | AppData SQLite | trace/call/snapshot IDs만 | conversation cascade |
| Source excerpt | 정책상 bounded/redacted | 원문 금지 | trace cascade |
| Receipt | AppData SQLite | digest/IDs만 | trace cascade |
| Validated Audit result | AppData SQLite | turn/result ID와 count만 | conversation cascade |

기본 로그 필드: timestamp, request_id, conversation_id, turn_id, tool_call_id, snapshot_id, stage, duration_ms, error_code, byte/count metrics. 사용자 대화·prompt·코드 원문·API key·절대 경로는 금지한다.

- 대화 저장 기본값은 ON이며 AppData 로컬 저장임을 첫 설정 화면에서 고지한다.
- 자동 보존 만료는 두지 않으며 사용자가 삭제할 때까지 유지한다. 저장 OFF 전환은 이후 신규 message persistence를 중단하고 현재 대화는 메모리에서만 계속한다. 기존 저장 대화는 자동 삭제하지 않고 별도 삭제 CTA를 요구한다.
- 저장 OFF에서는 일반 cloud chat과 local read-only advisor를 메모리에서 사용할 수 있지만 durable turn/trace/receipt를 만들 수 없으므로 external provider에 repository tool result를 보내는 기능과 cloud Audit Mode를 비활성화한다.
- Grounding excerpt는 redaction 후 최대 512 Unicode chars만 저장하고 전체 tool result 원문은 저장하지 않는다.
- conversation 삭제는 message/trace/source/receipt를 같은 transaction으로 삭제한다.
- conversation 삭제 시작 전 consent를 revoke하고 in-flight tool/network/stream을 cancel한다. 최대 2초 안에 terminal 상태가 확인되지 않으면 delete transaction을 시작하지 않고 `CONVERSATION_DELETE_FAILED`를 반환한다.
- custom prompt privacy 삭제는 factory profile을 새 active revision으로 만든 뒤 custom content versions를 제거한다. 기존 profile revision/turn은 nullable content FK, checksum, `content_deleted=true`만 남기고 원문이 삭제됐음을 표시한다.
- 삭제 대상이 앱 생성 migration backup/quarantine에 포함될 수 있으면 해당 backup/quarantine 전체를 함께 제거한다. 앱 수준 복구 불가를 보장하지만 OS/디스크 스냅샷의 물리적 삭제까지 보장한다고 표현하지 않는다.
- 일반 AppData에는 별도 at-rest encryption을 주장하지 않는다. API key 저장을 선택하면 Windows Credential Manager/macOS Keychain/Linux Secret Service를 사용하고 SQLite에는 원문·암호문 없이 `provider:<profile_uuid>` reference만 둔다. native store가 없거나 잠기면 file fallback 없이 session-only/재입력으로 실패 폐쇄한다.
- `audit_turn_results`에는 validated AnswerBundle만 저장하고 `raw_model_response`는 DB에 저장하지 않는다. evidence excerpt는 같은 512-char redaction 한도를 적용하며 invalid evidence/provider diagnostic raw body는 메모리에서 폐기한다.

## 10. 마이그레이션 및 복구

1. DB schema version을 확인한다.
2. SQLite online backup/checkpoint로 DB/WAL 일관성이 있는 `<db>.pre-cr-ux-001-<UTC>.sqlite`를 만들고 AppData `migration-artifacts.json`에 category/time/path를 기록한다.
3. 각 migration version을 별도 `BEGIN IMMEDIATE` transaction으로 수행한다.
4. 성공 시 schema version을 갱신하고 backup 정책에 따라 보존한다.
5. 실패 시 transaction rollback, 원본을 파괴하지 않고 quarantine 상태로 기록한다.
6. 후속 version이 실패하면 앞서 commit된 partial DB를 `<db>.quarantine-<UTC>/`에 DB/WAL/SHM 묶음으로 이동하고 원래 active DB 경로에는 새 DB를 생성해 factory prompts로 시작한다. pre-migration backup은 덮어쓰지 않고 사용자에게 복구 상태를 표시한다.

자동 `DROP`, 무조건 `INSERT OR REPLACE`를 이용한 conversation/prompt overwrite, 복구 실패 DB 삭제는 금지한다.

privacy wipe가 요청되면 live DB transaction이 성공한 뒤 앱이 만든 migration backup/WAL/SHM/quarantine의 관련 보존본도 정리한다. 어느 단계든 실패하면 UI를 삭제 완료로 표시하지 않는다.

storage open/migration/decode 오류는 `.ok()`, `flatten()`, 임의 UUID/현재 시각/`Ready` fallback으로 숨기지 않는다. DB unavailable에서는 factory prompt 기반 ephemeral chat만 허용하고 UI에 `저장되지 않음`을 표시한다. repository egress는 durable receipt store가 없으면 차단한다.

v0.2는 persistent diagnostic log 파일과 자동 crash upload를 기본 생성하지 않는다. stderr/console structured tracing에도 custom redacted Debug를 사용하고 Prompt/ChatMessage/SourceRef excerpt/provider raw event/error chain/절대 경로를 직접 출력하지 않는다. repository path는 stable path token으로 바꾸며 향후 opt-in log가 생기면 privacy wipe 범위에 포함한다.

## 11. 보안 검증 게이트

- repository tool 공개 API에 write/process 변형 0개
- 동의 전 loopback receiver repository bytes 0
- conversation 저장 OFF/ephemeral mode에서 external repository tool result 전송 0
- consent/profile/model/snapshot/ref/payload tamper 전부 fail-closed
- prompt injection이 tool registry/Kernel digest를 바꾸지 않음
- 취소 후 network read/tool call/delta 0
- stale/live hash mismatch 전송 0
- 대화 삭제 후 재시작 조회 0
- 로그/DB/receipt secret fixture raw match 0
- 기존 canonical egress, redirect, path, watcher, 100k/2GiB regression 유지

## 12. Accepted Risk

기존 `SEC-F007` quick-xml 위험 수용은 별도이며 이 변경으로 확대하지 않는다. Linux release scope가 실제 제품 배포에 포함되는 시점 또는 만료/trigger 발생 시 재감사한다.

## 13. 승인 상태

```text
Security contract: FROZEN FOR REVIEW
Dynamic tool egress implementation: PRODUCTION CONNECTED — RE-AUDIT PENDING

현재 OpenAI 호환/Gemini native tool round는 provider가 만든 최종 JSON bytes를 `ProviderBodyEgressGate`에 넘긴다. 앱 gate는 runtime consent capability, canonical seal, SQLite `Prepared` receipt, exact-body 재검증을 모두 통과한 뒤에만 송신을 허용한다. 응답 수신은 `Sent`, network/cancel의 송신 여부 불명은 `OutcomeUnknown`으로 닫고 redirect 자동 추적은 두 adapter 모두 금지한다. RepositoryToolGateway는 tool content와 SourceRef excerpt를 provider 직렬화 전에 redaction한다.

동일 provider body의 receipt terminal은 전체 ID 집합을 하나의 `BEGIN IMMEDIATE` batch CAS로 갱신한다. 하나라도 `Prepared`가 아니거나 테스트 killpoint가 commit 전에 발생하면 전체 rollback한다. repository-backed completed/Audit terminal도 최종 GroundingTrace/tool/source와 하나의 transaction으로 저장하며, UI는 transaction 성공 전 Completed로 확정하지 않는다.

startup reconciliation은 schema v6 runtime owner lease를 획득한 process만 수행한다. 30초 이내 heartbeat가 있는 owner가 존재하면 DB open 자체를 거부하며 live Prepared를 변경하지 않는다. stale/absent owner takeover 때만 Prepared를 OutcomeUnknown, orphan Pending/Streaming을 `INTERRUPTED_BY_RESTART` Failed로 바꾼다. busy/locked/read-only/permission/I/O/recovery 오류는 corruption이 아니며 quarantine/move/fresh DB 생성을 금지한다.
Implementation approval: GRANTED — 2026-08-19 CR-UX-001 GO
```
