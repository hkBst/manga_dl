#[allow(unused_imports)]
use std::{
    collections::HashSet,
    io::Write,
    time::{Duration, Instant},
};

use color_eyre::{
    eyre::{eyre, Context},
    owo_colors::OwoColorize,
    Result,
};
use fantoccini::{Client, Locator};
use reqwest::ClientBuilder as ReqClientBuilder;
use spinners::{Spinner, Spinners};
//use spinners::{Spinner, Spinners};

use crate::{
    cli::{Cli, LogLevel, MangaUrl},
    g_handle_popup,
    loading::{downloading_panel_data_msg, print_reqerr_count},
    setup_nav,
    sites::mangareader::{download_img_src, write_img},
};

use crate::ReqImageData;

pub async fn dl_rawmanga(client: &Client, url: &MangaUrl, args: &Cli) -> Result<()> {
    let (title, dl_path, mut sp) = setup_nav(client, url, args).await?;
    //let start = Instant::now();

    let index_map: Option<HashSet<&usize>> =
        args.indexes.as_ref().map(|slice| slice.iter().collect());

    let req_client = ReqClientBuilder::new().timeout(Duration::from_millis(2500));
    let req_client = req_client.build()?;

    let src_urls = get_all_image_srcs(&dl_path, client, index_map, &args.log).await?;
    let mut img_data = Vec::with_capacity(src_urls.len());
    let mut error_reports = Vec::with_capacity(src_urls.len());

    let max = src_urls.len();
    if !src_urls.is_empty() {
        for (i, img) in src_urls.into_iter().enumerate() {
            let res = download_img_src(&img.url, img.path, &req_client).await;

            match res {
                Ok(r) => img_data.push(r),
                Err(e) => {
                    let report = eyre!("failed on `{}. {}`: \n{}", i, img.url, e.to_string());
                    error_reports.push(report);
                }
            }
            let msg = downloading_panel_data_msg(i as u16, max as u16);
            sp = Spinner::new(Spinners::Arc, msg);
        }
    }
    sp.stop_with_newline();

    img_data
        .into_iter()
        .for_each(|data| write_img(&data).unwrap());

    if !error_reports.is_empty() {
        print_reqerr_count(error_reports.len(), &title);
    } else if !error_reports.is_empty() {
        print_reqerr_count(error_reports.len(), &title);
        println!("{}", "STDERROR:".bright_red());
        for e in error_reports.into_iter() {
            eprintln!("{}", e.red());
        }
    }

    Ok(())
}

async fn get_all_image_srcs(
    dl_path: &str,
    c: &Client,
    index_map: Option<HashSet<&usize>>,
    log: &LogLevel,
) -> Result<HashSet<ReqImageData>> {
    g_handle_popup(c).await.wrap_err(line!())?;
    let imgs = c.find_all(Locator::Css("div.page-chapter img")).await?;
    let mut new_imgs: HashSet<ReqImageData> = HashSet::new();

    //let max = imgs.len();
    // if the index map is Some, skip indexes that aren't specified
    for (i, img) in imgs.into_iter().enumerate() {
        if let Some(i_map) = &index_map {
            if !i_map.contains(&i) {
                continue;
            }
        }
        if let Some(src) = img.attr("src").await? {
            let path = format!("{dl_path}/{i}.jpg");
            let img = ReqImageData { url: src, path };
            match log {
                LogLevel::Trace | LogLevel::Verbose => println!("{:?}", img),
                _ => {}
            }
            new_imgs.insert(img);
        }
    }

    Ok(new_imgs)
}

#[tokio::test]
async fn test_reqwest() -> Result<(), Box<dyn std::error::Error>> {
    let url = "https://lovejp-cdn.site/one-piece/chapter-999/001.jpg";
    let bytes = reqwest::get(url).await?.bytes().await?;
    std::fs::write("test.jpg", &bytes)?;
    println!("Downloaded and saved test.jpg");
    Ok(())
}
