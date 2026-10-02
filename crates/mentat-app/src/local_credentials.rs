use mentat_core::MentatError;
use std::path::Path;

pub fn configured_gemini_key() -> Result<Option<String>, MentatError> {
    let cwd = std::env::current_dir().map_err(|_| invalid_file())?;
    let local = cwd.join(".env.local");
    if local.is_file() {
        return gemini_key(&local);
    }
    if let Ok(exe) = std::env::current_exe() {
        // Cargo 산출물을 탐색기에서 실행해도 사용자가 지정한 프로젝트 파일을 찾는다.
        if let Some(root) = cargo_root(&exe) {
            if root.join("Cargo.toml").is_file() {
                return gemini_key(&root.join(".env.local"));
            }
        }
    }
    Ok(None)
}

fn cargo_root(exe: &Path) -> Option<&Path> {
    let target = exe.parent()?.parent()?;
    (target.file_name()? == "target")
        .then(|| target.parent())
        .flatten()
}

/// 개발용 사용자 파일만 읽는다. 값은 로그나 오류 문구에 포함하지 않는다.
pub fn gemini_key(path: &Path) -> Result<Option<String>, MentatError> {
    if !path.exists() {
        return Ok(None);
    }
    let metadata = std::fs::metadata(path).map_err(|_| invalid_file())?;
    if metadata.len() > 8192 {
        return Err(invalid_file());
    }
    let text = std::fs::read_to_string(path).map_err(|_| invalid_file())?;
    parse_key(&text)
}

fn parse_key(text: &str) -> Result<Option<String>, MentatError> {
    let lines: Vec<_> = text
        .trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    if lines.is_empty() {
        return Ok(None);
    }
    let value = if lines.len() == 1 && !lines[0].contains('=') {
        lines[0]
    } else {
        lines
            .iter()
            .find_map(|line| {
                let (name, value) = line.split_once('=')?;
                matches!(name.trim(), "GEMINI_API_KEY" | "GOOGLE_API_KEY").then_some(value.trim())
            })
            .ok_or_else(invalid_file)?
    }
    .trim_matches(['"', '\''])
    .trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().any(char::is_whitespace) {
        return Err(invalid_file());
    }
    Ok(Some(value.to_string()))
}

fn invalid_file() -> MentatError {
    MentatError::PlatformError(
        ".env.local 형식을 확인하세요. 키만 한 줄 또는 GEMINI_API_KEY=값 형식으로 저장하세요."
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_bare_or_named_key_without_echoing_bad_input() {
        assert_eq!(
            parse_key("fixture-value\n").unwrap().as_deref(),
            Some("fixture-value")
        );
        assert_eq!(
            parse_key("# local\nGEMINI_API_KEY=\"fixture-value\"\n")
                .unwrap()
                .as_deref(),
            Some("fixture-value")
        );
        assert!(parse_key("").unwrap().is_none());
        let error = parse_key("SOMETHING=fixture-secret")
            .unwrap_err()
            .to_string();
        assert!(!error.contains("fixture-secret"));
    }

    #[test]
    fn development_layout_resolves_project_root_without_searching_arbitrary_ancestors() {
        let root = Path::new("project");
        let exe = root.join("target").join("release").join("mentat-app.exe");
        assert_eq!(cargo_root(&exe), Some(root));
        let installed = root.join("installed").join("mentat-app.exe");
        assert!(cargo_root(&installed).is_none());
    }
}
