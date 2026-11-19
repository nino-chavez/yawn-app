import {createRequire}from'node:module';import{writeFile,mkdir}from'node:fs/promises';import{resolve}from'node:path';const require=createRequire(import.meta.url);const{chromium,expect}=require('@playwright/test');const b=await chromium.launch({headless:true});const records=[],out=resolve(process.env.YAWN_REVIEW_OUT||'.artifacts/yawn-desktop-review')+'/';await mkdir(out,{recursive:true});const origin=process.env.YAWN_REVIEW_ORIGIN||'http://127.0.0.1:8876';
try{
 const p=await b.newPage({viewport:{width:1440,height:1100}});const errors=[];p.on('pageerror',e=>errors.push(e.message));await p.goto(origin+'/');
 for(const design of['current','refined'])for(const appearance of['light','dark'])for(const scene of['ledger','ledger-related','ledger-expanded','capture','first-run','interrupted','settings']){
  await p.locator(`[data-design="${design}"]`).click();await p.locator('#appearance').selectOption(appearance);await p.locator('#scene').selectOption(scene);await expect.poll(()=>p.locator('#capture').evaluate(i=>i.complete&&i.naturalWidth>0)).toBe(true);await expect.poll(()=>p.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);records.push({nativeFrame:true,design,appearance,scene,loaded:true});
 }
 await p.locator('[data-design="refined"]').click();await p.locator('#appearance').selectOption('light');await p.locator('#scene').selectOption('ledger');await p.locator('[data-view="interactive"]').click();const f=p.frameLocator('#app');await expect(f.locator('.ledger-fold').first()).toBeVisible();await expect(f.locator('html')).toHaveAttribute('data-yawn-study','refined');await expect(f.locator('html')).not.toHaveAttribute('data-yawn-review-line');
 const editor=f.locator('.ledger-editor');const original=await editor.inputValue();const before=await editor.boundingBox();const turn=f.locator('.ledger-turn').nth(1);const line=await turn.getAttribute('data-line');await turn.hover();const mark=f.locator(`.ledger-mark[data-line="${line}"] i`);await expect(mark).toHaveCSS('background-color','rgb(47, 95, 168)');await p.screenshot({path:out+'interactive-hover.png'});
 const fold=f.locator('.ledger-fold').first();await fold.focus();await expect(fold).toBeFocused();await expect(fold).toHaveCSS('outline-color','rgb(47, 95, 168)');await expect(fold).toHaveCSS('min-height','22px');await fold.press('Enter');await expect(fold).toHaveAttribute('aria-expanded','true');const after=await editor.boundingBox();for(const key of['x','y','width','height'])expect(Math.abs(after[key]-before[key])).toBeLessThan(.5);await p.keyboard.press('Escape');await expect(fold).toHaveAttribute('aria-expanded','false');await expect(fold).toBeFocused();
 await editor.focus();await editor.evaluate(node=>node.setSelectionRange(node.value.length,node.value.length));await p.keyboard.insertText(' review note');await p.waitForTimeout(1150);await expect(editor).toBeFocused();await expect(editor).toHaveValue(original+' review note');await editor.press('ControlOrMeta+z');await expect(editor).toHaveValue(original);records.push({hoverCue:true,keyboardCue:true,foldNoEditorMovement:true,escapeReturnsFocus:true,editingUndoAcrossPoll:true});
 const search=f.locator('[data-field="library-search"]');await search.fill('Harness');await expect(search).toHaveValue('Harness');await p.waitForTimeout(1000);await expect(search).toBeFocused();await search.fill('zzzz-no-meeting');await expect(f.locator('[data-action="open-meeting"]')).toHaveCount(0);await search.fill('');await expect(f.locator('[data-action="open-meeting"]').first()).toBeVisible();records.push({meetingSearch:true});
 await p.locator('#appearance').selectOption('dark');await expect(f.locator('html')).toHaveAttribute('data-theme','dark');await p.locator('#size').selectOption('960');await expect.poll(()=>f.locator('html').evaluate(()=>innerWidth)).toBe(960);await expect.poll(()=>f.locator('html').evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);await p.screenshot({path:out+'interactive-960-dark.png'});
 await expect(f.locator('.ledger-editor')).toBeVisible();
 const narrowEditor=f.locator('.ledger-editor');const narrowOriginal=await narrowEditor.inputValue();
 await narrowEditor.focus();await narrowEditor.evaluate(node=>{const end=node.value.indexOf('\n');node.setSelectionRange(end,end)});
 const addition=' — compare two longer alternatives before choosing the next release';await p.keyboard.insertText(addition);
 await p.waitForTimeout(1150);await expect(narrowEditor).toBeFocused();
 await expect(narrowEditor).toHaveValue(narrowOriginal.replace('\n',addition+'\n'));
 const firstLine=f.locator('.ledger-mirror-line').first();
 expect(await firstLine.innerText()).toContain(addition);
 await f.locator('.ledger-turn[data-line="0"]').hover();await expect(firstLine).toHaveCSS('background-color','rgba(143, 180, 255, 0.18)');
 await expect(f.locator('.ledger-mirror')).toHaveAttribute('aria-hidden','true');await expect(f.locator('.ledger-mirror')).toHaveCSS('pointer-events','none');
 await narrowEditor.focus();await narrowEditor.press('ControlOrMeta+z');await expect(narrowEditor).toHaveValue(narrowOriginal);
 await p.emulateMedia({reducedMotion:'reduce'});const narrowFold=f.locator('.ledger-fold').first();await narrowFold.click();await expect(narrowFold).toHaveAttribute('aria-expanded','true');await p.keyboard.press('Escape');await expect(narrowFold).toHaveAttribute('aria-expanded','false');
 records.push({wrappedLine960:true,mirrorHiddenFromAccessibility:true,notesUndo960:true,reducedMotionFold:true});
await p.setViewportSize({width:320,height:844});await expect.poll(()=>p.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);records.push({darkMode:true,smallDesktop:true,wrapper320:true});
 await p.setViewportSize({width:1440,height:1100});
 await p.locator('#scene').selectOption('settings');
 await expect(f.locator('.settings-group')).toHaveCount(6);
 await expect(f.locator('body')).not.toBeEmpty();
 records.push({settingsMounted:true});
 expect(errors).toEqual([]);records.push({pageErrors:errors});await p.close();
}finally{await b.close();await writeFile(out+'browser-results.json',JSON.stringify(records,null,2)+'\n');}console.log('Passed',records.length,'browser records');
