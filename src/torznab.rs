//! Parsing and URL construction for the Torznab protocol
//! (<https://torznab.github.io/spec-1.3-draft/index.html>).
//!
//! Torznab is used here in preference to scraping an arbitrary web UI: it
//! supplies a documented capability check (`?t=caps`) and a stable XML
//! response format, so a candidate endpoint can be validated before it is
//! ever trusted for a search.

use crate::torrent::Torrent;
use anyhow::{Context, Result, bail};
use quick_xml::{Reader, escape::unescape, events::Event};
use reqwest::Url;
use std::collections::HashMap;

pub fn build_url<'a, const N: usize>(
    endpoint: &Url,
    pairs: [(&'a str, &'a str); N],
    api_key: Option<&str>,
) -> Url {
    let mut url = endpoint.clone();
    {
        let mut query = url.query_pairs_mut();
        for (key, value) in pairs {
            query.append_pair(key, value);
        }
        if let Some(key) = api_key {
            query.append_pair("apikey", key);
        }
    }
    url
}

/// Returns true if `xml` is a Torznab capability document, i.e. its root
/// element is `<caps>`. This is the sole basis for treating an HTTP service
/// as a Torznab indexer.
pub fn is_caps_document(xml: &str) -> bool {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) | Ok(Event::Empty(event)) => {
                return local_name(event.name().as_ref()) == "caps";
            }
            Ok(Event::Decl(_) | Event::Comment(_) | Event::DocType(_) | Event::Text(_)) => {}
            Ok(Event::Eof) | Err(_) => return false,
            _ => {}
        }
    }
}

pub fn is_error_document(xml: &str) -> bool {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) | Ok(Event::Empty(event)) => {
                return local_name(event.name().as_ref()) == "error";
            }
            Ok(Event::Decl(_) | Event::Comment(_) | Event::DocType(_) | Event::Text(_)) => {}
            Ok(Event::Eof) | Err(_) => return false,
            _ => {}
        }
    }
}

