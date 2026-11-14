// The Madrasa user manual. Built by scripts/build-manual.sh (crates/manual) once per language:
// `sys.inputs.lang` is "en" or "ur". Everything the manual says is in manual.yaml, in both
// languages side by side, so the two books always have the same chapters, steps and
// screenshots; the screenshots are made by docs/manual/tools/shoot.mjs from the demo school.

#let lang = sys.inputs.at("lang", default: "en")
#let version = sys.inputs.at("version", default: "dev")
#let ur = lang == "ur"
#let data = yaml("manual.yaml")

// ---- the brand (assets/brand, ADR 0008) ---------------------------------------------------
#let navy = rgb("#0B2557")
#let blue = rgb("#0757BA")
#let gold = rgb("#F5B13B")
#let ink = rgb("#15264A")
#let muted = rgb("#64748B")
#let line-color = rgb("#DEE6F2")
#let tint = rgb("#F2F6FC")
#let gold-tint = rgb("#FFF4DC")

// Text in the book's language (a string, or a dictionary with "en" and "ur").
#let tr(v) = if type(v) == dictionary { v.at(lang, default: v.at("en", default: "")) } else { v }
// Text with a little markup (*bold*, _italic_, `code`), from the YAML.
#let md(v) = eval(tr(v), mode: "markup")
#let words = data.words
#let w(key) = tr(words.at(key))
#let start-side = if ur { "right" } else { "left" }

#set document(
  title: w("title") + " — Madrasa " + version,
  author: "Madrasa",
)

#set text(
  font: if ur { ("Noto Nastaliq Urdu", "Inter") } else { ("Inter", "Noto Nastaliq Urdu") },
  size: if ur { 10.5pt } else { 10pt },
  lang: lang,
  dir: if ur { rtl } else { ltr },
  fill: ink,
)
#set par(leading: if ur { 1.15em } else { 0.7em }, spacing: if ur { 1.4em } else { 1em })
#show raw: set text(font: "Inter", size: 0.92em, fill: navy)
#show raw: it => box(fill: tint, inset: (x: 3pt, y: 1.5pt), outset: (y: 1.5pt), radius: 2pt, it)
#show strong: set text(fill: navy)

// ---- cover ---------------------------------------------------------------------------------
#page(margin: (x: 2.2cm, y: 2.4cm), header: none, footer: none)[
  #place(top + left, dx: -2.2cm, dy: -2.4cm, rect(width: 21cm, height: 0.55cm, fill: navy))
  #place(top + left, dx: -2.2cm, dy: -1.85cm, rect(width: 21cm, height: 0.12cm, fill: gold))
  #v(3.2cm)
  #align(center, image("/assets/brand/madrasa-logo-vertical.svg", width: 6.2cm))
  #v(1.2cm)
  #align(center, text(size: 30pt, weight: "bold", fill: navy, w("title")))
  #v(0.3cm)
  #align(center, text(size: 13pt, fill: muted, w("subtitle")))
  #v(1fr)
  #align(center)[
    #text(size: 10pt, fill: muted)[#w("version") #version · #datetime.today().display("[day]-[month]-[year]")]
  ]
  #v(0.4cm)
  #align(center, box(width: 70%, text(size: 8.5pt, fill: muted, md(words.cover-note))))
]

// ---- pages ---------------------------------------------------------------------------------
#set page(
  paper: "a4",
  margin: (x: 1.9cm, top: 2.3cm, bottom: 2cm),
  header: context {
    let here = here().page()
    let chapters = query(heading.where(level: 1)).filter(h => h.location().page() <= here)
    set text(size: 8pt, fill: muted)
    grid(
      columns: (auto, 1fr, auto),
      column-gutter: 6pt,
      align: horizon,
      box(height: 0.5cm, image("/assets/brand/madrasa-mark.svg")),
      text(fill: navy, weight: "bold", w("title")),
      if chapters.len() > 0 { chapters.last().body },
    )
    v(-4pt)
    line(length: 100%, stroke: 0.4pt + line-color)
  },
  footer: context {
    set text(size: 8pt, fill: muted)
    grid(
      columns: (1fr, auto),
      [Madrasa #version · #w("footer")],
      counter(page).display(),
    )
  },
)

// Headings: chapters start a page, with their number in gold.
#set heading(numbering: (..n) => if n.pos().len() <= 2 { numbering("1.1", ..n) })
#show heading.where(level: 1): it => {
  pagebreak(weak: true)
  v(0.6cm)
  block(width: 100%, below: 0.5cm)[
    #text(size: 30pt, weight: "bold", fill: gold, counter(heading).display("1"))
    #h(0.4cm)
    #text(size: 21pt, weight: "bold", fill: navy, it.body)
  ]
  line(length: 100%, stroke: 1.2pt + gold)
  v(0.2cm)
}
// Nastaliq is tall: Urdu headings get more room above and below.
#show heading.where(level: 2): it => block(above: if ur { 2em } else { 1.3em }, below: 0.7em, breakable: false)[
  #text(size: 13.5pt, weight: "bold", fill: navy)[#counter(heading).display() #h(0.25em) #it.body]
  #v(if ur { -0.1em } else { -0.45em })
  #line(length: 100%, stroke: 0.6pt + blue.lighten(60%))
]
#show heading.where(level: 3): set text(size: 11pt, fill: blue)

