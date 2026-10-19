import { test } from "node:test";
import assert from "node:assert/strict";
import { clampPanelWidth, DEFAULT_PANEL_WIDTH, MIN_PANEL_WIDTH, storedPanelWidth } from "../../src/lib/assistantPanelWidth.ts";

test("the panel starts at its usual width", () => {
  assert.equal(DEFAULT_PANEL_WIDTH, 384);
  assert.equal(storedPanelWidth(null, 1400), 384);
});

test("a width that was kept comes back, clamped to what the window allows", () => {
  assert.equal(storedPanelWidth("500", 1400), 500);
  assert.equal(storedPanelWidth("100", 1400), MIN_PANEL_WIDTH);
  assert.equal(storedPanelWidth("5000", 1400), 720);
  assert.equal(storedPanelWidth("nonsense", 1400), 384);
  assert.equal(storedPanelWidth("", 1400), 384);
});

test("the panel leaves room for the editor in a narrow window", () => {
  assert.equal(clampPanelWidth(700, 900), 420);
  assert.equal(clampPanelWidth(700, 1400), 700);
  // Never narrower than the minimum, even in a tiny window
  assert.equal(clampPanelWidth(400, 500), MIN_PANEL_WIDTH);
});
