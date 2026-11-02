#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{
    BrowseCriteria, BrowseMode, BrowseSort, Listing, browse_error, browse_page, main_mode,
    star_span,
};
use osu_downloader::{
    Error,
    collection::{CollectionPage, CollectionSort, ModeCounts, PopularRange},
};
use std::{collections::BTreeMap, time::Duration};

fn criteria(query: &str, sort: BrowseSort) -> BrowseCriteria {
    BrowseCriteria {
        query: query.to_string(),
        sort,
    }
}

fn spread(buckets: &[(u8, u32)]) -> BTreeMap<u8, u32> {
    buckets.iter().copied().collect()
}

fn page(json: &str) -> CollectionPage {
    serde_json::from_str(json).unwrap()
}

const ONE_ROW: &str = r#"{
    "nextPageCursor": 2,
    "hasMore": true,
    "results": 845,
    "collections": [{
        "id": 44,
        "name": "speed practice",
        "uploader": { "username": "someone", "id": 7 },
        "description": "  fast maps\n",
        "beatmapCount": 601,
        "favourites": 660,
        "dateUploaded": { "_seconds": 1628459998 },
        "difficultySpread": { "1": 2, "2": 0, "3": 9, "4": 23, "5": 8, "6": 14, "7": 4 },
        "modes": { "osu": 556, "taiko": 33, "mania": 10, "fruits": 2 }
    }]
}"#;

#[test]
fn the_default_form_lists_popular_this_month() {
    assert_eq!(
        BrowseCriteria::default().listing(),
        Listing::Popular(PopularRange::Month)
    );
}

#[test]
fn popular_sorts_map_to_their_ranges() {
    for (sort, range) in [
        (BrowseSort::PopularWeek, PopularRange::Week),
        (BrowseSort::PopularMonth, PopularRange::Month),
        (BrowseSort::PopularYear, PopularRange::Year),
        (BrowseSort::PopularAllTime, PopularRange::AllTime),
    ] {
        assert_eq!(criteria("  ", sort).listing(), Listing::Popular(range));
    }
}

#[test]
fn search_sorts_map_to_the_client_sort() {
    for (sort, expected) in [
        (BrowseSort::Relevance, CollectionSort::Relevance),
        (BrowseSort::Newest, CollectionSort::Newest),
        (BrowseSort::Favourites, CollectionSort::Favourites),
        (BrowseSort::Maps, CollectionSort::Beatmaps),
    ] {
        assert_eq!(
            criteria(" stream ", sort).listing(),
            Listing::Search("stream", expected)
        );
    }
}

#[test]
fn search_text_wins_over_a_popular_sort() {
    assert_eq!(
        criteria("stream", BrowseSort::PopularWeek).listing(),
        Listing::Search("stream", CollectionSort::Relevance)
    );
}

#[test]
fn an_empty_search_keeps_a_chosen_sort() {
    assert_eq!(
        criteria("", BrowseSort::Maps).listing(),
        Listing::Search("", CollectionSort::Beatmaps)
    );
}

#[test]
fn criteria_read_the_names_the_frontend_sends() {
    let parsed: BrowseCriteria =
        serde_json::from_value(serde_json::json!({ "query": "x", "sort": "popular_all_time" }))
            .unwrap();
    assert_eq!(parsed, criteria("x", BrowseSort::PopularAllTime));

    let empty: BrowseCriteria = serde_json::from_value(serde_json::json!({})).unwrap();
    assert_eq!(empty, BrowseCriteria::default());
}

#[test]
fn a_page_becomes_rows_for_the_frontend() {
    let browsed = browse_page(page(ONE_ROW));
    assert_eq!(browsed.total, Some(845));
    assert_eq!(browsed.next_cursor, Some(2));

    let json = serde_json::to_value(&browsed.rows[0]).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "id": 44,
            "name": "speed practice",
            "description": "fast maps",
            "uploader": "someone",
            "difficulties": 601,
            "favourites": 660,
            "uploaded": 1628459998,
            "main_mode": "osu",
            "stars": [3, 7],
        })
    );
}

#[test]
fn a_row_without_statistics_has_no_mode_or_stars() {
    let browsed = browse_page(page(
        r#"{ "collections": [{
            "id": 1,
            "name": "bare",
            "uploader": { "id": 2, "username": "someone" },
            "description": null
        }] }"#,
    ));
    let row = &browsed.rows[0];
    assert_eq!(row.description, "");
    assert_eq!(row.main_mode, None);
    assert_eq!(row.stars, None);
    assert_eq!(row.uploaded, None);
}

#[test]
fn the_last_page_has_no_cursor() {
    let browsed = browse_page(page(
        r#"{ "nextPageCursor": 3, "hasMore": false, "collections": [] }"#,
    ));
    assert_eq!(browsed.next_cursor, None);
}

#[test]
fn main_mode_picks_the_mode_with_the_most_maps() {
    let modes = ModeCounts {
        osu: 1,
        taiko: 2,
        mania: 30,
        fruits: 4,
    };
    assert_eq!(main_mode(modes), Some(BrowseMode::Mania));
}

#[test]
fn main_mode_breaks_a_tie_towards_the_first_mode() {
    let modes = ModeCounts {
        osu: 0,
        taiko: 5,
        mania: 5,
        fruits: 5,
    };
    assert_eq!(main_mode(modes), Some(BrowseMode::Taiko));
}

#[test]
fn main_mode_is_none_for_an_empty_collection() {
    assert_eq!(main_mode(ModeCounts::default()), None);
}

#[test]
fn star_span_ignores_stray_maps_at_both_ends() {
    let buckets = spread(&[(1, 2), (3, 40), (4, 56), (9, 2)]);
    assert_eq!(star_span(&buckets), Some((3, 4)));
}

#[test]
fn star_span_keeps_a_bucket_past_the_trim() {
    let buckets = spread(&[(1, 6), (3, 88), (9, 6)]);
    assert_eq!(star_span(&buckets), Some((1, 9)));
}

#[test]
fn star_span_of_one_map_is_its_bucket() {
    assert_eq!(star_span(&spread(&[(1, 0), (6, 1), (10, 0)])), Some((6, 6)));
}

#[test]
fn star_span_is_none_without_maps() {
    assert_eq!(star_span(&spread(&[])), None);
    assert_eq!(star_span(&spread(&[(1, 0), (2, 0)])), None);
}

#[test]
fn errors_read_as_sentences() {
    let message = |err| browse_error(err).to_string();
    assert_eq!(
        message(Error::RateLimited {
            retry_after: Some(Duration::from_secs(30))
        }),
        "osu!Collector is limiting requests. Wait 30 seconds, then search again."
    );
    assert_eq!(
        message(Error::RateLimited { retry_after: None }),
        "osu!Collector is limiting requests. Wait a minute, then search again."
    );
    assert_eq!(
        message(Error::Timeout),
        "osu!Collector took too long to reply. Try again."
    );
    assert_eq!(
        message(Error::Parse("missing field".to_string())),
        "osu!Collector sent a reply osu-shelf can't read. The site may have changed."
    );
    assert_eq!(
        message(Error::HttpStatus(502)),
        "osu!Collector couldn't run this search. Try again later."
    );
    assert_eq!(
        message(Error::Network("connection refused".to_string())),
        "Couldn't reach osu!Collector. Check your connection and try again."
    );
}
