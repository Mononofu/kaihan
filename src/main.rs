use anyhow::{anyhow, bail, Result};
use clap::Parser;
use log::{error, info};
use pulldown_cmark::Event;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

mod markdown;
mod render;
mod stats;

#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None)]
struct Flags {
    #[arg(long)]
    input: String,

    #[arg(long)]
    output: String,

    /// The site url can be overriden for local development.
    #[arg(long)]
    siteurl: Option<String>,
}

#[derive(Deserialize, Debug)]
struct Config {
    author: String,
    sitename: String,
    siteurl: String,
    user_logo_url: String,
    content_path: String,
    templates_path: String,
    feed_all_atom: String,
    feed_all_rss: String,
    max_feed_entries: usize,
    github_user: String,
    github_access_token: String,
}

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
    tags: Vec<String>,
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
                        let slug = metadata
                            .get("title")
                            .unwrap()
                            .chars()
                            .filter(|c| c.is_alphanumeric() || c.is_ascii_whitespace())
                            .collect::<String>()
                            .to_ascii_lowercase()
                            .replace(' ', "-");
                        if metadata.get("layout").unwrap_or(&String::new()) == "page" {
                            PathBuf::new().join(slug)
                        } else {
                            PathBuf::new()
                                .join("blog")
                                .join(date.format("%Y/%m/%d").to_string())
                                .join(slug)
                        }
                        // prefix.join(path.file_stem().unwrap())
                    };
                    let tags = if let Some(tag_str) = metadata.get("tags") {
                        tag_str
                            .split(",")
                            .map(|t| t.trim().to_owned())
                            .filter(|t| !t.is_empty())
                            .collect()
                    } else {
                        vec![]
                    };

                    files.push(RawFile::Content(RawContent {
                        path: path,
                        metadata,
                        markdown: markdown.to_owned(),
                        timestamp: date,
                        status,
                        tags,
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

fn to_html(markdown: &str) -> Result<String> {
    let events = markdown::to_events(markdown)?;
    let mut content = String::new();
    pulldown_cmark::html::push_html(&mut content, events.into_iter());
    Ok(content)
}

impl RawContent {
    fn validate_links(&self, output_path: &Path) -> Result<()> {
        let events = markdown::to_events(&self.markdown)?;
        for e in events {
            if let Event::Start(pulldown_cmark::Tag::Link { dest_url: url, .. }) = e {
                // Verify that internal links are valid.
                if url.starts_with("/") {
                    let url = url.trim_matches('/');
                    // Strip # anchor links.
                    let url = url.split_once('#').map(|(a, _)| a).unwrap_or(url);

                    let target_file = output_path.join(&url);
                    if !target_file.exists() {
                        error!(
                            "Dangling internal URL in {:?}: {:}, expected {:?}",
                            self.path, url, target_file
                        );
                    }
                } else if !url.starts_with("http") && !url.starts_with("mailto") {
                    error!("Interal URLs should be absolute {:?}, external URLs should start with https://, got: {:}", self.path, url);
                }
            }
        }

        Ok(())
    }

    fn to_article(&self) -> Result<Article> {
        let title = self
            .metadata
            .get("title")
            .ok_or(anyhow!("Must have title!"))?;

        let mut summary_markdown = self
            .markdown
            .split_inclusive([' ', '\n'])
            .take(100)
            .collect::<Vec<_>>()
            .join("");
        summary_markdown.push_str("...");

        Ok(Article {
            title: title.clone(),
            url: self.path.to_str().unwrap().to_owned(),
            summary: to_html(&summary_markdown)?,
            content: to_html(&self.markdown)?,
            tags: self.tags.clone(),
            timestamp: self.timestamp,
            locale_date: self.timestamp.format("%a %d %B %Y").to_string(),
        })
    }
}

fn render_content(
    f: &RawContent,
    output_path: &Path,
    jinja: &minijinja::Environment,
    base_context: &minijinja::Value,
) -> Result<()> {
    let output_path = if f.status == ContentStatus::Draft {
        output_path.join("draft")
    } else {
        output_path.to_path_buf()
    };
    let dst = output_path.join(&f.path).join("index.html");

    std::fs::create_dir_all(dst.parent().unwrap())?;

    let tmpl = jinja.get_template(&format!(
        "{:}.html",
        f.metadata
            .get("layout")
            .map(|p| p.as_str())
            .unwrap_or("page")
    ))?;
    let html_output = tmpl.render(minijinja::context! {
    article => f.to_article()?,
    ..base_context.clone()})?;

    std::fs::write(dst, &html_output)?;

    Ok(())
}

fn read_templates(template_path: &Path) -> Result<minijinja::Environment> {
    let mut env = minijinja::Environment::new();
    env.set_auto_escape_callback(|_| minijinja::AutoEscape::None);

    for path in glob::glob(template_path.join("**/*").to_str().unwrap())? {
        let path = path?;
        if path.is_file() {
            let name = path.strip_prefix(template_path)?.to_str().unwrap();
            info!("loading template {path:?} as {name:?}");
            env.add_template_owned(name.to_owned(), std::fs::read_to_string(&path)?)?;
        }
    }

    Ok(env)
}

#[derive(Serialize, Debug)]
struct ArticlesPage {
    object_list: Vec<Article>,
}

#[derive(Serialize, Debug, Clone)]
struct Article {
    title: String,
    url: String,
    content: String,
    summary: String,
    tags: Vec<String>,
    timestamp: chrono::NaiveDateTime,
    locale_date: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Include log level, current time and file:line in each log message.
    env_logger::Builder::from_default_env()
        .format(|buf, record| {
            let style = buf.default_level_style(record.level());

            writeln!(
                buf,
                "[{}{}{:#}] {} {}:{} {}",
                style,
                record.level(),
                style,
                chrono::Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.args(),
            )
        })
        .init();

    let flags = Flags::try_parse()?;

    let blog_path = Path::new(&flags.input);

    let mut config: Config =
        toml::from_str(&std::fs::read_to_string(blog_path.join("config.toml"))?)?;
    if let Some(u) = flags.siteurl {
        config.siteurl = u;
    }

    let content_path = blog_path.join(&config.content_path);
    let templates_path = blog_path.join(&config.templates_path);

    let render_path = Path::new(&flags.output);

    let _ = std::fs::remove_dir_all(render_path);
    std::fs::create_dir_all(render_path)?;

    let files = read_source_files(&content_path, Path::new(""))?;
    let jinja = read_templates(&templates_path)?;

    let mut by_layout: HashMap<&str, Vec<&RawContent>> = HashMap::new();
    let mut by_tag: HashMap<&str, Vec<&RawContent>> = HashMap::new();
    for f in files.iter() {
        if let RawFile::Content(c) = f {
            if c.status != ContentStatus::Public {
                continue;
            }

            if let Some(layout) = c.metadata.get("layout") {
                by_layout.entry(layout).or_default().push(c);
            }
            c.tags
                .iter()
                .for_each(|t| by_tag.entry(t).or_default().push(c));
        }
    }
    by_layout
        .values_mut()
        .for_each(|v| v.sort_by(|a, b| (&a.path, a.timestamp).cmp(&(&b.path, b.timestamp))));
    for (layout, entries) in by_layout.iter() {
        println!("{:} {:}s", entries.len(), layout);
    }

    let recent_articles = by_layout
        .get("post")
        .unwrap()
        .iter()
        .rev()
        .map(|p| p.to_article())
        .collect::<Result<Vec<Article>>>()?;

    let mut pages = by_layout
        .get("page")
        .unwrap()
        .iter()
        .map(|p| p.to_article())
        .collect::<Result<Vec<Article>>>()?;
    pages.sort_by(|a, b| a.title.cmp(&b.title));

    let base_context = minijinja::context! {
        AUTHOR => config.author,
        SITENAME => config.author,
        SITEURL => config.siteurl,
        USER_LOGO_URL =>  config.user_logo_url,
        MENUITEMS => vec![("blog", "/")],
        DISPLAY_PAGES_ON_MENU => true,
        FEED_ALL_RSS => config.feed_all_rss,
        FEED_ALL_ATOM => config.feed_all_atom,
        pages => pages,
    };

    let tmpl = jinja.get_template("index.html")?;
    let index = tmpl.render(minijinja::context! {
    articles_page =>  ArticlesPage{object_list: recent_articles.iter().take(10).cloned().collect()},
    ..base_context.clone()})?;
    std::fs::write(render_path.join("index.html"), index)?;

    let tmpl = jinja.get_template("archives.html")?;
    let archives = tmpl.render(minijinja::context! {
    dates => recent_articles,
    ..base_context.clone()})?;
    std::fs::write(render_path.join("archives.html"), archives)?;

    let max_step = 5f32;
    let max_count = by_tag.values().map(|ps| ps.len()).max().unwrap_or(1) as f32;
    let tag_counts = by_tag
        .iter()
        .map(|(t, posts)| {
            (
                t,
                ((max_step - 1f32) * (1f32 - (posts.len() as f32).ln() / max_count.ln().max(1f32)))
                    .floor() as i32
                    + 1,
            )
        })
        .collect::<Vec<_>>();

    let tmpl = jinja.get_template("tags.html")?;
    let tags = tmpl.render(minijinja::context! {
    tag_cloud => tag_counts,
    ..base_context.clone()})?;
    std::fs::write(render_path.join("tags.html"), tags)?;

    for (tag, mut posts) in by_tag {
        posts.sort_by(|a, b| (&a.path, a.timestamp).cmp(&(&b.path, b.timestamp)));
        let articles = posts
            .iter()
            .rev()
            .take(10)
            .map(|p| p.to_article())
            .collect::<Result<_>>()?;
        let tmpl = jinja.get_template("tag.html")?;
        let tags = tmpl.render(minijinja::context! {
        tag => tag,
        articles_page => ArticlesPage{object_list: articles},
        ..base_context.clone()})?;
        let dst = render_path.join("tags").join(format!("{tag:}/index.html"));
        std::fs::create_dir_all(dst.parent().unwrap())?;
        std::fs::write(dst, tags)?;
    }

    render::feeds(&config, &recent_articles, &render_path)?;

    // Run once to render and save.
    for f in files.iter() {
        match f {
            RawFile::Content(c) => render_content(&c, render_path, &jinja, &base_context)?,
            RawFile::Static(i) => {
                let dst = render_path.join(&i.path);
                std::fs::create_dir_all(dst.parent().unwrap())?;
                std::fs::write(dst, &i.data)?;
            }
        }
    }

    // TODO(swj): How to best ignore dependencies checked into repos?
    // std::fs::write(
    //     render_path.join("js/language_usage.js"),
    //     stats::github_languages(&config).await?,
    // )?;

    // Run again to verify internal links.
    for f in files.iter() {
        if let RawFile::Content(c) = f {
            c.validate_links(render_path)?;
        }
    }

    Ok(())
}
