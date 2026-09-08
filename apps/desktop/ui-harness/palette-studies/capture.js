for(let i=0;i<100;i++){
 if(window.__paletteApplied && (location.pathname.includes('settings')?document.querySelector('#models')?.textContent.length>50:window.__layoutReady))break;
 await new Promise(r=>setTimeout(r,50));
}
const palette=new URLSearchParams(location.search).get('palette');
if(window.__paletteApplied!==palette)return{error:'Palette not applied'};
if(location.pathname.includes('settings'))document.querySelector('#transcription').scrollIntoView();
await new Promise(r=>setTimeout(r,160));
return{pass:true,palette,synthetic:true,width:innerWidth,height:innerHeight,colors:Object.fromEntries(['window','sidebar','content','label','accent'].map(k=>[k,getComputedStyle(document.documentElement).getPropertyValue('--'+k).trim()]))};
