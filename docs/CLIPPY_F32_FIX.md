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
- diff 검토: 코드 변경은 17개 `_f32` 접미사뿐이며 폭 값이 동일하다. Cargo manifest/lockfile/CI 설정은 기준 commit과 동일하다.
- 비Windows 실제 빌드와 exact-commit CI는 draft PR에서 확인한다.
