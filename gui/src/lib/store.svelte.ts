// App-wide state that outlives a screen: the collections the user added and how
// they're listed, the last update scan and what's picked from it, the last search and its picks,
// the last collection browse, and the downloads the backend is running. Switching pages in the
// sidebar keeps all of it.
import {
  browseCollections,
  cancelDownload,
  downloadFound,
  downloadUpdates,
  findMaps,
  findOwned,
  listDownloads,
  onDownloadStatus,
  onScanProgress,
  previewCollection,
  scanUpdates,
  startDownload,
  type BrowseCriteria,
  type BrowseMode,
  type BrowseRow,
  type CollectionPreview,
  type DownloadStatus,
  type FindCriteria,
  type FindRoute,
  type FindRow,
  type Owned,
  type ScanProgress,
  type UpdateReport,
} from "./api";

export type Entry =
  | { key: number; input: string; state: "loading" }
  | { key: number; input: string; state: "ready"; preview: CollectionPreview }
  | { key: number; input: string; state: "error"; message: string };

export type CollectionSort = "newest" | "oldest" | "name" | "maps";

/** What the API can't filter on, so it's applied to the rows loaded so far. */
export interface BrowseFilters {
  minDifficulties: number | null;
  maxDifficulties: number | null;
  mode: BrowseMode | "";
  minStars: number | null;
  maxStars: number | null;
}

export const noBrowseFilters: BrowseFilters = {
  minDifficulties: null,
  maxDifficulties: null,
  mode: "",
  minStars: null,
  maxStars: null,
};

/** Listing pages one search or "Load more" may fetch. */
const BROWSE_PAGES_PER_LOAD = 3;
/** New filter matches that end a load early. */
const BROWSE_MATCHES_PER_LOAD = 20;

export const store = $state({
  entries: [] as Entry[],
  /** How the Collections page shows the list. */
  collectionsView: { query: "", sort: "newest" as CollectionSort },
  /** Latest status per download id. */
  downloads: {} as Record<number, DownloadStatus>,
  /** Errors from asking the backend to start a download, per entry key. */
  startErrors: {} as Record<number, string>,
  updates: {
    scanning: false,
    progress: null as ScanProgress | null,
    report: null as UpdateReport | null,
    error: null as string | null,
    /** Collections ticked for download. */
    picked: [] as number[],
    /** Previously deleted sets picked again, as `${collection}:${set}`. */
    reincluded: [] as string[],
    downloadError: null as string | null,
    /** A download was started from this scan; check again before the next. */
    downloaded: false,
  },
  find: {
    /** The form as edited; null until the options load. */
    criteria: null as FindCriteria | null,
    searching: false,
    loadingMore: false,
    error: null as string | null,
    /** The form as it was when the shown results were searched. */
    searched: null as FindCriteria | null,
    route: null as FindRoute | null,
    rows: [] as FindRow[],
    total: 0,
    nextCursor: null as string | null,
    picked: [] as number[],
    downloadError: null as string | null,
    /** What the user already has; null until first read. */
    owned: null as Owned | null,
    hideOwned: false,
  },
  browse: {
    /** The form as edited. */
    criteria: { query: "", sort: "popular_month" } as BrowseCriteria,
    filters: { ...noBrowseFilters },
    searching: false,
    loadingMore: false,
    error: null as string | null,
    /** The form as it was when the shown results were searched. */
    searched: null as BrowseCriteria | null,
    rows: [] as BrowseRow[],
    /** Matches in all; popular listings don't say. */
    total: null as number | null,
    nextCursor: null as number | null,
  },
});

let nextKey = 0;
let listening = false;
let listeningScan = false;

/** Pick up downloads already running and follow their updates. Idempotent. */
export async function initDownloads() {
  if (listening) return;
  listening = true;
  await onDownloadStatus((status) => {
    store.downloads[status.id] = status;
  });
  for (const status of await listDownloads()) {
    store.downloads[status.id] = status;
  }
}

export async function addCollection(input: string) {
  const key = nextKey++;
  store.entries.push({ key, input, state: "loading" });
  let updated: Entry;
  try {
    const preview = await previewCollection(input);
    const duplicate = store.entries.some(
      (e) => e.state === "ready" && e.preview.id === preview.id,
    );
    updated = duplicate
      ? { key, input, state: "error", message: `"${preview.name}" is already in the list.` }
      : { key, input, state: "ready", preview };
  } catch (e) {
    updated = { key, input, state: "error", message: String(e) };
  }
  const index = store.entries.findIndex((e) => e.key === key);
  if (index !== -1) store.entries[index] = updated;
}