/// Parses a Torznab RSS search response into normalized [`Torrent`] records.
/// Items without a title are dropped as malformed rather than surfaced as
/// empty results.
pub fn parse_feed(xml: &str) -> Result<Vec<Torrent>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut results = Vec::new();
    let mut current: Option<Torrent> = None;
    let mut field: Option<String> = None;

    loop {
        match reader.read_event()? {
            Event::Start(event) => {
                let name = local_name(event.name().as_ref()).to_string();
                if name == "item" {
                    current = Some(Torrent::default());
                    field = None;
                } else if let Some(item) = current.as_mut() {
                    if name == "enclosure" {
                        let attributes = attributes(&event)?;
                        if let Some(url) = attributes.get("url") {
                            set_link(item, url);
                        }
                        if item.size_bytes.is_none() {
                            item.size_bytes = attributes
                                .get("length")
                                .and_then(|value| value.parse().ok());
                        }
                        field = None;
                    } else if name == "attr" {
                        let attributes = attributes(&event)?;
                        set_torznab_attr(item, &attributes);
                        field = None;
                    } else {
                        field = Some(name);
                    }
                }
            }
            Event::Empty(event) => {
                if let Some(item) = current.as_mut() {
                    match local_name(event.name().as_ref()) {
                        "enclosure" => {
                            let attributes = attributes(&event)?;
                            if let Some(url) = attributes.get("url") {
                                set_link(item, url);
                            }
                            if item.size_bytes.is_none() {
                                item.size_bytes = attributes
                                    .get("length")
                                    .and_then(|value| value.parse().ok());
                            }
                        }
                        "attr" => set_torznab_attr(item, &attributes(&event)?),
                        _ => {}
                    }
                }
            }
            Event::Text(text) => {
                if let (Some(item), Some(name)) = (current.as_mut(), field.as_deref()) {
                    let value = decode_xml_text(text.as_ref())?;
                    set_text_field(item, name, value);
                }
            }
            Event::CData(text) => {
                if let (Some(item), Some(name)) = (current.as_mut(), field.as_deref()) {
                    let value = decode_xml_text(text.as_ref())?;
                    set_text_field(item, name, value);
                }
            }
            Event::End(event) => {
                let name = local_name(event.name().as_ref()).to_string();
                if name == "item" {
                    if let Some(item) = current.take().filter(|item| !item.title.is_empty()) {
                        results.push(item);
                    }
                    field = None;
                } else if field.as_deref() == Some(name.as_str()) {
                    field = None;
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(results)
}

/// Parses a response, treating a Torznab `<error>` document as a hard
/// failure rather than an empty result set.
pub fn parse_search_response(body: &str) -> Result<Vec<Torrent>> {
    if is_error_document(body) {
        bail!("indexer returned a Torznab error response");
    }
    parse_feed(body).context("invalid Torznab XML")
}

fn local_name(name: &[u8]) -> &str {
    std::str::from_utf8(name)
        .unwrap_or_default()
        .rsplit(':')
        .next()
        .unwrap_or_default()
}

fn attributes(event: &quick_xml::events::BytesStart<'_>) -> Result<HashMap<String, String>> {
    event
        .attributes()
        .map(|attribute| {
            let attribute = attribute?;
            let key = local_name(attribute.key.as_ref()).to_string();
            let value = decode_xml_text(attribute.value.as_ref())?;
            Ok((key, value))
        })
        .collect()
}

fn decode_xml_text(value: &[u8]) -> Result<String> {
    let value = std::str::from_utf8(value).context("XML was not UTF-8")?;
    Ok(unescape(value)?.into_owned())
}

fn set_link(item: &mut Torrent, value: &str) {
    if value.starts_with("magnet:") {
        item.magnet = Some(value.to_string());
    } else if !value.is_empty() {
        item.download = Some(value.to_string());
    }
}

fn set_torznab_attr(item: &mut Torrent, attributes: &HashMap<String, String>) {
    let (Some(name), Some(value)) = (attributes.get("name"), attributes.get("value")) else {
        return;
    };
    match name.as_str() {
        "magneturl" => item.magnet = Some(value.clone()),
        "seeders" => item.seeders = value.parse().ok(),
        "peers" => item.leechers = value.parse().ok(),
        "leechers" => item.leechers = value.parse().ok(),
        "grabs" | "downloads" => item.grabs = value.parse().ok(),
        "size" => item.size_bytes = value.parse().ok(),
        "infohash" if item.magnet.is_none() => {
            item.magnet = Some(format!("magnet:?xt=urn:btih:{value}"));
        }
        _ => {}
    }
}

fn set_text_field(item: &mut Torrent, name: &str, value: String) {
    if value.is_empty() {
        return;
    }
    match name {
        "title" => item.title = value,
        "guid" | "link" => {
            if value.starts_with("magnet:") {
                item.magnet = Some(value);
            } else if name == "link" {
                item.details = Some(value);
            }
        }
        "category" => item.category = Some(value),
        "pubDate" | "published" => item.published = Some(value),
        "size" if item.size_bytes.is_none() => item.size_bytes = value.parse().ok(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_torznab_result_feed() {
        let xml = include_str!("../tests/fixtures/search_feed.xml");
        let results = parse_feed(xml).expect("feed parses");
        assert_eq!(results.len(), 2);
        let result = &results[0];
        assert_eq!(result.title, "Ubuntu & Friends");
        assert_eq!(result.magnet.as_deref(), Some("magnet:?xt=urn:btih:abc"));
        assert_eq!(
            result.download.as_deref(),
            Some("https://indexer.example/download/42")
        );
        assert_eq!(result.seeders, Some(27));
        assert_eq!(result.leechers, Some(4));
        assert_eq!(result.size_bytes, Some(1_073_741_824));
    }

    #[test]
    fn drops_items_without_a_title() {
        let xml = include_str!("../tests/fixtures/search_feed.xml");
        let results = parse_feed(xml).expect("feed parses");
        assert!(results.iter().all(|item| !item.title.is_empty()));
    }

    #[test]
    fn rejects_torznab_error_documents() {
        let xml = include_str!("../tests/fixtures/error_response.xml");
        assert!(parse_search_response(xml).is_err());
    }

    #[test]
    fn treats_malformed_xml_as_a_parse_failure() {
        let xml = include_str!("../tests/fixtures/malformed.xml");
        assert!(parse_search_response(xml).is_err());
    }

    #[test]
    fn treats_an_empty_feed_as_zero_results_not_an_error() {
        let xml = include_str!("../tests/fixtures/empty_feed.xml");
        let results = parse_search_response(xml).expect("empty feed is valid");
        assert!(results.is_empty());
    }

    #[test]
    fn identifies_caps_documents() {
        let caps = include_str!("../tests/fixtures/caps.xml");
        assert!(is_caps_document(caps));
        assert!(!is_caps_document("<html><title>not torznab</title></html>"));
    }

    #[test]
    fn preserves_existing_endpoint_query_parameters() {
        let endpoint = Url::parse("https://example.test/torznab?profile=linux").unwrap();
        let url = build_url(&endpoint, [("t", "search"), ("q", "ubuntu")], Some("key"));
        assert_eq!(
            url.as_str(),
            "https://example.test/torznab?profile=linux&t=search&q=ubuntu&apikey=key"
        );
    }
}
