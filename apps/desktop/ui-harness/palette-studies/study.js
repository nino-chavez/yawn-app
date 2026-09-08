// Palette-only studies. All meeting content below is synthetic.
export async function color(){
 const palettes=await fetch('/palettes.json').then(r=>r.json());
 const p=palettes.find(p=>p.id===new URLSearchParams(location.search).get('palette'))||palettes[0];
 document.documentElement.dataset.theme='dark';
 for(const [key,value]of Object.entries(p.tokens))document.documentElement.style.setProperty('--'+key,value);
 for(const [key,from]of Object.entries({'selection-active':'selection','on-selection':'label'}))document.documentElement.style.setProperty('--'+key,p.tokens[from]);
 if(p.id!=='current')for(const [key,from]of Object.entries({'status-record-text':'record','status-attention-text':'attention','toast-attention-text':'attention','link-hover':'accent'}))document.documentElement.style.setProperty('--'+key,p.tokens[from]);
 window.__paletteApplied=p.id;
}
export async function apply(s,render){
 await color();
 const scene=new URLSearchParams(location.search).get('scene');
 if(scene==='attention'){s.selected=null;s.snapshot={...s.snapshot,startup:'failed',error:'Yawn could not finish preparing Apple speech. Your saved meetings are still here. Check your connection, then try again.'};render();window.__layoutReady=true;return;}
 const selected=s.selected;
 selected.row.label='A calmer first meeting';
 s.library.rows[1].label='Weekly product check-in';
 selected.note={...selected.note,state:'note',claims:[
 {ordinal:1,claimType:'decision',claim:'Start with Apple speech. Offer a model download when someone needs another option.',locatorCount:1,spans:[{sourceTurnIndex:0,text:'Let’s start with Apple speech and keep the model download optional.'}]},
 {ordinal:2,claimType:'action',claim:'Review the first-run experience with three new users before Friday.',locatorCount:1,spans:[{sourceTurnIndex:1,text:'I’ll review the first-run experience with three new users before Friday.'}]}],lock:{locked:false,unreadable:false},canConfirmOperator:true};
 selected.operatorNoteDraft='Keep the first step simple. Explain the next choice only when it becomes useful.';
 selected.transcript.turns=[{sourceTurnIndex:0,speaker:'Me',start:15,end:23,text:'Let’s start with Apple speech and keep the model download optional.'},{sourceTurnIndex:1,speaker:'Them',start:25,end:32,text:'I’ll review the first-run experience with three new users before Friday.'}];
 selected.evidenceSplit={open:true,ordinal:1,turnIndex:0};
 render();
 await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
 window.__layoutReady=true;
}