// A part: a divider page in the brand's night blue.
#let part-page(n, title, intro) = page(fill: navy, header: none, footer: none)[
  #v(1fr)
  #text(size: 12pt, fill: gold, weight: "bold")[#w("part") #n]
  #v(0.2cm)
  #text(size: 28pt, fill: white, weight: "bold", title)
  #v(0.4cm)
  #line(length: 4cm, stroke: 2pt + gold)
  #v(0.4cm)
  #box(width: 80%, text(size: 11pt, fill: white.darken(8%), intro))
  #v(2fr)
]

// Which editions a chapter is about.
#let chip-colors = (
  lite: (blue, white),
  standard: (navy, white),
  lan: (rgb("#5B4BB7"), white),
  nexus: (gold, navy),
  parent: (rgb("#19835A"), white),
  all: (tint, navy),
)
#let chip(key) = {
  let (bg, fg) = chip-colors.at(key)
  box(fill: bg, inset: (x: 6pt, y: 3pt), radius: 8pt, text(size: 8pt, weight: "bold", fill: fg, w("ed-" + key)))
}

// A tip, a warning: a tinted box with a gold bar on the reading side.
#let note(kind, body) = block(
  width: 100%,
  fill: if kind == "warning" { rgb("#FDE8E8") } else { gold-tint },
  stroke: ((start-side): 3pt + (if kind == "warning" { rgb("#C62828") } else { gold })),
  inset: (x: 10pt, y: 8pt),
  radius: 2pt,
  breakable: false,
)[
  #text(weight: "bold", fill: navy, w(kind)) #h(0.3em) #body
]

// A screenshot, framed. Desktop screenshots are taken in a window 1280 pixels wide, which fills
// the line; a cut-out (a card, a dialog) keeps the same scale, so text is the same size in every
// picture. `width` overrides that (the phone screens). The builder serves `<name>.jpg` from
// `<name>.webp` (crates/manual).
#let shot(id, width: auto) = context {
  let file = "screenshots/" + lang + "/" + id + ".jpg"
  let pixels = measure(image(file)).width / 1pt
  let w = if width == auto { calc.min(pixels / 1280, 1) * 100% } else { width }
  block(
    width: w,
    stroke: 0.6pt + line-color,
    radius: 3pt,
    clip: true,
    image(file, width: 100%),
  )
}

// Every step shows its screen, except one that happens outside Madrasa (`outside: true`).
#let step(n, s) = {
  let outside = s.at("outside", default: false)
  assert(
    "shot" in s or outside,
    message: "a step without a screenshot (add `shot:`, or `outside: true`): " + tr(s.text),
  )
  block(breakable: false, above: 0.9em, below: 0.5em)[
    #grid(
      columns: (auto, 1fr),
      column-gutter: 8pt,
      align: (center + top, start + top),
      box(width: 1.35em, height: 1.35em, fill: blue, radius: 50%,
        align(center + horizon, text(size: 8.5pt, weight: "bold", fill: white, font: "Inter", str(n)))),
      {
        if outside {
          box(fill: tint, stroke: 0.5pt + line-color, inset: (x: 5pt, y: 2pt), radius: 6pt,
            text(size: 7.5pt, weight: "bold", fill: muted, w("outside")))
          h(5pt)
        }
        md(s.text)
      },
    )
    #if "shot" in s {
      v(0.3em)
      // `width` in the YAML is a percentage of the line, such as "46%" for a phone screen.
      let width = s.at("width", default: auto)
      if width != auto { width = float(width.trim("%")) * 1% }
      pad(x: 0.4cm, shot(s.shot, width: width))
    }
  ]
  if "tip" in s { note("tip", md(s.tip)) }
  if "warning" in s { note("warning", md(s.warning)) }
}

// ---- contents ------------------------------------------------------------------------------
#page(header: none)[
  #v(0.5cm)
  #text(size: 21pt, weight: "bold", fill: navy, w("contents"))
  #v(0.2cm)
  #line(length: 100%, stroke: 1.2pt + gold)
  #v(0.3cm)
  #show outline.entry.where(level: 1): it => { v(0.5em); strong(it) }
  #outline(title: none, depth: 2, indent: 1.2em)
]

// ---- the book ------------------------------------------------------------------------------
#for (pi, part) in data.parts.enumerate() {
  part-page(pi + 1, tr(part.title), md(part.intro))
  for chapter in part.chapters {
    [= #tr(chapter.title) #label(chapter.id)]
    if "editions" in chapter {
      block(below: 0.8em, chapter.editions.map(chip).join(h(4pt)))
    }
    if "intro" in chapter { md(chapter.intro) }
    for task in chapter.at("tasks", default: ()) {
      [== #tr(task.title)]
      if "intro" in task { md(task.intro) }
      for (i, s) in task.at("steps", default: ()).enumerate() { step(i + 1, s) }
      if "after" in task { md(task.after) }
    }
  }
}