export function removeEntry(key: number) {
  store.entries = store.entries.filter((e) => e.key !== key);
  delete store.startErrors[key];
}

/** The newest download of a collection, if any. */
export function downloadFor(collectionId: number): DownloadStatus | undefined {
  let latest: DownloadStatus | undefined;
  for (const status of Object.values(store.downloads)) {
    if (
      status.kind === "collection" &&
      status.collection_id === collectionId &&
      (!latest || status.id > latest.id)
    ) {
      latest = status;
    }
  }
  return latest;
}

export const isActive = (status: DownloadStatus | undefined) =>
  !!status && !["done", "failed", "cancelled"].includes(status.phase);

export async function download(entry: Entry) {
  delete store.startErrors[entry.key];
  try {
    await startDownload(entry.input);
  } catch (e) {
    store.startErrors[entry.key] = String(e);
  }
}

export const cancel = (status: DownloadStatus) => cancelDownload(status.id);

/** The newest update download, if any. */
export function latestUpdateRun(): DownloadStatus | undefined {
  let latest: DownloadStatus | undefined;
  for (const status of Object.values(store.downloads)) {
    if (status.kind === "update" && (!latest || status.id > latest.id)) latest = status;
  }
  return latest;
}

export const reincludeKey = (collectionId: number, setId: number) => `${collectionId}:${setId}`;

/** Scan for updates; afterwards every collection with something new is ticked. */
export async function scanForUpdates() {
  const u = store.updates;
  if (u.scanning) return;
  if (!listeningScan) {
    listeningScan = true;
    await onScanProgress((progress) => {
      if (store.updates.scanning) store.updates.progress = progress;
    });
  }
  u.scanning = true;
  u.progress = null;
  u.error = null;
  u.downloadError = null;
  u.downloaded = false;
  try {
    const report = await scanUpdates();
    u.report = report;
    u.picked = report.collections
      .filter((c) => c.missing.some((m) => !m.previously_deleted))
      .map((c) => c.id);
    u.reincluded = [];
  } catch (e) {
    u.error = String(e);
  } finally {
    u.scanning = false;
    u.progress = null;
  }
}

/** Map sets the current pick downloads: each once, however many collections miss it. */
export function pickedSetIds(): Set<number> {
  const u = store.updates;
  const ids = new Set<number>();
  for (const collection of u.report?.collections ?? []) {
    if (!u.picked.includes(collection.id)) continue;
    for (const set of collection.missing) {
      if (
        !set.previously_deleted ||
        u.reincluded.includes(reincludeKey(collection.id, set.beatmapset_id))
      ) {
        ids.add(set.beatmapset_id);
      }
    }
  }
  return ids;
}

export async function downloadPickedUpdates() {
  const u = store.updates;
  u.downloadError = null;
  try {
    await downloadUpdates({
      collection_ids: [...u.picked],
      reincluded: u.reincluded.map((key) => {
        const [collection_id, beatmapset_id] = key.split(":").map(Number);
        return { collection_id, beatmapset_id };
      }),
    });
    u.downloaded = true;
  } catch (e) {
    u.downloadError = String(e);
  }
}

/** The newest find download, if any. */
export function latestFindRun(): DownloadStatus | undefined {
  let latest: DownloadStatus | undefined;
  for (const status of Object.values(store.downloads)) {
    if (status.kind === "find" && (!latest || status.id > latest.id)) latest = status;
  }
  return latest;
}

/** Search with the form as it is. Results start unticked: a search can match
 * thousands of map sets, and the user picks what they want. */
export async function search() {
  const f = store.find;
  if (!f.criteria || f.searching) return;
  const criteria = $state.snapshot(f.criteria);
  f.searching = true;
  f.error = null;
  f.downloadError = null;
  try {
    const page = await findMaps(criteria);
    f.searched = criteria;
    f.route = page.route;
    f.rows = page.rows;
    f.total = page.total;
    f.nextCursor = page.next_cursor;
    f.picked = [];
    void refreshOwned();
  } catch (e) {
    f.error = String(e);
  } finally {
    f.searching = false;
  }
}

/** Re-read what the user already has, to mark it in the results. A failure
 * only loses the marks, so it isn't shown as an error. */
