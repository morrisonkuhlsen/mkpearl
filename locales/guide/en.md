# Start here
## Your first document
?? first start begin paragraph text
In Typst, you write the text as it is. A blank line starts a new paragraph. The source is on the left, and the finished pages on the right.

The examples in this pane are live: edit the code below and watch the result change at once.
```example
Hello, world!

This is the second paragraph.
```
> Try: change "world" to your name and add a third paragraph.
## Bold, italic and underline
?? bold italic underline highlight emphasis formatting
Use `*asterisks*` for bold and `_underscores_` for italic. For the rest, call a function with `#`, such as `#underline[...]`.
```example
This is *important*, and this is _delicate_.

#underline[Underlined] and #strike[struck out].

#highlight[Highlighter] works too.
```
> Try: make a word bold and italic at once.
## Headings
?? heading title section subsection numbering
Start the line with `=`. Each further `=` goes down a level.
```example
= Chapter
== Section
=== Subsection
The subsection's text.
```
To number the headings, use a rule at the start of the document:
```example
#set heading(numbering: "1.1")
= Introduction
== Goals
= Methods
```
> Try: change the numbering to "I.a".
## Lists
?? list bullets numbered items
Use `-` for bullets and `+` for numbered lists. Indent with spaces for sub-items.
```example
- First
- Second
  - A detail

+ Step one
+ Step two
```
> Try: make a shopping list with a sub-item.
# Layout
## Page and margins
?? page margin paper a4 a5 size landscape page numbers
The `#set page` rule sets the paper, margins and numbering, from where it appears onwards. In Pearl, you can also drag the margins on the ruler.
```text
#set page(paper: "a4", margin: 2.5cm, numbering: "1")
```
```example
#set page(width: 8cm, height: 4cm, margin: 1cm, fill: luma(240))
A small page, just to see the effect.
```
> Try: raise the margin to 2cm and watch the text adjust.
## Font and size
?? font size letter colour language text
The `#set text` rule changes how text looks from there on. Declare the language too: it sets the hyphenation and words such as "Figure".
```example
#set text(size: 14pt, fill: blue, lang: "en")
Blue and larger text.

#text(size: 9pt, fill: gray)[Only this part is smaller.]
```
> Try: change the colour to `red` and the size to 18pt.
## Alignment and spacing
?? align centre justify space leading paragraph indent
`#align` places a block, and `#set par` controls paragraphs. `#v` and `#h` make space.
```example
#align(center)[Centred]
#v(8pt)
#align(right)[On the right]

#set par(justify: true, first-line-indent: 1em)
This paragraph is justified, with its first line indented, as in many academic papers.
```
> Try: change `center` to `left`.
## Columns and grids
?? columns grid side by side
`#columns` splits text into columns. `#grid` puts things side by side with no borders.
```example
#grid(
  columns: (1fr, 1fr),
  gutter: 8pt,
  rect(width: 100%)[Left],
  rect(width: 100%)[Right],
)
```
> Try: add a third column.
# Content
## Images and figures
?? image figure photo caption picture paste
In Pearl, the easiest way is to paste the image with Ctrl+V or use the button in the Insert tab: the file goes to the document's folder, and the code goes in by itself.
```text
#figure(
  image("images/photo.png", width: 60%),
  caption: [My caption],
)
```
`#figure` works with any content, and numbers itself:
```example
#figure(
  rect(width: 3cm, height: 1.5cm, fill: aqua),
  caption: [A rectangle],
)
```
> Try: change the rectangle's width and the caption.
## Tables
?? table rows columns cells header
Say how many columns the table has and list the cells in order.
```example
#table(
  columns: 2,
  [*Name*], [*Grade*],
  [Ana], [9.5],
  [Beto], [8.0],
)
```
> Try: add an "Absences" column.
## Links and footnotes
?? link address url note footnote
Addresses become links by themselves. `#link` gives them other words, and `#footnote` makes a note at the foot of the page.
```example
See https://typst.app or the #link("https://typst.app")[Typst site].

A claim.#footnote[The explanation goes here.]
```
> Try: add a second note.
# Math
## Formulas
?? math formula equation dollar inline block
Put the formula between `$`. With no spaces at the ends it stays in the line; with spaces it becomes a centred block. The Formulas pane, in the strip on the right, has over 90 ready-made formulas of algebra, calculus, statistics and other fields, and the Ready-made formulas button is in the Math tab.
```example
The area is $A = pi r^2$ in this case.

$ E = m c^2 $
```
> Try: write $a^2 + b^2 = c^2$ as a block.
## Fractions, roots and symbols
?? fraction root sum integral greek symbols subscript superscript
A slash makes a fraction. Symbols have full names, such as `alpha` and `sum`. See them all in the Formulas pane, under Symbols.
```example
$ x = (-b plus.minus sqrt(b^2 - 4 a c)) / (2 a) $

$ sum_(i=1)^n i = (n(n+1)) / 2 $

$ alpha + beta >= gamma $
```
> Try: write the integral of x from 0 to 1 with `integral_0^1 x dif x`.
## Matrices and numbered equations
?? matrix numbered equation aligned cases system
`mat` and `cases` build structures. `&` aligns lines, and a rule on `math.equation` numbers the equations.
```example
#set math.equation(numbering: "(1)")
$ A = mat(1, 2; 3, 4) $

$ f(x) &= x^2 + 2x \
       &= x(x + 2) $
```
> Try: make the matrix 3 by 3.
# References
## Labels and cross-references
?? label cross reference ref at figure section
Give something a name with `<name>` and point at it with `@name`. The numbers update themselves.
```example
#set heading(numbering: "1.")
= Methods <methods>
As described in @methods and in @fig-a.

#figure(rect(width: 1cm, height: 1cm), caption: [A square]) <fig-a>
```
> Try: make another heading with a label and cite it.
## Bibliography and citations
?? bibliography citation cite bib doi abnt apa references
Open the References pane: import a `.bib` or paste a DOI, and click to cite. Pearl adds the bibliography when it's missing. In code, it looks like this:
```text
As @lecun2015 showed.

#bibliography("refs.bib", style: "apa")
```
The pane's **Check online** button looks up each reference in public services.
## Contents and lists
?? contents index outline list of figures tables
`#outline` builds the contents from the headings, and can list figures and tables too.
```example
#outline()

= Introduction
= Conclusion
```
```text
#outline(title: [Figures], target: figure.where(kind: image))
```
> Try: change the title with `title: [Index]`.
# Programming
## Variables
?? variable let value expression calculation
`#let` keeps a value, and `#` uses it in the text. Typst does sums too.
```example
#let name = "Pearl"
#let total = 3 * 7

#name worked out #total.
```
> Try: make a variable `year` and show `year + 1`.
## Functions
?? function def parameter argument reuse
`#let` makes functions too, useful for repeating a format.
```example
#let warning(body) = rect(fill: yellow, inset: 6pt)[*Warning:* #body]

#warning[Save before leaving.]
#warning[Check the references.]
```
> Try: change the warning's colour.
## Set and show rules
?? set show rule style template transform
`#set` sets default values, and `#show` transforms what appears. That keeps the style in one place.
```example
#show heading: set text(fill: maroon)
#show "Pearl": name => strong(name)

= A coloured heading
Pearl is bold here.
```
> Try: change the headings' colour.
## Conditions and loops
?? if else for while loop condition repeat
`if` chooses and `for` repeats. Inside `#[...]`, you're back to writing text.
```example
#let grade = 8

#if grade >= 7 [Passed] else [Failed]

#for i in range(1, 4) [
  - Item #i
]
```
> Try: change the grade to 5.
# Pearl
## Typing on the pages
?? pearl type page edit click caret wysiwyg
Click any text on a page: the caret appears there, and what you type goes into the source. This also works with only the pages on show. A click in a blank goes to the nearest text. The source remains the only thing that changes, so undo, sessions and history stay the same.
## Ruler and margins
?? pearl ruler margins drag
Drag the edge between the grey and the light part of the ruler to change a margin. On release, Pearl updates the `#set page` rule in the source, and one Ctrl+Z undoes it all.
## Sessions and Claude
?? pearl session collaborate real time claude ai link
Start a session from the share icon, at the top of the window, and send the link: whoever joins edits the same document, encrypted. The Claude pane talks about the document and can join the session as one more person, with your API key.
## Versions and recovery
?? pearl history version recover autosave save
The Versions pane keeps the document as you save: see what changed, look at old pages and restore. If the program closes suddenly, the unsaved text is offered when it opens again.
## Shortcuts
?? pearl shortcuts keyboard f1 help
F1 opens this documentation and searches for the word at the caret. Ctrl+F searches the document, Ctrl+H finds and replaces, Ctrl+S saves, Ctrl+E exports the PDF, Ctrl+Space asks for suggestions, and Ctrl+B and Ctrl+I apply bold and italic.
## Figures on the pages
?? pearl figure image resize move drag delete page
Click an image on a page to select it. Drag the little square at its corner to change its size: Pearl writes the `width:` in the source. Drag the image itself to move it to another paragraph. The Delete key takes it out of the text without deleting the file, and Esc clears the selection. One Ctrl+Z undoes each action.
## Letterhead and watermark
?? letterhead watermark draft logo header footer paper
In the Page Layout tab, the Letterhead and watermark button opens a window with a live preview: name, address, logo and footer on one side, and a background text or image on the other, the image with its opacity. On applying, Pearl writes a `#set page` rule at the start of the document, and opens on the same values next time. Without the button, it looks like this:
```example
#set page(width: 10cm, height: 6cm, background: rotate(-45deg, text(30pt, fill: luma(225), "DRAFT")))
Some text over the watermark.
```
> Try: change "DRAFT" to "COPY" and change the angle.
## Layout variants
?? layout variants font margins spacing pages fit compact
In the Page Layout tab, the Layout variants button shows the document set in a few other ways, on its own pages, each with its page count. Nothing changes until you choose one: Pearl then writes its rules in the source, as one Ctrl+Z. A variant that looks the same as now says so, which means something in the document overrules its rules.
## Fit in pages
?? fit pages limit shrink margins spacing size call journal
In the Page Layout tab, Fit in pages looks for the smallest changes that bring the document to the number of pages you ask for: first the spacing, then the margins, and the type size last. It shows each change and the first and last pages, before and after, and nothing is written until you apply it, as one Ctrl+Z. The words stay as they are. When the target is out of reach within its limits, it says how close it can get.
## Typography watch
?? typography widow orphan heading margin table figure problems
The Problems pane also points out what reads badly on the finished pages: a paragraph's first or last line alone on a page, a heading alone at the end of a page, tables and text past the margin, and figures two or more pages from their first mention. With the pane open, each is marked on the page, and a click goes to its passage. Ignore keeps one quiet until that passage changes. It reads only this file's running text, and pages in columns aren't checked for widows and orphans.
## Accessibility
?? accessibility alt text screen reader contrast heading pdf/ua export
The Problems pane also points out what keeps a document from reaching everyone: images with no alternative text, skipped heading levels, and text too faint for its background. Exporting warns about them first, and the warning can be turned off, but it never stops an export. Export accessible PDF asks for PDF/UA-1: if the document doesn't meet it, nothing is written, and Pearl says what's missing.
```example
#image(bytes("<svg xmlns='http://www.w3.org/2000/svg' width='40' height='20'><rect width='40' height='20' fill='teal'/></svg>"), alt: "A teal rectangle", width: 2cm)
```
> Try: take out the `alt:`, in a document of your own, and open the Problems pane.
## Timeline of the writing
?? timeline history version slider passage paragraph deleted recover bring back
In the Versions pane, Timeline of the writing opens a slider over the document's versions, with the pages of each, and the passages it has that the document no longer does. Moving the slider changes nothing. Bring back puts one passage where it was, as one Ctrl+Z, and leaves the rest as it is. A version that can't be read is skipped, with a notice.
## Comparing versions on the pages
?? pearl compare version difference review before after history
In the Versions pane, choose a version and click Compare on the pages. The pages from before and now go side by side: green for what came in, red for what went out and blue for what changed place. The Previous and Next buttons go through the changes, and nothing in the document is altered.
