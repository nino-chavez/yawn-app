---
surface: Today
build: 13-candidate
commit: 5230d72
device: iPhone 17 Pro simulator, iOS 26.5, content size Large (08 at XXXL, 09 with Increase Contrast)
reviewer: minder-blind-review (fresh subagent, no repo access)
implementer: sessions a53072ba and codex threads 01a05b4b, 01a04ef9
kind: cold
cold: true
states: [01, 02, 03, 04, 05, 06, 08, 09, 10, 11, 12]
verdict: revise
---

## Image by image

01, active imported calendar event.
Now: nothing is happening; the day is clear until 5:40 PM.
Next: an event called "Test calendar appointment" starting 5:40 PM, 59 minutes away.
Who: nobody. No person is named anywhere on this screen.
When and where: 5:40 PM at "Test location", with no address and no travel time.
Can do: choose a map place, open Details, add an activity or a task, change the day, open Filters.
Seconds: about 14, and most of that was spent deciding whether the dark button was the event or a setting.
Eye landed: first the word "Today", second the dark filled "Choose map place" button, third the event title.

02, upcoming family activity.
Now: nothing; clear until 3:45 PM.
Next: Gymnastics at 3:45 PM, 45 minutes away.
Who: Maya, on a screen titled "Zoey's Day".
When and where: 3:45 PM at Westside Gymnastics, no address, no travel time.
Can do: the same five things as 01.
Seconds: about 12, plus a second pass to work out whose day this is.
Eye landed: first "Zoey's Day", second the dark "Choose map place" button, third the pink flower tile and then "Gymnastics".

03, actionable task, mid scroll.
Now: unanswerable. Nothing in view says what time it is or what is current.
Next: Soccer practice 4:15 to 5:15 PM is the topmost item, then a pickup 5:30 to 5:45 PM.
Who: nobody is named on either event; the task names nobody either.
When and where: Zilker Park Field 4, then Sunshine Montessori. No date is visible on the screen.
Can do: open Details on either event, expand "Attention: Priority", mark "Pack the gym bag" done, open its overflow.
Seconds: about 20, and "what is happening right now" never resolved from these pixels.
Eye landed: first "Soccer practice", second the two lavender "Attention: Priority" chips, third the dark "Mark done".

04, dense mixed day.
Now: Personal focus time, running until 3:30 PM.
Next: Gymnastics at 3:45 PM, visible only as a title at the very bottom edge.
Who: Zoey now, Maya next, on a screen titled "Zoey's Day".
When and where: Library now; the next item's place is below the fold.
Can do: mark done, open Details, open the overflow, expand Attention, open "Preparation & details", add, change day, filter.
Seconds: about 9 for the current item, and the next item needed a scroll I could not take.
Eye landed: first "Personal focus time", second "Mark done", third "Add activity".

05, empty day.
Now: nothing else is scheduled today.
Next: nothing.
Who: nobody.
When and where: not applicable.
Can do: add an activity, add a task, change the day, open Filters on an empty list.
Seconds: about 5. This is the fastest screen in the set by a wide margin.
Eye landed: first "Today", second "Nothing else scheduled today", third the dark "Add activity".

06, calendar refresh failure.
Now: unanswerable; this is a scrolled position with no now anchor.
Next: "Test calendar appointment" at 5:41 PM, 59 minutes away.
Who: nobody.
When and where: "Test location", no address.
Can do: choose a map place, open Details by either of two routes, open Calendar setup. Also worth knowing: part of the day is missing, and the screen says so at the very bottom.
Seconds: about 18, and I only learned the day was incomplete after reading everything above it.
Eye landed: first the event title, second the dark "Choose map place", third the warning triangle.

08, accessibility XXXL.
Now: unanswerable. The RIGHT NOW card is on screen but its content is cut off at "Personal focus time".
Next: not visible.
Who: not visible.
When and where: not visible.
Can do: nothing. No control is reachable in this viewport.
Seconds: about 30, and four of the five questions were still unanswered when I stopped.
Eye landed: first "Zoey's Day", second "Pilot family · Thursday, August 20", third the pink "Now" pill.

