use base64::prelude::*;
use std::{collections::HashSet, fs::read_dir, time::Duration};

use crate::{
    cli::{Cli, MangaUrl},
    error::{DownloadImageError, MangaReaderError},
    g_close_open_window,
    loading::print_reqerr_count,
    setup_nav,
};
use crate::{error::MainError, loading::downloading_panel_data_msg};
use color_eyre::{
    eyre::{eyre, Context, Result},
    owo_colors::OwoColorize,
    Section,
};
use fantoccini::{Client, Locator};
use reqwest::{Client as ReqClient, ClientBuilder as ReqClientBuilder};
use spinners::{Spinner, Spinners};
use std::process::Command;

use crate::{ImageData, ReqImageData};

/// Arguments
///
/// * `index` - needed because mangareader has a popup on render,
pub async fn dl_mangareader(
    client: &Client,
    url: &MangaUrl,
    args: &Cli,
    index: usize,
) -> Result<()> {
    let (title, dl_path, mut _sp) = setup_nav(client, url, args).await?;
    //let start = Instant::now();

    if index == 0 {
        select_reading_mode(client).await?;
    }
    let max = count_pages(client).await? - 1;

    // hold all the bytes and formatted paths of the imgs
    // to write all at once at the very end
    let mut img_data_vec: Vec<ImageData> = Vec::new();

    let req_client = ReqClientBuilder::new().timeout(Duration::from_millis(2500));
    let req_client = req_client.build()?;

    // if the panel_canvas is an <img> elemement get req is required
    // hold src urls till the end via this vector to download concurrently
    // (url, dl_path)
    let mut src_urls: HashSet<ReqImageData> = HashSet::new();
    let is_imgs = _find_images(client).await;
    let mut errors = Vec::with_capacity(max.into());

    for i in 0..max {
        if errors.len() > 3 {
            break;
        }
        if !is_imgs {
            match download_panel_canvas(
                i,
                client,
                &dl_path,
                &mut img_data_vec,
                Duration::from_secs(5),
            )
            .await
            {
                Ok(_) => (),
                Err(DownloadImageError::MissingCanvasElement(_)) => {
                    if let Err(e) = download_panel_img(i, client, &dl_path, &mut src_urls).await {
                        errors.push(e);
                    }
                }
                Err(e) => return Err(e.into()),
            };
        } else if let Err(img_err) = download_panel_img(i, client, &dl_path, &mut src_urls).await {
            if let Err(canvas_err) = download_panel_canvas(
                i,
                client,
                &dl_path,
                &mut img_data_vec,
                Duration::from_secs(5),
            )
            .await
            {
                errors.push(canvas_err);
                errors.push(img_err);
            };
        }
        _sp = Spinner::new(Spinners::Dots3, downloading_panel_data_msg(i, max));
        client.execute("hozNextImage()", vec![]).await?;
    }
    _sp.stop_with_newline();
    let mut error_reports: Vec<color_eyre::Report> = Vec::with_capacity(src_urls.len());

    if !src_urls.is_empty() {
        for (i, img) in src_urls.into_iter().enumerate() {
            let res = download_img_src(&img.url, img.path, &req_client).await;

            match res {
                Ok(r) => img_data_vec.push(r),
                Err(e) => {
                    let report = eyre!("failed on `{}. {}`: \n{}", i, img.url, e.to_string());
                    error_reports.push(report);
                }
            }
        }
    }

    //_sp.stop_with_newline();

    if !error_reports.is_empty() {
        print_reqerr_count(error_reports.len(), &title);
    } else if !errors.is_empty() {
        print_reqerr_count(errors.len(), &title);
        println!("{}", "STDERROR:".bright_red());
        for e in errors.into_iter() {
            eprintln!("{}", e.red());
        }
    }

    //let elapsed = start.elapsed();
    //print_download_complete_msg(elapsed);

    //_sp = Spinner::new(Spinners::Triangle, style_text!("writing data to file.."));

    img_data_vec
        .into_iter()
        .for_each(|data| write_img(&data).unwrap());

    Ok(())
}

async fn _find_images(c: &Client) -> bool {
    if let Ok(elements) = c.find_all(Locator::Css("img.image-horizontal")).await {
        for el in elements {
            if el.attr("src").await.is_ok() {
                return true;
            }
        }
    }
    false
}

