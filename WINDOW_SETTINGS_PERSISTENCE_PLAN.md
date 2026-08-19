# 창 크기·설정 영속성 보완 계획

- **상태:** `IMPLEMENTED — VERIFICATION IN PROGRESS`
- **기준 요청:** 좌우 폭 25% 확대, 세로 높이 10% 확대, 자유 resize 및 종료 후 복원, 전체 설정 round-trip 점검
- **기준일:** 2026-08-19
- **승인:** 2026-08-19 `WINDOW-SETTINGS GO`

## 1. 현재 구현 점검 결과

### 1.1 창 크기

- 승인 전 최초 크기: `250×600` egui logical points
- 현재 최소 크기: `240×360`
- 현재 저장: resize 정지 500ms 후 `ui_preferences`에 upsert
- 현재 종료 저장: `MentatChatApp::drop()`에서 마지막 관측 크기 저장
- 현재 재실행 복원: `main`의 `initial_window_size()`가 AppData SQLite 값을 읽어 최초 viewport에 적용
- 현재 오류 처리: resize/종료 저장 오류를 `let _ = ...`로 버려 사용자가 저장 실패를 알 수 없음
- 현재 clamp: 최소 크기와 고정 상한 `1600×1200`만 적용하며 실제 monitor work area는 반영하지 않음

### 1.2 설정값별 round-trip 상태

| 설정 | 현재 저장 | 현재 재로딩 | 판정 |
|---|---|---|---|
| 창 width/height | O | O | 구현됨, 종료 flush/error 검증 보강 필요 |
| composer submit mode | O | O | DB는 구현됐으나 설정 UI가 없음 |
| always-on-top pin | X | 시작마다 true | 누락 |
| provider 종류/base URL/name/timeout | 활성화 시 O | O | 구현됨 |
| provider 선택 model ID | DB에는 O | 시작 시 의도적으로 clear | 재검증 UX와 복원 계약 미완성 |
| provider capability/활성 상태 | X | 시작 시 Draft | 안전하지만 round-trip 미완성 |
| API key | X | X | OS native credential store 영속화 필요 |
| System/Persona active prompt revision | O | O | 구현됨 |
| System/Persona 원문·factory ref | O | O | 구현됨 |
| Persona selector 표시 | active source와 독립적으로 DefaultAnalyst 시작 | 불완전 | 수정 필요 |
| 적용 전 prompt draft | X | X | 의도된 동작, dirty-close 확인 후 폐기 |
| 최근 conversation/messages | O | O | 구현됨 |
| repository runtime session | metadata만 O | 재실행 시 자동 session 복원 X | 재검증·재인덱싱 정책 필요 |
| settings/drawer 열림 상태 | X | X | transient UI이므로 저장하지 않음 |

## 2. 확정 제안값

### 2.1 새 기본 크기

요청의 “하단폭 10%”는 세로 높이 10% 증가로 해석한다.

```text
기존: 250 × 600
신규: 312.5 × 660
계산: width 250 × 1.25, height 600 × 1.10
```

- egui가 `f32` logical points를 사용하므로 `312.5×660.0`을 그대로 저장한다.
- 자유 resize를 제한하지 않도록 최소 크기는 현재 `240×360`을 유지한다.
- 고정 최대 `1600×1200` 대신 현재 monitor work area 안으로만 clamp한다.

### 2.2 기존 사용자 크기 migration

- preference가 없으면 `312.5×660`으로 시작한다.
- 저장값이 역사적 기본값 `250×600`과 ±1 point 이내이고 layout revision이 구형이면 한 번만 새 기본값으로 올린다.
- `600×800`처럼 사용자가 조절한 값은 그대로 보존한다.
- 이를 위해 `ui_preferences.layout_revision`을 추가하고 현재 revision을 `2`로 둔다.

### 2.3 API key 저장 보안 결정

#### 기각: 자체 암호 알고리즘

- 앱이 재실행 후 사용자 입력 없이 복호화하려면 복호화 key도 바이너리, 설정 파일, machine ID 또는 같은 사용자 영역에서 다시 얻을 수 있어야 한다.
- 암호문과 key 또는 key derivation 재료를 같은 공격자가 얻을 수 있으면 파일 복사·역공학에 대한 보호는 제한적이며 난독화에 가깝다.
- 자체 알고리즘은 기밀성뿐 아니라 nonce 재사용, 무결성, key rotation, downgrade, format validation 문제를 새로 만든다.
- 따라서 자체 cipher/encoding/XOR/machine-ID derivation은 사용하지 않는다.

