//! osucollector.com client.
//!
//! Behind the `collection` feature.

use crate::{Error, Result, http};
use reqwest::{IntoUrl, Url};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::BTreeMap, time::Duration};
use tokio::time::sleep;
use tracing::warn;

const COLLECTOR_API_BASE: &str = "https://osucollector.com/api/collections";

/// Collections asked for per listing page, the most `search` will return.
pub const COLLECTIONS_PER_PAGE: u32 = 50;

/// Collection metadata from osucollector.com.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Collection {
    /// Collection ID.
    pub id: u32,
    /// Collection name.
    pub name: String,
    /// Collection description.
    #[serde(default)]
    pub description: Option<String>,
    /// Uploader information.
    pub uploader: Uploader,
    /// Beatmapsets in this collection.
    pub beatmapsets: Vec<Beatmapset>,
    /// Number of favourites.
    #[serde(default)]
    pub favourites: u32,
}

/// Uploader information.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Uploader {
    /// Uploader user ID.
    pub id: u32,
    /// Uploader username.
    pub username: String,
}

/// Beatmapset in a collection.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Beatmapset {
    /// Beatmapset ID.
    pub id: u32,
    /// Individual beatmaps in this set.
    #[serde(default)]
    pub beatmaps: Vec<Beatmap>,
}

/// Individual beatmap.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Beatmap {
    /// Beatmap ID.
    pub id: u32,
    /// MD5 checksum.
    pub checksum: String,
}

/// One collection in a listing: its metadata, without its map sets.
///
/// Listings don't say how many beatmapsets a collection holds, so its download
/// size isn't known until it's fetched with [`CollectionClient::fetch`].
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSummary {
    /// Collection ID.
    pub id: u32,
    /// Collection name.
    pub name: String,
    /// Collection description.
    #[serde(default)]
    pub description: Option<String>,
    /// Uploader information.
    pub uploader: Uploader,
    /// Number of beatmaps (difficulties, not beatmapsets).
    #[serde(default)]
    pub beatmap_count: u32,
    /// Number of favourites.
    #[serde(default)]
    pub favourites: u32,
    /// When the collection was uploaded.
    #[serde(default)]
    pub date_uploaded: Option<Timestamp>,
    /// When the collection last changed.
    #[serde(default)]
    pub date_last_modified: Option<Timestamp>,
    /// Beatmaps per game mode.
    #[serde(default)]
    pub modes: ModeCounts,
    /// Beatmaps per star-rating bucket, keyed 1 to 10.
    #[serde(default)]
    pub difficulty_spread: BTreeMap<u8, u32>,
}

/// A point in time as osucollector.com sends it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub struct Timestamp {
    /// Seconds since the Unix epoch.
    #[serde(rename = "_seconds")]
    pub seconds: u64,
}

/// Beatmaps per game mode in a collection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct ModeCounts {
    /// osu! (standard) beatmaps.
    #[serde(default)]
    pub osu: u32,
    /// osu!taiko beatmaps.
    #[serde(default)]
    pub taiko: u32,
    /// osu!mania beatmaps.
    #[serde(default)]
    pub mania: u32,
    /// osu!catch beatmaps.
    #[serde(default)]
    pub fruits: u32,
}

/// One page of a collection listing.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CollectionPage {
    /// The collections on this page.
    pub collections: Vec<CollectionSummary>,
    /// Whether another page follows this one.
    #[serde(rename = "hasMore", default)]
    pub has_more: bool,
    /// Pass this to the same listing to get the next page.
    #[serde(rename = "nextPageCursor", default)]
    pub next_cursor: Option<u64>,
    /// Matches across every page; only searches report it.
    #[serde(rename = "results", default)]
    pub total: Option<u32>,
}

/// How [`CollectionClient::search`] orders its results. Always highest first:
/// the API ignores requests for ascending order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CollectionSort {
    /// Best text match first; newest first when the search text is empty.
    #[default]
    Relevance,
    /// Most recently uploaded first.
    Newest,
    /// Most favourited first.
    Favourites,
    /// Most beatmaps first.
    Beatmaps,
}

impl CollectionSort {
    fn sort_by(self) -> Option<&'static str> {
        match self {
            Self::Relevance => None,
            Self::Newest => Some("dateUploaded"),
            Self::Favourites => Some("favourites"),
            Self::Beatmaps => Some("beatmapCount"),
        }
    }
}

