# 멀티 감사 2 수정 및 검증 장부

- 기준: `docs/multi_audit/2/final_audit_report_2.md`, HEAD `0dd109b`
- 시작일: 2026-09-08
- 상태: 수정 반영 및 구현자 재검증 완료, 독립 재감사 대기/HOLD
- 봉인된 원본/보완/manifest는 수정하지 않는다.

## 실행 순서와 계약

1. FIN-001/002: 동의 철회는 공유 cancellation으로 실행 중 gate에 전파한다. 모든 stream/outcome은 conversation/turn ID에 결속한다. submit 진입점은 활성 turn이 있으면 무조건 거부한다. 새 대화는 이전 작업 취소·terminal 저장·scan 취소 후 repository를 해제한다.
2. FIN-005/006/009: 민감 파일은 content/search뿐 아니라 path/metadata catalog에서도 제외한다. 파일은 열린 handle의 대상과 metadata/read/hash를 일치시킨다. privacy 삭제는 앱 생성 보존본 삭제까지 성공해야 완료다.
3. FIN-003/004/010/012: watcher 변화와 disconnect는 gateway를 stale로 만든다. scan 오류는 omission이다. Audit 본문은 검증 claim에서 합성한다. persistence 실패는 명시적 상태로 반환한다.
4. FIN-007/008/011: 결과 전체 JSON을 64KiB/call, 256KiB/turn로 계산한다. SSE 입력은 round 4MiB, line 1MiB, tool arguments 64KiB/24 calls로 제한한다. authorize 이후 drop은 RAII로 receipt OutcomeUnknown을 기록한다.
5. FIN-013~018: delta flush, round text, prompt reset, Markdown, keyboard, 책임 문서 정합성을 검증한다.

## 검증 기준

각 수정은 해당 정상/거부/지연/취소 회귀를 실행한다. 최종 workspace tests, fmt, strict Clippy, release build와 검증 한계를 기록한다. 실계정 provider 비용 발생 호출과 과거 미완료 기능을 완료로 주장하지 않는다.

## finding 상태

모든 finding은 기준 HEAD의 실제 분기와 대조해 채택했다. 아래 `수정`은 구현자 상태이며 독립 감사 PASS를 뜻하지 않는다.

| ID | 대조한 실제 원인 / 변경 | 검증 근거 |
|---|---|---|
| FIN-001 | checkbox bool과 task capability 분리 → 공유 cancellation, gate 재검사, revoke DB 기록 | `revoked_shared_token_blocks_next_body_approval`, `revoked_after_prepare_sends_no_connection_and_finishes_receipt` |
| FIN-002 | 공통 submit guard 없음/untagged event → 진입 guard, conversation/turn ID, scan generation, 새 대화 작업 종료 | `duplicate_submit_and_old_turn_events_cannot_replace_current_message` |
| FIN-003 | 기본 app watcher 미연결/disconnect=false → watcher poll, gateway stale, stale admission 취소, snapshot freshness 저장, 상시 attach CTA | 기존 watcher matrix + `disconnected_worker_marks_snapshot_unavailable`; native GUI 전수는 미실행 |
| FIN-004 | walker/metadata/inspect error continue → 각각 ScanOmission으로 전달 | 기존 completeness/scan suite, 오류 분기 코드 대조; OS permission matrix 추가 검증 필요 |
| FIN-005 | 민감 파일명 helper 미소비 → gateway catalog 전체 필터 | `sensitive_files_are_absent_from_every_tool_surface` |
| FIN-006 | 경로 검사 후 이름 재open → descriptor 최종 경로 검사, 같은 handle metadata/read/hash | 기존 canonical safety + `opened_descriptor_keeps_original_content_and_rejects_parent_escape`; Unix runtime/ancestor swap stress 미실행 |
| FIN-007 | content만 계수 → 전체 JSON 보수적 계수, 128 omission 집계 한도 | search/read 테스트의 실제 serialized bytes ≤ 계수 assertion |
| FIN-008 | SSE 무상한 → round 4MiB/buffer 1MiB/args 64KiB/24 calls | `oversized_sse_without_newline_is_bounded_in_both_adapters` |
| FIN-009 | DeleteReceipt artifact 공란 → 정규 부모 내부 backup/quarantine cleanup, 실패 전 live row 유지 | `privacy_cleanup_removes_only_application_backups_and_rejects_unknown_children`; 삭제된 경로 receipt 기록 |
| FIN-010 | unverified direct_answer 사용 → 검증된 claim composer | `audit_validator_accepts_only_gateway_catalog_evidence`의 임의 본문 교체 assertion |
| FIN-011 | authorize 후 drop cleanup 없음 → PendingEgress RAII/한 번만 terminal | `drop_ends_uncertain_batch_once_and_success_does_not_repeat`, `pending_send_timeout_finishes_receipt_without_restart` |
| FIN-012 | seed/read/bind/create/restore 오류 무시 → seed/prompt 오류 세션 전용, bind 성공 확인, create 오류 표시, projection 오류 표시 | 관련 기존 storage 실패/복원 테스트, current branch 코드 대조; GUI fault injection 전수 미실행 |
| FIN-013 | UI delta별 DB UPDATE → UI 밖 250ms/4KiB worker, 마지막 flush 후 terminal | `thousand_deltas_are_drained_before_worker_completion`; 실제 DB write count/frame p95 미계측 |
| FIN-014 | mixed round visible text 유실 → 이전 round text를 history/final에 보존 | local tool loop의 visible delta=final assertion |
| FIN-015 | System edit가 Persona label 변경 → 수정 대상 분리, 개별 reset CTA | 코드 대조 및 기존 prompt/Persona suite; native reset E2E 미실행 |
| FIN-016 | list/emphasis/link 누락 → list marker, styled text, 명시적 safe hyperlink | `lists_emphasis_and_explicit_http_links_survive_parsing`, no-fetch/unsafe link 회귀 |
| FIN-017 | Escape drawer/close cancel/hotkey status 누락 → drawer clear, close 전 cancel, 설정 status 표시 | 코드 대조 및 기존 keyboard/lifecycle suite; native focus traversal 미실행 |
| FIN-018 | analysis가 모든 orchestration 소유로 기술 → app composition과 analysis round 책임 분리 명시 | SYSTEM_ARCHITECTURE/DEC-AUDIT-002/current call graph 대조 |

