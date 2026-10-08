import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { transformSync } from 'esbuild';

// Exercise the production hook with controlled native responses. Resolving them
// out of order reproduces races without a WebView or access to real photographs.
const source = readFileSync(new URL('../src/hooks/useAppNavigation.ts', import.meta.url), 'utf8');
const compiled = transformSync(source, { loader: 'ts', format: 'cjs' }).code;
const tick = () => new Promise((resolve) => setImmediate(resolve));
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};

function harness() {
  const initial = { exposure: 0 };
  const editor = {
    selectedImage: null,
    adjustments: initial,
    originalSize: { width: 0, height: 0 },
    patchesSentToBackend: new Set(),
    finalPreviewUrl: null,
    interactivePatch: null,
    isSliderDragging: false,
  };
  const library = {
    imageList: [],
    multiSelectedPaths: [],
    currentFolderPath: null,
    imageRatings: {},
    rootPaths: [],
    expandedFolders: new Set(),
    sortCriteria: { key: 'date_taken' },
    pushNavHistory: () => {},
  };
  const ui = {},
    process = { thumbnails: {}, mediumThumbnails: {} };
  const settings = { appSettings: { pinnedFolders: [], libraryViewMode: 'flat' } };
  for (const [state, setter] of [
    [editor, 'setEditor'],
    [library, 'setLibrary'],
    [ui, 'setUI'],
    [process, 'setProcess'],
  ]) {
    state[setter] = (value) => Object.assign(state, typeof value === 'function' ? value(state) : value);
  }
  editor.resetHistory = (adjustments) => {
    editor.adjustments = adjustments;
  };
  const cache = new Map();
  cache.isProtected = () => false;
  const pending = [];
  const invoke = (command, args) => {
    if (command === 'cancel_thumbnail_generation' || command === 'StartBackgroundIndexing') return Promise.resolve();
    const response = deferred();
    pending.push({ command, args, ...response });
    return response.promise;
  };
  const take = (command, path) => {
    const index = pending.findIndex(
      (p) => p.command === command && (!path || p.args.path === path || p.args.paths?.includes(path)),
    );
    assert.notEqual(index, -1, `Missing native call ${command} ${path ?? ''}`);
    return pending.splice(index, 1)[0];
  };
  const refs = Object.fromEntries(
    [
      'transformWrapperRef',
      'preloadedDataRef',
      'cachedEditStateRef',
      'selectedImagePathRef',
      'isBackendReadyRef',
      'latestRenderedJobIdRef',
      'previewJobIdRef',
      'currentResRef',
      'prevAdjustmentsRef',
    ].map((key) => [key, { current: null }]),
  );
  const modules = {
    react: { useCallback: (fn) => fn, useRef: (value) => ({ current: value }) },
    '@tauri-apps/api/core': { invoke },
    '@tauri-apps/plugin-dialog': { open: () => {} },
    '@tauri-apps/api/path': { homeDir: () => '' },
    'react-toastify': { toast: { error: () => {} } },
    '../components/ui/AppProperties': {
      Invokes: new Proxy({}, { get: (_, key) => key }),
      LibraryViewMode: { Recursive: 'recursive' },
    },
    '../utils/adjustments': { INITIAL_ADJUSTMENTS: initial, normalizeLoadedAdjustments: (value) => value },
    '../utils/ImageLRUCache': { globalImageCache: cache },
    './useEditorActions': { debouncedSave: { flush: () => {} }, debouncedSetHistory: { cancel: () => {} } },
    './useLibraryActions': { clearLibrarySelection: () => {} },
    './useSortedLibrary': { computeSortedLibrary: () => library.imageList },
  };
  for (const [name, state] of [
    ['Editor', editor],
    ['Library', library],
    ['UI', ui],
    ['Process', process],
    ['Settings', settings],
  ]) {
    modules[`../store/use${name}Store`] = { [`use${name}Store`]: { getState: () => state } };
  }
  const module = { exports: {} };
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    require: (id) => {
      assert.ok(modules[id], `Unmocked import: ${id}`);
      return modules[id];
    },
    setTimeout: () => {},
    URL: { revokeObjectURL: () => {} },
    console,
  });
  const actions = module.exports.useAppNavigation({ clearThumbnailQueue: () => {}, refs });
  const addCached = (path) =>
    cache.set(path, {
      selectedImage: { path, isReady: true },
      adjustments: { exposure: 1 },
      originalSize: { width: 100, height: 100 },
      previewSize: { width: 100, height: 100 },
    });
  const selectCached = async (path) => {
    addCached(path);
    const selection = actions.handleImageSelect(path);
    take('is_image_cached', path).resolve(true);
    await selection;
  };
  return { actions, editor, library, ui, refs, take, addCached, selectCached };
}

