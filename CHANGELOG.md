# Changelog

Every release of Pearl, newest first, in the spirit of
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow
[semantic versioning](https://semver.org/). Nothing is released yet.

## [Unreleased]

- An editor for Typst: source on the left, live pages on the right, a ribbon, the File view, a left
  rail and a right strip, zoom in the status bar, four themes and a splash screen.
- Real-time sessions, directly between host and guests, encrypted with the key in the link.
- Autocompletion, closing brackets, help on hover and a search with its matches on show.
- Claude as a person in the session, with your own API key.
- Version history for saved documents: see what changed, look at past pages and restore.
- Rulers in centimetres around the pages, with the margins shaded.
- Recovery: text left unsaved by a crash is offered back when Pearl opens again.
- Spell checking in Brazilian Portuguese: misspelt words underlined, and a right click for suggestions,
  ignoring a word or adding it to your dictionary.
- Images by pasting or dragging, or from any folder through the ribbon: copied into `images/` beside
  the document, with a figure at the caret. An unsaved document asks where to save first. Dragging doesn't work under Wayland yet (#15).
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
  background text or image. It's written as a rule in the source, and the window opens on it again.
- Typing on the pages: a click puts a caret there, also with only the pages on show, and what's typed
  goes into the source.
- Figures on the pages: a click selects an image, its corner resizes it, dragging it moves its block to
  another paragraph, and Delete takes it out of the text. Each is one undo step.
- Margins dragged on the rulers change the `#set page` rule, as one undo step.
- Copying and pasting with other programs, which didn't reach the system's clipboard.
- Packages for Linux, Windows and macOS, built by a release workflow.
- Third-party licences, readable under About.
- English and Brazilian Portuguese: choose under File, Options; it starts in the system's language.
