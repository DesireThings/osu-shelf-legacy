use super::{
    Beatmapset, COLLECTIONS_PER_PAGE, Collection, CollectionPage, CollectionSort, ModeCounts,
    PopularRange, Timestamp, Uploader, popular_url, recent_url, search_url,
};

/// A page as `/search` sends it, with the fields the client ignores left in.
const SEARCH_PAGE: &str = r#"{
    "nextPageCursor": 2,
    "hasMore": true,
    "results": 845,
    "collections": [{
        "id": 44,
        "name": "speed practice",
        "uploader": { "username": "someone", "id": 7, "rank": 0 },
        "description": "fast maps",
        "beatmapCount": 601,
        "favourites": 660,
        "dateLastModified": { "_seconds": 1629586307 },
        "dateUploaded": { "_seconds": 1628459998, "_nanoseconds": 0 },
        "difficultySpread": { "1": 56, "2": 85, "10": 3 },
        "bpmSpread": { "150": 6, "160": 21 },
        "modes": { "osu": 556, "taiko": 33, "mania": 10, "fruits": 2 }
    }]
}"#;

fn sample_collection() -> Collection {
    Collection {
        id: 1,
        name: "Test".to_string(),
        description: None,
        uploader: Uploader {
            id: 1,
            username: "test".to_string(),
        },
        beatmapsets: vec![
            Beatmapset {
                id: 100,
                beatmaps: vec![],
            },
            Beatmapset {
                id: 200,
                beatmaps: vec![],
            },
        ],
        favourites: 0,
    }
}

#[test]
fn beatmapset_ids_preserves_order_and_deduplicates() {
    let mut collection = sample_collection();
    collection.beatmapsets.push(Beatmapset {
        id: 100,
        beatmaps: vec![],
    });
    assert_eq!(collection.beatmapset_ids(), vec![100, 200]);
}

#[test]
fn folder_name_sanitizes_and_appends_id() {
    let mut collection = sample_collection();
    collection.name = "weird / name *".to_string();
    assert_eq!(collection.folder_name(), "weird _ name _-1");
}

#[test]
fn folder_name_falls_back_when_name_is_blank() {
    let mut collection = sample_collection();
    collection.name = "   ".to_string();
    assert_eq!(collection.folder_name(), "collection-1");
}

#[test]
fn folder_name_prefixes_reserved_dos_device_stem() {
    let mut collection = sample_collection();
    collection.id = 7;
    collection.name = "CON.txt".to_string();
    assert_eq!(collection.folder_name(), "_CON.txt-7");
}

#[test]
fn folder_name_prefixes_bare_and_case_insensitive_reserved_names() {
    let mut collection = sample_collection();
    collection.id = 7;
    collection.name = "Com1 .log".to_string();
    assert_eq!(collection.folder_name(), "_Com1 .log-7");

    collection.name = "aux".to_string();
    assert_eq!(collection.folder_name(), "_aux-7");
}

#[test]
fn folder_name_leaves_normal_dotted_names_alone() {
    let mut collection = sample_collection();
    collection.id = 7;
    collection.name = "myset.osz".to_string();
    assert_eq!(collection.folder_name(), "myset.osz-7");
}

#[test]
fn collection_page_parses_a_search_response() {
    let page: CollectionPage = serde_json::from_str(SEARCH_PAGE).unwrap();
    assert!(page.has_more);
    assert_eq!(page.next_cursor, Some(2));
    assert_eq!(page.total, Some(845));

    let summary = &page.collections[0];
    assert_eq!(summary.id, 44);
    assert_eq!(summary.name, "speed practice");
    assert_eq!(summary.description.as_deref(), Some("fast maps"));
    assert_eq!(summary.uploader.username, "someone");
    assert_eq!(summary.beatmap_count, 601);
    assert_eq!(summary.favourites, 660);
    assert_eq!(
        summary.date_uploaded,
        Some(Timestamp {
            seconds: 1628459998
        })
    );
    assert_eq!(
        summary.modes,
        ModeCounts {
            osu: 556,
            taiko: 33,
            mania: 10,
            fruits: 2
        }
    );
    assert_eq!(summary.difficulty_spread.get(&10), Some(&3));
}

#[test]
fn collection_page_parses_the_last_page_of_a_listing() {
    let page: CollectionPage =
        serde_json::from_str(r#"{ "nextPageCursor": null, "hasMore": false, "collections": [] }"#)
            .unwrap();
    assert!(!page.has_more);
    assert_eq!(page.next_cursor, None);
    assert_eq!(page.total, None);
    assert!(page.collections.is_empty());
}

#[test]
fn collection_summary_tolerates_missing_statistics() {
    let page: CollectionPage = serde_json::from_str(
        r#"{ "collections": [{
            "id": 1,
            "name": "bare",
            "uploader": { "id": 2, "username": "someone" }
        }] }"#,
    )
    .unwrap();
    let summary = &page.collections[0];
    assert_eq!(summary.beatmap_count, 0);
    assert_eq!(summary.date_uploaded, None);
    assert_eq!(summary.modes, ModeCounts::default());
    assert!(summary.difficulty_spread.is_empty());
}

#[test]
fn search_url_sends_text_and_page_size() {
    let url = search_url("stream", CollectionSort::Relevance, None).unwrap();
    assert_eq!(url.path(), "/api/collections/search");
    assert_eq!(
        url.query(),
        Some(format!("search=stream&perPage={COLLECTIONS_PER_PAGE}").as_str())
    );
}

#[test]
fn search_url_encodes_text() {
    let url = search_url("a&sortBy=x b", CollectionSort::Relevance, None).unwrap();
    let pairs: Vec<_> = url.query_pairs().collect();
    assert_eq!(pairs[0].0, "search");
    assert_eq!(pairs[0].1, "a&sortBy=x b");
    assert_eq!(pairs.len(), 2);
}

#[test]
fn search_url_maps_each_sort() {
    for (sort, sort_by) in [
        (CollectionSort::Newest, "dateUploaded"),
        (CollectionSort::Favourites, "favourites"),
        (CollectionSort::Beatmaps, "beatmapCount"),
    ] {
        let url = search_url("", sort, Some(3)).unwrap();
        assert_eq!(
            url.query(),
            Some(
                format!(
                    "search=&sortBy={sort_by}&orderBy=desc&perPage={COLLECTIONS_PER_PAGE}&cursor=3"
                )
                .as_str()
            )
        );
    }
}

#[test]
fn popular_url_maps_each_range() {
    for (range, value) in [
        (PopularRange::Today, "today"),
        (PopularRange::Week, "week"),
        (PopularRange::Month, "month"),
        (PopularRange::Year, "year"),
        (PopularRange::AllTime, "alltime"),
    ] {
        let url = popular_url(range, None).unwrap();
        assert_eq!(url.path(), "/api/collections/popularv2");
        assert_eq!(
            url.query(),
            Some(format!("range={value}&perPage={COLLECTIONS_PER_PAGE}").as_str())
        );
    }
}

#[test]
fn recent_url_passes_the_cursor_on() {
    let url = recent_url(Some(24304)).unwrap();
    assert_eq!(url.path(), "/api/collections/recent");
    assert_eq!(
        url.query(),
        Some(format!("perPage={COLLECTIONS_PER_PAGE}&cursor=24304").as_str())
    );
}
