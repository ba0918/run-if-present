use std::env;
use std::ffi::{c_char, CString, OsStr, OsString};
use std::fs;
use std::io;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::cli::{Arguments, Condition};

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

    let (program, child_arguments) = match arguments.condition {
        Condition::Path { path, mut command } => {
            let guard =
                expand_tilde(&path).map_err(|source| diagnostic("expand", path, source, 1))?;
            match fs::metadata(&guard) {
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(diagnostic("inspect", guard, error, 1)),
            }
            let requested = command.remove(0);
            let program = if requested.as_bytes().contains(&b'/') {
                requested
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
            (program, command)
        }
        Condition::Command { command, arguments } => {
            let Some(program) = resolve_command(&command)? else {
                return Ok(());
            };
            (program.into_os_string(), arguments)
        }
    };

    let error = replace_process(&program, &child_arguments);
    let code = match error.kind() {
        io::ErrorKind::NotFound => 127,
        io::ErrorKind::PermissionDenied => 126,
        _ if error.raw_os_error() == Some(8) => 126,
        _ => 1,
    };
    Err(diagnostic("execute", program, error, code))
}

fn replace_process(program: &OsStr, arguments: &[OsString]) -> io::Error {
    let program = CString::new(program.as_bytes()).expect("OS strings cannot contain NUL bytes");
    let mut argv = Vec::with_capacity(arguments.len() + 1);
    argv.push(program.clone());
    argv.extend(arguments.iter().map(|argument| {
        CString::new(argument.as_bytes()).expect("OS strings cannot contain NUL bytes")
    }));
    let mut argv_pointers: Vec<*const c_char> = argv.iter().map(|value| value.as_ptr()).collect();
    argv_pointers.push(std::ptr::null());

    let environment: Vec<CString> = env::vars_os()
        .map(|(key, value)| {
            let mut entry = key.into_vec();
            entry.push(b'=');
            entry.extend(value.into_vec());
            CString::new(entry).expect("environment entries cannot contain NUL bytes")
        })
        .collect();
    let mut environment_pointers: Vec<*const c_char> =
        environment.iter().map(|value| value.as_ptr()).collect();
    environment_pointers.push(std::ptr::null());

    unsafe extern "C" {
        fn execve(
            pathname: *const c_char,
            argv: *const *const c_char,
            envp: *const *const c_char,
        ) -> i32;
    }

    // Direct execve avoids execvp's shell fallback for executable-format errors.
    unsafe {
        execve(
            program.as_ptr(),
            argv_pointers.as_ptr(),
            environment_pointers.as_ptr(),
        );
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

    if let Ok(found) = which::which_in(command, Some(&path), &cwd) {
        return Ok(Some(found));
    }

    let mut inspection_error = None;
    let mut unusable = None;
    for directory in env::split_paths(&path) {
        let candidate = if directory.is_absolute() {
            directory.join(command)
        } else {
            cwd.join(directory).join(command)
        };
        match fs::metadata(&candidate) {
            Ok(metadata) if is_invokable(&metadata) => return Ok(Some(candidate)),
            Ok(_) => unusable = unusable.or(Some(candidate)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => inspection_error = inspection_error.or(Some((candidate, error))),
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
    // The specification requires the standard library's Unix account-database fallback.
    #[allow(deprecated)]
    expand_tilde_with(path, || {
        if env::var_os("HOME").is_some_and(|value| value.is_empty()) {
            // std::env::home_dir treats an empty HOME as a path, so hide it only while asking
            // for the account-database fallback and restore it before executing the child.
            env::remove_var("HOME");
            let home = env::home_dir();
            env::set_var("HOME", "");
            home
        } else {
            env::home_dir()
        }
    })
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
        Ok(home.join(OsString::from_vec(bytes[2..].to_vec())))
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