/// The time window for [`CollectionClient::popular`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PopularRange {
    /// Popular today.
    Today,
    /// Popular this week.
    Week,
    /// Popular this month.
    #[default]
    Month,
    /// Popular this year.
    Year,
    /// Popular of all time.
    AllTime,
}

impl PopularRange {
    fn as_str(self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::Week => "week",
            Self::Month => "month",
            Self::Year => "year",
            Self::AllTime => "alltime",
        }
    }
}

impl Collection {
    /// Beatmapset IDs in this collection (preserving order, deduplicated).
    pub fn beatmapset_ids(&self) -> Vec<u32> {
        let mut seen = std::collections::HashSet::with_capacity(self.beatmapsets.len());
        self.beatmapsets
            .iter()
            .map(|b| b.id)
            .filter(|id| seen.insert(*id))
            .collect()
    }

    /// Total number of beatmaps (not beatmapsets).
    pub fn beatmap_count(&self) -> usize {
        self.beatmapsets.iter().map(|b| b.beatmaps.len()).sum()
    }

    /// Default folder name for this collection: `<sanitized-name>-<id>`.
    pub fn folder_name(&self) -> String {
        format!("{}-{}", sanitize_collection_name(&self.name), self.id)
    }
}

fn sanitize_collection_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        out.push(match c {
            '/' | '\\' | '\0' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            other => other,
        });
    }

    let trimmed = out.trim();
    if trimmed.is_empty() {
        return "collection".to_string();
    }

    // Windows resolves a path component to a DOS device when the component's
    // stem (text before the first dot, trailing spaces ignored, case-insensitive)
    // matches a reserved name such as CON, PRN, AUX, NUL, COM1-9 or LPT1-9,
    // extension included: `CON.txt` is the console. Prefixing `_` breaks that
    // resolution on Windows and is inert everywhere else.
    let stem = trimmed
        .split_once('.')
        .map_or(trimmed, |(stem, _)| stem)
        .trim_end();
    if is_reserved_device_name(stem) {
        format!("_{trimmed}")
    } else {
        trimmed.to_string()
    }
}

fn is_reserved_device_name(stem: &str) -> bool {
    matches!(
        stem.to_ascii_lowercase().as_str(),
        "con"
            | "prn"
            | "aux"
            | "nul"
            | "com1"
            | "com2"
            | "com3"
            | "com4"
            | "com5"
            | "com6"
            | "com7"
            | "com8"
            | "com9"
            | "lpt1"
            | "lpt2"
            | "lpt3"
            | "lpt4"
            | "lpt5"
            | "lpt6"
            | "lpt7"
            | "lpt8"
            | "lpt9"
    )
}

/// Client for fetching collections from osucollector.com.
#[derive(Debug, Clone)]
pub struct CollectionClient {
    client: reqwest::Client,
}

impl CollectionClient {
    /// New client backed by the library's default reqwest client.
    ///
    /// # Panics
    ///
    /// Panics if the underlying reqwest client builder fails — which only
    /// happens if the system's TLS backend cannot initialise.
    pub fn new() -> Self {
        Self::build(None)
    }

    /// Like [`new`](Self::new), but every request sends `user_agent`.
    ///
    /// # Panics
    ///
    /// Same as [`new`](Self::new).
    pub fn with_user_agent(user_agent: &str) -> Self {
        Self::build(Some(user_agent))
    }

    fn build(user_agent: Option<&str>) -> Self {
        Self {
            client: http::create_api_client(user_agent)
                .expect("failed to build default reqwest client"),
        }
    }

    /// Fetch a collection by ID. Performs a single request — wrap in a retry
    /// loop on the caller side if you need to retry, or use [`fetch_retrying`](Self::fetch_retrying).
    pub async fn fetch(&self, collection_id: u32) -> Result<Collection> {
        self.get_json(format!("{COLLECTOR_API_BASE}/{collection_id}"))
            .await
    }

    /// One page of collections matching `text` in their name, uploader or
    /// description; empty `text` matches every collection. Performs a single
    /// request and never waits on a rate limit, so callers can show the error.
    pub async fn search(
        &self,
        text: &str,
        sort: CollectionSort,
        cursor: Option<u64>,
    ) -> Result<CollectionPage> {
        self.get_json(search_url(text, sort, cursor)?).await
    }

