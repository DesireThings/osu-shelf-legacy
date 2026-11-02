//! Browsing osu!Collector collections, for the GUI: the search form as plain
//! values, the listing it asks for, and one page of rows.
//!
//! The API can't filter by mode, stars or size, so each row carries what a
//! frontend needs to filter the pages it has loaded.

use crate::{
    config::constants::USER_AGENT,
    utils::{AppError, Result},
};
use osu_downloader::{
    Error,
    collection::{
        CollectionClient, CollectionPage, CollectionSort, CollectionSummary, ModeCounts,
        PopularRange,
    },
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::LazyLock};

/// Share of a collection's maps ignored at each end of its star span, so one
/// stray difficulty doesn't stretch it.
const STAR_SPAN_TRIM_PERCENT: u64 = 5;

static COLLECTION_CLIENT: LazyLock<CollectionClient> =
    LazyLock::new(|| CollectionClient::with_user_agent(USER_AGENT));

/// How results are ordered. The API only sorts highest first.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowseSort {
    Relevance,
    Newest,
    Favourites,
    Maps,
    PopularWeek,
    #[default]
    PopularMonth,
    PopularYear,
    PopularAllTime,
}

/// The browse form as plain values.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowseCriteria {
    pub query: String,
    pub sort: BrowseSort,
}

/// The listing a form asks for.
#[derive(Debug, PartialEq, Eq)]
enum Listing<'a> {
    Search(&'a str, CollectionSort),
    Popular(PopularRange),
}

impl BrowseCriteria {
    /// Popular listings can't take search text, so typed text wins over a
    /// popular sort rather than being dropped.
    fn listing(&self) -> Listing<'_> {
        let text = self.query.trim();
        let popular = match self.sort {
            BrowseSort::PopularWeek => Some(PopularRange::Week),
            BrowseSort::PopularMonth => Some(PopularRange::Month),
            BrowseSort::PopularYear => Some(PopularRange::Year),
            BrowseSort::PopularAllTime => Some(PopularRange::AllTime),
            _ => None,
        };
        if let Some(range) = popular.filter(|_| text.is_empty()) {
            return Listing::Popular(range);
        }
        let sort = match self.sort {
            BrowseSort::Newest => CollectionSort::Newest,
            BrowseSort::Favourites => CollectionSort::Favourites,
            BrowseSort::Maps => CollectionSort::Beatmaps,
            _ => CollectionSort::Relevance,
        };
        Listing::Search(text, sort)
    }
}

/// The game mode a collection mostly holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BrowseMode {
    Osu,
    Taiko,
    Mania,
    Catch,
}

/// One collection in the results.
#[derive(Debug, Clone, Serialize)]
pub struct BrowseRow {
    pub id: u32,
    pub name: String,
    /// Empty when the uploader wrote none.
    pub description: String,
    pub uploader: String,
    /// Difficulties, not map sets: listings don't count sets.
    pub difficulties: u32,
    pub favourites: u32,
    /// Upload time in seconds since the Unix epoch.
    pub uploaded: Option<u64>,
    pub main_mode: Option<BrowseMode>,
    /// Lowest and highest whole-star buckets the bulk of the maps sit in:
    /// `(4, 6)` means 4.00 to 6.99. Bucket 1 also holds maps under 1 star, and
    /// bucket 10 holds everything from 10 up.
    pub stars: Option<(u8, u8)>,
}

/// One page of results.
#[derive(Debug, Clone, Serialize)]
pub struct BrowsePage {
    pub rows: Vec<BrowseRow>,
    /// Matches in all; popular listings don't say.
    pub total: Option<u32>,
    /// Pass back to [`browse`] for the next page.
    pub next_cursor: Option<u64>,
}

/// Run the search, or fetch the page after `cursor`. Makes one request.
pub async fn browse(criteria: &BrowseCriteria, cursor: Option<u64>) -> Result<BrowsePage> {
    let page = match criteria.listing() {
        Listing::Search(text, sort) => COLLECTION_CLIENT.search(text, sort, cursor).await,
        Listing::Popular(range) => COLLECTION_CLIENT.popular(range, cursor).await,
    }
    .map_err(browse_error)?;
    Ok(browse_page(page))
}

fn browse_page(page: CollectionPage) -> BrowsePage {
    BrowsePage {
        total: page.total,
        next_cursor: page.next_cursor.filter(|_| page.has_more),
        rows: page.collections.into_iter().map(browse_row).collect(),
    }
}

fn browse_row(collection: CollectionSummary) -> BrowseRow {
    BrowseRow {
        id: collection.id,
        main_mode: main_mode(collection.modes),
        stars: star_span(&collection.difficulty_spread),
        name: collection.name,
        description: collection
            .description
            .map(|text| text.trim().to_string())
            .unwrap_or_default(),
        uploader: collection.uploader.username,
        difficulties: collection.beatmap_count,
        favourites: collection.favourites,
        uploaded: collection.date_uploaded.map(|date| date.seconds),
    }
}

/// The mode with the most maps; a tie goes to the mode listed first.
fn main_mode(modes: ModeCounts) -> Option<BrowseMode> {
    [
        (BrowseMode::Osu, modes.osu),
        (BrowseMode::Taiko, modes.taiko),
        (BrowseMode::Mania, modes.mania),
        (BrowseMode::Catch, modes.fruits),
    ]
    .into_iter()
    .filter(|(_, maps)| *maps > 0)
    .rev()
    .max_by_key(|(_, maps)| *maps)
    .map(|(mode, _)| mode)
}

/// The buckets left after trimming the top and bottom of `spread`.
fn star_span(spread: &BTreeMap<u8, u32>) -> Option<(u8, u8)> {
    let total: u64 = spread.values().map(|maps| u64::from(*maps)).sum();
    Some((
        first_past_trim(spread.iter(), total)?,
        first_past_trim(spread.iter().rev(), total)?,
    ))
}

/// The first bucket where the running count of maps passes the trimmed share.
fn first_past_trim<'a>(buckets: impl Iterator<Item = (&'a u8, &'a u32)>, total: u64) -> Option<u8> {
    let mut seen = 0;
    for (bucket, maps) in buckets {
        seen += u64::from(*maps);
        if seen * 100 > total * STAR_SPAN_TRIM_PERCENT {
            return Some(*bucket);
        }
    }
    None
}

fn browse_error(err: Error) -> AppError {
    match err {
        Error::RateLimited {
            retry_after: Some(wait),
        } => AppError::api_dynamic(format!(
            "osu!Collector is limiting requests. Wait {} seconds, then search again.",
            wait.as_secs()
        )),
        Error::RateLimited { retry_after: None } => {
            AppError::api("osu!Collector is limiting requests. Wait a minute, then search again.")
        }
        Error::Timeout => AppError::api("osu!Collector took too long to reply. Try again."),
        Error::Parse(_) => AppError::api(
            "osu!Collector sent a reply osu-shelf can't read. The site may have changed.",
        ),
        Error::NotFound | Error::HttpStatus(_) => {
            AppError::api("osu!Collector couldn't run this search. Try again later.")
        }
        other => AppError::api_dynamic(format!("Couldn't reach osu!Collector: {other}")),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/engine_browse.rs"]
mod tests;
