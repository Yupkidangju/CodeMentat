use mentat_core::MentatError;
use std::{
    fs::File,
    path::{Component, Path, PathBuf},
};

/// 경로 검사 후 다시 여는 대신 실제 읽을 handle의 최종 경로를 검사한다.
pub(crate) fn open_beneath(root: &Path, relative: &Path) -> Result<File, MentatError> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(MentatError::ExternalPathBlocked(
            "상대 경로만 허용합니다.".into(),
        ));
    }
    let root = root.canonicalize().map_err(io_error)?;
    let file = File::open(root.join(relative)).map_err(io_error)?;
    let actual = handle_path(&file).map_err(io_error)?;
    if !actual.starts_with(&root) || !file.metadata().map_err(io_error)?.is_file() {
        return Err(MentatError::ExternalPathBlocked(
            "열린 파일 handle이 저장소 밖 또는 일반 파일이 아닙니다.".into(),
        ));
    }
    Ok(file)
}
fn io_error(error: std::io::Error) -> MentatError {
    MentatError::IoError(error.to_string())
}

#[cfg(windows)]
fn handle_path(file: &File) -> std::io::Result<PathBuf> {
    use std::{
        ffi::OsString,
        os::windows::{ffi::OsStringExt, io::AsRawHandle},
    };
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFinalPathNameByHandleW(
            handle: *mut std::ffi::c_void,
            path: *mut u16,
            size: u32,
            flags: u32,
        ) -> u32;
    }
    let mut buffer = vec![0u16; 32768];
    let length = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            0,
        )
    };
    if length == 0 || length as usize >= buffer.len() {
        return Err(std::io::Error::last_os_error());
    }
    Ok(PathBuf::from(OsString::from_wide(
        &buffer[..length as usize],
    )))
}
#[cfg(target_os = "linux")]
fn handle_path(file: &File) -> std::io::Result<PathBuf> {
    use std::os::fd::AsRawFd;
    std::fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd()))
}
#[cfg(target_os = "macos")]
fn handle_path(file: &File) -> std::io::Result<PathBuf> {
    use std::os::{fd::AsRawFd, unix::ffi::OsStrExt};
    unsafe extern "C" {
        fn fcntl(fd: i32, cmd: i32, ...) -> i32;
    }
    let mut buffer = [0u8; 1024];
    if unsafe { fcntl(file.as_raw_fd(), 50, buffer.as_mut_ptr()) } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    let end = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(buffer.len());
    Ok(PathBuf::from(std::ffi::OsStr::from_bytes(&buffer[..end])))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opened_descriptor_keeps_original_content_and_rejects_parent_escape() {
        use std::io::Read;
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("source"), "inside").unwrap();
        let mut file = open_beneath(root.path(), Path::new("source")).unwrap();
        // 파일명 교체 뒤에도 이미 검증한 descriptor는 원래 객체를 읽는다.
        std::fs::rename(root.path().join("source"), root.path().join("old")).unwrap();
        std::fs::write(root.path().join("source"), "replacement").unwrap();
        let mut text = String::new();
        file.read_to_string(&mut text).unwrap();
        assert_eq!(text, "inside");
        assert!(open_beneath(root.path(), Path::new("../outside")).is_err());
    }
}