#### 조건부 가능: 사용자 master password 기반 encrypted vault

- 사용자가 매 실행 시 별도 master password를 입력한다면 표준 KDF와 AEAD를 이용한 로컬 vault는 가능하다.
- 이 경우에도 자체 알고리즘이 아니라 검증된 `Argon2id + XChaCha20-Poly1305` 또는 동급 library를 사용해야 한다.
- master password나 파생 key를 저장하면 보호 효과가 다시 약해지므로 무인 자동 복호화와 양립하지 않는다.
- Code Mentat의 “재실행 후 자동 복원” 요구에는 UX가 맞지 않아 기본안으로 채택하지 않는다.

#### 채택: OS native credential store

| 플랫폼 | 기본 저장소 | 보호 경계 |
|---|---|---|
| Windows | Credential Manager 또는 user-scoped DPAPI | 동일 Windows 사용자 credential과 장치에 결속; machine scope 금지 |
| macOS | Keychain Services | encrypted keychain item과 app/access control |
| Linux | Secret Service compatible keyring | 로그인 session의 collection/item/lock 정책 |

- SQLite에는 API key 원문·암호문을 넣지 않고 `credential_ref`만 저장한다.
- 식별자는 `CodeMentat/provider/<profile_uuid>`처럼 provider profile에 결속한다.
- 저장소가 없거나 잠겨 있으면 encrypted-file fallback을 만들지 않고 session-only로 강등하며 UI에 `다시 입력 필요`를 표시한다.
- key 조회 실패 시 빈 key, 이전 key 또는 다른 profile key로 fallback하지 않는다.
- provider profile 삭제·key 저장 해제 시 native credential item도 명시적으로 삭제한다.
- API key는 request 직전에만 메모리로 가져오고 Debug/log/receipt/SQLite에 포함하지 않으며 사용 후 가능한 범위에서 zeroize한다.
- 같은 사용자 권한으로 이미 실행 중인 malware, process memory dump, 입력 후킹까지 막는 완전한 보호는 아니며 이는 UI에 과대 보장하지 않는다.

## 3. 구현 작업

### Task 1 — 크기 정책과 migration

**범위:** core preference 모델, SQLite migration, app 초기 크기 계산

**수용 기준:**

- fresh install과 역사적 기본값은 `312.5×660`으로 시작한다.
- 사용자 custom size는 upgrade 후에도 ±1 point 이내로 유지된다.
- NaN/무한/0/음수와 monitor 밖 크기는 fail-closed clamp된다.

**예상 파일:**

- `crates/mentat-core/src/models.rs`
- `crates/mentat-storage/src/db.rs`
- `crates/mentat-storage/src/conversation.rs`
- `crates/mentat-app/src/chat_app.rs`
- `crates/mentat-app/src/main.rs`

### Task 2 — 종료 flush와 실제 resize 복원

**범위:** resize debounce, 닫기/Ctrl+Q/정상 종료 공통 flush, 저장 실패 표시

**수용 기준:**

- resize 중에는 500ms debounce를 유지한다.
- `×`, `Ctrl+Q`, 정상 window close 모두 Close command 전에 마지막 크기를 동기 저장한다.
- 저장 실패 시 닫힌 것으로 가장하지 않고 오류를 표시하며 재시도 또는 명시적 종료을 선택한다.
- 상태 전환에서는 계속 `ViewportCommand::InnerSize` 0건이다.

**예상 파일:**

- `crates/mentat-app/src/chat_app.rs`
- app UI tests

### Task 3 — 비밀 제외 설정 inventory와 round-trip 보완

**범위:** 적용된 비밀 아닌 설정의 저장·복원, transient/secret 경계 명시

**수용 기준:**