export async function refreshOwned() {
  try {
    store.find.owned = await findOwned();
  } catch {
    store.find.owned = null;
  }
}

/** The next page of the shown results (osu! API only). */
export async function loadMore() {
  const f = store.find;
  if (!f.searched || !f.nextCursor || f.loadingMore) return;
  f.loadingMore = true;
  f.error = null;
  try {
    const page = await findMaps(f.searched, f.nextCursor);
    const known = new Set(f.rows.map((r) => r.id));
    const fresh = page.rows.filter((r) => !known.has(r.id));
    f.rows = [...f.rows, ...fresh];
    f.nextCursor = page.next_cursor;
  } catch (e) {
    f.error = String(e);
  } finally {
    f.loadingMore = false;
  }
}

/** Queue the ticked sets; resolves to the new run's id, or null when it failed. */
export async function downloadPicked(): Promise<number | null> {
  const f = store.find;
  if (!f.searched || !f.route) return null;
  f.downloadError = null;
  const sizes: Record<number, number> = {};
  for (const row of f.rows) {
    if (row.size !== null && f.picked.includes(row.id)) sizes[row.id] = row.size;
  }
  try {
    return await downloadFound({
      criteria: f.searched,
      route: f.route,
      beatmapset_ids: f.rows.filter((r) => f.picked.includes(r.id)).map((r) => r.id),
      sizes,
    });
  } catch (e) {
    f.downloadError = String(e);
    return null;
  }
}

/** Whether a row passes the filters. A star filter matches when the row's range overlaps it. */
export function matchesBrowseFilters(row: BrowseRow, filters: BrowseFilters): boolean {
  const { minDifficulties, maxDifficulties, mode, minStars, maxStars } = filters;
  if (minDifficulties !== null && row.difficulties < minDifficulties) return false;
  if (maxDifficulties !== null && row.difficulties > maxDifficulties) return false;
  if (mode && row.main_mode !== mode) return false;
  if (minStars === null && maxStars === null) return true;
  if (!row.stars) return false;
  const [from, to] = starBounds(row.stars);
  return (minStars === null || to > minStars) && (maxStars === null || from <= maxStars);
}

/** The star ratings a row's buckets cover: from inclusive, to exclusive. Bucket 1 also holds maps
 * under 1 star, and bucket 10 everything from 10 up. */
export function starBounds([low, high]: [number, number]): [number, number] {
  return [low === 1 ? 0 : low, high === 10 ? Infinity : high + 1];
}

/** Add pages from the browse cursor until enough rows pass the filters. A failure keeps the
 * pages already loaded. */
async function loadBrowsePages(criteria: BrowseCriteria) {
  const b = store.browse;
  let matches = 0;
  for (let i = 0; i < BROWSE_PAGES_PER_LOAD; i++) {
    const page = await browseCollections(criteria, b.nextCursor);
    const known = new Set(b.rows.map((r) => r.id));
    const fresh = page.rows.filter((r) => !known.has(r.id));
    b.rows = [...b.rows, ...fresh];
    b.total = page.total;
    b.nextCursor = page.next_cursor;
    matches += fresh.filter((r) => matchesBrowseFilters(r, b.filters)).length;
    if (b.nextCursor === null || matches >= BROWSE_MATCHES_PER_LOAD) return;
  }
}

/** Browse with the form as it is, replacing the shown results. */
export async function browseSearch() {
  const b = store.browse;
  if (b.searching || b.loadingMore) return;
  const criteria = $state.snapshot(b.criteria);
  b.searching = true;
  b.error = null;
  b.searched = criteria;
  b.rows = [];
  b.total = null;
  b.nextCursor = null;
  try {
    await loadBrowsePages(criteria);
  } catch (e) {
    b.error = String(e);
  } finally {
    b.searching = false;
  }
}

/** Load further pages of the shown results. */
export async function browseMore() {
  const b = store.browse;
  if (!b.searched || b.nextCursor === null || b.searching || b.loadingMore) return;
  b.loadingMore = true;
  b.error = null;
  try {
    await loadBrowsePages(b.searched);
  } catch (e) {
    b.error = String(e);
  } finally {
    b.loadingMore = false;
  }
}

/** The list entry for a collection, whether it came from Browse or a pasted link. */
export function entryForCollection(id: number): Entry | undefined {
  const input = String(id);
  return store.entries.find((e) => (e.state === "ready" ? e.preview.id === id : e.input === input));
}
