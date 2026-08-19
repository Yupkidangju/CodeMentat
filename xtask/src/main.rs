use std::collections::HashSet;
use std::env;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Platform {
    Current,
    Windows,
    Linux,
    Macos,
    All,
}

impl Platform {
    fn parse(value: &str) -> Result<Self, String> {
        match value.to_ascii_lowercase().as_str() {
            "current" | "host" => Ok(Self::Current),
            "windows" | "win" => Ok(Self::Windows),
            "linux" => Ok(Self::Linux),
            "macos" | "mac" | "darwin" => Ok(Self::Macos),
            "all" => Ok(Self::All),
            _ => Err(format!("지원하지 않는 플랫폼입니다: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arch {
    X86_64,
    Aarch64,
}

impl Arch {
    fn parse(value: &str) -> Result<Self, String> {
        match value.to_ascii_lowercase().as_str() {
            "x86_64" | "x64" | "amd64" => Ok(Self::X86_64),
            "aarch64" | "arm64" => Ok(Self::Aarch64),
            _ => Err(format!("지원하지 않는 아키텍처입니다: {value}")),
        }
    }

    fn host_default() -> Self {
        if env::consts::ARCH == "aarch64" {
            Self::Aarch64
        } else {
            Self::X86_64
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildProfile {
    Debug,
    Release,
}

impl BuildProfile {
    fn parse(value: &str) -> Result<Self, String> {
        match value.to_ascii_lowercase().as_str() {
            "debug" | "dev" => Ok(Self::Debug),
            "release" => Ok(Self::Release),
            _ => Err(format!("지원하지 않는 프로필입니다: {value}")),
        }
    }

    fn directory(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BuildOptions {
    platform: Platform,
    arch: Arch,
    profile: BuildProfile,
    gates: bool,
    dry_run: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            platform: Platform::Current,
            arch: Arch::host_default(),
            profile: BuildProfile::Release,
            gates: false,
            dry_run: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Action {
    Menu,
    Build(BuildOptions),
    Check { dry_run: bool },
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandSpec {
    program: String,
    args: Vec<String>,
}

impl CommandSpec {
    fn cargo(args: &[&str]) -> Self {
        Self {
            program: "cargo".to_string(),
            args: args.iter().map(|value| (*value).to_string()).collect(),
        }
    }

    fn display(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BuildPlan {
    commands: Vec<CommandSpec>,
    artifacts: Vec<String>,
    explicit_targets: Vec<&'static str>,
}

fn parse_args(args: Vec<String>) -> Result<Action, String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Ok(Action::Menu);
    };
    match command {
        "menu" if args.len() == 1 => Ok(Action::Menu),
        "menu" => Err("menu 명령에는 옵션을 사용할 수 없습니다.".to_string()),
        "help" | "-h" | "--help" => Ok(Action::Help),
        "check" => {
            let mut dry_run = false;
            for option in &args[1..] {
                match option.as_str() {
                    "--dry-run" => dry_run = true,
                    _ => return Err(format!("알 수 없는 옵션입니다: {option}")),
                }
            }
            Ok(Action::Check { dry_run })
        }
        "build" => parse_build_options(&args[1..]).map(Action::Build),
        _ => Err(format!("알 수 없는 명령입니다: {command}")),
    }
}

fn parse_build_options(args: &[String]) -> Result<BuildOptions, String> {
    let mut options = BuildOptions::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--platform" => {
                index += 1;
                options.platform = Platform::parse(required_value(args, index, "--platform")?)?;
            }
            "--arch" => {
                index += 1;
                options.arch = Arch::parse(required_value(args, index, "--arch")?)?;
            }
            "--profile" => {
                index += 1;
                options.profile = BuildProfile::parse(required_value(args, index, "--profile")?)?;
            }
            "--gates" => options.gates = true,
            "--dry-run" => options.dry_run = true,
            option => return Err(format!("알 수 없는 옵션입니다: {option}")),
        }
        index += 1;
    }
    Ok(options)
}

fn required_value<'a>(args: &'a [String], index: usize, option: &str) -> Result<&'a str, String> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("{option} 값이 필요합니다."))
}

fn target_triples(platform: Platform, arch: Arch) -> Vec<&'static str> {
    match (platform, arch) {
        (Platform::Current, _) => Vec::new(),
        (Platform::Windows, Arch::X86_64) => vec!["x86_64-pc-windows-msvc"],
        (Platform::Windows, Arch::Aarch64) => vec!["aarch64-pc-windows-msvc"],
        (Platform::Linux, Arch::X86_64) => vec!["x86_64-unknown-linux-gnu"],
        (Platform::Linux, Arch::Aarch64) => vec!["aarch64-unknown-linux-gnu"],
        (Platform::Macos, Arch::X86_64) => vec!["x86_64-apple-darwin"],
        (Platform::Macos, Arch::Aarch64) => vec!["aarch64-apple-darwin"],
        (Platform::All, _) => vec![
            "x86_64-pc-windows-msvc",
            "aarch64-pc-windows-msvc",
            "x86_64-unknown-linux-gnu",
            "aarch64-unknown-linux-gnu",
            "x86_64-apple-darwin",
            "aarch64-apple-darwin",
        ],
    }
}

fn quality_gate_commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec::cargo(&["fmt", "--all", "--", "--check"]),
        CommandSpec::cargo(&[
            "clippy",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--locked",
            "--",
            "-D",
            "warnings",
        ]),
        CommandSpec::cargo(&["test", "--workspace", "--locked"]),
    ]
}

fn build_plan(options: &BuildOptions) -> BuildPlan {
    let explicit_targets = target_triples(options.platform, options.arch);
    let mut commands = if options.gates {
        quality_gate_commands()
    } else {
        Vec::new()
    };
    let targets: Vec<Option<&str>> = if explicit_targets.is_empty() {
        vec![None]
    } else {
        explicit_targets.iter().copied().map(Some).collect()
    };
    let mut artifacts = Vec::new();
    for target in targets {
        let mut args = vec![
            "build".to_string(),
            "--locked".to_string(),
            "-p".to_string(),
            "mentat-app".to_string(),
        ];
        if options.profile == BuildProfile::Release {
            args.push("--release".to_string());
        }
        if let Some(target) = target {
            args.push("--target".to_string());
            args.push(target.to_string());
        }
        commands.push(CommandSpec {
            program: "cargo".to_string(),
            args,
        });
        artifacts.push(artifact_path(target, options.profile));
    }
    BuildPlan {
        commands,
        artifacts,
        explicit_targets,
    }
}

fn artifact_path(target: Option<&str>, profile: BuildProfile) -> String {
    let executable = if target.is_some_and(|value| value.contains("windows"))
        || (target.is_none() && env::consts::OS == "windows")
    {
        "mentat-app.exe"
    } else {
        "mentat-app"
    };
    match target {
        Some(target) => format!("target/{target}/{}/{executable}", profile.directory()),
        None => format!("target/{}/{executable}", profile.directory()),
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be inside the workspace")
        .to_path_buf()
}

fn installed_targets(root: &Path) -> Result<HashSet<String>, String> {
    let output = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("rustup 실행 실패: {error}"))?;
    if !output.status.success() {
        return Err("설치된 Rust target 목록을 확인하지 못했습니다.".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

fn preflight_targets(plan: &BuildPlan, root: &Path, dry_run: bool) -> Result<(), String> {
    if dry_run || plan.explicit_targets.is_empty() {
        return Ok(());
    }
    let installed = installed_targets(root)?;
    let missing: Vec<_> = plan
        .explicit_targets
        .iter()
        .copied()
        .filter(|target| !installed.contains(*target))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "설치되지 않은 Rust target: {}\n준비 명령: rustup target add {}",
            missing.join(", "),
            missing.join(" ")
        ))
    }
}

fn execute_spec(spec: &CommandSpec, root: &Path, dry_run: bool) -> Result<(), String> {
    println!("$ {}", spec.display());
    if dry_run {
        return Ok(());
    }
    let status = Command::new(&spec.program)
        .args(&spec.args)
        .current_dir(root)
        .status()
        .map_err(|error| format!("명령 실행 실패 ({}): {error}", spec.display()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("명령 실패 ({}): {status}", spec.display()))
    }
}

fn execute_build(options: BuildOptions) -> Result<(), String> {
    let root = workspace_root();
    let plan = build_plan(&options);
    preflight_targets(&plan, &root, options.dry_run)?;
    for command in &plan.commands {
        execute_spec(command, &root, options.dry_run)?;
    }
    println!("예상 산출물:");
    for artifact in &plan.artifacts {
        println!("  - {artifact}");
    }
    Ok(())
}

fn execute_check(dry_run: bool) -> Result<(), String> {
    let root = workspace_root();
    for command in quality_gate_commands() {
        execute_spec(&command, &root, dry_run)?;
    }
    Ok(())
}

fn interactive_menu() -> Result<(), String> {
    println!("Code Mentat 멀티 플랫폼 빌드");
    println!("  1) 현재 플랫폼 debug");
    println!("  2) 현재 플랫폼 release");
    println!("  3) 현재 플랫폼 release + 품질 게이트");
    println!("  4) Windows x86_64 release");
    println!("  5) Linux x86_64 release");
    println!("  6) macOS aarch64 release");
    println!("  7) macOS x86_64 release");
    println!("  8) 전체 문서화 target release");
    println!("  9) 품질 게이트만 실행");
    println!("  0) 종료");
    print!("선택: ");
    io::stdout()
        .flush()
        .map_err(|error| format!("메뉴 출력 실패: {error}"))?;
    let mut choice = String::new();
    io::stdin()
        .read_line(&mut choice)
        .map_err(|error| format!("메뉴 입력 실패: {error}"))?;

    let x64 = Arch::X86_64;
    let explicit = |platform, arch| BuildOptions {
        platform,
        arch,
        profile: BuildProfile::Release,
        gates: false,
        dry_run: false,
    };
    match choice.trim() {
        "1" => execute_build(BuildOptions {
            profile: BuildProfile::Debug,
            ..BuildOptions::default()
        }),
        "2" => execute_build(BuildOptions::default()),
        "3" => execute_build(BuildOptions {
            gates: true,
            ..BuildOptions::default()
        }),
        "4" => execute_build(explicit(Platform::Windows, x64)),
        "5" => execute_build(explicit(Platform::Linux, x64)),
        "6" => execute_build(explicit(Platform::Macos, Arch::Aarch64)),
        "7" => execute_build(explicit(Platform::Macos, x64)),
        "8" => execute_build(explicit(Platform::All, x64)),
        "9" => execute_check(false),
        "0" => Ok(()),
        value => Err(format!("알 수 없는 메뉴 선택입니다: {value}")),
    }
}

fn print_help() {
    println!(
        "Code Mentat 빌드 오케스트레이터\n\n\
사용법:\n  cargo mentat-build\n  cargo mentat-build build [옵션]\n  cargo mentat-build check [--dry-run]\n\n\
build 옵션:\n  --platform <current|windows|linux|macos|all>\n  --arch <x86_64|aarch64>\n  --profile <debug|release>\n  --gates\n  --dry-run"
    );
}

fn run() -> Result<(), String> {
    match parse_args(env::args().skip(1).collect())? {
        Action::Menu => interactive_menu(),
        Action::Build(options) => execute_build(options),
        Action::Check { dry_run } => execute_check(dry_run),
        Action::Help => {
            print_help();
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("오류: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn empty_arguments_select_interactive_menu() {
        assert_eq!(parse_args(args(&[])).unwrap(), Action::Menu);
    }

    #[test]
    fn command_mode_parses_platform_arch_profile_and_switches() {
        let action = parse_args(args(&[
            "build",
            "--platform",
            "macos",
            "--arch",
            "aarch64",
            "--profile",
            "release",
            "--gates",
            "--dry-run",
        ]))
        .unwrap();
        assert_eq!(
            action,
            Action::Build(BuildOptions {
                platform: Platform::Macos,
                arch: Arch::Aarch64,
                profile: BuildProfile::Release,
                gates: true,
                dry_run: true,
            })
        );
    }

    #[test]
    fn all_platforms_expand_to_the_documented_target_matrix() {
        assert_eq!(
            target_triples(Platform::All, Arch::X86_64),
            vec![
                "x86_64-pc-windows-msvc",
                "aarch64-pc-windows-msvc",
                "x86_64-unknown-linux-gnu",
                "aarch64-unknown-linux-gnu",
                "x86_64-apple-darwin",
                "aarch64-apple-darwin",
            ]
        );
    }

    #[test]
    fn gated_release_plan_is_locked_and_builds_only_the_app() {
        let plan = build_plan(&BuildOptions {
            platform: Platform::Linux,
            arch: Arch::X86_64,
            profile: BuildProfile::Release,
            gates: true,
            dry_run: false,
        });
        assert_eq!(plan.commands[0].display(), "cargo fmt --all -- --check");
        assert_eq!(
            plan.commands.last().unwrap().display(),
            "cargo build --locked -p mentat-app --release --target x86_64-unknown-linux-gnu"
        );
        assert_eq!(
            plan.artifacts,
            vec!["target/x86_64-unknown-linux-gnu/release/mentat-app"]
        );
    }

    #[test]
    fn unknown_option_is_rejected_instead_of_ignored() {
        let error = parse_args(args(&["build", "--unknown"])).unwrap_err();
        assert!(error.contains("알 수 없는 옵션"));
    }
}
