# Rust 1.99 부동소수점 리터럴 CI 수정

- 기준: `367cff669b610930015a7ad785ef640684b4c6c9`, CI run `36985788020`.
- 목표: `float_literal_f32_fallback` 진단 17건을 없애고 strict Clippy 게이트를 통과한다.
- 범위: `mentat-app/src/app.rs`, `theme.rs`, `widgets/pill_bar.rs`의 `Stroke::new` 폭 리터럴에 컴파일러가 제안한 `_f32`를 명시한다.
- 불변조건: 폭 값과 UI 동작, 의존성, lockfile, CI의 `-D warnings`를 유지한다. 새 기능, 리팩터링, 자격 증명 변경은 하지 않는다.
- 순서: 격리 worktree에서 Rust 1.99 실패 재현 → 17개 리터럴 수정 → fmt/Clippy/workspace tests/build 검증 → diff 검토.
- 회귀 기준: 기존 strict Clippy 명령이 실패를 재현하고 수정 후 통과한다. 기존 테마·pill geometry 회귀는 workspace tests로 확인한다.
- 게시 범위: 사용자가 승인한 전용 브랜치 push와 draft PR의 정확한 commit CI 확인. 병합과 배포는 수행하지 않는다.

## 검증

Windows x86_64, task-local Rust 1.99.0에서 아래 명령을 실행하고 결과를 기록한다.

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo mentat-build build --platform all --profile release --dry-run
cargo mentat-build build --platform current --profile release
```

## 실행 결과 (2026-10-02)

- 수정 전 strict Clippy: exit 101, CI와 동일한 `float_literal_f32_fallback` 17건 재현.
- 수정 후 fmt와 strict Clippy: PASS (exit 0).
- workspace tests: PASS (199 passed, 0 failed, 2 ignored). 테마 3개와 pill geometry 3개 회귀 포함.
- ignored 항목: native credential store smoke와 100k/2GiB benchmark. 이번 범위에서 실행하지 않았다.
- 6-target release build-plan dry-run: PASS (exit 0).
- Windows locked release build: PASS (exit 0, Rust 1.99.0 optimized build, 1m 47s).
- Clippy 수정 commit의 diff 검토: 코드 변경은 17개 `_f32` 접미사뿐이며 폭 값이 동일하다. Cargo manifest/lockfile/CI 설정은 기준 commit과 동일하다.
- 비Windows 실제 빌드와 exact-commit CI는 draft PR에서 확인한다.

## macOS 응답 크기 회귀 fixture 보정

- 근거: `4f32f4a`의 CI run `36993285438`에서 세 OS의 strict Clippy는 통과했으나 macOS `model_verification_rejects_oversized_untrusted_response`가 서버 body write의 `ConnectionReset`으로 실패했다.
- 원인: production `parse_bounded_json`은 과대 `Content-Length`를 body 읽기 전에 거부한다. fixture는 거부 후에도 1MiB+1 body 전체 write 성공을 요구하여 OS TCP 버퍼·종료 동작에 의존했다.
- 범위: 해당 테스트만 과대 header를 보내고 oneshot으로 verification 완료까지 연결을 유지한다. body를 보내지 않으므로 reset write 경합을 없애며, 5초 timeout과 기존 `MODEL_VERIFY_RESPONSE_TOO_LARGE` assertion으로 body를 기다리지 않는 early rejection을 확인한다.
- 불변조건: production adapter, 응답 크기 제한, 오류 코드, 테스트 실행 및 CI lint 정책을 유지한다. 테스트 skip이나 보안 동작 변경은 하지 않는다.
- 검증 순서: targeted 회귀 → fixture의 광고 크기를 허용 경계로 바꾼 임시 negative control이 실패하는지 확인 후 복원 → fmt/strict Clippy/workspace tests/Windows release → 기존 draft PR의 새 commit CI terminal 결과 확인.
- 로컬 결과 (Rust 1.99.0): targeted 회귀 PASS. 광고 크기를 1MiB로 바꾼 negative control은 5.01초 뒤 `MODEL_VERIFY_READ_ERROR`와 요구한 `MODEL_VERIFY_RESPONSE_TOO_LARGE`의 불일치로 exit 101; 과대 header fixture를 즉시 복원했다.
- 복원 후 전체 fmt/strict Clippy/workspace tests (199 passed, 2 ignored)/6-target dry-run/Windows locked release (19.89초) 모두 exit 0.
- 변경 검토: `lib.rs`의 production 영역과 두 provider adapter, Cargo manifest/lockfile, CI 설정은 변경하지 않았다. 새 테스트 skip은 없으며 기존 보안 assertion을 유지한다.

## macOS watcher 무시 경로 회귀 수정

- 사용자 승인: 기존 draft PR #1에서 남은 macOS watcher 실패를 수정하고 exact-commit CI를 완료한다. 병합·배포는 수행하지 않는다.
- 기준: `d796ab6`, CI run `36994850356`의 `test_dbg_f002_ignored_paths_and_access_events_do_not_mark_stale` 실패.
- 목표: 무시 경로 변경은 STALE을 만들지 않고 실제 tracked 변경은 감지한다. unknown/rescan/ignore-control/worker 오류·disconnect의 fail-closed 정책을 유지한다.
- 진단 순서: test-only event/root/disposition 로그로 macOS 실제 실패 event를 확인 → 원인에 맞는 최소 수정과 regression → 전체 fmt/Clippy/tests/build 검증 → 같은 draft PR의 exact-commit CI terminal 확인.
- 가설: notify macOS backend의 canonical event 경로와 입력 root의 별칭 차이, 또는 OS가 전달한 상위 디렉터리/제어 event가 원인일 수 있다. event 증거 전에는 확정하지 않는다.
- 임시 진단 로그는 최종 수정에서 제거한다. CI 설정, lint 강도, 응답 크기 security assertion과 기존 watcher assertion은 완화·skip하지 않는다.
