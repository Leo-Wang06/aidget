//! Widget CSP: declared data origins never authorize executable remote content.

use std::collections::BTreeSet;

use base64::{engine::general_purpose::STANDARD, Engine};
use lol_html::{text, HtmlRewriter, Settings};
use sha2::{Digest, Sha256};

use crate::common::error::{Error, Result};
use crate::widgets::{model::WidgetPermission, package::WidgetConfig};

/// Build a policy from the same bytes returned in an HTML response. Non-HTML
/// resources pass `None` and receive no inline-script grants.
pub fn for_document(config: &WidgetConfig, origin: &str, html: Option<&[u8]>) -> Result<String> {
    let local = format!("{origin}/content/{}/", config.id);
    let remote = config.network.join(" ");
    let assets = format!("{local} {remote} data: blob:");
    let scripts = if config.permissions.contains(&WidgetPermission::Script) {
        let mut sources = vec![local];
        if let Some(html) = html {
            sources.extend(inline_script_hashes(html)?);
        }
        sources.join(" ")
    } else {
        "'none'".into()
    };
    let connect = if remote.is_empty() { "'none'" } else { &remote };
    Ok(format!("default-src 'none'; base-uri 'none'; form-action 'none'; object-src 'none'; frame-src 'none'; worker-src 'none'; script-src {scripts}; script-src-attr 'none'; style-src {assets} 'unsafe-inline'; img-src {assets}; font-src {assets}; media-src {assets}; connect-src {connect}"))
}

fn inline_script_hashes(bytes: &[u8]) -> Result<BTreeSet<String>> {
    let html = std::str::from_utf8(bytes)
        .map_err(|error| Error::InvalidParams(format!("widget HTML must be UTF-8: {error}")))?;
    // HTML tokenization normalizes CRLF/CR and replaces NUL before script execution.
    let html = html
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\0', "\u{fffd}");
    let mut hashes = BTreeSet::new();
    let mut script = String::new();
    let mut rewriter = HtmlRewriter::new(
        Settings::new().append_element_content_handler(text!("script:not([src])", |chunk| {
            script.push_str(chunk.as_str());
            if chunk.last_in_text_node() {
                hashes.insert(format!(
                    "'sha256-{}'",
                    STANDARD.encode(Sha256::digest(script.as_bytes()))
                ));
                script.clear();
            }
            Ok(())
        })),
        |_: &[u8]| {},
    );
    rewriter
        .write(html.as_bytes())
        .map_err(|error| Error::InvalidParams(format!("cannot parse widget HTML: {error}")))?;
    rewriter
        .end()
        .map_err(|error| Error::InvalidParams(format!("cannot parse widget HTML: {error}")))?;
    Ok(hashes)
}
