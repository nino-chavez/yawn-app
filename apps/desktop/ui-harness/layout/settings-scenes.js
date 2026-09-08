const settingsScene=new URLSearchParams(location.search).get('scene')||'settings';
const baseInvoke=window.__TAURI__.core.invoke;
window.__TAURI__.core.invoke=async(command,args)=>{
 const r=await baseInvoke(command,args);
 if(command==='first_run_permissions'&&settingsScene==='settings-permissions')return {microphone:'denied',systemAudio:'unavailable',probeUnavailable:true};
 if(command==='get_transcription_engine_settings'){
  if(settingsScene==='settings-apple-missing')r.apple={state:'assets-required',reason:'Apple speech needs a one-time preparation.',locale:'en-US'};
  if(settingsScene==='settings-apple-error')r.apple={state:'failed',reason:'Apple speech could not be prepared. Check your connection and try again.',locale:'en-US'};
  if(settingsScene==='settings-busy'){r.canChange=false;r.operationActive=true;r.apple={state:'installing',reason:'Preparing Apple speech on this Mac…',locale:'en-US'};}
 }
 if(['transcript_model_settings','note_model_settings'].includes(command)){
  if(settingsScene==='settings-download'){r.state='downloading';r.changeActive=true;r.canChange=false;r.downloadedBytes=220000000;r.totalBytes=463665005;}
  if(settingsScene==='settings-model-error'){r.state='failed';r.error='The model download could not be verified. Check the connection and try again.';}
  if(settingsScene==='settings-no-notes'&&command==='note_model_settings'){r.options.forEach(o=>{o.stored=false;o.active=false;});r.activeModelId=null;}
 }
 return r;
};

window.__layoutAppliedScene=settingsScene;