async fn download_panel_canvas(
    index: u16,
    c: &Client,
    dl_path: &str,
    img_data_vec: &mut Vec<ImageData>,
    dur: Duration,
) -> Result<(), DownloadImageError> {
    // Construct the selector with the provided index
    let selector = "div.ds-item.active .image-horizontal";

    // Check if the canvas element exists at the given selector
    if c.wait()
        .at_most(dur)
        .for_element(Locator::Css(selector))
        .await
        .is_ok()
    {
        // Execute JavaScript to get the data URL of the canvas element
        let script = r#"
            var canvas = document.querySelector(arguments[0]);
            return canvas ? canvas.toDataURL() : null;
        "#;

        // Execute the script with the selector as argument
        let data_url = c
            .execute(script, vec![selector.into()])
            .await
            .expect("failed to execute js toDataUrl() script")
            .as_str()
            .unwrap()
            .trim()
            .to_string();

        let base64_data = data_url
            .strip_prefix("data:image/jpeg;base64,")
            .or_else(|| data_url.strip_prefix("data:image/png;base64,"))
            .ok_or_else(|| DownloadImageError::InvalidDataUrl(data_url.clone()))?;

        // Decode the Base64 string into binary data
        let decoded_data = BASE64_STANDARD
            .decode(base64_data)
            .expect("failed to decode base64 string into binary.");

        let index = index + 1;
        let path = format!("{dl_path}/{index}.jpg");

        // Add the image data to the vector
        let img = ImageData {
            bytes: decoded_data,
            path,
        };
        img_data_vec.push(img);
    } else {
        return Err(DownloadImageError::MissingCanvasElement(
            dl_path.to_string(),
        ));
    }

    Ok(())
}

async fn download_panel_img(
    index: u16,
    c: &Client,
    dl_path: &str,
    src_urls: &mut HashSet<ReqImageData>,
) -> Result<(), DownloadImageError> {
    // Try to locate the image element using the CSS selector
    if let Ok(elm) = c
        .wait()
        .at_most(Duration::from_millis(2000))
        .for_element(Locator::Css(
            "div.ds-item.active > div.ds-image.loaded > img.image-horizontal",
        ))
        .await
    {
        // Retrieve the image URL from the 'src' attribute
        if let Some(img_url) = elm.attr("src").await? {
            // Ensure the URL is valid before attempting to download
            let img_url = img_url.trim();
            if !img_url.is_empty() {
                let index = index + 1;
                let path = format!("{dl_path}/{index}.jpg");
                let img = ReqImageData {
                    url: img_url.to_string(),
                    path,
                };
                src_urls.insert(img);
            }
        }
    } else {
        let path = format!("{index}.jpg");
        return Err(DownloadImageError::MissingImgElement(path));
    }

    Ok(())
}

pub async fn download_img_src(
    url: &str,
    path: String,
    c: &ReqClient,
) -> Result<ImageData, DownloadImageError> {
    let res = c
        .get(url)
        .send()
        .await
        .map_err(|e| DownloadImageError::GetReqwest(url.to_string(), e.to_string()))?;
    let bytes = res
        .bytes()
        .await
        .wrap_err(format!("failed to decode src_url to bytes: {url}"))?
        .to_vec();
    let img = ImageData { bytes, path };

    Ok(img)
}

async fn count_pages(c: &Client) -> Result<u16, MainError> {
    let selector = Locator::Css("span.hoz-total-image");
    let pgs_elm = c
        .wait()
        .at_most(Duration::from_millis(1000))
        .for_element(selector)
        .await
        .map_err(|e| {
            eyre!("total pages element not found:\n  {}{:?}", e, selector)
                .with_note(|| "try running with -- --full")
        })?;

    let text = pgs_elm.html(true).await?;

    text.parse::<u16>()
        .map_err(|_| MainError::ParseCounterElement(text))
}

pub fn write_img(data: &ImageData) -> Result<()> {
    std::fs::write(&data.path, &data.bytes)?;
    Ok(())
}

#[allow(dead_code)]
pub fn crop_imgs(dl_path: &str) -> Result<(), MainError> {
    for (i, entry) in read_dir(dl_path)?.enumerate() {
        let entry = entry?;
        let path = entry.path();

        let path = path.to_string_lossy();

        Command::new("magick")
            .arg(path.as_ref()) // Path to the image
            .arg("-fuzz")
            .arg("10%")
            .arg("-fill")
            .arg("tran_sparent")
            .arg("-opaque")
            .arg("rgb(17,17,17)")
            .arg("-trim")
            .arg("+repage")
            .arg(path.as_ref()) // Output file name
            .spawn()
            .expect("error launching magick")
            .wait()?;

        print!("\r{i}");
    }

    Ok(())
}

/// exists because mangareader asks new profiles to select the orientation of the reader
async fn select_reading_mode(c: &Client) -> Result<(), MangaReaderError> {
    if let Ok(btn) = c
        .wait()
        .at_most(Duration::from_millis(2000))
        .for_element(Locator::Css("a.rtl-row:nth-child(2)"))
        .await
    {
        btn.click()
            .await
            .with_error(|| MangaReaderError::SelectReadingMode {
                info: "".to_string(),
            })?;
        g_close_open_window(c).await?;

        btn.click().await?;
    }

    Ok(())
}
