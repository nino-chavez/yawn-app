// Fixture set for the rethink concepts. The four library rows are the real
// meetings on this Mac as they render today (titles, dates, status). The note,
// transcript, and during-state are SYNTHETIC (from the harness fixture) and are
// labeled as such in every frame. Nothing here is product data.
export const meetings = [
  { id: "m1", title: "Meeting · Sep 1, 2026", date: "Sep 1, 2026", time: "9:41 AM", length: "0:32",
    status: "transcript-ready", statusText: "Transcript ready", excerpt: "No note yet. Transcript retained on this Mac.", group: "Yesterday" },
  { id: "m2", title: "Meeting · Sep 1, 2026", date: "Sep 1, 2026", time: "8:12 PM", length: "0:11",
    status: "recovered-interrupted", statusText: "Needs attention", excerpt: "Recording was interrupted before it finished. Audio retained.", group: "Yesterday" },
  { id: "m3", title: "Meeting · Sep 1, 2026", date: "Sep 1, 2026", time: "7:58 PM", length: "0:09",
    status: "recovered-interrupted", statusText: "Needs attention", excerpt: "Recording was interrupted before it finished. Audio retained.", group: "Yesterday" },
  { id: "m4", title: "um, rule-based like system where it's like these products and these fields,", date: "Aug 10, 2026", time: "2:03 PM", length: "3:12",
    status: "transcript-ready", statusText: "Transcript ready", excerpt: "Transcript available. No note generated.", group: "Previous 30 days" },
];
// State 3 uses a synthetic meeting with a generated note so a claim can open its source.
export const noteMeeting = {
  id: "fx", title: "Launch checklist review", date: "Sep 2, 2026", time: "10:05 AM", length: "24:18",
  status: "ready", statusText: "Note ready", excerpt: "Reviewed the launch checklist; ship the retry review before Friday.", group: "Today",
  fixture: "Fixture note — synthetic meeting, not data from this Mac",
  note: {
    overview: "The team reviewed the launch checklist and confirmed next steps for the retry review.",
    decisions: [
      { text: "Ship the retry review before Friday.", source: 2 },
      { text: "Keep the transcript diff visible before any keep-or-promote choice.", source: 3 },
    ],
    followups: [
      { text: "Sam drafts the release note by Thursday.", source: 4 },
    ],
    questions: [
      { text: "Whether the 30-day trash window applies to exported bundles.", source: 5 },
    ],
  },
  transcript: [
    { i: 1, t: "00:00:04", who: "Me",   text: "We reviewed the fixture launch checklist today." },
    { i: 2, t: "00:01:12", who: "Them", text: "Everyone agreed to ship the retry review before Friday." },
    { i: 3, t: "00:03:40", who: "Me",   text: "The diff has to be on screen before anyone picks keep or promote." },
    { i: 4, t: "00:05:02", who: "Them", text: "I'll draft the release note by Thursday." },
    { i: 5, t: "00:07:15", who: "Me",   text: "Does the thirty-day window cover exported bundles? I don't know yet." },
    { i: 6, t: "00:07:31", who: "Them", text: "Open question. Park it." },
  ],
};
export const during = {
  elapsed: "2:14", startedAt: "10:05 AM", pauses: 1, pausedFor: "0:03", paused: false,
  notes: "ask about export window\nretry diff before choose",
  liveTurns: [
    { t: "00:00:04", who: "Me", text: "We reviewed the fixture launch checklist today." },
    { t: "00:01:12", who: "Them", text: "Everyone agreed to ship the retry review before Friday." },
  ],
  fixture: "Fixture capture — synthetic, not a real recording",
};
export const attention = {
  meeting: meetings[1],
  headline: "This recording was interrupted before it finished.",
  detail: "Yawn quit while finishing the file. The audio is retained on this Mac; no transcript was made.",
  actions: ["Transcribe retained audio", "Move to Trash"],
};
export const states = {
  1: "Home with meetings, nothing selected",
  2: "During capture, two minutes in, one pause",
  3: "After: a meeting with a generated note and one claim's source open",
  4: "Needs attention: the recovered-interrupted meeting",
};
