<script lang="ts">
  import { onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import type { BrowseMode, BrowseRow, BrowseSort } from "./api";
  import {
    addCollection,
    browseMore,
    browseSearch,
    entryForCollection,
    matchesBrowseFilters,
    noBrowseFilters,
    removeEntry,
    starBounds,
    store,
  } from "./store.svelte";

  let { onReview }: { onReview: () => void } = $props();

  const b = store.browse;

  const popularSorts: { id: BrowseSort; label: string }[] = [
    { id: "popular_week", label: "Popular this week" },
    { id: "popular_month", label: "Popular this month" },
    { id: "popular_year", label: "Popular this year" },
    { id: "popular_all_time", label: "Popular of all time" },
  ];
  const listSorts: { id: BrowseSort; label: string }[] = [
    { id: "newest", label: "Newest" },
    { id: "favourites", label: "Most favourites" },
    { id: "maps", label: "Most difficulties" },
  ];
  const modes: { id: BrowseMode; label: string }[] = [
    { id: "osu", label: "osu!" },
    { id: "taiko", label: "osu!taiko" },
    { id: "mania", label: "osu!mania" },
    { id: "catch", label: "osu!catch" },
  ];

  onMount(() => {
    if (!b.searched) void browseSearch();
  });

  const hasText = $derived(b.criteria.query.trim() !== "");
  /** Popular listings can't take search text. */
  const sorts = $derived(
    hasText
      ? [{ id: "relevance" as const, label: "Best match" }, ...listSorts]
      : [...popularSorts, ...listSorts],
  );

  // Keep the dropdown on a sort it offers as text comes and goes.
  $effect(() => {
    const sort = b.criteria.sort;
    if (hasText && sort.startsWith("popular_")) b.criteria.sort = "relevance";
    if (!hasText && sort === "relevance") b.criteria.sort = "popular_month";
  });

  const filtering = $derived(
    (Object.keys(noBrowseFilters) as (keyof typeof noBrowseFilters)[]).some(
      (key) => b.filters[key] !== noBrowseFilters[key],
    ),
  );
  const visibleRows = $derived(b.rows.filter((row) => matchesBrowseFilters(row, b.filters)));
  const listed = $derived(store.entries.filter((e) => e.state === "ready").length);
  const busy = $derived(b.searching || b.loadingMore);

  const plural = (n: number, one: string, many: string) =>
    `${n.toLocaleString()} ${n === 1 ? one : many}`;

  const resultLine = $derived.by(() => {
    const checked = b.rows.length.toLocaleString();
    const of = b.total === null ? "" : ` of ${b.total.toLocaleString()}`;
    return filtering
      ? `${plural(visibleRows.length, "match", "matches")} in ${checked}${of} checked`
      : `Showing ${checked}${of}`;
  });

  function starLabel(stars: [number, number]): string {
    const [from, to] = starBounds(stars);
    if (from === 10) return "10+★";
    return `${from}–${to === Infinity ? "10+" : to}★`;
  }

  /** The row's one meta line. */
  function meta(row: BrowseRow): string {
    const mode = modes.find((m) => m.id === row.main_mode);
    return [
      `by ${row.uploader}`,
      plural(row.difficulties, "difficulty", "difficulties"),
      `♥ ${row.favourites.toLocaleString()}`,
      row.uploaded !== null
        ? new Date(row.uploaded * 1000).toLocaleDateString(undefined, { dateStyle: "medium" })
        : null,
      mode ? `mostly ${mode.label}` : null,
      row.stars ? starLabel(row.stars) : null,
    ]
      .filter((part): part is string => part !== null)
      .join(" · ");
  }

  function add(row: BrowseRow) {
    const failed = entryForCollection(row.id);
    if (failed) removeEntry(failed.key);
    addCollection(String(row.id));
  }
</script>

<header class="page-head">
  <h1>Browse collections</h1>
  <p class="muted">
    Search osu!Collector's collections and add the ones you want. Results come from
    osucollector.com; osu-shelf isn't affiliated with it.
  </p>
</header>

<div class="browse-layout">
  <form
    class="card browse-form"
    onsubmit={(e) => {
      e.preventDefault();
      browseSearch();
    }}
  >
    <div class="add-row">
      <input
        class="text-input"
        bind:value={b.criteria.query}
        placeholder="Search collections…"
        aria-label="Search collections"
      />
      <button class="primary" type="submit" disabled={busy}>
        {b.searching ? "Searching…" : "Search"}
      </button>
    </div>
    <label class="field sort">
      <span class="field-label">Sort</span>
      <select bind:value={b.criteria.sort}>
        {#each sorts as sort (sort.id)}<option value={sort.id}>{sort.label}</option>{/each}
      </select>
    </label>

    <div class="grid">
      <label class="field">
        <span class="field-label">Min difficulties</span>
        <input class="text-input" type="number" min="0" bind:value={b.filters.minDifficulties} />
      </label>
      <label class="field">
        <span class="field-label">Max difficulties</span>
        <input class="text-input" type="number" min="0" bind:value={b.filters.maxDifficulties} />
      </label>
      <label class="field">
        <span class="field-label">Min stars</span>
        <input class="text-input" type="number" min="0" step="0.5" bind:value={b.filters.minStars} />
      </label>
      <label class="field">
        <span class="field-label">Max stars</span>
        <input class="text-input" type="number" min="0" step="0.5" bind:value={b.filters.maxStars} />
      </label>
      <label class="field">
        <span class="field-label">Main mode</span>
        <select bind:value={b.filters.mode}>
          <option value="">Any</option>
          {#each modes as mode (mode.id)}<option value={mode.id}>{mode.label}</option>{/each}
        </select>
      </label>
    </div>
    <p class="muted small">
      Filters apply to the results loaded so far, and sizes count difficulties, not map sets.
      {#if filtering}
        <button type="button" class="link" onclick={() => (b.filters = { ...noBrowseFilters })}>
          Clear filters
        </button>
      {/if}
    </p>
  </form>

  <div class="browse-results">
    {#if b.error && b.rows.length === 0}
      <section class="card"><p class="error-text">{b.error}</p></section>
    {/if}

    {#if b.searching && b.rows.length === 0}
      <section class="card"><p class="muted">Searching…</p></section>
    {:else if b.searched && b.rows.length === 0 && !b.error}
      <section class="card"><p class="muted">No collections match. Try other words.</p></section>
    {:else if b.rows.length > 0}
      <p class="muted small result-line">{resultLine}</p>
      {#if visibleRows.length === 0}
        <section class="card">
          <p class="muted">
            None of the loaded collections pass these filters.
            {#if b.nextCursor !== null}Load more to keep looking, or change the filters.{/if}
          </p>
        </section>
      {/if}

      <ul class="entry-list">
        {#each visibleRows as row (row.id)}
          {@const entry = entryForCollection(row.id)}
          <li class="card entry-card">
            <div class="entry">
              <div class="entry-main">
                <span class="entry-title">{row.name}</span>
                <span class="muted small">{meta(row)}</span>
                {#if row.description}
                  <span class="muted small description">{row.description}</span>
                {/if}
                {#if entry?.state === "error"}
                  <span class="error-text small">{entry.message}</span>
                {/if}
              </div>
              <div class="entry-buttons">
                <button
                  class="secondary"
                  onclick={() => openUrl(`https://osucollector.com/collections/${row.id}`)}
                >
                  Open on osu!Collector
                </button>
                {#if entry?.state === "ready"}
                  <button class="secondary" disabled>Added</button>
                {:else if entry?.state === "loading"}
                  <button class="secondary" disabled>Adding…</button>
                {:else}
                  <button class="primary" onclick={() => add(row)}>
                    {entry ? "Try again" : "Add"}
                  </button>
                {/if}
              </div>
            </div>
          </li>
        {/each}
      </ul>

      {#if b.error}
        <p class="error-text small">{b.error}</p>
      {/if}
      {#if b.nextCursor !== null}
        <div class="actions left">
          <button class="secondary" onclick={browseMore} disabled={busy}>
            {b.loadingMore ? "Loading…" : "Load more"}
          </button>
        </div>
      {/if}
    {/if}

    {#if listed > 0}
      <div class="actions review-bar">
        <button class="primary" onclick={onReview}>
          Review your list ({listed.toLocaleString()})
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .browse-layout {
    display: grid;
    gap: 20px;
  }
  .browse-results {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .browse-form {
    margin-bottom: 0;
  }
  @container (min-width: 1000px) {
    .browse-layout {
      grid-template-columns: minmax(300px, 360px) minmax(0, 1fr);
      align-items: start;
    }
    .browse-form {
      position: sticky;
      top: 24px;
    }
  }
  .sort {
    margin-top: 12px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
    margin-top: 10px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  select {
    font: inherit;
    padding: 7px 8px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
  }
  .result-line {
    margin: 0 0 12px;
  }
  .description {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
    overflow-wrap: anywhere;
  }
  .review-bar {
    position: sticky;
    bottom: 0;
    z-index: 3;
    padding: 12px 0;
    background: var(--bg);
  }
</style>
