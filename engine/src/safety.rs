// SPDX-License-Identifier: AGPL-3.0-only

//! Untrusted-input safety: fetch policy and resource limits (CORE-578).
//!
//! Spec: `docs/specifications/untrusted-input-safety.spec.md`.
//!
//! One [`Policy`] is active per render, on the rendering thread. The
//! pipeline installs it with [`install`] (a guard restores the previous
//! policy on drop). Every resource read the document can cause (images,
//! `<link>` stylesheets, `<iframe src>`, `@font-face url()`, SVG `<image>`)
//! asks [`allow_local`] or [`allow_remote`] first. Layout asks
//! [`check_page_count`] and [`check_deadline`]; a tripped limit is recorded
//! and the pipeline turns it into a [`LimitError`] after layout returns.
//!
//! With no policy installed (the default CLI, the harness contract) every
//! check allows and records nothing, so plain renders are unchanged.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Which document-referenced resources may be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchPolicy {
    /// Legacy behavior: any local path; remote references are skipped
    /// (offline contract) without a diagnostic.
    Unrestricted,
    /// No resource reads at all (`data:` URIs still decode).
    None,
    /// Local reads confined to the policy roots; every remote reference is
    /// blocked.
    Local,
    /// `Local`, plus remote references to the listed hosts are permitted.
    AllowList(Vec<String>),
}

impl FetchPolicy {
    /// Parse a `--fetch` value. `allow-list` takes its hosts separately.
    pub fn parse(mode: &str, hosts: Option<&str>) -> Result<FetchPolicy, String> {
        match (mode, hosts) {
            ("unrestricted", _) => Ok(FetchPolicy::Unrestricted),
            ("none", _) => Ok(FetchPolicy::None),
            ("local", _) => Ok(FetchPolicy::Local),
            ("allow-list", Some(h)) => {
                let list: Vec<String> = h
                    .split(',')
                    .map(|s| s.trim().to_ascii_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect();
                if list.is_empty() {
                    Err("`--fetch allow-list` needs at least one host".to_string())
                } else {
                    Ok(FetchPolicy::AllowList(list))
                }
            }
            ("allow-list", None) => Err("`--fetch allow-list` needs a host list".to_string()),
            (other, _) => Err(format!(
                "unknown --fetch mode `{other}` (expected none, local, allow-list or unrestricted)"
            )),
        }
    }
}

/// The policy and limits for one render.
#[derive(Clone, Debug)]
pub struct Policy {
    pub fetch: FetchPolicy,
    /// Directories local reads are confined to (canonical paths).
    pub roots: Vec<PathBuf>,
    pub max_pages: Option<usize>,
    pub deadline: Option<Instant>,
    /// Largest accepted image, in pixels (width × height), checked from the
    /// image header before any decode.
    pub max_image_pixels: Option<u64>,
}

impl Policy {
    /// A policy that allows everything and limits nothing.
    pub fn unrestricted() -> Policy {
        Policy {
            fetch: FetchPolicy::Unrestricted,
            roots: Vec::new(),
            max_pages: None,
            deadline: None,
            max_image_pixels: None,
        }
    }

    /// True when this policy changes nothing (the legacy render).
    pub fn is_unrestricted(&self) -> bool {
        self.fetch == FetchPolicy::Unrestricted
            && self.max_pages.is_none()
            && self.deadline.is_none()
            && self.max_image_pixels.is_none()
    }

    /// Add a root directory (canonicalized when it exists).
    pub fn with_root(mut self, root: &Path) -> Policy {
        let r = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        if !self.roots.contains(&r) {
            self.roots.push(r);
        }
        self
    }
}

/// Stable codes for a limit that stopped a render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitKind {
    PageCount,
    Timeout,
    InputSize,
    ImageSize,
}

impl LimitKind {
    pub fn code(self) -> &'static str {
        match self {
            LimitKind::PageCount => "LIMIT_PAGE_COUNT",
            LimitKind::Timeout => "LIMIT_TIMEOUT",
            LimitKind::InputSize => "LIMIT_INPUT_SIZE",
            LimitKind::ImageSize => "LIMIT_IMAGE_SIZE",
        }
    }
}

/// A render stopped by a limit. No PDF is produced.
#[derive(Clone, Debug)]
pub struct LimitError {
    pub kind: LimitKind,
    pub message: String,
    /// `fetch-blocked` messages collected before the stop.
    pub blocked: Vec<String>,
}

