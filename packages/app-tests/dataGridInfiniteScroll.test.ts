import { strict as assert } from "node:assert";
import { test } from "vitest";
import { dataGridScrollPosition, isDataGridNearScrollBottom, shouldCheckInfiniteScrollAfterScroll } from "../../apps/desktop/src/lib/dataGridInfiniteScroll.ts";

test("horizontal-only scroll does not check infinite scroll", () => {
  assert.equal(shouldCheckInfiniteScrollAfterScroll(dataGridScrollPosition(240, 0), dataGridScrollPosition(240, 180)), false);
});

test("vertical scroll checks infinite scroll even when horizontal offset also changes", () => {
  assert.equal(shouldCheckInfiniteScrollAfterScroll(dataGridScrollPosition(240, 0), dataGridScrollPosition(360, 180)), true);
});

test("first scroll position only establishes the infinite scroll baseline", () => {
  assert.equal(shouldCheckInfiniteScrollAfterScroll(undefined, dataGridScrollPosition(360, 180)), false);
});

test("near-bottom check matches the grid threshold", () => {
  assert.equal(isDataGridNearScrollBottom({ scrollTop: 801, scrollHeight: 1000, clientHeight: 100 }), true);
  assert.equal(isDataGridNearScrollBottom({ scrollTop: 800, scrollHeight: 1000, clientHeight: 100 }), false);
});