09, increased contrast.
Now: nothing; clear until 5:42 PM.
Next: "Test calendar appointment" at 5:42 PM, 59 minutes away.
Who: nobody.
When and where: "Test location", no address.
Can do: identical to 01.
Seconds: about 14, the same as 01, because the screen is the same as 01 apart from the clock.
Eye landed: first "Today", second "Choose map place", third the event title.

10, completed task with undo.
Now: Personal focus time, struck through and marked done.
Next: Gymnastics, partly covered by the undo card.
Who: Zoey.
When and where: Library, until 3:30 PM.
Can do: undo, dismiss, mark not done, open Details, open the overflow.
Seconds: about 11, most of it spent on the banner rather than the day.
Eye landed: first the strikethrough title, second the "Marked done." card, third the dark "Undo".

11, add activity review sheet.
Now: reviewing a draft activity before it is saved.
Next: saving it.
Who: nobody is named on the draft.
When and where: Aug 20, 2026, 9:00 to 10:00 AM, with no place.
Can do: save the activity, go back to edit, open five optional details that were not added.
Seconds: about 7, then a second look because the name says afternoon and the time says morning.
Eye landed: first "Review before saving", second "Afternoon pickup", third the dark "Save activity".

12, activity details sheet.
Now: reading the full record for one imported event.
Next: not applicable; this sheet is about one item.
Who: nobody is named.
When and where: Sep 1, 2026, 5:42 to 6:12 PM at "Test location".
Can do: choose a map place, set Attention, add arrival instructions, close.
Seconds: about 10, and this is the only screen that told me plainly what I could and could not change.
Eye landed: first "Activity details", second the amber "Imported calendar · read-only" badge, third the event title.

## 1. Information hierarchy

The screen makes the app loudest and the day second. In 01, 02, 06 and 09 the largest, highest contrast, most saturated element on screen is the filled dark "Choose map place" button, roughly 290 points wide and pinned mid card. It resolves a data gap inside the app. The event it belongs to sits above it in dark semibold at maybe 22 points, unfilled. A caregiver glancing at this reads a button before they read an appointment.

The most useful number on the screen is the smallest. "in 59 minutes" and "in 45 minutes" render at body size in gray, trailing the absolute time in the same line, after a middle dot. For someone deciding whether to leave, the interval is the answer and the wall clock is the lookup. The weights are reversed inside a single line.

The top of every list screen is spent on identity and plumbing before content. In 01, 02, 09 and 08 the first three lines are the title, the date subtitle, and a gray provenance note with a calendar icon, either "Calendar connected · imported events stay read-only" or "Synthetic pilot day · no family data". That is the highest value real estate on a phone, and it holds a sentence that never changes and never helps.

Below that, three more bands push the day down: the create row, the seven day strip with its two circular arrows, and a "Schedule" heading paired with a "Filters" pill, followed by an "Up Next" heading and a gray subtitle explaining what "Up Next" means. In 04, that stack pushes the next activity of a dense day to the very bottom edge of the first screen. The current item is visible and the next item is not, which is the wrong way round for a screen you glance at between other things.

Who is unanswerable in three of the eleven states. 01, 06 and 09 name no person. Family items carry a first name on a quiet gray line below the location, smaller than the provenance text beside it. When the person is the reason the item exists, the person should not be the faintest line in the card.

Whose day it is cannot be answered on 02, 04 and 10. The screen is titled "Zoey's Day", the current item belongs to Zoey, and the next item belongs to Maya. A per person day view is a reasonable idea. A per person day view showing another person's items is not readable.

## 2. What competes

Three create affordances sit side by side in one row: "Add activity" filled, "Add task" outlined, and an unlabeled circular overflow. Two of them are shouting and the third is a mystery, and none of them is what a glance needs.

Two navigation systems stack vertically with nothing between them. The day strip is one way to change what you are looking at; "Schedule" plus "Filters" is another. Together they take about a fifth of the first screen before any content.

Every card gives two routes to the same detail screen, and the pair is styled differently between states. In 03 it is a card level chevron plus a full width "Details" button. In 06 it is a full width "Details" button plus a separate "Details" row with its own chevron at the card's foot. Two routes is one too many; two routes that change shape between states is a reader tax.