## 재감사 범위와 제한

- 기존 OS process lock/force-kill/atomic terminal/receipt batch CAS 회귀를 유지한다.
- C1~C5와 기존 CR 미완료 기능은 이번 수정으로 완료됐다고 주장하지 않는다. live provider probe의 품질, derived history policy, compaction은 별도 잔여다.
- 이 장부는 소스 수정과 실행 근거다. GUI E2E/Unix 실행/OS 권한·ancestor-swap stress/frame p95의 미검증을 PASS로 바꾸지 않는다.
- 공식 파일 handle 근거: https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getfinalpathnamebyhandlew . hash/read 전에 열린 handle의 final path를 검사하며 이후 이름으로 다시 열지 않는다.

## 최종 실행 증거 (2026-09-08, Windows 작업 트리)

기준 HEAD `0dd109bad5fff1cdf4d0f51542b1c3f800403d30`에 이 장부의 수정이 더해진 작업 트리에서 실행했다. 이번 요청에는 commit/push가 없으므로 clean commit 증거라고 부르지 않는다.

| 명령/검사 | 결과 |
|---|---|
| `cargo test --workspace --locked` | PASS: 188 passed, 0 failed, 2 ignored. mixed tool round의 Completed/Cancelled 텍스트 보존 둘 다 검증 |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS |
| `cargo build --release --locked -p mentat-app` | PASS, optimized profile |
| `cargo test -p mentat-repository --locked test_dbg_f003_100k_2gib_benchmark_profile -- --ignored --nocapture` | PASS: 100,000 files / 2,147,483,648 bytes, scan 99,762ms, peak working set 45,424,640 bytes (<128MiB), fixture 전체 153.94초 |
| `git diff --check` | PASS, LF→CRLF 안내만 있음 |
| sealed source manifest SHA-256 대조 | 8/8 일치, 원본 변경 없음 |
| `cargo audit --file Cargo.lock` | FAIL: 최신 advisory 1,242건/의존성 549개 검사. quick-xml 0.30.0 High 2건: RUSTSEC-2026-0194/0195. 경고 3건: paste, ttf-parser unmaintained 및 chacha20 yanked |

기존 SEC-F007은 여전히 열려 있다. 의존성 major upgrade와 Linux 접근성 호환 검증이 필요하며, 이번 FIN 수정으로 해소했다고 간주하지 않는다. native credential smoke, 실제 GUI·provider·비Windows 실행은 이번 최종 gate에서 실행하지 않았다.

구현자 재검토에서는 취소 terminal의 이전 round 텍스트 유실과 privacy quarantine의 알 수 없는 일반 파일 삭제 가능성을 추가 보완했다. quarantine은 DB/WAL/SHM 이름만 허용하며 모든 child 검증 후 삭제한다. 이 검토는 독립 감사 결과가 아니다. FIN-003/004/006/012/013/015/017의 표에 명시한 실제 환경 검증은 후속 재감사에서 확인해야 한다.
