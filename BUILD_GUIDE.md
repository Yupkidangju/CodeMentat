# Code Mentat 빌드 및 패키징 가이드 (BUILD_GUIDE.md)

- **문서 버전:** 1.0.0
- **지원 플랫폼:** Windows (x86_64), Linux (x86_64), macOS (arm64, x86_64)

---

## 1. 개발 환경 요구사항

- **Rust 도구 체인:** Rust stable (1.80 이상)
- **C 컴파일러 (옵션):** SQLite 번들 컴파일용 (MSVC, GCC 또는 Clang)

---

## 2. 빌드 명령어

### 2.1 개발 모드 빌드 및 실행
```bash
# 전체 워크스페이스 검사
cargo check --workspace

# 컴팩트 위젯 실행
cargo run -p mentat-app
```

### 2.2 전체 단위 및 통합 테스트 실행
```bash
cargo test --workspace
```

### 2.3 릴리스 바이너리 빌드
```bash
cargo build --release -p mentat-app
```
산출물: `target/release/mentat-app.exe` (Windows) 또는 `target/release/mentat-app` (Linux/macOS)

---

## 3. 플랫폼별 고려사항

- **Windows:** `eframe / egui`가 Direct3D/OpenGL 백엔드를 자동 선택하며, 프레임리스(Decorated=false) 윈도우로 렌더링됩니다.
- **macOS:** Metal 백엔드와 App Sandbox를 지원하며, 다이얼로그는 네이티브 Cocoa 파일 선택기를 사용합니다.
- **Linux:** X11 및 Wayland 환경에서 동작하며 `libxkbcommon` 및 `libglvnd`가 필요할 수 있습니다.

## 4. 전체 품질 게이트

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
