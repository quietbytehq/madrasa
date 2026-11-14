//! Build the Madrasa user manual as PDF, in English and Urdu, from `docs/manual`.
//!
//! ```text
//! madrasa-manual [--root .] [--out dist-manual] [--lang en|ur]... [--version 0.4.2]
//! ```
//!
//! Typesets `<root>/docs/manual/manual.typ` once per language (`sys.inputs.lang`) with the typst crates
//! the API already uses and the fonts the API bundles (Inter, Noto Nastaliq Urdu), so the
//! result does not depend on what is installed on the machine. Writes
//! `Madrasa-User-Manual-EN.pdf` and `Madrasa-User-Manual-UR.pdf`. Run through
//! `scripts/build-manual.sh`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{bail, Context};
use chrono::Datelike;
use typst::diag::{FileError, FileResult, Severity, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime, Dict, IntoValue, Smart};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World, WorldExt};
use typst_layout::PagedDocument;

const FONT_FILES: &[&[u8]] = &[
    include_bytes!("../../api/fonts/Inter-Regular.ttf"),
    include_bytes!("../../api/fonts/Inter-Bold.ttf"),
    include_bytes!("../../api/fonts/NotoNastaliqUrdu-Regular.ttf"),
    include_bytes!("../../api/fonts/NotoNastaliqUrdu-Bold.ttf"),
];

struct Manual {
    root: PathBuf,
    main: FileId,
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    sources: Mutex<HashMap<FileId, Source>>,
    files: Mutex<HashMap<FileId, Bytes>>,
    today: chrono::NaiveDate,
}

fn project_file(path: &str) -> anyhow::Result<FileId> {
    let vpath = VirtualPath::new(path).map_err(|e| anyhow::anyhow!("{path}: {e:?}"))?;
    Ok(FileId::new(RootedPath::new(VirtualRoot::Project, vpath)))
}

impl Manual {
    fn new(root: &Path, inputs: Dict) -> anyhow::Result<Manual> {
        let mut book = FontBook::new();
        let mut fonts = Vec::new();
        for data in FONT_FILES {
            for font in Font::iter(Bytes::new(*data)) {
                book.push(font.info().clone());
                fonts.push(font);
            }
        }
        Ok(Manual {
            root: root.to_path_buf(),
            main: project_file("docs/manual/manual.typ")?,
            library: LazyHash::new(Library::builder().with_inputs(inputs).build()),
            book: LazyHash::new(book),
            fonts,
            sources: Mutex::new(HashMap::new()),
            files: Mutex::new(HashMap::new()),
            today: chrono::Local::now().date_naive(),
        })
    }

    fn path(&self, id: FileId) -> FileResult<PathBuf> {
        if !matches!(id.root(), VirtualRoot::Project) {
            return Err(FileError::Other(Some("packages are not used".into())));
        }
        id.vpath()
            .realize(&self.root)
            .map_err(|_| FileError::AccessDenied)
    }

    fn read(&self, id: FileId) -> FileResult<Vec<u8>> {
        let path = self.path(id)?;
        std::fs::read(&path).map_err(|e| FileError::from_io(e, &path))
    }

    /// "file.typ:12: message", for people.
    fn describe(&self, d: &SourceDiagnostic) -> String {
        let at = d
            .span
            .id()
            .and_then(|id| {
                let source = self.source(id).ok()?;
                let range = self.range(d.span)?;
                let line = source.lines().byte_to_line(range.start)? + 1;
                Some(format!("{}:{line}: ", id.vpath().get_without_slash()))
            })
            .unwrap_or_default();
        let mut s = format!("{at}{}", d.message);
        for hint in &d.hints {
            s.push_str(&format!("\n  hint: {}", hint.v));
        }
        s
    }
}

impl World for Manual {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }
    fn main(&self) -> FileId {
        self.main
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if let Some(s) = self.sources.lock().expect("lock").get(&id) {
            return Ok(s.clone());
        }
        let text = String::from_utf8(self.read(id)?).map_err(|_| FileError::InvalidUtf8)?;
        let source = Source::new(id, text);
        self.sources
            .lock()
            .expect("lock")
            .insert(id, source.clone());
        Ok(source)
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if let Some(b) = self.files.lock().expect("lock").get(&id) {
            return Ok(b.clone());
        }
        let bytes = Bytes::new(self.read(id)?);
        self.files.lock().expect("lock").insert(id, bytes.clone());
        Ok(bytes)
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }
    fn today(&self, _offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        Datetime::from_ymd(
            self.today.year(),
            self.today.month() as u8,
            self.today.day() as u8,
        )
    }
}

/// Typeset the manual in one language; returns the PDF.
fn build(root: &Path, lang: &str, version: &str) -> anyhow::Result<Vec<u8>> {
    let mut inputs = Dict::new();
    inputs.insert("lang".into(), lang.into_value());
    inputs.insert("version".into(), version.into_value());
    let world = Manual::new(root, inputs)?;
    let compiled = typst::compile::<PagedDocument>(&world);
    for w in compiled.warnings.iter() {
        eprintln!("warning: {}", world.describe(w));
    }
    let doc = match compiled.output {
        Ok(doc) => doc,
        Err(errors) => {
            for e in errors.iter().filter(|e| e.severity == Severity::Error) {
                eprintln!("error: {}", world.describe(e));
            }
            bail!("the {lang} manual did not typeset");
        }
    };
    let options = typst_pdf::PdfOptions {
        ident: Smart::Custom(format!("madrasa-user-manual-{lang}")),
        ..Default::default()
    };
    typst_pdf::pdf(&doc, &options).map_err(|errors| {
        for e in errors.iter() {
            eprintln!("error: {}", world.describe(e));
        }
        anyhow::anyhow!("the {lang} manual could not be written as PDF")
    })
}

fn main() -> anyhow::Result<()> {
    // The repository: the manual reads the brand files under assets/ too.
    let mut root = PathBuf::from(".");
    let mut out = PathBuf::from("dist-manual");
    let mut langs: Vec<String> = Vec::new();
    let mut version = env!("CARGO_PKG_VERSION").to_string();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().with_context(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--root" => root = PathBuf::from(value()?),
            "--out" => out = PathBuf::from(value()?),
            "--lang" => langs.push(value()?),
            "--version" => version = value()?,
            "-h" | "--help" => {
                println!("madrasa-manual [--root .] [--out dist-manual] [--lang en|ur]... [--version X]");
                return Ok(());
            }
            other => bail!("unknown argument {other}"),
        }
    }
    if langs.is_empty() {
        langs = vec!["en".into(), "ur".into()];
    }
    std::fs::create_dir_all(&out)?;
    for lang in &langs {
        if !matches!(lang.as_str(), "en" | "ur") {
            bail!("--lang must be en or ur");
        }
        let started = std::time::Instant::now();
        let pdf = build(&root, lang, &version)?;
        let file = out.join(format!("Madrasa-User-Manual-{}.pdf", lang.to_uppercase()));
        std::fs::write(&file, &pdf).with_context(|| format!("writing {}", file.display()))?;
        println!(
            "{} ({} KB, {:.1} s)",
            file.display(),
            pdf.len() / 1024,
            started.elapsed().as_secs_f32()
        );
    }
    Ok(())
}