The RIGHT NOW card in 04 carries five controls in one card: "Mark done", "Details", an overflow, "Attention: Priority", and "Preparation & details". That is the highest value slot on the screen and it is the most contended.

"Attention: Priority" in soft lavender sits directly beneath "Mark done" in dark plum, at similar width. Two purple family pills, adjacent, one an action and one a disclosure. In 03 the lavender chip appears twice within one screen and outweighs both event locations.

Amber does two jobs. It outlines the "Up Next" card as a good thing and it outlines the calendar failure card as a bad thing, in the same screen, in 06. The border color no longer carries meaning.

The full color emoji tiles compete with everything. A lightning bolt on soccer, a dolphin on a Montessori pickup, sparkles on a gym bag, a flower on gymnastics, a stack of books on focus time. They are the only saturated multicolor objects in a restrained palette, they sit in the leading position of every row, and they mean nothing.

## 3. Subtractive pass

Remove the persistent provenance line under the date. "Calendar connected · imported events stay read-only" is a settings fact. Lost: a first run reassurance, which belongs in first run.

Remove the "Up Next" subtitle, "The next priority activity in your day". The heading already says it. Lost: nothing.

Remove one of the two detail routes per card. Lost: nothing; both go to the same place.

Remove the "Dismiss" button on the undo card in 10. A confirmation that expires on its own does not need a chore attached. Lost: an explicit close for someone who wants the space back sooner, which a swipe covers.

Remove "Filters" when the list is empty, as in 05. Lost: nothing, since there is nothing to filter.

Remove "Details to save" and its line "Minder saves the reviewed details shown here" in 11, and the footer that explains what the save button will do. Lost: nothing; both restate the button.

Remove the unlabeled overflow next to "Add activity" from the primary row, or label it. Lost: a shortcut nobody can predict.

Combine the day strip, "Schedule" and "Filters" into one band. Lost: a little breathing room, gained: roughly a third of a screen.

Combine the two badges in 12, "Scheduled" and "Imported calendar · read-only", into one line. Lost: nothing.

Demote the three clause provenance tail on every card, "From Synthetic family calendar · Shapes my day · read-only", to a single word or a small mark, with the full sentence in Details. Lost: real transparency. A caregiver has a genuine need to know an item came from a shared calendar and cannot be changed here. Keep the fact, spend one word on it, not three clauses on every row.

Demote "Attention: Priority" out of the card and into Details, where 12 already explains it. Lost: fast triage for someone who uses that field heavily, which argues for keeping it as a small mark rather than a pill.

Disclose the destination paragraph on demand. "Choose a place in Apple Maps. Minder uses it only after you select it; a typed Calendar location alone is not enough for a safe estimate" is three lines of explanation attached to a one line problem. Lost: the reason behind the rule, which belongs one tap away.

Replace the emoji tile with a quiet monochrome mark, or drop it. Lost: color and a little warmth, which the cream ground already supplies.

## 4. Interaction semantics

"Add activity", filled dark with a plus. Creates something on the day. Unambiguous.

"Add task", outlined with a checklist glyph. Creates an undated item. Unambiguous, though the difference between an activity and a task is never stated on this screen.

The unlabeled circular overflow beside them. Unknown. Not unambiguous; a three dot circle at the end of a create row could be more create options, more view options, or app settings.

The circular left and right arrows around the day strip. Move the strip by a week or a day. Shape is clear, the unit is not, and the fifth day chip is clipped at the right edge in every capture, which reads as broken rather than as a scroll hint.

Day chips such as "WED 2" and "TODAY 1". Switch the day being shown. Unambiguous, and the filled treatment on today is correct.

"Filters", pill with a slider glyph. Narrows the list. Unambiguous. Should not exist on an empty day.

The "Clear" pill at the top right of the RIGHT NOW card in 01, 05 and 09. This is the single most ambiguous control in the set. It carries a check in a circle and sits where a button sits, so it reads as "clear this card". In 04 and 10 the same slot holds a pink "Now" pill, which is plainly a status. So "Clear" is an adjective describing the day, dressed as a verb. Change the word or change the shape.