impl std::fmt::Display for LimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.kind.code(), self.message)
    }
}

impl std::error::Error for LimitError {}

#[derive(Default)]
struct State {
    policy: Option<Policy>,
    tripped: Option<(LimitKind, String)>,
    blocked: Vec<String>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Restores the previous state when dropped.
pub struct Guard {
    prev: Option<State>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        let prev = self.prev.take().unwrap_or_default();
        STATE.with(|s| *s.borrow_mut() = prev);
    }
}

/// Make `policy` active on this thread until the guard drops.
pub fn install(policy: Policy) -> Guard {
    let fresh = State {
        policy: Some(policy),
        tripped: None,
        blocked: Vec::new(),
    };
    let prev = STATE.with(|s| std::mem::replace(&mut *s.borrow_mut(), fresh));
    Guard { prev: Some(prev) }
}

fn with_policy<R>(f: impl FnOnce(&Policy) -> R) -> Option<R> {
    STATE.with(|s| s.borrow().policy.as_ref().map(f))
}

fn record_blocked(msg: String) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if !s.blocked.contains(&msg) {
            s.blocked.push(msg);
        }
    });
}

fn trip(kind: LimitKind, msg: String) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.tripped.is_none() {
            s.tripped = Some((kind, msg));
        }
    });
}

/// True once any limit has tripped (layout uses it to stop early).
pub fn tripped() -> bool {
    STATE.with(|s| s.borrow().tripped.is_some())
}

/// Drain the `fetch-blocked` messages (in first-seen order).
pub fn take_blocked() -> Vec<String> {
    STATE.with(|s| std::mem::take(&mut s.borrow_mut().blocked))
}

/// The tripped limit, if any, as an error carrying the blocked messages.
pub fn take_limit_error() -> Option<LimitError> {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let (kind, message) = s.tripped.take()?;
        Some(LimitError {
            kind,
            message,
            blocked: std::mem::take(&mut s.blocked),
        })
    })
}

/// Lexically normalize `.` and `..` components (no filesystem access).
fn normalize(p: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// May the document read the local file `path`? Under a restricted policy
/// the REAL path (symlinks resolved) must lie inside a root; a refusal is
/// recorded as a `fetch-blocked` diagnostic. `what` names the reference
/// (`img`, `link`, ...) for the message.
pub fn allow_local(path: &Path, what: &str) -> bool {
    let verdict = with_policy(|p| match &p.fetch {
        FetchPolicy::Unrestricted => Ok(()),
        FetchPolicy::None => Err(format!(
            "{what} `{}` blocked: --fetch none allows no resource reads",
            path.display()
        )),
        FetchPolicy::Local | FetchPolicy::AllowList(_) => {
            let abs = if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir().unwrap_or_default().join(path)
            };
            // Canonical path when the file exists (resolves symlinks);
            // otherwise the lexical form (a missing file reads nothing, but
            // an escape attempt is still reported).
            let real = std::fs::canonicalize(&abs).unwrap_or_else(|_| normalize(&abs));
            if p.roots.iter().any(|r| real.starts_with(r)) {
                Ok(())
            } else {
                Err(format!(
                    "{what} `{}` blocked: outside the allowed root",
                    path.display()
                ))
            }
        }
    });
    match verdict {
        None | Some(Ok(())) => true,
        Some(Err(msg)) => {
            record_blocked(msg);
            false
        }
    }
}

