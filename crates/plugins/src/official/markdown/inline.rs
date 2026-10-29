//! Inline Markdown → HTML/text conversion used by
//! [`super::to_html`] / [`super::to_text`].
//!
//! Supported subset (the documented limits of this port): code spans,
//! images/links, bold (`**`), strikethrough (`~~`), and italic (`*`). The web
//! node delegates to `marked` from a CDN; this is a dependency-free subset.

/// Escape the five HTML-significant characters.
pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// Convert one line/paragraph of Markdown to inline HTML (input is escaped here).
pub fn inline_html(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('`') {
        out.push_str(&emphasis(&escape_html(&rest[..start])));
        let after = &rest[start + 1..];
        match after.find('`') {
            Some(end) => {
                out.push_str("<code>");
                out.push_str(&escape_html(&after[..end]));
                out.push_str("</code>");
                rest = &after[end + 1..];
            }
            None => {
                out.push('`');
                rest = after;
            }
        }
    }
    out.push_str(&emphasis(&escape_html(rest)));
    out
}

/// Strip Markdown inline syntax down to plain text.
pub fn inline_text(text: &str) -> String {
    let media = map_links(
        &map_links(text, "![", &|label: &str, _: &str| label.trim().to_string()),
        "[",
        &|label: &str, _: &str| label.trim().to_string(),
    );
    let mut value = media;
    for marker in ["**", "~~", "`", "*"] {
        value = value.replace(marker, "");
    }
    value.trim().to_string()
}

fn emphasis(text: &str) -> String {
    let images = map_links(text, "![", &|label: &str, url: &str| {
        if is_safe_url(url) {
            format!("<img src=\"{}\" alt=\"{}\" />", url.trim(), label.trim())
        } else {
            label.trim().to_string()
        }
    });
    let links = map_links(&images, "[", &|label: &str, url: &str| {
        if is_safe_url(url) {
            format!("<a href=\"{}\">{}</a>", url.trim(), label.trim())
        } else {
            label.trim().to_string()
        }
    });
    let strong = replace_pairs(&links, "**", "<strong>", "</strong>");
    let struck = replace_pairs(&strong, "~~", "<del>", "</del>");
    replace_pairs(&struck, "*", "<em>", "</em>")
}

fn is_safe_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    !(lower.starts_with("javascript:")
        || lower.starts_with("vbscript:")
        || lower.starts_with("data:"))
}

fn map_links(text: &str, prefix: &str, map: &dyn Fn(&str, &str) -> String) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find(prefix) {
        out.push_str(&rest[..start]);
        let after = &rest[start + prefix.len()..];
        let parsed = after.find("](").and_then(|label_end| {
            after[label_end + 2..]
                .find(')')
                .map(|url_end| (label_end, url_end))
        });
        match parsed {
            Some((label_end, url_end)) => {
                let label = &after[..label_end];
                let url = &after[label_end + 2..label_end + 2 + url_end];
                out.push_str(&map(label, url));
                rest = &after[label_end + 2 + url_end + 1..];
            }
            None => {
                out.push_str(prefix);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn replace_pairs(input: &str, delimiter: &str, open_tag: &str, close_tag: &str) -> String {
    let mut out = String::new();
    let mut rest = input;
    while let Some(start) = rest.find(delimiter) {
        out.push_str(&rest[..start]);
        let after = &rest[start + delimiter.len()..];
        match after.find(delimiter) {
            Some(end) if !after[..end].trim().is_empty() => {
                out.push_str(open_tag);
                out.push_str(&after[..end]);
                out.push_str(close_tag);
                rest = &after[end + delimiter.len()..];
            }
            _ => {
                out.push_str(delimiter);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}
