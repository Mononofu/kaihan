use crate::{Article, Config};
use anyhow::Result;
use std::path::Path;

pub fn feeds(cfg: &Config, articles: &Vec<Article>, output_path: &Path) -> Result<()> {
    rss(cfg, articles, output_path)?;
    atom(cfg, articles, output_path)?;

    Ok(())
}

fn rss(cfg: &Config, articles: &Vec<Article>, output_path: &Path) -> Result<()> {
    let items: Vec<_> = articles
        .iter()
        .take(cfg.max_feed_entries)
        .map(|a| {
            let url = std::path::PathBuf::from(&cfg.siteurl)
                .join(&a.url)
                .to_str()
                .unwrap()
                .to_owned();
            rss::ItemBuilder::default()
                .title(a.title.clone())
                .link(url.clone())
                .author(cfg.author.clone())
                .description(a.summary.clone())
                .pub_date(a.timestamp.and_utc().to_rfc2822())
                .guid(rss::GuidBuilder::default().value(url).build())
                .build()
        })
        .collect();

    let channel = rss::ChannelBuilder::default()
        .title(cfg.sitename.clone())
        .link(cfg.siteurl.clone())
        .last_build_date(chrono::Utc::now().to_rfc2822())
        .items(items)
        .build();

    std::fs::create_dir_all(output_path.join(&cfg.feed_all_rss).parent().unwrap())?;
    std::fs::write(output_path.join(&cfg.feed_all_rss), channel.to_string())?;

    Ok(())
}

fn atom(cfg: &Config, articles: &Vec<Article>, output_path: &Path) -> Result<()> {
    let author = atom_syndication::PersonBuilder::default()
        .name(cfg.author.clone())
        .build();

    let entries: Vec<_> = articles
        .iter()
        .take(cfg.max_feed_entries)
        .map(|a| {
            let url = std::path::PathBuf::from(&cfg.siteurl)
                .join(&a.url)
                .to_str()
                .unwrap()
                .to_owned();
            atom_syndication::EntryBuilder::default()
                .title(a.title.clone())
                .id(url.clone())
                .updated(a.timestamp.and_utc())
                .author(author.clone())
                .link(atom_syndication::LinkBuilder::default().href(url).build())
                .summary(
                    atom_syndication::TextBuilder::default()
                        .value(a.summary.clone())
                        .r#type(atom_syndication::TextType::Html)
                        .build(),
                )
                .build()
        })
        .collect();

    let feed = atom_syndication::FeedBuilder::default()
        .title(cfg.sitename.clone())
        .id(cfg.siteurl.clone())
        .updated(chrono::Utc::now())
        .author(author)
        .link(
            atom_syndication::LinkBuilder::default()
                .href(cfg.siteurl.clone())
                .build(),
        )
        .entries(entries)
        .build();

    std::fs::create_dir_all(output_path.join(&cfg.feed_all_rss).parent().unwrap())?;
    std::fs::write(output_path.join(&cfg.feed_all_atom), feed.to_string())?;

    Ok(())
}
