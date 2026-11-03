// Real renderer check for the selected note/transcript inspection affordance.
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const wait = async selector => { for (let n=0;n<80;n++) { const node=document.querySelector(selector); if (node) return node; await sleep(50); } throw new Error(`Missing ${selector}`); };
(await wait('[data-action="open-meeting"]')).click();
await sleep(300); // Let the asynchronous meeting-open projection settle before editing.
const editor = await wait('.ledger-editor');
const fold = await wait('.ledger-fold');
const checks=[];
const check=(id,pass,actual)=>checks.push({id,pass:Boolean(pass),actual});
// An offscreen WKWebView cannot establish physical :focus-visible. The
// companion browser check exercises real pointer and keyboard modalities.
check('one-native-editor',document.querySelectorAll('.ledger-editor').length===1,document.querySelectorAll('.ledger-editor').length);
check('larger-disclosure',fold.getBoundingClientRect().height>=22,fold.getBoundingClientRect().height);
check('mirror-hidden-from-accessibility',document.querySelector('.ledger-mirror').getAttribute('aria-hidden')==='true',document.querySelector('.ledger-mirror').getAttribute('aria-hidden'));
const original=editor.value;
editor.focus();editor.setSelectionRange(editor.value.length,editor.value.length);
document.execCommand('insertText',false,' review note');
await sleep(1150);
check('editing-after-poll',document.activeElement===editor&&editor.value===original+' review note',editor.value.length);
document.execCommand('undo');
check('native-undo-after-poll',editor.value===original,editor.value.length);
editor.setSelectionRange(0,0);
document.execCommand('insertText',false,'Untimed review line\n');
await sleep(1150);
const shifted=document.querySelector('.ledger-mark[data-line="1"]');
check('moment-shifts-with-leading-line',Boolean(shifted)&&Boolean(document.querySelector('.ledger-turn[data-line="1"]')),shifted?.dataset.line);
document.execCommand('undo');await sleep(100);
check('leading-line-undo',editor.value===original,editor.value.length);
return {name:'Rail inspection',pass:checks.every(c=>c.pass),keyboardModality:'verified separately in headless browser',checks};
