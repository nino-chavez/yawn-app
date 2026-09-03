export const observedMeetings = [
  { id: "m1", group: "Previous 7 days", title: "Meeting · Sep 1, 2026", meta: "8:55 AM · transcript available", excerpt: "" },
  { id: "m2", group: "Previous 7 days", title: "Meeting · Sep 1, 2026", meta: "8:48 AM · interrupted", excerpt: "", attention: true },
  { id: "m3", group: "Previous 7 days", title: "Meeting · Sep 1, 2026", meta: "8:44 AM · interrupted", excerpt: "", attention: true },
  {
    id: "sparse",
    group: "Previous 30 days",
    title: "um, rule-based like system where it's like these products and these fields,",
    meta: "1:59 PM · transcript available",
    excerpt: "The discussion centers on introducing something to BigCommerce.",
  },
];

export const sparseMeeting = {
  id: "sparse",
  title: "um, rule-based like system where it's like these products and these fields,",
  metadata: "Aug 10, 2026 · Meeting note",
  stateFact: "The audio was already deleted. The transcript and note remain available, but this meeting cannot be retranscribed.",
  overview: [
    "The discussion centers on introducing something to BigCommerce.",
    "The speaker wants to reframe the topic.",
  ],
  recordReason: "Yawn is finishing your last meeting. Recording will be available again shortly.",
};

// A real transcript-only meeting. This is deliberately separate from sparse:
// sparse proves the generated-note reading surface, while this state proves
// the first local generation action and its surrounding hierarchy.
export const noNoteMeeting = {
  id: "no-note",
  title: "I want it to be what time I work",
  metadata: "Aug 19, 2026 · Transcript",
  stateFact: "The audio was already deleted. The transcript remains available, but this meeting cannot be retranscribed.",
  generationHelp: "Runs the downloaded note model on this Mac. It usually takes several minutes, longer for long meetings. Nothing leaves your computer.",
  recordReason: "Yawn is finishing your last meeting. Recording will be available again shortly.",
};

export const denseMeeting = {
  id: "dense",
  title: "Launch checklist review",
  metadata: "Sep 2, 2026 · 10:05 AM · 24:18 · Note ready",
  overview: "The team reviewed the launch checklist and confirmed the retry-review release plan.",
  decisions: [
    { text: "Ship the retry review before Friday.", source: true },
    { text: "Keep the transcript diff visible before any keep-or-promote choice." },
  ],
  followups: ["Sam drafts the release note by Thursday."],
  questions: ["Whether the 30-day trash window applies to exported bundles."],
  transcript: [
    { who: "Me", time: "00:00:04", text: "We reviewed the launch checklist today." },
    { who: "Them", time: "00:01:12", text: "Everyone agreed to ship the retry review before Friday.", selected: true },
    { who: "Me", time: "00:03:40", text: "The diff has to be on screen before anyone picks keep or promote." },
  ],
};

export const attentionMeeting = {
  id: "m2",
  title: "Meeting · Sep 1, 2026",
  metadata: "Sep 1, 2026 · 8:48 AM · Interrupted",
  headline: "This recording did not finish.",
  detail: "Yawn quit while finishing the file. The retained audio can still be transcribed on this Mac.",
};