"Choose map place", filled dark with a map glyph. Opens a place picker so travel time can be computed. The label is clear. Its weight is wrong, and on an imported read only event it implies you are editing the event when you are adding a Minder side detail.

"Details", full width outlined with an info glyph. Opens the detail sheet. Unambiguous.

The card level chevron in 03 and the "Details" row in 06. Both open the same sheet as the button above them. Redundant, and the chevron in particular implies a different destination.

"Attention: Priority" with a caret. A disclosure or picker for an attention level, as 12 confirms by placing it under "Editable in Minder". The shape is right and the word is wrong: "Attention" reads as an alert about the item, not a field you set. Rename it and it becomes clear at no structural cost.

"Mark done", filled dark with an open circle. Completes the item. Unambiguous. On an imported calendar event it needs to say that completion is local to Minder, which the detail sheet does say and the card does not.

"Mark not done", outlined with a filled check. Reverses completion. Unambiguous, if clumsy to read.

The per item overflow circle on task and activity cards. Unknown, same problem as the create row overflow.

"Preparation & details" with a chevron, in 04 and 10. Opens something more than Details, apparently. Two rows on one card, both leading to detail, with different names. Not unambiguous.

"Undo" and "Dismiss" in 10. Restores the incomplete state, and closes the banner. Both clear. "Dismiss" should not need to exist.

"Open Calendar setup" in 06. Goes to calendar selection. Clear, though it is styled as bold text rather than a control.

"Back to edit" and "Close" on the sheets. Return to the form, and dismiss the sheet. Both clear.

"Optional details not added (5)" with a chevron. Opens the five things you skipped. Clear enough, though a count is less useful than names.

"Save activity". Commits the draft. Unambiguous.

Controls that should not exist where they appear. "Choose map place" sits inside the section headed "Calendar · read-only" in 12, directly under a paragraph saying title, time and location come from Calendar. An editable action inside a read only section contradicts the sheet's own organizing idea; it belongs under "Editable in Minder". The read only Title, Location and Calendar rows in 12 are drawn as form rows, label left, value right, hairline separators, which is the shape of an editable field. Draw them as facts, not fields. The card level chevron should not exist alongside a Details button. "Dismiss" should not exist on a self expiring confirmation.

## 5. Copy

Text that explains the app rather than the day:

"Calendar connected · imported events stay read-only"

"Choose a place in Apple Maps. Minder uses it only after you select it; a typed Calendar location alone is not enough for a safe estimate."

"From Synthetic family calendar · Shapes my day · read-only"

"The next priority activity in your day"

"Minder saves the reviewed details shown here."

"Title, time, and location come from Calendar. In Minder, you can mark this complete or add arrival instructions, people, notes, reminders, and tasks."

Text that reassures rather than informs:

"Choosing Save activity will save this activity in Minder. Calendar stays unchanged."

"Undo is available for Personal focus time."

"Saved Minder details stay local and may need review."

Text that reads like a system rather than a person:

"Synthetic pilot day · no family data"

"Test calendar appointment", "Test location", "From Minder Pilot Test"

"Optional details not added (5)"

"Details to save"

"Marked done."

"Attention: Priority"

The test scaffolding is worth calling out on its own. Four of the eleven captures show an event literally named "Test calendar appointment" at a place named "Test location" from a calendar named "Minder Pilot Test", and three more are headed "Synthetic pilot day · no family data". I cannot tell from pixels whether that string set can reach a real user. If any of it can, it is the loudest defect in the build.

## 6. Typography, color, spacing, shape, depth

The ground is a warm oatmeal cream. Cards are near white with generous corner radius, around 16 to 20 points, and hairline warm gray borders. One color does the heavy work, a deep plum navy, used for headings, primary fills and the selected day chip. Three accents appear: amber for the "Up Next" outline and the read only badge, a rose pink for the "Now" pill, and a pale lavender for the attention chip.

