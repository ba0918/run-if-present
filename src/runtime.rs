use std::env;
use std::ffi::{c_char, CStr, CString, OsStr, OsString};
use std::fs;
use std::io;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::cli::{Arguments, Condition};

const SIGPIPE: i32 = 13;
const SIG_IGN: usize = 1;
const SIG_ERR: usize = usize::MAX;

// Holds the SIGPIPE disposition the wrapper was started with, or SIG_ERR when it is unknown.
static INHERITED_SIGPIPE: AtomicUsize = AtomicUsize::new(SIG_ERR);

unsafe extern "C" {
    fn signal(signal: i32, handler: usize) -> usize;
}

// Rust sets SIGPIPE to SIG_IGN before main, which the child would inherit through exec. This
// loader constructor runs earlier and records the caller's disposition; exec has already reset
// any caught handler, so only SIG_DFL or SIG_IGN can be observed here.
#[used]
#[cfg_attr(target_os = "linux", link_section = ".init_array")]
#[cfg_attr(target_os = "macos", link_section = "__DATA,__mod_init_func")]
static CAPTURE_INHERITED_SIGPIPE: unsafe extern "C" fn() = capture_inherited_sigpipe;

unsafe extern "C" fn capture_inherited_sigpipe() {
    let inherited = unsafe { signal(SIGPIPE, SIG_IGN) };
    if inherited != SIG_ERR && unsafe { signal(SIGPIPE, inherited) } != SIG_ERR {
        INHERITED_SIGPIPE.store(inherited, Ordering::Relaxed);
    }
}

