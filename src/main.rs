use anyhow::{anyhow, bail, Result};
use pulldown_cmark::{Event, Tag, TagEnd};
use std::collections::HashMap;
use std::fs;
use std::os::macos::raw::stat;
use std::path::{Path, PathBuf};

#[derive(PartialEq, Debug)]
enum ContentStatus {
    Public,
    Draft,
    Hidden,
}

impl TryFrom<&str> for ContentStatus {
    type Error = anyhow::Error;

    fn try_from(s: &str) -> Result<ContentStatus> {
        match s.to_ascii_lowercase().as_str() {
            "public" => Ok(ContentStatus::Public),
            "draft" => Ok(ContentStatus::Draft),
            "hidden" => Ok(ContentStatus::Hidden),
            _ => bail!("unknown content status {:}", s),
        }
    }
}

struct RawContent {
    // Path relative to the root of the website.
    path: PathBuf,
    // Markdown contents of the file.
    markdown: String,
    metadata: HashMap<String, String>,
    timestamp: chrono::NaiveDateTime,
    status: ContentStatus,
}

struct StaticContent {
    path: PathBuf,
    data: Vec<u8>,
}

enum RawFile {
    Static(StaticContent),
    Content(RawContent),
}

fn read_source_files(current: &Path, prefix: &Path) -> Result<Vec<RawFile>> {
    let mut files = Vec::new();

    for entry in fs::read_dir(current)? {
        let path = entry?.path();
        if path.is_dir() {
            files.extend(read_source_files(
                &path,
                &prefix.join(path.components().last().unwrap()),
            )?);
        } else if path.ends_with(".DS_Store") {
            // Ignore Mac OS settings file.
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            match ext {
                "markdown" | "md" => {
                    let contents = std::fs::read_to_string(&path)?;

                    let (metadata, markdown) = contents
                        .split_once("\n\n")
                        .ok_or(anyhow!("Must have metadata!"))?;
                    assert!(metadata.contains("title: "));

                    let metadata = metadata
                        .split("\n")
                        .map(|l| {
                            l.split_once(": ")
                                .map(|(k, v)| (k.to_ascii_lowercase(), v.to_owned()))
                                .ok_or(anyhow!("Metadata must be : delimited"))
                        })
                        .collect::<Result<HashMap<String, String>>>()?;

                    let mut date = metadata
                        .get("date")
                        .ok_or(anyhow!("Must have date metadata: {:?}", metadata))?
                        .clone();
                    if !date.contains(" ") {
                        date.push_str(" 00:00");
                    }
                    let date = chrono::NaiveDateTime::parse_from_str(&date, "%Y-%m-%d %H:%M")?;

                    let status = metadata
                        .get("status")
                        .unwrap_or(&"public".to_owned())
                        .as_str()
                        .try_into()?;

                    let path = if let Some(p) = metadata.get("save_as") {
                        Path::new(p).to_path_buf()
                    } else {
                        prefix.join(path.file_stem().unwrap())
                    };

                    files.push(RawFile::Content(RawContent {
                        path: path,
                        metadata,
                        markdown: markdown.to_owned(),
                        timestamp: date,
                        status,
                    }))
                }
                "py" => {}
                _ => files.push(RawFile::Static(StaticContent {
                    path: if prefix.to_string_lossy() == "extra" {
                        // Extra files should go directly into the page root.
                        PathBuf::from(path.file_name().unwrap())
                    } else {
                        prefix.join(path.file_name().unwrap())
                    },
                    data: std::fs::read(&path)?,
                })),
            }
        }
    }
    Ok(files)
}

