# Changelog

Every release of Pearl, newest first, in the spirit of
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow
[semantic versioning](https://semver.org/).

## [Unreleased]

### Changed

- A new mark, [¶]: a pilcrow in square brackets, in nacre, after MK Matrix's. It is the program's
  icon and shows on the splash screen, in the File view and beside the document's name.
- The ribbon takes one row: the groups' names no longer sit under the controls, and show when the
  pointer rests between them.
- The pages sit on a warm desk, lifted by a wide, soft shadow as paper is.
- The rulers stay out of sight until the pointer comes near the edge they run along.
- File › New shows each template by its own first page, drawn by Typst.
- The icons are in colour by meaning, in duotone: each family of commands draws its objects in
  its own ink over a soft tint of it; letters such as B and I keep the text's colour.

### Fixed

- More than 300 versions in one day no longer delete the older history: the day's extra versions go
  first, and one per hour for a week and one per day for three months stay (#44).
- Restoring a version stops, and says so, when the text it would replace can't be kept in the
  history first. A failed recovery copy no longer turns recovery off: it's tried again every few
  seconds, and the status bar says the text has no copy until one is written (#46).

## [0.1.0] - 2026-11-06

The first release.

- An editor for Typst: source on the left, live pages on the right, a ribbon, the File view, a left
  rail and a right strip, zoom in the status bar, four themes and a splash screen.
- Real-time sessions, directly between host and guests, encrypted with the key in the link.
- Autocompletion, closing brackets, help on hover and a search with its matches on show.
- Claude as a person in the session, with your own API key.
- Version history for saved documents: see what changed, look at past pages and restore.
- A more refined look: pages with a soft shadow on a lighter desk, a right strip of icons with their names under the pointer, a File view with its templates as sheets on one row, centred, and with long paths shortened in the middle; the ribbon's fields are aligned.
- The time worked on each document, in the Details pane: the total and the latest days, across saves and days. Only active time counts, and it stays on this computer.
- A colour picker for text and highlight: a palette with tints and shades, the latest colours used, and any other colour by a square, hue, opacity, hex code or RGB numbers. The ribbon's button shows the colour last used.
- A calmer editor: more air between lines, quieter line numbers, the caret's line marked, and source colours that go with the rest of the interface.
- Softly rounded corners on menus, fields, buttons and cards.
- Dark paper in the View tab, under the dark theme: the pages in dark tones while you write, with pictures, the document and its PDF as they are.
- The pearl beside the document's name in the title bar.
- The open file followed live: what another program writes to it, such as a coding agent, shows in the source and on the pages by itself, with the text before it in the version history. Unsaved text is never replaced without asking.
- A terminal beside the document, from the left rail: Claude Code, Codex or a shell, started by a click in the document's folder through your login shell, so programs in `~/.local/bin` are found. Colours, links, selection, copying and pasting, scrolling and resizing work as in other terminals; what the program writes to the document shows on the pages as it writes. Claude Code is told which document is open, so asking it to write in Pearl edits that file, and a document with no file is saved first. A program that isn't installed says so.
- Folders as projects: open one from the File view, with Ctrl+Shift+O or on the command line, and its files show in a tree beside the rail. Its main document opens, choosing another saves the one open first, and paths start at the folder's top, so a chapter can import a file above its own folder. Files other programs add show up, and a change to one the document reads shows on the pages. A file opened by a relative path on the command line is remembered by its whole path.
- Rulers in centimetres around the pages, with the margins shaded.
- Recovery: text left unsaved by a crash is offered back when Pearl opens again.
- Spell checking in Brazilian Portuguese: misspelt words underlined, and a right click for suggestions,
  ignoring a word or adding it to your dictionary.
- Images by pasting or dragging, or from any folder through the ribbon: copied into `images/` beside
  the document, with a figure at the caret. An unsaved document asks where to save first.
- A References pane: import a `.bib`, search it and click to cite at the caret, with the bibliography
  added to the document when it has none.
- References by DOI, and their check: citations with no entry and entries never cited are shown as
  you write; on request, each reference is looked up at doi.org or Crossref. Only DOIs and titles leave
  your computer, and only when you ask.
- A Documentation pane in the right strip: lessons with examples you can edit and see at once, a search,
  a reference of every Typst function, and F1 for the word at the caret. Where the strip's names
  don't fit, its icons stand alone.
- Over 90 ready-made formulas of algebra, trigonometry, geometry, calculus, linear algebra, statistics,
  probability, inference, finance and physics: with previews and a search, in the Formulas pane and
  through the Math tab, and a click puts one at the caret.
- Messages in the status bar stay long enough to be read, instead of giving way to the next compile.
- Letterhead and watermark, from a window with a live preview: name, address, logo and footer, and a
  background text or image, with the image's opacity. It's written as a rule in the source, and the
  window opens on it again.
- Comparing versions on the pages: before and now side by side, with what came in, went out or changed
  place marked on the finished pages.
- A typography watch in the Problems pane: widows, orphans, headings alone at the end of a page, what
  goes past the margin and figures far from their first mention, marked on the pages. A click goes to
  the passage, and Ignore lasts until the passage changes.
- Layout variants: the document set in a few other ways, as thumbnails of its own pages with their page
  counts. Choosing one writes its rules in the source, as one undo step.
- Fit in pages: the smallest changes to spacing, margins and type size that bring a document to a number
  of pages, shown before and after, to apply or refuse. It says how close it gets when it can't.
- Accessibility: the Problems pane points out images with no alternative text, skipped heading levels and
  text too faint for its background, and exporting warns about them first. Export accessible PDF asks
  for PDF/UA-1 and says what's missing when it can't be met.
- A PDF is written whole or not at all: a failed export leaves what was there before.
- A timeline of the writing: a slider over the versions with their pages, and one deleted passage brought
  back at a click, as one undo step, without restoring the rest.
- Find and replace (Ctrl+H): one match at a time, seeing each first, or all at once as one undo step.
  The search can match case and whole words.
- Comments on the text: threads tied to a passage, with replies, resolved and reopened, shared in a session
  and kept in a file beside the document, outside its text and its PDF.
- Reviewing on the pages: strike out, highlight or note on the finished pages. Each mark finds its source
  and becomes a suggestion to accept, as one undo step, or decline. Comments show on the pages too.
- On Windows, the window's close button is named as such, apart from the Close of a pane or a view.
- A splash screen with the words on Morrison Kühlsen's ink, as in MK Matrix, beside a picture whose clouds drift by, whose duck bobs, whose fish breathes bubbles and whose frog blinks.
- Typing on the pages: a click puts a caret there, also with only the pages on show, and what's typed
  goes into the source.
- Figures on the pages: a click selects an image, its corner resizes it, dragging it moves its block to
  another paragraph, and Delete takes it out of the text. Each is one undo step.
- Margins dragged on the rulers change the `#set page` rule, as one undo step.
- Copying and pasting with other programs, which didn't reach the system's clipboard.
- Saving replaces the file all at once and reaches the disk first, so a full disk or a power cut leaves the old text or the new, never part of it. A document in a folder where no new file can be created can't be saved there any more, and says so.
- On Windows, Pearl opens without a console window beside it, whose closing ended the program without asking to save. The program and its installer have Pearl's icon, and the program its version; a failure to start says so in a message.
- Several carets at once, as in VS Code: Alt+click adds or removes one, Alt+drag adds a selection, Shift+Alt+drag selects a box, and Shift+Alt or Ctrl+Alt with Up or Down adds one on the next line. Typing, accents, deleting, the arrows, copying and pasting work at all of them, with as many pasted lines as carets going in one each, and Esc goes back to one.
- Discarding changes to open another document no longer marks them as saved when nothing opens (the file dialog cancelled, a recent file that's gone, a bad session link): they stay unsaved, with their recovery copy, and closing still asks.
- A file changed outside Pearl (another editor, `git pull`, a sync folder) is no longer written over unseen: before saving, Pearl keeps what the file says as a version and asks whether to use it, write yours over it, or leave it for now.
- Undo stays within the open document: after opening or starting another, or recovering or joining one, it can't bring the previous text back into it, where autosave would write it over the file.
- Packages for Linux, Windows and macOS, built by a release workflow.
- Third-party licences, readable under About.
- English and Brazilian Portuguese: choose under File, Options; it starts in the system's language.
- Answering Save when opening a recent document now opens the one clicked, not whichever took its place on the list (#40).

