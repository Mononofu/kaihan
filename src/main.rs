use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

struct RawFile {
    // Path relative to the root of the website.
    path: PathBuf,
    // Markdown contents of the file.
    markdown: String,
    metadata: HashMap<String, String>,
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
        } else {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext == "markdown" || ext == "md" {
                    let contents = std::fs::read_to_string(&path)?;

                    let (metadata, markdown) = contents
                        .split_once("\n\n")
                        .ok_or(anyhow!("Must have metadata!"))?;
                    assert!(metadata.contains("title: "));

                    let metadata = metadata
                        .split("\n")
                        .map(|l| {
                            l.split_once(": ")
                                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                                .ok_or(anyhow!("Metadata must be : delimited"))
                        })
                        .collect::<Result<HashMap<String, String>>>()?;

                    files.push(RawFile {
                        path: prefix.join(path.file_stem().unwrap()),
                        metadata,
                        markdown: markdown.to_owned(),
                    })
                }
            }
        }
    }
    Ok(files)
}

fn render_file(f: &RawFile, output_path: &Path) -> Result<()> {
    if let Some(s) = f.metadata.get("status") {
        if s == "draft" {
            return Ok(());
        }
    }

    let title = f.metadata.get("title").ok_or(anyhow!("Must have title!"))?;

    let style_css = std::fs::read_to_string(
        "/Users/mononofu/Library/CloudStorage/Dropbox/blog/themes/svbhack/templates/style.css",
    )?;

    let parser = pulldown_cmark::Parser::new(&f.markdown);

    // TODO(swj): handle tables, footnotes
    let mut content = String::new();
    pulldown_cmark::html::push_html(&mut content, parser);

    let dst = output_path.join(&f.path).with_extension("html");

    std::fs::create_dir_all(dst.parent().unwrap())?;

    let html_output = format!(
        "<!DOCTYPE html>
<html lang='us'>

<head>
  <meta charset='UTF-8'>
  <style type='text/css'>
  {style_css}
  </style>

  <title>{title}</title>
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
    let blog_path = "/Users/mononofu/Dropbox/blog/content/";

    let render_path = Path::new("/Users/mononofu/tmp/blog/");

    std::fs::remove_dir_all(render_path)?;
    std::fs::create_dir_all(render_path)?;

    let files = read_source_files(Path::new(blog_path), Path::new(""))?;

    for f in files {
        render_file(&f, render_path)?;
    }

    println!("Hello, world!");
    Ok(())
}
