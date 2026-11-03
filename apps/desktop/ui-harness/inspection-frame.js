// Native content capture. Related-state posing is explicit because an offscreen
// WKWebView has no physical pointer/keyboard modality; browser tests cover that.
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
for(let n=0;n<80&&!document.querySelector('[data-action="open-meeting"]');n++) await sleep(50);
document.querySelector('[data-action="open-meeting"]')?.click();
for(let n=0;n<80&&!document.querySelector('.ledger.has-turns');n++) await sleep(50);
await sleep(300);
const ledger=document.querySelector('.ledger.has-turns');
if(!ledger)return{pass:false,error:'Ledger did not render'};
const pose=new URLSearchParams(location.search).get('inspection')||'rest';
if(pose==='expanded'){
 const fold=ledger.querySelector('.ledger-fold');fold.click();await sleep(250);document.getAnimations().forEach(a=>a.finish());
 if(fold.getAttribute('aria-expanded')!=='true')return{pass:false,error:'Fold did not expand'};
}
if(pose==='related'||pose==='expanded'){
 const line=ledger.querySelector('.ledger-fold').dataset.line;
 const poseInspection=()=>{
  for(const node of ledger.querySelectorAll('.ledger-mark,.ledger-mirror-line,.ledger-turn')) node.classList.toggle('is-inspected',node.dataset.line===line);
 };
 poseInspection();setInterval(poseInspection,20);
}
return{pass:true,pose,renderer:'Offscreen WKWebView content; synthetic commands; related-state pose only',errors:window.__errors||[],overflow:document.documentElement.scrollWidth>innerWidth};
