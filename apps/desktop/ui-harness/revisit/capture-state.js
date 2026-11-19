const sleep=ms=>new Promise(r=>setTimeout(r,ms));
await sleep(900);
if (mode === 'settings-retirement' && document.querySelectorAll('.settings-group').length < 4) return {pass:false,error:'Settings content did not mount'};
if ((window.__errors || []).length) return {pass:false,error:'Frontend errors',errors:window.__errors};
const p=new URLSearchParams(location.search),study=p.get('study')||'current',probe=p.get('probe')||'none';
if(probe==='related'||probe==='expanded'){
 const fold=document.querySelector('.ledger-fold');if(!fold)return{pass:false,error:'No fold for relationship check'};
 const editor=document.querySelector('.ledger-editor'),before=editor.getBoundingClientRect();fold.focus();if(study==='refined')document.documentElement.dataset.yawnReviewLine=fold.dataset.line;
 if(probe==='expanded'){fold.click();await sleep(300);document.getAnimations().forEach(a=>a.finish());if(fold.getAttribute('aria-expanded')!=='true')return{pass:false,error:'Fold did not expand'};const after=editor.getBoundingClientRect();if(['top','left','width','height'].some(k=>Math.abs(before[k]-after[k])>.5))return{pass:false,error:'Note editor moved on fold expansion'};}
 if(study==='refined'){
  const mark=document.querySelector(`.ledger-mark[data-line="${fold.dataset.line}"] i`);if(!mark)return{pass:false,error:'No matching note mark'};
  await sleep(150);const sample=document.createElement('i');sample.style.color='var(--moment)';document.body.append(sample);const expected=getComputedStyle(sample).color;sample.remove();if(getComputedStyle(mark).backgroundColor!==expected)return{pass:false,error:'Relationship cue not visible',mark:getComputedStyle(mark).backgroundColor,expected,root:document.documentElement.dataset.yawnStudy,line:document.documentElement.dataset.yawnReviewLine,rules:document.querySelector('#review-relationships')?.sheet?.cssRules.length};
 }
}
const editor=document.querySelector('.ledger-editor');const turns=[...document.querySelectorAll('.ledger-turns>.ledger-turn,.ledger-turns>.ledger-fold,.ledger-turns>.ledger-fold-turns')].map(n=>n.getBoundingClientRect()).sort((a,b)=>a.top-b.top);const overlap=turns.some((r,i)=>i&&r.top<turns[i-1].bottom-.5);
if(overlap)return{pass:false,error:'Transcript controls overlap'};
if(document.documentElement.scrollWidth>innerWidth)return{pass:false,error:'Horizontal overflow'};if(document.querySelector('[data-action="generate-note"]'))return{pass:false,error:'Retired generation control rendered'};
return {pass:true,study,probe,mode,theme:document.documentElement.dataset.theme,errors:window.__errors||[],notes:editor?.value??null,folds:document.querySelectorAll('.ledger-fold').length,retiredGenerationControl:!!document.querySelector('[data-action="generate-note"]'),overflow:document.documentElement.scrollWidth>innerWidth};
