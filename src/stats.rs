use std::collections::HashMap;

use anyhow::Result;
use futures::StreamExt;

use crate::Config;

pub async fn github_languages(cfg: &Config) -> Result<String> {
    let bytes_per_language = github_bytes_per_lang(cfg).await?;
    let total: i64 = bytes_per_language.values().sum();

    let fractions = bytes_per_language
        .iter()
        .map(|(l, n)| {
            let p = *n as f32 / total as f32 * 100.0;
            format!("\"{l:}\": {p:}")
        })
        .collect::<Vec<_>>()
        .join(", ");

    Ok(format!("var languages = {{{:}}};", fractions))
}

async fn github_bytes_per_lang(cfg: &Config) -> Result<HashMap<String, i64>> {
    let mut bytes_per_lang: HashMap<String, i64> = HashMap::new();

    let octocrab = octocrab::OctocrabBuilder::new()
        .personal_token(cfg.github_access_token.clone())
        .build()?;

    let mut page = octocrab
        .users(&cfg.github_user)
        .repos()
        .per_page(100)
        .send()
        .await?;

    loop {
        let tasks = futures::stream::FuturesUnordered::new();
        for repo in &page {
            tasks.push(async {
                octocrab
                    .repos(&cfg.github_user, &repo.name)
                    .list_languages()
                    .await
            });
        }

        for langs in std::pin::pin!(tasks).collect::<Vec<_>>().await {
            for (lang, n) in langs? {
                *bytes_per_lang.entry(lang).or_default() += n;
            }
        }
        page = match octocrab.get_page::<_>(&page.next).await? {
            Some(next_page) => next_page,
            None => break,
        }
    }

    Ok(bytes_per_lang)
}