fn restore_inherited_sigpipe() -> io::Result<()> {
    let inherited = INHERITED_SIGPIPE.load(Ordering::Relaxed);
    if inherited == SIG_ERR {
        return Err(io::Error::other(
            "could not capture the inherited SIGPIPE disposition",
        ));
    }
    if unsafe { signal(SIGPIPE, inherited) } == SIG_ERR {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub enum RunError {
    Diagnostic {
        operation: &'static str,
        operand: OsString,
        source: io::Error,
        code: i32,
    },
}

impl RunError {
    pub fn code(&self) -> i32 {
        match self {
            Self::Diagnostic { code, .. } => *code,
        }
    }

    pub fn print(&self) {
        match self {
            Self::Diagnostic {
                operation,
                operand,
                source,
                ..
            } => eprintln!(
                "run-if-present: {operation}: {}: {source}",
                escape_operand(operand)
            ),
        }
    }
}

pub fn run(arguments: Arguments) -> Result<(), RunError> {
    if let Some(directory) = arguments.chdir {
        let directory = expand_tilde(&directory)
            .map_err(|source| diagnostic("expand", directory, source, 1))?;
        match fs::metadata(&directory) {
            Ok(metadata) if !metadata.is_dir() => {
                return Err(diagnostic(
                    "chdir",
                    directory,
                    io::Error::new(io::ErrorKind::NotADirectory, "not a directory"),
                    1,
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(diagnostic("inspect", directory, error, 1)),
        }
        env::set_current_dir(&directory)
            .map_err(|source| diagnostic("chdir", directory, source, 1))?;
    }

    let execution = match arguments.condition {
        Condition::Path { path, mut command } => {
            let guard =
                expand_tilde(&path).map_err(|source| diagnostic("expand", path, source, 1))?;
            match fs::metadata(&guard) {
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(diagnostic("inspect", guard, error, 1)),
            }
            let requested = command.remove(0);
            let pathname = if requested.as_bytes().contains(&b'/') {
                requested.clone()
            } else {
                resolve_command(&requested)?
                    .ok_or_else(|| {
                        diagnostic(
                            "execute",
                            requested.clone(),
                            io::Error::new(io::ErrorKind::NotFound, "command not found"),
                            127,
                        )
                    })?
                    .into_os_string()
            };
            Execution {
                pathname,
                argv0: requested,
                arguments: command,
            }
        }
        Condition::Command { command, arguments } => {
            let Some(pathname) = resolve_command(&command)? else {
                return Ok(());
            };
            Execution {
                pathname: pathname.into_os_string(),
                argv0: command,
                arguments,
            }
        }
    };

    restore_inherited_sigpipe()
        .map_err(|source| diagnostic("prepare execution", execution.pathname.clone(), source, 1))?;
    let error = replace_process(&execution.pathname, &execution.argv0, &execution.arguments);
    let code = match error.kind() {
        io::ErrorKind::NotFound => 127,
        io::ErrorKind::PermissionDenied => 126,
        _ if error.raw_os_error() == Some(8) => 126,
        _ => 1,
    };
    Err(diagnostic("execute", execution.pathname, error, code))
}

struct Execution {
    pathname: OsString,
    argv0: OsString,
    arguments: Vec<OsString>,
}

fn replace_process(pathname: &OsStr, argv0: &OsStr, arguments: &[OsString]) -> io::Error {
    let pathname = CString::new(pathname.as_bytes()).expect("OS strings cannot contain NUL bytes");
    let mut argv = Vec::with_capacity(arguments.len() + 1);
    argv.push(CString::new(argv0.as_bytes()).expect("OS strings cannot contain NUL bytes"));
    argv.extend(arguments.iter().map(|argument| {
        CString::new(argument.as_bytes()).expect("OS strings cannot contain NUL bytes")
    }));
    let mut argv_pointers: Vec<*const c_char> = argv.iter().map(|value| value.as_ptr()).collect();
    argv_pointers.push(std::ptr::null());

    unsafe extern "C" {
        fn execv(pathname: *const c_char, argv: *const *const c_char) -> i32;
    }

    // execv hands the child libc's live environment array untouched; rebuilding it through Rust
    // would discard duplicate keys and entries without `=`. execvp is avoided for its shell
    // fallback on ENOEXEC.
    unsafe {
        execv(pathname.as_ptr(), argv_pointers.as_ptr());
    }
    io::Error::last_os_error()
}

fn resolve_command(command: &OsStr) -> Result<Option<PathBuf>, RunError> {
    if command.as_bytes().contains(&b'/') {
        return classify_explicit(PathBuf::from(command));
    }

    let Some(path) = env::var_os("PATH").filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let cwd =
        env::current_dir().map_err(|source| diagnostic("working directory", ".", source, 1))?;

    let mut inspection_error = None;
    let mut unusable = None;
    for directory in env::split_paths(&path) {
        let literal = literal_candidate(command, &directory, &cwd);
        if let Ok(discovered) = which::which_in(command, Some(&directory), &cwd) {
            if same_lexical_path_ignoring_curdir(&discovered, &literal) {
                if let Some(candidate) = retain_search_result(
                    classify_search_candidate(discovered),
                    &mut inspection_error,
                    &mut unusable,
                ) {
                    return Ok(Some(candidate));
                }
                continue;
            }
        }
        if let Some(candidate) = retain_search_result(
            classify_search_candidate(literal),
            &mut inspection_error,
            &mut unusable,
        ) {
            return Ok(Some(candidate));
        }
    }

    if let Some((candidate, error)) = inspection_error {
        Err(diagnostic("inspect executable", candidate, error, 1))
    } else if let Some(candidate) = unusable {
        Err(diagnostic(
            "resolve executable",
            candidate,
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "not an executable regular file",
            ),
            126,
        ))
    } else {
        Ok(None)
    }
}

fn same_lexical_path_ignoring_curdir(left: &Path, right: &Path) -> bool {
    left.components()
        .filter(|component| !matches!(component, Component::CurDir))
        .eq(right
            .components()
            .filter(|component| !matches!(component, Component::CurDir)))
}

fn literal_candidate(command: &OsStr, directory: &Path, cwd: &Path) -> PathBuf {
    if directory.is_absolute() {
        directory.join(command)
    } else {
        cwd.join(directory).join(command)
    }
}

enum SearchCandidate {
    Invokable(PathBuf),
    Absent,
    Unusable(PathBuf),
    InspectionFailed(PathBuf, io::Error),
}

fn classify_search_candidate(candidate: PathBuf) -> SearchCandidate {
    match fs::metadata(&candidate) {
        Ok(metadata) if is_invokable(&metadata) => SearchCandidate::Invokable(candidate),
        Ok(_) => SearchCandidate::Unusable(candidate),
        Err(error) if error.kind() == io::ErrorKind::NotFound => SearchCandidate::Absent,
        Err(error) => SearchCandidate::InspectionFailed(candidate, error),
    }
}

fn retain_search_result(
    result: SearchCandidate,
    inspection_error: &mut Option<(PathBuf, io::Error)>,
    unusable: &mut Option<PathBuf>,
) -> Option<PathBuf> {
    match result {
        SearchCandidate::Invokable(candidate) => Some(candidate),
        SearchCandidate::Absent => None,
        SearchCandidate::Unusable(candidate) => {
            if unusable.is_none() {
                *unusable = Some(candidate);
            }
            None
        }
        SearchCandidate::InspectionFailed(candidate, error) => {
            if inspection_error.is_none() {
                *inspection_error = Some((candidate, error));
            }
            None
        }
    }
}

fn classify_explicit(candidate: PathBuf) -> Result<Option<PathBuf>, RunError> {
    match fs::metadata(&candidate) {
        Ok(metadata) if is_invokable(&metadata) => Ok(Some(candidate)),
        Ok(_) => Err(diagnostic(
            "resolve executable",
            candidate,
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "not an executable regular file",
            ),
            126,
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(diagnostic("inspect executable", candidate, error, 1)),
    }
}

fn is_invokable(metadata: &fs::Metadata) -> bool {
    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
}

fn expand_tilde(path: &OsStr) -> io::Result<PathBuf> {
    expand_tilde_with(path, home_directory)
}

fn home_directory() -> Option<PathBuf> {
    match env::var_os("HOME") {
        Some(home) if !home.is_empty() => Some(PathBuf::from(home)),
        _ => account_database_home(),
    }
}

#[cfg(target_os = "linux")]
#[repr(C)]
struct Passwd {
    pw_name: *mut c_char,
    pw_passwd: *mut c_char,
    pw_uid: u32,
    pw_gid: u32,
    pw_gecos: *mut c_char,
    pw_dir: *mut c_char,
    pw_shell: *mut c_char,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct Passwd {
    pw_name: *mut c_char,
    pw_passwd: *mut c_char,
    pw_uid: u32,
    pw_gid: u32,
    pw_change: i64,
    pw_class: *mut c_char,
    pw_gecos: *mut c_char,
    pw_dir: *mut c_char,
    pw_shell: *mut c_char,
    pw_expire: i64,
}

fn account_database_home() -> Option<PathBuf> {
    unsafe extern "C" {
        fn getuid() -> u32;
        fn getpwuid(uid: u32) -> *const Passwd;
    }

    // The wrapper is single-threaded, and the account-database pointer is cloned immediately
    // before another libc lookup can invalidate its static storage.
    let record = unsafe { getpwuid(getuid()) };
    if record.is_null() {
        return None;
    }
    let directory = unsafe { (*record).pw_dir };
    if directory.is_null() {
        return None;
    }
    let bytes = unsafe { CStr::from_ptr(directory) }.to_bytes().to_vec();
    Some(PathBuf::from(OsString::from_vec(bytes)))
}

fn expand_tilde_with(path: &OsStr, home: impl FnOnce() -> Option<PathBuf>) -> io::Result<PathBuf> {
    let bytes = path.as_bytes();
    if bytes != b"~" && !bytes.starts_with(b"~/") {
        return Ok(PathBuf::from(path));
    }
    let home = home()
        .filter(|value| !value.as_os_str().is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory is unavailable"))?;
    if bytes == b"~" {
        Ok(home)
    } else {
        let suffix = bytes[2..]
            .iter()
            .skip_while(|byte| **byte == b'/')
            .copied()
            .collect();
        Ok(home.join(OsString::from_vec(suffix)))
    }
}

fn diagnostic(
    operation: &'static str,
    operand: impl Into<OsString>,
    source: io::Error,
    code: i32,
) -> RunError {
    RunError::Diagnostic {
        operation,
        operand: operand.into(),
        source,
        code,
    }
}

fn escape_operand(value: &OsStr) -> String {
    let mut escaped = String::from("\"");
    for byte in value.as_bytes() {
        match byte {
            b'\\' => escaped.push_str("\\\\"),
            b'\"' => escaped.push_str("\\\""),
            b'\n' => escaped.push_str("\\n"),
            b'\r' => escaped.push_str("\\r"),
            b'\t' => escaped.push_str("\\t"),
            0x20..=0x7e => escaped.push(char::from(*byte)),
            _ => escaped.push_str(&format!("\\x{byte:02x}")),
        }
    }
    escaped.push('"');
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_path_comparison_ignores_only_curdir_components() {
        assert!(same_lexical_path_ignoring_curdir(
            Path::new("/work/./bin/tool"),
            Path::new("/work/bin/tool")
        ));
        assert!(!same_lexical_path_ignoring_curdir(
            Path::new("/work/link/../bin/tool"),
            Path::new("/work/bin/tool")
        ));
        assert!(!same_lexical_path_ignoring_curdir(
            Path::new("~/bin/tool"),
            Path::new("/home/user/bin/tool")
        ));
    }

    #[test]
    fn repeated_slashes_after_tilde_keep_a_non_utf8_suffix_relative_to_home() {
        let home = PathBuf::from("/home/example");
        let path = OsString::from_vec(vec![b'~', b'/', b'/', b'/', b'g', 0xff]);

        assert_eq!(
            expand_tilde_with(&path, || Some(home.clone())).unwrap(),
            home.join(OsString::from_vec(vec![b'g', 0xff]))
        );
        assert_eq!(
            expand_tilde_with(OsStr::new("~//etc"), || Some(home.clone())).unwrap(),
            home.join("etc")
        );
    }

    #[test]
    fn expands_exact_tilde_without_utf8_conversion() {
        let home = PathBuf::from(OsString::from_vec(vec![b'/', b'h', 0xff]));
        assert_eq!(
            expand_tilde_with(OsStr::new("~"), || Some(home.clone())).unwrap(),
            home
        );
    }

    #[test]
    fn leaves_named_user_tilde_literal() {
        assert_eq!(
            expand_tilde_with(OsStr::new("~someone/path"), || None).unwrap(),
            PathBuf::from("~someone/path")
        );
    }

    #[test]
    fn rejects_an_empty_home_from_the_user_database() {
        let error = expand_tilde_with(OsStr::new("~/path"), || Some(PathBuf::new())).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }
}