/// The host of an `http(s)://` URL, lowercased, without port or userinfo.
fn url_host(url: &str) -> Option<String> {
    let lower = url.trim().to_ascii_lowercase();
    let rest = lower
        .strip_prefix("http://")
        .or_else(|| lower.strip_prefix("https://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    let host = if let Some(v6) = host.strip_prefix('[') {
        v6.split(']').next()?.to_string()
    } else {
        host.split(':').next()?.to_string()
    };
    (!host.is_empty()).then_some(host)
}

/// May the document reference the remote (or non-file scheme) URL `url`?
/// `Unrestricted` allows silently (the renderer is offline and skips it
/// anyway). `AllowList` allows listed `http(s)` hosts. Everything else is
/// recorded as `fetch-blocked`.
pub fn allow_remote(url: &str, what: &str) -> bool {
    let verdict = with_policy(|p| match &p.fetch {
        FetchPolicy::Unrestricted => Ok(()),
        FetchPolicy::AllowList(hosts) => match url_host(url) {
            Some(h) if hosts.contains(&h) => Ok(()),
            _ => Err(format!("{what} `{url}` blocked: host not in --fetch allow-list")),
        },
        FetchPolicy::None | FetchPolicy::Local => Err(format!(
            "{what} `{url}` blocked: remote references are not fetched under this policy"
        )),
    });
    match verdict {
        None | Some(Ok(())) => true,
        Some(Err(msg)) => {
            record_blocked(msg);
            false
        }
    }
}

/// True when a restricted fetch policy is active (SVG nested image hrefs
/// are refused outright then).
pub fn restricted_fetch() -> bool {
    with_policy(|p| p.fetch != FetchPolicy::Unrestricted).unwrap_or(false)
}

/// Layout asks before starting page `pages_done + 1`. Returns false (and
/// trips `LIMIT_PAGE_COUNT`) when that page would exceed `max_pages`.
pub fn check_page_count(pages_done: usize) -> bool {
    let limit = with_policy(|p| p.max_pages).flatten();
    match limit {
        Some(max) if pages_done >= max => {
            trip(
                LimitKind::PageCount,
                format!("the document needs more than {max} pages (--max-pages {max})"),
            );
            false
        }
        _ => true,
    }
}

/// False (and trips `LIMIT_TIMEOUT`) once the deadline has passed.
pub fn check_deadline() -> bool {
    let deadline = with_policy(|p| p.deadline).flatten();
    match deadline {
        Some(d) if Instant::now() >= d => {
            trip(LimitKind::Timeout, "the render exceeded --timeout".to_string());
            false
        }
        _ => true,
    }
}

/// False (and trips `LIMIT_IMAGE_SIZE`) when a `w`×`h` image exceeds the
/// pixel budget. Called with header dimensions, before decode.
pub fn check_image_size(w: u32, h: u32) -> bool {
    let limit = with_policy(|p| p.max_image_pixels).flatten();
    match limit {
        Some(max) if (w as u64) * (h as u64) > max => {
            trip(
                LimitKind::ImageSize,
                format!("an image of {w}x{h} pixels exceeds --max-image-pixels {max}"),
            );
            false
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_policy_allows_everything() {
        assert!(allow_local(Path::new("/etc/passwd"), "img"));
        assert!(allow_remote("http://example.com/x.png", "img"));
        assert!(check_page_count(1_000_000));
        assert!(check_deadline());
        assert!(take_blocked().is_empty());
    }

    #[test]
    fn local_policy_confines_to_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("in.png"), b"x").unwrap();
        std::fs::write(dir.path().join("out.png"), b"x").unwrap();
        let _g = install(Policy {
            fetch: FetchPolicy::Local,
            ..Policy::unrestricted()
        }
        .with_root(&root));
        assert!(allow_local(&root.join("in.png"), "img"));
        assert!(!allow_local(&root.join("../out.png"), "img"));
        assert!(!allow_local(Path::new("/etc/passwd"), "img"));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path().join("out.png"), root.join("link.png")).unwrap();
            assert!(!allow_local(&root.join("link.png"), "img"), "symlink escape");
        }
        assert!(!allow_remote("https://example.com/a.png", "img"));
        let blocked = take_blocked();
        assert!(blocked.iter().all(|m| m.contains("blocked")), "{blocked:?}");
        assert!(blocked.len() >= 3, "{blocked:?}");
    }

    #[test]
    fn allow_list_matches_hosts() {
        let _g = install(Policy {
            fetch: FetchPolicy::parse("allow-list", Some("cdn.example.com")).unwrap(),
            ..Policy::unrestricted()
        });
        assert!(allow_remote("https://CDN.example.com:443/a.png", "img"));
        assert!(!allow_remote("https://evil.example.com/a.png", "img"));
        assert!(!allow_remote("https://cdn.example.com@evil.com/a.png", "img"));
    }

    #[test]
    fn guard_restores_previous_state() {
        {
            let _g = install(Policy {
                fetch: FetchPolicy::None,
                ..Policy::unrestricted()
            });
            assert!(!allow_local(Path::new("/x"), "img"));
        }
        assert!(allow_local(Path::new("/x"), "img"));
    }

    #[test]
    fn page_limit_trips_once() {
        let _g = install(Policy {
            max_pages: Some(2),
            ..Policy::unrestricted()
        });
        assert!(check_page_count(1));
        assert!(!check_page_count(2));
        let e = take_limit_error().unwrap();
        assert_eq!(e.kind.code(), "LIMIT_PAGE_COUNT");
    }
}