    /// One page of the most popular collections in `range`, most popular
    /// first. Performs a single request, as [`search`](Self::search) does.
    pub async fn popular(
        &self,
        range: PopularRange,
        cursor: Option<u64>,
    ) -> Result<CollectionPage> {
        self.get_json(popular_url(range, cursor)?).await
    }

    /// One page of the newest collections. Performs a single request, as
    /// [`search`](Self::search) does.
    pub async fn recent(&self, cursor: Option<u64>) -> Result<CollectionPage> {
        self.get_json(recent_url(cursor)?).await
    }

    async fn get_json<T: DeserializeOwned>(&self, url: impl IntoUrl) -> Result<T> {
        let response = self.client.get(url).send().await?;

        let status = response.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .map(Duration::from_secs);
            return Err(Error::RateLimited { retry_after });
        }

        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(Error::NotFound);
        }

        if !status.is_success() {
            return Err(Error::HttpStatus(status.as_u16()));
        }

        let bytes = response.bytes().await?;
        serde_json::from_slice(&bytes).map_err(Into::into)
    }

    /// Fetch with the library's built-in retry policy:
    /// - on [`Error::RateLimited`]: sleep for `retry_after` (capped at 60s, defaults to 30s)
    /// - on transient [`Error::Network`] / [`Error::Timeout`]: exponential backoff (2^attempt seconds)
    /// - on any other error: return immediately
    ///
    /// `attempts` is the maximum number of tries (so `attempts = 3` means up to 2 retries).
    pub async fn fetch_retrying(&self, collection_id: u32, attempts: u8) -> Result<Collection> {
        let attempts = attempts.max(1);
        let mut last_error: Option<Error> = None;

        for attempt in 1..=attempts {
            match self.fetch(collection_id).await {
                Ok(collection) => return Ok(collection),
                Err(Error::RateLimited { retry_after }) => {
                    let delay = retry_after
                        .unwrap_or(Duration::from_secs(30))
                        .min(Duration::from_secs(60));
                    warn!(
                        attempt,
                        delay_secs = delay.as_secs(),
                        "rate limited by osucollector.com; waiting before retry"
                    );
                    sleep(delay).await;
                    last_error = Some(Error::RateLimited { retry_after });
                }
                Err(err) if err.is_transient() && attempt < attempts => {
                    let delay_secs = 2_u64.pow((attempt - 1) as u32);
                    warn!(
                        attempt,
                        remaining = attempts - attempt,
                        delay_secs,
                        error = %err,
                        "fetch collection attempt failed; retrying"
                    );
                    sleep(Duration::from_secs(delay_secs)).await;
                    last_error = Some(err);
                }
                Err(err) => return Err(err),
            }
        }

        Err(last_error.unwrap_or_else(|| Error::network("all retry attempts failed")))
    }
}

impl Default for CollectionClient {
    fn default() -> Self {
        Self::new()
    }
}

fn search_url(text: &str, sort: CollectionSort, cursor: Option<u64>) -> Result<Url> {
    let mut params = vec![("search", text.to_string())];
    if let Some(sort_by) = sort.sort_by() {
        params.push(("sortBy", sort_by.to_string()));
        params.push(("orderBy", "desc".to_string()));
    }
    listing_url("search", params, cursor)
}

fn popular_url(range: PopularRange, cursor: Option<u64>) -> Result<Url> {
    listing_url(
        "popularv2",
        vec![("range", range.as_str().to_string())],
        cursor,
    )
}

fn recent_url(cursor: Option<u64>) -> Result<Url> {
    listing_url("recent", Vec::new(), cursor)
}

fn listing_url(listing: &str, mut params: Vec<(&str, String)>, cursor: Option<u64>) -> Result<Url> {
    params.push(("perPage", COLLECTIONS_PER_PAGE.to_string()));
    if let Some(cursor) = cursor {
        params.push(("cursor", cursor.to_string()));
    }
    Url::parse_with_params(&format!("{COLLECTOR_API_BASE}/{listing}"), &params)
        .map_err(|err| Error::invalid_url(err.to_string()))
}

#[cfg(test)]
#[path = "../tests/collection.rs"]
mod tests;