- `UiPreferences`는 width/height/submit mode/pin/layout revision을 round-trip한다.
- provider 종류/base URL/name/timeout/마지막 선택 model은 복원하되 시작 상태는 `NeedsRevalidation`이며 자동 Active가 아니다.
- active prompt source에서 Persona selector와 System preset 표시를 역산해 재실행 후 UI와 실제 prompt가 일치한다.
- 설정 UI에 submit mode 선택을 추가하고 재실행 후 동일하게 복원한다.
- SQLite에는 API key 대신 native credential reference만 저장한다.
- 미적용 prompt draft와 settings/drawer open state는 저장하지 않는다.

**예상 파일:**

- `crates/mentat-core/src/models.rs`
- `crates/mentat-storage/src/db.rs`
- `crates/mentat-storage/src/conversation.rs`
- `crates/mentat-app/src/provider_setup.rs`
- `crates/mentat-app/src/chat_app.rs`
- `crates/mentat-app/src/widgets/settings_panel.rs`

### Task 4 — Native SecretStore와 API key round-trip

**범위:** platform-neutral secret port와 Windows/macOS/Linux native adapter

**수용 기준:**

- `SecretStore::put/get/delete(profile_id)`만 API key bytes를 취급하고 SQLite는 reference만 저장한다.
- 저장을 선택한 key는 정상 로그아웃/재실행 뒤 같은 provider profile에서만 복원된다.
- native store unavailable/locked/access denied/corrupt 결과는 session-only 또는 재입력으로 실패 폐쇄하며 file fallback은 없다.
- profile/model/base URL 변경이 다른 profile의 key를 재사용하지 않는다.
- key 원문은 Debug, tracing, crash diagnostic, migration backup, test snapshot에 0바이트다.

**예상 파일:**

- `crates/mentat-core/src/ports.rs`
- `crates/mentat-platform/src/lib.rs` 또는 platform별 secret module
- `crates/mentat-app/src/provider_setup.rs`
- `crates/mentat-app/src/widgets/settings_panel.rs`
- provider activation integration tests

### Task 5 — 전체 재실행 검증

**자동 테스트:**

- fresh DB → `312.5×660`
- legacy `250×600/revision 1` → `312.5×660/revision 2`
- custom `600×800/revision 1` → `600×800/revision 2`
- `320×420`, `600×800`, `1000×900` 저장→DB close→reopen exact round-trip
- button close/Ctrl+Q/dirty prompt close별 마지막 size flush
- pin/submit mode/provider non-secret fields/prompt preset/persona 재실행 동등성
- native store test double에서 API key put/get/delete round-trip
- SQLite/backup/quarantine/log/API debug의 API key byte scan 0건
- native store 실패 시 plaintext/encrypted-file fallback 0건
- corrupt/future preference enum·float fail-closed

**실제 Windows smoke:**

1. 새 AppData에서 `312.5×660` 확인
2. `600×800`로 resize 후 `×` 종료→재실행 동일 크기 확인
3. settings/prompt/repository 화면 전환 후 크기 불변 확인
4. pin과 submit mode 변경→재실행 복원 확인
5. provider/model/key 저장 후 재실행 시 model은 `재검증 필요`, API key는 native store에서만 복원되는지 확인
6. key 저장 해제와 profile 삭제 후 native credential item이 없어지는지 확인

## 4. 구현 결과

- 기본 크기 `312.5×660`, 최소 `240×360`, layout revision 2 적용
- SQLite v5에서 역사적 `250×600`만 1회 확대하고 custom size 보존
- width/height/submit mode/pin/layout revision round-trip 구현
- 마지막 선택 model ID를 복원하되 catalog/생성 검증 전 Active 금지
- Prompt active revision에서 Persona factory/custom 표시 상태 복원
- Windows Credential Manager/macOS Keychain/Linux Secret Service 기반 `SecretStore` 구현
- API key 원문은 native store, SQLite에는 `provider:<profile_uuid>` reference만 저장
- native store unavailable/missing이면 자동 활성화 없이 재입력 요구
- 닫기/Ctrl+Q 전에 마지막 창·입력·핀 설정 동기 저장

## 5. 완료 게이트

```text
Planning: APPROVED
Implementation: COMPLETE
Verification: PASS WITH RUNTIME LIMITATION — custom resize drag automation only 미실행
```

최종 workspace/release/security/runtime gate 결과는 `IMPLEMENTATION_SUMMARY.md`에 기록한다.