fn render_content(f: &RawContent, output_path: &Path) -> Result<()> {
    if let Some(s) = f.metadata.get("status") {
        if s == "draft" {
            return Ok(());
        }
    }

    let title = f.metadata.get("title").ok_or(anyhow!("Must have title!"))?;

    let style_css = std::fs::read_to_string(
        "/Users/mononofu/Library/CloudStorage/Dropbox/blog/themes/svbhack/templates/style.css",
    )?;
    let pygments_css = std::fs::read_to_string(
        "/Users/mononofu/Library/CloudStorage/Dropbox/blog/themes/svbhack/templates/pygments.css",
    )?;

    let mut options = pulldown_cmark::Options::empty();
    options.insert(pulldown_cmark::Options::ENABLE_TABLES);
    options.insert(pulldown_cmark::Options::ENABLE_MATH);
    options.insert(pulldown_cmark::Options::ENABLE_FOOTNOTES);

    let parser = pulldown_cmark::Parser::new_ext(&f.markdown, options);

    // TODO(swj): Support link archiving.
    let parser = parser.map(|e| match e {
        Event::Start(pulldown_cmark::Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(pulldown_cmark::Tag::Link {
            link_type,
            dest_url: pulldown_cmark::CowStr::Boxed(
                dest_url.trim_start_matches("!").to_owned().into_boxed_str(),
            ),
            title,
            id,
        }),
        _ => e,
    });

    // println!("Path: {:?}", f.path);
    let mut in_footnote = false;
    let mut events = vec![];
    let mut footnote_events = vec![];

    // Move footnotes to the end of the post.
    parser.for_each(|e| {
        if let Event::Start(Tag::FootnoteDefinition(_)) = e {
            in_footnote = true;
        }
        let footnote_done = if let Event::End(TagEnd::FootnoteDefinition) = e {
            true
        } else {
            false
        };
        if in_footnote {
            footnote_events.push(e);
        } else {
            events.push(e);
        }
        if footnote_done {
            in_footnote = false;
        }
    });

    if !footnote_events.is_empty() {
        events.push(Event::Rule);
        events.extend(footnote_events);
    }

    let mut content = String::new();
    pulldown_cmark::html::push_html(&mut content, events.into_iter());

    let output_path = if f.status == ContentStatus::Draft {
        output_path.join("draft")
    } else {
        output_path.to_path_buf()
    };
    let dst = output_path.join(&f.path).with_extension("html");

    std::fs::create_dir_all(dst.parent().unwrap())?;

    // TODO(swj): use real jinja templates
    let html_output = format!(
        "<!DOCTYPE html>
<html lang='us'>

<head>
  <meta charset='UTF-8'>
  <title>{title}</title>
  <style type='text/css'>
  {pygments_css}
  {style_css}
  </style>

  <link rel='preload' href='/css/katex.min.css'  as='style' onload=\"this.onload=null;this.rel='stylesheet'\">
  <script defer src='/js/katex.min.js'></script>
    <script>
    const katexOptions = {{
      delimiters: [
          {{left: '$$', right: '$$', display: true}},
          {{left: '$', right: '$', display: false}},
      ],
      throwOnError : false
    }};
    document.addEventListener('DOMContentLoaded', function() {{
        for (const e of document.getElementsByClassName('math')) katex.render(e.textContent, e);
    }});
    // document.addEventListener('DOMContentLoaded', function() {{
    //     katex.render(document.body, katexOptions);
    // }});
</script>

</head>
<body>

  <main>
  {content}
  </main>
</body>
</html>
"
    );

    std::fs::write(dst, &html_output)?;

    Ok(())
}

fn main() -> Result<()> {
    let blog_path = Path::new("/Users/mononofu/Dropbox/blog/content/");

    let render_path = Path::new("/Users/mononofu/tmp/blog/");

    std::fs::remove_dir_all(render_path)?;
    std::fs::create_dir_all(render_path)?;

    let files = read_source_files(blog_path, Path::new(""))?;

    let mut collections: HashMap<&str, Vec<&RawContent>> = HashMap::new();
    for f in files.iter() {
        if let RawFile::Content(c) = f {
            if c.status != ContentStatus::Public {
                continue;
            }

            if let Some(layout) = c.metadata.get("layout") {
                collections.entry(layout).or_default().push(c);
            }
        }
    }
    collections
        .values_mut()
        .for_each(|v| v.sort_by(|a, b| (&a.path, a.timestamp).cmp(&(&b.path, b.timestamp))));
    for (layout, entries) in collections.iter() {
        println!("{:} {:}s", entries.len(), layout);
    }

    for f in files {
        match f {
            RawFile::Content(c) => render_content(&c, render_path)?,
            RawFile::Static(i) => {
                let dst = render_path.join(i.path);
                std::fs::create_dir_all(dst.parent().unwrap())?;
                std::fs::write(dst, &i.data)?;
            }
        }
    }

    Ok(())
}
