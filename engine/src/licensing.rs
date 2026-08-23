//! License resolution (CORE-115, iteration one — mocked verification).
//!
//! Typeanvil ships as a commercial closed-source binary. A self-hosted
//! install activates by providing a license file; an unlicensed install
//! still renders but stamps a visible watermark on every page (Prince's
//! model). This module owns only *resolution*: finding the license file,
//! classifying what was found, and deciding licensed-vs-missing.
//!
//! Iteration one mocks verification: every found source parses into a
//! valid [`License`], and a missing file also resolves to
//! [`LicenseState::Valid`] — every install is treated as licensed. The
//! enum shapes and error paths exist now so the real Ed25519 verifier
//! later swaps into one function body without touching callers.
//!
//! Debug builds (`debug_assertions`) never touch the filesystem: they are
//! always licensed, so the harness and tests exercise the clean path.

use std::path::{Path, PathBuf};

/// How the resolved license was located. Distinguishes the three lookup
/// sources in tests; carries no behavior.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LicenseSource {
    /// The `--license <path>` CLI argument.
    CommandLine(PathBuf),
    /// The `TYPEANVIL_LICENSE` environment variable.
    EnvVar(PathBuf),
    /// `license.dat` next to the executable.
    ExecutableAdjacent(PathBuf),
}

impl LicenseSource {
    /// The file this source points at.
    pub fn path(&self) -> &Path {
        match self {
            LicenseSource::CommandLine(p)
            | LicenseSource::EnvVar(p)
            | LicenseSource::ExecutableAdjacent(p) => p,
        }
    }
}

/// A parsed license. Iteration one carries only mock fields; edition and
/// expiry/seat fields are reserved for the real verifier issue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct License {
    pub customer: String,
    pub edition: Edition,
}

/// License tier. Fields reserved; unused in iteration one beyond the mock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edition {
    Trial,
    Standard,
    Enterprise,
}

/// Outcome of resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LicenseState {
    Valid(License),
    Missing,
}

/// A found-but-unusable license. Missing is NOT an error (it watermarks);
/// broken files fail loudly so a user who believed they were licensed is
/// never silently watermarked (spec Behavior §5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LicenseError {
    /// The path could not be read (missing directory, permissions, ...).
    Unreadable { path: PathBuf, reason: String },
    /// Read fine but does not parse as a license structure.
    Malformed { path: PathBuf, reason: String },
}

impl std::fmt::Display for LicenseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LicenseError::Unreadable { path, reason } => {
                write!(f, "license file {} unreadable: {reason}", path.display())
            }
            LicenseError::Malformed { path, reason } => {
                write!(f, "license file {} malformed: {reason}", path.display())
            }
        }
    }
}

impl std::error::Error for LicenseError {}

/// Filename probed next to the executable (rule 3 of the lookup order).
const LICENSE_FILENAME: &str = "license.dat";

/// Resolve the license for this process, following the fixed lookup order:
///
/// 1. `--license <path>` CLI argument,
/// 2. `TYPEANVIL_LICENSE` environment variable,
/// 3. `license.dat` adjacent to the running executable (`current_exe()`,
///    never the working directory),
/// 4. nothing → `Ok(LicenseState::Missing)` — not an error.
///
/// Debug builds short-circuit to licensed without touching the filesystem.
pub fn resolve(cli_path: Option<&Path>) -> Result<LicenseState, LicenseError> {
    if cfg!(debug_assertions) {
        return Ok(LicenseState::Valid(mock_license()));
    }
    resolve_with_exe(
        cli_path,
        || std::env::var_os("TYPEANVIL_LICENSE").filter(|v| !v.is_empty()),
        || std::env::current_exe().ok(),
    )
}

/// The full probe, parameterized for tests: env value accessor and exe-path
/// accessor are injected so tests drive lookup order and adjacency without
/// mutating process-global state or spawning binaries.
pub fn resolve_with_exe<E, X>(
    cli_path: Option<&Path>,
    env_license: E,
    exe_path: X,
) -> Result<LicenseState, LicenseError>
where
    E: FnOnce() -> Option<std::ffi::OsString>,
    X: FnOnce() -> Option<PathBuf>,
{
    let mut found: Option<LicenseSource> = None;

    // 1. CLI arg wins outright.
    if let Some(p) = cli_path {
        found = Some(LicenseSource::CommandLine(p.to_path_buf()));
    }
    // 2. Environment variable (empty string counts as unset).
    if found.is_none() {
        if let Some(v) = env_license() {
            found = Some(LicenseSource::EnvVar(PathBuf::from(v)));
        }
    }
    // 3. `license.dat` beside the executable. If `current_exe()` fails we
    //    skip this probe entirely (spec edge case) and fall to Missing.
    if found.is_none() {
        if let Some(exe) = exe_path() {
            let adjacent = exe.parent().unwrap_or(Path::new(".")).join(LICENSE_FILENAME);
            if adjacent.exists() {
                found = Some(LicenseSource::ExecutableAdjacent(adjacent));
            }
        }
    }

    match found {
        None => Ok(LicenseState::Missing),
        // Iteration-one mock: any found source parses into a valid license.
        // The real verifier replaces ONLY this branch's body.
        Some(source) => parse_license(source.path()).map(|license| LicenseState::Valid(license)),
    }
}

/// Parse license bytes into a [`License`]. Iteration-one mock: the file must
/// be readable; its contents are accepted wholesale (a later issue swaps in
/// Ed25519 signature verification here). Unreadable files surface as
/// [`LicenseError::Unreadable`] so a user who pointed us at a bad path hears
/// about it instead of silently rendering unlicensed.
fn parse_license(path: &Path) -> Result<License, LicenseError> {
    let contents = std::fs::read(path).map_err(|e| LicenseError::Unreadable {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;
    let _ = contents;
    Ok(mock_license())
}

/// The hardcoded iteration-one license.
fn mock_license() -> License {
    License {
        customer: "development".to_string(),
        edition: Edition::Trial,
    }
}
