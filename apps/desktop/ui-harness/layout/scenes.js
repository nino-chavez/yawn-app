// Synthetic render-only scenes. Production render functions remain unchanged.
const params=new URLSearchParams(location.search);
export async function applyScene(s,render){
 const scene=params.get('scene')||'meeting';
 const longTitle='Operations review — integration readiness and ownership across regional onboarding, billing, and customer support teams';
 const token='VendorIntegrationReadinessReviewForNorthAmericaAndInternationalCustomerSupportOwnership';
 const prose='Confirm the owner and due date before sharing the revised onboarding plan with the customer support and billing teams. '+token;
 const sha='0'.repeat(64);
 const now=Math.floor(Date.now()/1000);
 const selected=s.selected;
 if(selected){
  Object.assign(selected.row,{label:longTitle});
  Object.assign(selected.note,{meetingDeletionHandle:'delete-meeting',audioDeletionHandle:'delete-audio',transcriptDeletionHandle:'delete-transcript',microphonePlaybackHandle:'mic',systemPlaybackHandle:'system',lock:{locked:false,unreadable:false},canConfirmOperator:true});
  selected.operatorNoteDraft=prose;
  if(selected.transcript){ selected.transcript.turns=[
   {sourceTurnIndex:0,sourceSpeaker:'Me',speaker:'Me',start:1,end:6,text:prose},
   {sourceTurnIndex:1,sourceSpeaker:'Them',speaker:'Alexandria Montgomery-Worthington',start:7,end:10,text:prose,speakerCorrected:true},
   {sourceTurnIndex:2,sourceSpeaker:'Them',speaker:'Them',start:11,end:14,text:'',withheld:true}
  ];}
 }
 if(scene==='first-run'||scene==='empty'){s.selected=null;s.library={...s.library,rows:[],total:0,firstRunSheetSeen:scene==='empty'};}
 if(scene==='library-loading'||scene==='library-stalled'){s.selected=null;s.library=null;s.libraryStalled=scene==='library-stalled';}
 if(scene==='library-unavailable'){s.selected=null;s.library={state:'unavailable',rows:[],message:prose,firstRunSheetSeen:true};}
 if(scene==='no-matches'){s.selected=null;s.search=token;s.library={...s.library,rows:[],total:5,filterActive:true};}
 if(scene.startsWith('capture-')){
  const capture=scene==='capture-context'?'recording':scene.slice(8);
  s.selected=null;s.activeView='capture';
  s.snapshot={...s.snapshot,startup:'ready',capture,meeting_id:'synthetic-capture',mic_state:'capturing',system_state:'capturing',started_at_epoch_seconds:now-430,capture_state_started_at_epoch_seconds:now-90,transcription_last_worker_heartbeat_at_epoch_seconds:now-3,turns:capture==='transcript-ready'?[{sourceTurnIndex:0,speaker:'Me',start:1,end:6,text:prose}]:[],current_transcript_sha256:capture==='transcript-ready'?sha:null,error:['transcription-failed','summary-failed','recovered-interrupted'].includes(capture)?'The microphone stopped sending audio. Check the connection before recording another meeting.':null};
  s.noteDraft=prose;s.contextDraft=prose;
 }
 if(scene==='capture-pausing'){s.snapshot.capture='recording';s.snapshot.capture_pause_change_pending=true;}
 if(scene==='capture-resuming'){s.snapshot.capture='paused';s.snapshot.capture_pause_change_pending=true;}
 if(scene==='model-downloading'||scene==='model-verifying'||scene==='model-failed'){
  s.snapshot.model_setup={...s.snapshot.model_setup,state:scene.slice(6),selectedModelId:'whisper-large-v3-turbo-q4',downloadedBytes:221000000,totalBytes:463665005,error:scene==='model-failed'?'The speech-model download could not be verified. Check the connection and try again.':null};
 }
 if(scene==='start'||scene==='start-denied'||scene==='start-guided'){
  s.modal='start';s.startSheetGuided=scene==='start-guided';
  if(scene==='start-denied')s.permissions={microphone:'denied',systemAudio:'unavailable',probeUnavailable:true};
 }
 if(scene==='rename'){s.modal='rename-meeting';s.renameDraft=longTitle;}
 if(scene==='speaker'){s.modal='speaker-correction';s.speakerCorrection={sourceSpeaker:'Them',sourceLabel:'Them',sourceTurnIndex:1};s.speakerCorrectionDraft='Alexandria Montgomery-Worthington';}
 if(['delete-recording','delete-transcript','delete-meeting','lock-meeting','unlock-meeting','lock-unavailable'].includes(scene)){
  s.modal=scene==='lock-unavailable'?'lock-meeting':scene;
  if(scene==='lock-unavailable')selected.note.canConfirmOperator=false;
 }
 if(scene==='manage')s.meetingManagementOpen=true;
 if(scene==='locked'){selected.note={state:'locked',lock:{locked:true,unreadable:false},meetingDeletionHandle:'delete-meeting'};selected.transcript=null;}
 if(scene==='unavailable'){selected.note={state:'unavailable',message:prose,meetingDeletionHandle:'delete-meeting'};selected.transcript=null;}
 if(scene==='recovered'){selected.note.state='recovered-interrupted';selected.note.claims=[];selected.transcript=null;}
 if(scene==='recovered-empty'){selected.note={state:'recovered-interrupted',meetingDeletionHandle:'delete-meeting'};selected.transcript=null;}
 if(scene==='generating')s.generatingMeetingId=selected.row.meetingId;
 if(scene==='generation-unavailable'){selected.note.noteGenerationAvailable=false;selected.note.noteGenerationUnavailableReason=prose;}
 if(scene==='note'||scene==='inspector'||scene==='popover'){
  selected.note.state='note';selected.note.claims=[{ordinal:1,claimType:'decision',claim:prose,handle:'claim-handle-1'},{ordinal:2,claimType:'action',claim:prose,handle:'claim-handle-2'}];
  if(scene==='popover'){selected.row.label='Synthetic meeting';selected.note.claims.forEach(c=>{c.locatorCount=1;c.spans=[{sourceTurnIndex:0,text:prose}];});}
  if(scene==='inspector')selected.evidenceSplit={open:true,ordinal:1,turnIndex:0};
 }
 if(scene==='vocabulary'||scene==='vocabulary-empty'){
  s.modal='vocabulary';s.vocabulary={meetingId:selected.row.meetingId,sourceTranscriptSha256:sha,loading:false,sourcePhrase:token,preferredReplacement:'Vendor integration readiness review',entries:scene==='vocabulary-empty'?[]:[{id:'entry',sourcePhrase:token,preferredReplacement:'Vendor integration readiness review for international customers',enabled:true,appliedTurnCount:123}],pendingDeleteId:'entry',editingId:''};
 }
 if(scene==='retry'||scene==='retry-no-note'||scene==='retry-skipped'){
  s.modal='transcript-retry';s.transcriptRetry=await window.__TAURI__.core.invoke('transcript_retry_start',{meetingId:selected.row.meetingId});
 }
 if(scene==='trash'||scene==='trash-empty'){
  s.selected=null;s.trashOpen=true;s.trash={entries:scene==='trash-empty'?[]:Array.from({length:6},(_,i)=>({meetingId:`trash-${i}`,label:i===1?token:longTitle,deletedAtEpochSeconds:now-3000,purgeAfterEpochSeconds:now+30*86400}))};
 }
 if(scene==='toast')s.notice=prose;
 if(scene==='error-toast')s.error={message:prose,action:{action:'retry-startup',label:'Try again'}};
 if(scene==='transcript'||scene==='transcript-withheld'){selected.transcriptMatch={sourceTurnIndex:1};}
 if(scene==='long-title')selected.row.label=token;
 if(scene==='playback')s.audioPlayback={state:'playing',source:'microphone'};
 if(scene==='startup-error')s.snapshot={...s.snapshot,startup:'failed',error:prose};
 render();
 if(scene==='transcript'||scene==='transcript-withheld'){document.querySelector('details.transcript-disclosure').open=true;const pane=document.querySelector('.doc-main'),card=document.querySelector('.transcript-workspace');pane.scrollTop+=card.getBoundingClientRect().top-pane.getBoundingClientRect().top-12;if(scene==='transcript-withheld'){const turns=document.querySelector('.transcript-scroll');turns.scrollTop=turns.scrollHeight;}}
 if(scene==='capture-context')document.querySelector('details')?.setAttribute('open','');
 if(params.get('scale'))document.documentElement.style.zoom=params.get('scale');
 await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
 if(scene==='popover'){const a=[...document.querySelectorAll('.claim-source-button')].at(-1);const pane=document.querySelector('.doc-main');pane.scrollTop+=a.getBoundingClientRect().bottom-(innerHeight-24);a.dispatchEvent(new MouseEvent('mouseover',{bubbles:true}));await new Promise(r=>setTimeout(r,500));}
 if(scene.startsWith('search-')){const q=document.querySelector('[data-field="library-search"]');q.value=scene==='search-noresult'?'noresult':scene==='search-incomplete'?'incomplete':'budget';q.dispatchEvent(new Event('input',{bubbles:true}));await new Promise(r=>setTimeout(r,400));document.querySelector('[data-action="search-transcripts"]')?.click();await new Promise(r=>setTimeout(r,100));}
 window.__layoutAppliedScene=scene;
 window.__layoutReady=true;
}
