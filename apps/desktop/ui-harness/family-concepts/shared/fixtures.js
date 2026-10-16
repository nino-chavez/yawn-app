// Fixtures for the 2026-10-15 family rethink concepts. EVERY meeting, note,
// time, transcript turn, and library row here is INVENTED for the comparison.
// Nothing comes from a recording or from this Mac's library. Every frame
// carries a label saying so.

export function clock(seconds) {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}

// ---------------------------------------------------------------- Mac ----

export const mac = {
  fixture: "Invented content",
  library: [
    { id: "m0", title: "Launch checklist review", time: "10:05 AM", length: "24:18",
      status: "ready", statusText: "Transcript ready", excerpt: "retry review ships before Friday — Sam owns", group: "Today" },
    { id: "m1", title: "Meeting · Oct 14, 2026", time: "3:30 PM", length: "6:41",
      status: "attention", statusText: "Needs attention", excerpt: "Recording was interrupted. Audio kept.", group: "Yesterday" },
    { id: "m2", title: "Vendor call", time: "9:00 AM", length: "18:04",
      status: "ready", statusText: "Transcript ready", excerpt: "ask for the Q1 rate card before signing", group: "Yesterday" },
    { id: "m3", title: "Q4 schedule", time: "11:15 AM", length: "31:52",
      status: "ready", statusText: "Transcript ready", excerpt: "Ana out first two weeks of Nov", group: "Previous 7 days" },
    { id: "m4", title: "Budget draft", time: "1:15 PM", length: "52:10",
      status: "ready", statusText: "Transcript ready", excerpt: "three options, pick by the 20th", group: "Previous 30 days" },
  ],
  meeting: {
    id: "m0", title: "Launch checklist review", date: "Oct 15, 2026", time: "10:05 AM",
    length: "24:18", lengthSeconds: 1458, retention: "Audio kept until Oct 22",
    // Typed lines with the moment each was first typed. The moment is the
    // capability the brief flags as not built; frames that use it say so.
    notes: [
      { t: 48,   text: "retry review ships before Friday — Sam owns" },
      { t: 130,  text: "diff must be on screen before keep / promote" },
      { t: 245,  text: "export: does the 30-day window cover bundles? (open)" },
      { t: 440,  text: "Dana → release note Thursday" },
      { t: 690,  text: "budget stays under 1,200 → no board vote" },
      { t: 902,  text: "QA pass on the installer Monday" },
      { t: 1180, text: "ask Priya about the notarization delay" },
      { t: 1390, text: "next sync Oct 28" },
    ],
    transcript: [
      { t: 4,    who: "You",  text: "Okay, let's go through the launch checklist from the top." },
      { t: 31,   who: "Them", text: "The retry review is the only thing still open on my side." },
      { t: 44,   who: "You",  text: "Can that ship before Friday? Sam, is that yours?" },
      { t: 52,   who: "Them", text: "Yes. I'll own it. Friday is fine if nothing else lands on me." },
      { t: 118,  who: "Them", text: "One thing on the retry flow. People pick keep or promote too fast." },
      { t: 127,  who: "You",  text: "Then the diff has to be on screen before either button is." },
      { t: 141,  who: "Them", text: "Agreed. We'll block the buttons until the diff renders." },
      { t: 236,  who: "You",  text: "Export. Does the thirty-day trash window apply to exported bundles?" },
      { t: 249,  who: "Them", text: "I don't think we ever decided that. Let's park it as an open question." },
      { t: 431,  who: "You",  text: "Release note. Who's drafting it?" },
      { t: 438,  who: "Them", text: "Dana said she'd have a draft by Thursday." },
      { t: 672,  who: "Them", text: "What's the most we can spend on the installer work without going back to the board?" },
      { t: 686,  who: "You",  text: "If it stays under twelve hundred, nobody has to vote on it." },
      { t: 701,  who: "Them", text: "Good. That covers the signing certificate and the QA day." },
      { t: 895,  who: "You",  text: "Speaking of QA, when does the installer pass happen?" },
      { t: 904,  who: "Them", text: "Monday. Full pass on a clean machine." },
      { t: 1171, who: "You",  text: "Notarization has been slow all week. Has anyone talked to Priya?" },
      { t: 1183, who: "Them", text: "Not yet. I'll ask her after this." },
      { t: 1366, who: "You",  text: "Last thing. Same time on the twenty-eighth?" },
      { t: 1381, who: "Them", text: "Works for me." },
      { t: 1402, who: "You",  text: "Then that's the list. Thanks, everyone." },
      { t: 1419, who: "Them", text: "Thanks. Talk Friday." },
    ],
  },
  during: {
    elapsed: 252, // 4:12
    typed: [
      { t: 48,  text: "retry review ships before Friday — Sam owns" },
      { t: 130, text: "diff must be on screen before keep / promote" },
      { t: 245, text: "export: does the 30-day window cover bundles? (op" },
    ],
    liveTurns: 9,
  },
  attention: {
    id: "m1", title: "Meeting · Oct 14, 2026", date: "Oct 14, 2026", time: "3:30 PM",
    captured: "6:41", retention: "Audio kept until Oct 21",
    headline: "This recording was interrupted.",
    detail: "Yawn quit at 3:36 PM while the meeting was being recorded. The first 6:41 of audio is kept on this Mac. No transcript was made.",
    primary: "Transcribe the audio", secondary: "Move to Trash",
    notes: [{ t: 95, text: "gutters: get a second quote" }],
  },
  settings: {
    groups: [
      { id: "recording", name: "Recording", rows: [
        { label: "Microphone", detail: "MacBook Pro Microphone", state: "Authorized", kind: "ok" },
        { label: "System audio", detail: "Audio from the meeting app", state: "Authorized", kind: "ok" },
      ], action: "Check again" },
      { id: "transcription", name: "Transcription", rows: [
        { label: "Apple speech", detail: "On this Mac, English", state: "Selected", kind: "selected" },
        { label: "Yawn Small", detail: "1.1 GB, not downloaded", state: "Download", kind: "action" },
      ] },
      { id: "speakers", name: "Speakers", rows: [
        { label: "Anonymous speaker analysis", detail: "Runs only when you choose Analyze speakers", state: "Ready", kind: "ok" },
      ] },
      { id: "storage", name: "Storage", rows: [
        { label: "Meetings", detail: "7 meetings, 1.8 GB", state: "Show in Finder", kind: "action" },
        { label: "Audio", detail: "Kept 1, 7 or 30 days, chosen per meeting", state: "", kind: "none" },
        { label: "Trash", detail: "2 meetings, deleted after 30 days", state: "Empty Trash", kind: "action" },
      ] },
      { id: "updates", name: "Updates", rows: [
        { label: "Check for updates automatically", detail: "Installed while Yawn is idle", state: "on", kind: "toggle" },
        { label: "Version 0.6.16", detail: "Up to date", state: "Check Now", kind: "action" },
      ] },
    ],
    privacy: "Nothing leaves this Mac. Yawn has no account and makes no network calls with meeting data.",
  },
};