Type is a geometric leaning sans with a tall x height, set very bold at the top and dropping to a mid gray for everything secondary. There are effectively three weights and about five sizes, and the jump from the screen title to body is large. Line spacing is loose and comfortable. Vertical rhythm is generous to a fault: on 05 the content stops a third of the way down and the remaining two thirds are empty cream.

Depth is used exactly once, and correctly. The RIGHT NOW card is the only element carrying a soft shadow; everything else is flat with a border. That single decision is the clearest piece of hierarchy in the whole system.

It holds together as one character except for two intrusions. The full color emoji tiles belong to a different, louder product. And the amber outline means two opposite things within one screen in 06.

Three adjectives: calm, tidy, over explained.

## 7. Accessibility states

At XXXL in 08, nothing clips, nothing truncates mid word, nothing overlaps, and every touch target grows. The pink "Now" pill keeps its shape and its icon scales with it. The type system genuinely scales, which is the hard part and it is done.

What breaks is what the layout chooses to spend the screen on. The title, a two line subtitle and a three line provenance note consume roughly 55 percent of the viewport before any content, and the RIGHT NOW card is cut off at the third word of the activity title. None of the five questions can be answered without scrolling, and no control is reachable. The header material that is merely nice at Large becomes the entire screen at XXXL. It should collapse, truncate or move as sizes grow.

With Increase Contrast on in 09, the screen does not visibly respond. 09 and 01 differ only by the clock. The elements that most need the help are exactly the ones unchanged: the mid gray body paragraph under "Destination needed for directions", the gray provenance tail, the pale lavender attention chip, and the cream on near white card edges, which are separated by a hairline that is already close to invisible. I cannot tell from pixels whether the setting is wired at all; I can say the result is indistinguishable.

## 8. Empty and failure states

05 helps. "Nothing else scheduled today" plus "Add an activity to organize the rest of today" is two short lines, states the fact, offers the move, and does not lecture. It is the fastest screen in the set. Two things undercut it. The message lives inside a card labeled RIGHT NOW, but it is a statement about the whole day, not about this minute. And a "Filters" pill floats over an empty region, offering to narrow nothing.

06 informs rather than lectures, and it is honest in the right way: it names the cause, names the consequence, says what happens to your own data, and gives one action. The problem is position. The card sits below a full height activity card, so the news that part of your day is now hidden arrives last. When the list on screen is incomplete, that fact outranks the list.

## 9. Sheets

12 knows exactly what it is for. It splits the record into "Calendar · read-only" and "Editable in Minder" and explains the split in one sentence. That is the clearest model communication in the build. Two flaws: "Choose map place" is placed inside the read only section, and the read only rows are drawn in the shape of editable fields.

11 half knows. It is framed correctly as a confirmation, "Review before saving" with "Back to edit" rather than a generic back. But the draft it is confirming is named "Afternoon pickup" and scheduled 9:00 to 10:00 AM, and the review flagged nothing. Either that is nonsense seed data in a candidate build, or the review step does not check the thing a review step exists to check. Blind, I cannot separate the two, and either way this sheet had one job on this screen and did not do it. The rest of the sheet is mostly empty, and what fills it is a tautology and a footer explaining the button below it.

## 10. Element classification

