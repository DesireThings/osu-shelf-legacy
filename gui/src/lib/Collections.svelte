<script lang="ts">
  import DownloadProgress from "./DownloadProgress.svelte";
  import {
    addCollection,
    cancel,
    download,
    downloadFor,
    isActive,
    removeEntry,
    store,
    type CollectionSort,
    type Entry,
  } from "./store.svelte";

  let { onNext }: { onNext: () => void } = $props();

  let link = $state("");

  function add() {
    const input = link.trim();
    if (!input) return;
    link = "";
    addCollection(input);
  }

  const ready = $derived(store.entries.filter((e) => e.state === "ready"));
  /** Ready collections not downloaded yet (or whose last try didn't finish). */
  const notStarted = $derived(
    ready.filter((e) => {
      const status = downloadFor(e.preview.id);
      return !status || status.phase === "cancelled" || status.phase === "failed";
    }),
  );
  const anyActive = $derived(ready.some((e) => isActive(downloadFor(e.preview.id))));
  const allDone = $derived(
    ready.length > 0 && ready.every((e) => downloadFor(e.preview.id)?.phase === "done"),
  );

  function downloadAll() {
    for (const entry of notStarted) download(entry);
  }

  type Ready = Extract<Entry, { state: "ready" }>;

  const sorts: { id: CollectionSort; label: string; compare: (a: Ready, b: Ready) => number }[] = [
    { id: "newest", label: "Newest first", compare: (a, b) => b.key - a.key },
    { id: "oldest", label: "Oldest first", compare: (a, b) => a.key - b.key },
    {
      id: "name",
      label: "Name A-Z",
      compare: (a, b) =>
        a.preview.name.localeCompare(b.preview.name, undefined, {
          numeric: true,
          sensitivity: "base",
        }) || a.key - b.key,
    },
    {
      id: "maps",
      label: "Most maps",
      compare: (a, b) => b.preview.beatmapsets - a.preview.beatmapsets || a.key - b.key,
    },
  ];

  /** The tools hide below two collections, so the query must stop applying too. */
  const showTools = $derived(ready.length > 1);
  const query = $derived(showTools ? store.collectionsView.query.trim() : "");
  const matches = $derived.by(() => {
    const needle = query.toLowerCase();
    return ready.filter(
      (e) =>
        e.preview.name.toLowerCase().includes(needle) ||
        e.preview.uploader.toLowerCase().includes(needle),
    );
  });
  /** Lookups and failures stay on top, past the filter, so a new link is never hidden. */
  const visible = $derived<Entry[]>([
    ...store.entries.filter((e) => e.state !== "ready").reverse(),
    ...[...matches].sort(sorts.find((s) => s.id === store.collectionsView.sort)!.compare),
  ]);
</script>

<header class="page-head">
  <h1>Add collections</h1>
  <p class="muted">
    Copy a collection's link from osucollector.com and paste it below. Add as many as you like,
    then download them all.
  </p>
</header>

<form
  class="card add-row"
  onsubmit={(e) => {
    e.preventDefault();
    add();
  }}
>
  <input
    class="text-input"
    bind:value={link}
    placeholder="https://osucollector.com/collections/12345"
    aria-label="osu!collector link"
  />
  <button class="primary" type="submit" disabled={!link.trim()}>Add</button>
</form>

{#if store.entries.length === 0}
  <p class="muted empty">No collections yet. Paste a link above to start.</p>
{:else}
  {#if notStarted.length > 1}
    <div class="actions left list-actions">
      <button class="primary" onclick={downloadAll}>Download all {notStarted.length}</button>
    </div>
  {/if}

  {#if showTools}
    <div class="list-tools">
      <input
        class="text-input"
        bind:value={store.collectionsView.query}
        placeholder="Filter by name or uploader"
        aria-label="Filter collections"
      />
      <select bind:value={store.collectionsView.sort} aria-label="Sort collections">
        {#each sorts as sort (sort.id)}<option value={sort.id}>{sort.label}</option>{/each}
      </select>
    </div>
    {#if query}
      {#if matches.length > 0}
        <p class="muted small list-count">Showing {matches.length} of {ready.length}</p>
      {:else}
        <p class="muted empty">
          No collections match "{query}".
          <button class="secondary" onclick={() => (store.collectionsView.query = "")}>Clear</button>
        </p>
      {/if}
    {/if}
  {/if}

  <ul class="entry-list">
    {#each visible as entry (entry.key)}
      {@const status = entry.state === "ready" ? downloadFor(entry.preview.id) : undefined}
      <li class="card entry-card" class:entry-error={entry.state === "error"}>
        <div class="entry">
          {#if entry.state === "loading"}
            <div class="entry-main">
              <span class="entry-title muted">Looking up {entry.input}…</span>
            </div>
          {:else if entry.state === "ready"}
            <div class="entry-main">
              <span class="entry-title">{entry.preview.name}</span>
              <span class="muted small">
                by {entry.preview.uploader} · {entry.preview.beatmapsets} map sets ·
                {entry.preview.beatmaps} difficulties
              </span>
            </div>
          {:else}
            <div class="entry-main">
              <span class="entry-title">Couldn't add {entry.input}</span>
              <span class="error-text small">{entry.message}</span>
            </div>
          {/if}

          <div class="entry-buttons">
            {#if entry.state === "ready"}
              {#if isActive(status)}
                <button class="secondary" onclick={() => cancel(status!)}>Cancel</button>
              {:else if status?.phase === "done"}
                <button class="secondary" onclick={() => download(entry)}>Check again</button>
              {:else}
                <button class="primary" onclick={() => download(entry)}>
                  {status ? "Try again" : "Download"}
                </button>
              {/if}
            {/if}
            {#if !isActive(status)}
              <button class="secondary" onclick={() => removeEntry(entry.key)}>Remove</button>
            {/if}
          </div>
        </div>

        {#if store.startErrors[entry.key]}
          <p class="error-text small">{store.startErrors[entry.key]}</p>
        {/if}
        {#if status}
          <DownloadProgress {status} />
        {/if}
      </li>
    {/each}
  </ul>
{/if}

{#if allDone}
  <section class="card success">
    <h2>All downloads finished</h2>
    <p>Next, get them into osu!: one step for every collection at once.</p>
    <div class="actions left">
      <button class="primary" onclick={onNext}>Continue to import</button>
    </div>
  </section>
{:else}
  <div class="actions">
    <button class="secondary" onclick={onNext}>
      {anyActive ? "Go to import (downloads keep running)" : "Continue to import"}
    </button>
  </div>
{/if}

<style>
  .list-tools {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    margin: 0 0 12px;
  }
  .list-tools .text-input {
    min-width: 180px;
  }
  select {
    font: inherit;
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
  }
  .list-count {
    margin: 0 0 12px;
  }
</style>