// --------------------------------------------------------------- iPad ----
// The same invented garden meeting the 2026-10-14 iPad concepts used, so the
// selected structure is compared on content the operator has already seen.

export const ipad = {
  fixture: "Invented content",
  meetingTitle: "Tue, Oct 14 · 10:02 AM",
  meetingLength: 1385, // 23:05
  lines: [
    { id: 0,  t: 18,   text: "Garden plan — fall",     indent: 36,  tilt: -1.2 },
    { id: 1,  t: 47,   text: "beds 4–6: new soil",     indent: 74,  tilt: 0.8 },
    { id: 2,  t: 82,   text: "2 yards? delivered",     indent: 104, tilt: -0.6 },
    { id: 3,  t: 125,  text: "Sam → quote by Fri",     indent: 62,  tilt: 1.4 },
    { id: 4,  t: 171,  text: "water timer broken",     indent: 44,  tilt: -1.6 },
    { id: 5,  t: 210,  text: "ask city: compost bins", indent: 80,  tilt: 0.4 },
    { id: 6,  t: 242,  text: "shed lock!!",            indent: 58,  tilt: -2.0 },
    { id: 7,  t: 465,  text: "volunteer day Nov 8",    indent: 40,  tilt: 1.0 },
    { id: 8,  t: 680,  text: "budget $1,200 max",      indent: 96,  tilt: -0.8 },
    { id: 9,  t: 902,  text: "no vote if under",       indent: 128, tilt: 0.6 },
    { id: 10, t: 1120, text: "photos to Dana",         indent: 52,  tilt: -1.0 },
    { id: 11, t: 1315, text: "next mtg Oct 28",        indent: 46,  tilt: 1.2 },
  ],
  addedAfterStop: { id: 12, t: null, text: "call Sam Mon", indent: 70, tilt: -0.9 },
  transcript: [
    { t: 4,    text: "Okay, let's start with the beds along the fence." },
    { t: 31,   text: "Four through six are basically clay at this point." },
    { t: 70,   text: "We'd need maybe two yards of soil, and someone to deliver it." },
    { t: 118,  text: "Sam said he could get us a quote by Friday." },
    { t: 164,  text: "The water timer on the east side stopped working last week." },
    { t: 202,  text: "Does the city still give out compost bins? I think they do." },
    { t: 238,  text: "And the shed lock is broken again, so the tools just sit there." },
    { t: 340,  text: "Moving on. The volunteer day." },
    { t: 451,  text: "November eighth works for most people." },
    { t: 545,  text: "What's the most we can spend without going back to the board?" },
    { t: 678,  text: "If we keep it under twelve hundred, nobody has to vote on it." },
    { t: 830,  text: "That should cover the soil and the timer." },
    { t: 982,  text: "Can someone send Dana the photos from today?" },
    { t: 1180, text: "I'll take care of that tonight." },
    { t: 1308, text: "So, same time on the twenty-eighth?" },
    { t: 1371, text: "Sounds good. Thanks, everyone." },
  ],
  // How many turns are folded between two notes at a real talking pace
  // (about one turn every ten seconds). Used for the fold counts.
  foldedBetween(a, b) { return Math.max(0, Math.round((b - a) / 10) - 1); },
  past: [
    { id: 0, day: "Tue, Oct 14", time: "10:02 AM", minutes: 23, firstLines: ["Garden plan — fall", "beds 4–6: new soil"], group: "Today", marks: [18, 47, 82, 125, 171, 210, 242, 465, 680, 902, 1120, 1315], length: 1385 },
    { id: 1, day: "Mon, Oct 13", time: "3:30 PM", minutes: 41, firstLines: ["Roof quote", "gutters too?"], group: "This week", marks: [60, 300, 520, 800, 1100, 1500, 1900, 2300], length: 2460 },
    { id: 2, day: "Thu, Oct 9",  time: "9:00 AM", minutes: 18, firstLines: ["Q4 schedule", "Ana out Nov"], group: "Last week", marks: [20, 140, 300, 420, 600, 780, 950], length: 1080 },
    { id: 3, day: "Tue, Oct 7",  time: "1:15 PM", minutes: 52, firstLines: ["Budget draft", "3 options"], group: "Last week", marks: [90, 400, 700, 1200, 1700, 2100, 2600, 2900], length: 3120 },
  ],
  duringSeconds: 252,   // 4:12
  pausedSeconds: 690,   // 11:30
  tappedLine: 8,        // "budget $1,200 max"
  interruptedSeconds: 401,
};

export const states = {
  1: "Mac · After, transcript ready",
  2: "Mac · During, 4:12 in",
  3: "Mac · Needs attention",
  4: "Mac · Settings",
  5: "Mac · First run",
  6: "iPad · During, 4:12 in",
  7: "iPad · After, replay paused at 11:30",
  8: "iPad · Before, consent",
  9: "iPad · Meeting list",
};
