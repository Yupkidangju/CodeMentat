# Code Mentat 빌드 및 패키징 가이드 (BUILD_GUIDE.md)

- **문서 버전:** 1.0.0
- **지원 플랫폼:** Windows (x86_64), Linux (x86_64), macOS (arm64, x86_64)

---

## 1. 개발 환경 요구사항

- **Rust 도구 체인:** Rust stable (1.80 이상)
- **C 컴파일러 (옵션):** SQLite 번들 컴파일용 (MSVC, GCC 또는 Clang)

---

## 2. 통합 빌드 오케스트레이터

빌드 로직은 workspace `xtask` 한 곳에서 관리한다. 인자가 없으면 메뉴형, 인자가 있으면 CI/자동화에 적합한 명령형으로 실행된다.

```bash
# Cargo alias: 대화형 메뉴
cargo mentat-build

# Windows wrapper: 메뉴 또는 동일 인자 전달
./scripts/build.ps1
./scripts/build.ps1 build --platform windows --arch x86_64 --profile release

# Linux/macOS wrapper
sh ./scripts/build.sh
sh ./scripts/build.sh build --platform current --profile release --gates
```

### 2.1 명령형 계약

```text
cargo mentat-build build \
  --platform <current|windows|linux|macos|all> \
  --arch <x86_64|aarch64> \
  --profile <debug|release> \
  [--gates] [--dry-run]

cargo mentat-build check [--dry-run]
cargo mentat-build help
```

- `build` 기본값: `--platform current --profile release`
- `--platform all`: Windows/Linux/macOS의 x86_64와 aarch64 총 6개 target을 순서대로 계획한다.
- `--gates`: `fmt → clippy → workspace test → build` 순서로 실행한다.
- `--dry-run`: 명령과 예상 산출물만 출력한다.
- 명시적 타깃은 먼저 `rustup target list --installed`로 확인한다. 미설치 target, linker 또는 SDK 오류는 실패로 종료하며 host 빌드로 fallback하지 않는다.

### 2.2 지원 target triple

| 플랫폼 | x86_64 | aarch64 |
|---|---|---|
| Windows | `x86_64-pc-windows-msvc` | `aarch64-pc-windows-msvc` |
| Linux | `x86_64-unknown-linux-gnu` | `aarch64-unknown-linux-gnu` |
| macOS | `x86_64-apple-darwin` | `aarch64-apple-darwin` |

예: `rustup target add aarch64-apple-darwin`. Rust target만으로 system linker/SDK가 설치되지는 않으므로 일반적으로 각 native OS 또는 준비된 cross toolchain/CI runner에서 빌드한다.

GitHub Actions의 Windows/Linux/macOS matrix는 각 runner에서 `--platform current` 실제 release build와 `--platform all --dry-run` 계약 검증을 실행한다.

## 3. 직접 Cargo 빌드 명령어

### 3.1 개발 모드 빌드 및 실행
```bash
# 전체 워크스페이스 검사
cargo check --workspace

# 컴팩트 위젯 실행
cargo run -p mentat-app
```

### 3.2 전체 단위 및 통합 테스트 실행
```bash
cargo test --workspace
```

### 3.3 릴리스 바이너리 빌드
```bash
cargo build --release -p mentat-app
```
산출물: `target/release/mentat-app.exe` (Windows) 또는 `target/release/mentat-app` (Linux/macOS)

---

## 4. 플랫폼별 고려사항

- **Windows:** `eframe / egui`가 Direct3D/OpenGL 백엔드를 자동 선택하며, 프레임리스(Decorated=false) 윈도우로 렌더링됩니다.
- **macOS:** Metal 백엔드와 App Sandbox를 지원하며, 다이얼로그는 네이티브 Cocoa 파일 선택기를 사용합니다.
- **Linux:** X11 및 Wayland 환경에서 동작하며 `libxkbcommon` 및 `libglvnd`가 필요할 수 있습니다.

## 5. 전체 품질 게이트

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo test -p mentat-repository --locked test_dbg_f003_100k_2gib_benchmark_profile -- --ignored --nocapture
cargo build --release --locked -p mentat-app
cargo audit --file Cargo.lock
```

- `notify 8.2.0`은 Windows/macOS/Linux의 OS 파일 변경 event를 사용합니다.
- `global-hotkey 0.6.4`는 Windows/macOS/X11에서 전역 표시·포커스 단축키를 등록합니다. 등록 충돌 시 창 숨김 없이 Tier 1 접기로 제한됩니다.
- Windows 100k/2GiB ignored gate의 기준은 Windows 11 Pro 10.0.26200 x64, Ryzen 7 8845HS, RAM 27.8GiB, rustc 1.96.0 debug test profile이며 peak working set 상한은 128MiB입니다.