- Screen title, "Today" or "Zoey's Day": usable but weak
- Date subtitle line: correct
- Provenance line under the date, "Calendar connected · imported events stay read-only": unnecessary
- RIGHT NOW card: correct
- Shadow on the RIGHT NOW card, the only elevation in the system: correct
- "Clear" pill in the card's top right: defect
- "Now" pill in the same slot: correct
- "Clear until 5:40 PM" line: correct
- "Add activity", filled: correct
- "Add task", outlined: correct
- Unlabeled overflow beside the create pair: unnecessary
- Day strip: appealing but wrong
- Clipped fifth day chip at the right edge: defect
- Circular week arrows: usable but weak
- "Schedule" section heading: usable but weak
- "Filters" pill: usable but weak
- "Filters" pill shown over an empty list: unnecessary
- "Up Next" heading: correct
- "The next priority activity in your day" subtitle: unnecessary
- Amber outline on the Up Next card: usable but weak
- Emoji tile on every row: appealing but wrong
- Activity title: correct
- Absolute start time, "Starts 5:40 PM": correct
- Relative time, "in 59 minutes": usable but weak
- Time range on list rows, "4:15 PM–5:15 PM": correct
- "Destination needed for directions" heading: correct
- The three line explanation beneath it: usable but weak
- "Choose map place" filled button on a card: appealing but wrong
- Person name line, "Maya" or "Zoey": usable but weak
- Location line with pin glyph: correct
- Provenance tail, "From X · Shapes my day · read-only": usable but weak
- Card level chevron: unnecessary
- "Details" full width button: correct
- "Details" row with chevron at the card foot: defect
- "Attention: Priority" chip: usable but weak
- Left accent bar on event cards: correct
- "Mark done": correct
- "Mark not done" with strikethrough title: correct
- Per item overflow circle: usable but weak
- "Preparation & details" row: usable but weak
- "Tasks" heading with "No date or time": correct
- Task card, "In Minder · editable": correct
- Empty state line and its helper sentence: correct
- Undo card: usable but weak
- "Undo": correct
- "Dismiss": unnecessary
- Completed item still occupying the RIGHT NOW slot and still labeled "Now": defect
- Calendar failure card: usable but weak
- "Open Calendar setup": correct
- Sheet header with "Back to edit" or "Close": correct
- "Activity draft" badge: correct
- Draft name and time in 11, "Afternoon pickup" at 9:00 AM: defect
- "Details to save" with its subtitle: unnecessary
- "Optional details not added (5)": usable but weak
- Footer reassurance above "Save activity": unnecessary
- "Save activity": correct
- "Scheduled" badge in 12: usable but weak
- "Imported calendar · read-only" amber badge: usable but weak
- Read only rows drawn as form rows: appealing but wrong
- "Choose map place" inside the read only section: defect
- Read only versus editable section split in 12: correct
- Absence of any response to Increase Contrast: defect
- Header consuming the whole viewport at XXXL: defect
- Test and pilot strings visible throughout: defect

## Verdicts

01: revise. The loudest element on the screen is a button about map data, and nobody is named.

02: revise. Answers the day well, but a screen titled for Zoey leads with Maya's activity.

03: revise. No temporal anchor survives scrolling, so the first question cannot be answered at all.

04: revise. The current item is well presented and the next item is pushed off the screen by chrome.

05: revise. The message and the action are right, and a Filters pill sits over an empty list.

06: revise. The failure card is well written and arrives after everything it invalidates.

08: revise. Type scales correctly and the layout spends the entire viewport on the header.

09: revise. Indistinguishable from 01, so the setting buys the user nothing.

10: revise. Reversal is handled honestly, but a completed item still holds the "right now" slot under a banner that covers the next one.

11: revise. A review step that did not surface a morning time on an activity named afternoon.

12: revise. The clearest screen in the set, undone by an edit action filed under read only.

## Three findings to fix first

1. The review sheet in 11 approved "Afternoon pickup" at 9:00 to 10:00 AM without comment. Either the seed data is wrong in a candidate build or the confirmation step validates nothing. This is the only place in the set where the product's stated job, checking before saving, visibly did not happen.

2. What the screen makes loudest is not what the glance needs. Demote "Choose map place" out of primary fill, raise the relative interval above the wall clock, and move the persistent provenance line off the top of the screen. The same edit fixes 04, where chrome pushes the next activity off the first screen of a dense day.

3. Two accessibility results, one fix each. Make muted body copy, the lavender chip and card borders respond to Increase Contrast, since 09 currently buys nothing. Make the header collapse as text size grows, since at XXXL it consumes the screen and leaves all five questions unanswered.

## What a stronger version would feel like

It would answer the next thing, the time until it, the place and the person before the eye has finished landing, and everything about how the data got there would wait until asked. It would be quiet in exactly the places it is currently loud, so the one dark button on screen is always the thing you actually do next. And it would stay legible as the day fills up, the text grows, and the calendar misbehaves, because the parts that explain the app would give way first and the parts that describe the day would be the last to go.
