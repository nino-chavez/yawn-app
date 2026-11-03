// Run against ui-harness/run.sh serve. Playwright may resolve through NODE_PATH.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdir, writeFile, readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
const { chromium, webkit, expect } = createRequire(import.meta.url)('@playwright/test');
const origin = process.env.HARNESS_ORIGIN || 'http://127.0.0.1:8753';
const out = resolve(process.env.HARNESS_REVIEW_OUT || '.artifacts/rail-inspection');
await mkdir(out, { recursive: true });
const records = [];
const csp = JSON.parse(await readFile(new URL("../src-tauri/tauri.conf.json", import.meta.url))).app.security.csp;
for (const [engine, launcher] of [['chromium', chromium], ['webkit', webkit]]) {
  const browser = await launcher.launch({ headless: true });
  try {
    for (const appearance of ['light', 'dark']) {
      const page = await browser.newPage({ viewport: { width: 1080, height: 900 }, colorScheme: appearance });
      const errors = [];
      page.on('pageerror', error => errors.push(error.message));
      await page.route('**/harness.html?mode=ledger', async route => {
        const response = await route.fetch();
        await route.fulfill({ response, headers: { ...response.headers(), 'content-security-policy': csp } });
      });
      await page.goto(`${origin}/harness.html?mode=ledger`);
      await page.locator('[data-action="open-meeting"]').first().click();
      const editor = page.locator('.ledger-editor');
      await expect(editor).toHaveCount(1);
      await expect(editor).toBeVisible();
      const original = await editor.inputValue();
      const blue = appearance === 'light' ? 'rgb(47, 95, 168)' : 'rgb(143, 180, 255)';
      const red = appearance === 'light' ? 'rgb(199, 57, 44)' : 'rgb(255, 106, 92)';
      const tint = appearance === 'light' ? 'rgba(47, 95, 168, 0.12)' : 'rgba(143, 180, 255, 0.18)';
      const turn = page.locator('.ledger-turn').nth(1);
      const line = await turn.getAttribute('data-line');
      const mark = page.locator(`.ledger-mark[data-line="${line}"] i`);
      const mirrorLine = page.locator(`.ledger-mirror-line[data-line="${line}"]`);
      const neutral = await mark.evaluate(node => getComputedStyle(node).backgroundColor);
      await turn.hover();
      await expect(mark).toHaveCSS('background-color', blue);
      await expect(mirrorLine).toHaveCSS('background-color', tint);
      // Negative control: clearing inspection classes must remove the cue.
      const withoutRules = await page.locator('.ledger').evaluate(ledger => {
        const inspected = [...ledger.querySelectorAll('.is-inspected')];
        inspected.forEach(node => node.classList.remove('is-inspected'));
        const marks = [...ledger.querySelectorAll('.ledger-mark i')].map(node => getComputedStyle(node).backgroundColor);
        inspected.forEach(node => node.classList.add('is-inspected'));
        return { marks };
      });
      assert.ok(withoutRules.marks.every(color => color !== blue));
      await expect(mark).toHaveCSS('background-color', blue);
      await page.screenshot({ path: `${out}/${engine}-${appearance}-related.png`, fullPage: true });
      await editor.focus();
      await page.mouse.move(0, 0);
      await expect(mark).toHaveCSS('background-color', neutral);
      const fold = page.locator('.ledger-fold').first();
      await fold.focus();
      // macOS WebKit uses Option-Tab to reach all controls without changing OS preferences.
      await page.keyboard.press(engine === 'webkit' ? 'Alt+Tab' : 'Tab');
      await page.keyboard.press(engine === 'webkit' ? 'Alt+Shift+Tab' : 'Shift+Tab');
      await expect(fold).toBeFocused();
      await expect(fold).toHaveCSS('outline-color', blue);
      const focusLine = await fold.getAttribute('data-line');
      await expect(page.locator(`.ledger-mark[data-line="${focusLine}"] i`)).toHaveCSS('background-color', blue);
      const activeRecordingColor = await page.locator(`.ledger-mark[data-line="${focusLine}"]`).evaluate(node => {
        node.classList.add('live');
        const color = getComputedStyle(node.querySelector('i')).backgroundColor;
        node.classList.remove('live');
        return color;
      });
      assert.equal(activeRecordingColor, red);
      const before = await editor.boundingBox();
      await fold.press('Enter');
      await expect(fold).toHaveAttribute('aria-expanded', 'true');
      const after = await editor.boundingBox();
      for (const side of ['x', 'y', 'width', 'height']) assert.ok(Math.abs(after[side] - before[side]) < 0.5);
      await page.keyboard.press('Escape');
      await expect(fold).toHaveAttribute('aria-expanded', 'false');
      await expect(fold).toBeFocused();
      await editor.focus();
      await editor.evaluate(node => node.setSelectionRange(node.value.length, node.value.length));
      await page.keyboard.insertText(' review note');
      await page.waitForTimeout(1150);
      await expect(editor).toBeFocused();
      await expect(editor).toHaveValue(original + ' review note');
      await editor.press('ControlOrMeta+z');
      await expect(editor).toHaveValue(original);
      await editor.evaluate(node => node.setSelectionRange(0, 0));
      await page.keyboard.insertText('Untimed review line\n');
      await page.waitForTimeout(1150);
      const shifted = page.locator('.ledger-turn[data-line="1"]');
      await expect(shifted).toBeVisible();
      await shifted.hover();
      await expect(page.locator('.ledger-mirror-line[data-line="1"]')).toHaveCSS('background-color', tint);
      await expect(page.locator('.ledger-mirror-line[data-line="0"]')).toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');
      await editor.focus();
      await editor.press('ControlOrMeta+z');
      await expect(editor).toHaveValue(original);
      await page.setViewportSize({ width: 960, height: 760 });
      await editor.focus();
      await editor.evaluate(node => { const end = node.value.indexOf('\n'); node.setSelectionRange(end, end); });
      const extra = ' — compare two longer alternatives before choosing the next release';
      await page.keyboard.insertText(extra);
      await page.waitForTimeout(1150);
      await expect(editor).toHaveValue(original.replace('\n', extra + '\n'));
      await page.locator('.ledger-turn[data-line="0"]').hover();
      await expect(page.locator('.ledger-mirror-line[data-line="0"]')).toHaveCSS('background-color', tint);
      assert.ok(await page.locator('.ledger-mirror-line[data-line="0"]').evaluate(node => node.offsetHeight > parseFloat(getComputedStyle(node).lineHeight)));
      await expect(page.locator('.ledger-mirror')).toHaveAttribute('aria-hidden', 'true');
      await expect(page.locator('.ledger-mirror')).toHaveCSS('pointer-events', 'none');
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      await page.emulateMedia({ reducedMotion: 'reduce' });
      await fold.click();
      await expect(fold).toHaveAttribute('aria-expanded', 'true');
      assert.ok(await page.locator('.ledger-fold-turns').evaluate(node => parseFloat(getComputedStyle(node).animationDuration) <= 0.01));
      await page.keyboard.press('Escape');
      await expect(fold).toHaveAttribute('aria-expanded', 'false');
      assert.deepEqual(errors, []);
      records.push({ engine, appearance, hover: true, keyboard: true, recordingColor: true, editorUndoAcrossPoll: true, shiftedLine: true, narrowWrap: true, reducedMotion: true, negativeControl: true, packagedCSP: true });
      await page.close();
    }
  } finally {
    await browser.close();
    await writeFile(`${out}/browser-results.json`, JSON.stringify(records, null, 2) + '\n');
  }
}
console.log('Passed', records.length, 'complete engine/appearance journeys');