test('a slow cached selection cannot replace a newer uncached photo', async () => {
  const h = harness();
  h.addCached('A');
  const old = h.actions.handleImageSelect('A');
  await h.actions.handleImageSelect('B');
  h.take('is_image_cached', 'A').resolve(true);
  await old;
  assert.equal(h.editor.selectedImage.path, 'B');
  assert.equal(h.refs.selectedImagePathRef.current, 'B');
});

test('returning to the library cancels a pending selection', async () => {
  const h = harness();
  h.addCached('A');
  const old = h.actions.handleImageSelect('A');
  h.actions.handleBackToLibrary();
  h.take('is_image_cached', 'A').resolve(true);
  await old;
  assert.equal(h.editor.selectedImage, null);
  assert.equal(h.ui.activeView, 'library');
});

test('selecting the current photo cancels another pending selection but keeps its own load', async () => {
  const h = harness();
  await h.selectCached('B');
  h.addCached('A');
  const old = h.actions.handleImageSelect('A');
  await h.actions.handleImageSelect('B');
  h.take('is_image_cached', 'A').resolve(true);
  await old;
  h.take('LoadImage', 'B').resolve({ width: 600, height: 400 });
  await tick();
  assert.equal(h.editor.selectedImage.path, 'B');
  assert.equal(h.editor.originalSize.width, 600);
  assert.equal(h.refs.isBackendReadyRef.current, true);
});

test('returning to the same path does not revive its earlier native response', async () => {
  const h = harness();
  await h.selectCached('A');
  const old = h.take('LoadImage', 'A');
  await h.actions.handleImageSelect('B');
  await h.selectCached('A');
  h.take('LoadImage', 'A').resolve({ width: 600, height: 400 });
  await tick();
  old.resolve({ width: 10, height: 10 });
  await tick();
  assert.equal(h.editor.originalSize.width, 600);
});

test('metadata arriving after an edit preserves the new adjustments', async () => {
  const h = harness();
  await h.selectCached('A');
  const edited = { exposure: 2 };
  h.editor.setEditor({ adjustments: edited });
  h.take('LoadMetadata', 'A').resolve({ adjustments: { exposure: -1 } });
  await tick();
  assert.equal(h.editor.adjustments, edited);
});

test('metadata still refreshes an untouched cached photo', async () => {
  const h = harness();
  await h.selectCached('A');
  h.take('LoadMetadata', 'A').resolve({ adjustments: { exposure: -1 } });
  await tick();
  assert.equal(h.editor.adjustments.exposure, -1);
});

test('an old load failure cannot make the new photo ready prematurely', async () => {
  const h = harness();
  await h.selectCached('A');
  await h.selectCached('B');
  h.take('LoadImage', 'A').reject(new Error('Old file unavailable'));
  await tick();
  assert.equal(h.refs.isBackendReadyRef.current, false);
});

test('a stale folder listing cannot replace the current folder or finish its loading state', async () => {
  const h = harness();
  const old = h.actions.handleSelectSubfolder('A');
  await tick();
  const current = h.actions.handleSelectSubfolder('B');
  await tick();
  h.take('ListImagesInDir', 'A').resolve([{ path: 'A/photo', rating: 5 }]);
  await old;
  assert.equal(h.library.currentFolderPath, 'B');
  assert.equal(h.library.isViewLoading, true);
  assert.equal(h.library.imageList.length, 0);
  h.take('ListImagesInDir', 'B').resolve([]);
  await current;
  assert.equal(h.library.isViewLoading, false);
});

test('album and folder requests share cancellation', async () => {
  const h = harness();
  const old = h.actions.handleSelectAlbum('album-a', 'A', ['A/photo']);
  await tick();
  const current = h.actions.handleSelectSubfolder('B');
  await tick();
  h.take('ListImagesInDir', 'B').resolve([]);
  await current;
  h.take('GetAlbumImages', 'A/photo').resolve([{ path: 'A/photo', rating: 5 }]);
  await old;
  assert.equal(h.library.currentFolderPath, 'B');
  assert.equal(h.library.imageList.length, 0);
  assert.equal(h.library.imageRatings['A/photo'], undefined);
});

test('EXIF sorting cannot restore the previous folder after navigation', async () => {
  const h = harness();
  const old = h.actions.handleSelectSubfolder('A');
  await tick();
  h.take('ListImagesInDir', 'A').resolve([{ path: 'A/photo' }]);
  await tick();
  const current = h.actions.handleSelectSubfolder('B');
  await tick();
  h.take('ListImagesInDir', 'B').resolve([]);
  await current;
  h.take('ReadExifForPaths', 'A/photo').resolve({ 'A/photo': { iso: 100 } });
  await old;
  assert.equal(h.library.currentFolderPath, 'B');
  assert.equal(h.library.imageList.length, 0);
});
