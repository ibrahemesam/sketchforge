//! Custom link targets: page links, external links and the scheme allowlist.
//!
//! A link is either an internal page reference (`data:page/id,<id>`,
//! `data:page/<name>` or `page:<name>`) or an external URL with an `http`,
//! `https` or `mailto` scheme. Everything else is refused before it can reach
//! the operating system's opener.

/// A classified link target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// Internal page: a page id or a page name.
    Page(String),
    /// External target that passed the scheme allowlist.
    External(String),
    /// Anything that would be unsafe or meaningless to open.
    Unsafe(String),
}

/// Classify `link` without touching the filesystem or the OS.
pub fn classify(link: &str) -> LinkTarget {
    let link = link.trim();
    if link.is_empty() {
        return LinkTarget::Unsafe("the link is empty".into());
    }
    if let Some(rest) = link.strip_prefix("data:page/id,") {
        return non_empty_page(rest);
    }
    if let Some(rest) = link.strip_prefix("data:page/") {
        return non_empty_page(rest);
    }
    if let Some(rest) = link.strip_prefix("page:") {
        return non_empty_page(rest);
    }
    let scheme = link
        .split_once(':')
        .map(|(s, _)| s.to_ascii_lowercase())
        .unwrap_or_default();
    match scheme.as_str() {
        "http" | "https" | "mailto" if link.len() > scheme.len() + 1 => {
            LinkTarget::External(link.to_string())
        }
        "" => LinkTarget::Unsafe("the link has no scheme".into()),
        "javascript" | "vbscript" | "file" | "data" => {
            LinkTarget::Unsafe(format!("the {scheme}: scheme is not allowed"))
        }
        other => LinkTarget::Unsafe(format!("the {other}: scheme is not allowed")),
    }
}

fn non_empty_page(value: &str) -> LinkTarget {
    let value = percent_decode(value);
    if value.trim().is_empty() {
        LinkTarget::Unsafe("the page link names no page".into())
    } else {
        LinkTarget::Page(value)
    }
}

/// Decode `%XX` escapes; draw.io percent-encodes page names in links.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(a), Some(b)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(a * 16 + b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Open an already-classified external link with the platform's opener. The
/// argument is passed directly, never through a shell.
pub fn open_external(link: &str) -> Result<(), String> {
    let status = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(link).status()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(link)
            .status()
    } else {
        std::process::Command::new("xdg-open").arg(link).status()
    };
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("the opener exited with {s}")),
        Err(e) => Err(format!("could not start the opener: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_links_accept_both_drawio_forms() {
        assert_eq!(
            classify("data:page/id,abc123"),
            LinkTarget::Page("abc123".into())
        );
        assert_eq!(
            classify("data:page/Flow%20chart"),
            LinkTarget::Page("Flow chart".into())
        );
        assert_eq!(classify("page:Details"), LinkTarget::Page("Details".into()));
        assert_eq!(
            classify("data:page/"),
            LinkTarget::Unsafe("the page link names no page".into())
        );
    }

    #[test]
    fn external_links_are_limited_to_safe_schemes() {
        assert_eq!(
            classify("https://example.com/a?b=1"),
            LinkTarget::External("https://example.com/a?b=1".into())
        );
        assert_eq!(
            classify("MAILTO:me@example.com"),
            LinkTarget::External("MAILTO:me@example.com".into())
        );
        for bad in [
            "javascript:alert(1)",
            "data:text/html,hi",
            "file:///etc/passwd",
            "vbscript:msgbox(1)",
            "ftp://example.com",
            "example.com",
        ] {
            assert!(
                matches!(classify(bad), LinkTarget::Unsafe(_)),
                "{bad} should be refused"
            );
        }
    }
}
