use std::time::{Duration, Instant};

use fantoccini::Client;

use crate::{
    cli::{Cli, LogLevel, MangaUrl},
    g_close_open_window,
    loading::print_download_complete_msg,
    setup_nav,
    sites::mangareader::write_img,
    ImageData, ReqImageData, WebsiteActions,
};

use reqwest::Client as ReqClient;

use super::mangagun::hide_elements_with_max_index;

pub async fn dl_mangafire(client: &Client, url: &MangaUrl, cli: &Cli) -> color_eyre::Result<()> {
    let (_, dl_path, _) = setup_nav(client, url, cli).await?;
    let start = Instant::now();

    let query = "img[data-number][src]".to_string();
    let sleep = Duration::from_millis(1000);
    let retries = 3;
    let page_count = client.count_total_pages("span > b.total-page").await?;

    let html = client.find(fantoccini::Locator::Css("html")).await?;
    html.click().await?;

    g_close_open_window(client).await?;

    // first hover over the progress bar or else you cant click on the last page
    client
        .find(fantoccini::Locator::Css("div#progress-bar"))
        .await?
        .click()
        .await?;

    for i in 1..page_count {
        if i % 2 == 0 {
            client
                .find(fantoccini::Locator::Css(&format!("li[data-page='{i}']",)))
                .await?
                .click()
                .await?;
        }
    }

    let imgs = client
        .query_selector_all(&query, page_count, retries, sleep)
        .await?;

    let req_c = ReqClient::new();
    let mut img_data: Vec<ImageData> = Vec::with_capacity(imgs.len());

    for (i, img) in imgs.iter().enumerate() {
        let src = img.attr("src").await?;
        if let Some(src) = src {
            let path = format!("{dl_path}/{i}.jpg");
            let req_data = ReqImageData { url: src, path };
            let data = req_data.dl_src(&req_c).await?;
            img_data.push(data);
        }
    }

    img_data.iter().for_each(|d| write_img(d).unwrap());

    match cli.log {
        LogLevel::Verbose | LogLevel::Trace => {
            let elapsed = start.elapsed();
            print_download_complete_msg(elapsed);
        }
        _ => { /* skip */ }
    }

    Ok(())
}
