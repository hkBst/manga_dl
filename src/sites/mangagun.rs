use std::{
    collections::HashSet,
    io::Write,
    time::{Duration, Instant},
};

use color_eyre::{
    eyre::{eyre, Context},
    Result,
};
use fantoccini::{elements::Element, Client, Locator};
use spinners::{Spinner, Spinners};
use tokio::time::sleep;

use crate::{
    cli::{LogLevel, MangaUrl},
    g_handle_popup,
    loading::downloading_panel_data_msg,
    setup_nav, ImageData, PROGRAM_CLI,
};
use crate::{loading::print_elapsed, WriteAllExt};

pub async fn dl_mangagun(client: &Client, url: &MangaUrl) -> Result<()> {
    let (_, dl_path, mut sp) = setup_nav(client, url).await?;
    let start = Instant::now();

    // hide the top-navbar
    execute_set_element_hidden_inline(client, ".navbar").await?;
    // hide the bottom-navbar
    execute_set_element_hidden_inline(client, "#rd-side_icon").await?;

    let index_map: Option<HashSet<&usize>> = PROGRAM_CLI
        .pages
        .as_ref()
        .map(|slice| slice.iter().collect());
    let img_data = get_all_images(&dl_path, client, &mut sp, index_map).await?;

    match PROGRAM_CLI.log {
        LogLevel::Verbose | LogLevel::Trace => {
            let elapsed = start.elapsed();
            println!();
            print_elapsed(elapsed);
        }
        _ => { /* skip */ }
    }

    // img_data: HashSet<ImageData>
    img_data.into_iter().write_all()?;

    Ok(())
}

const IMAGE_LOAD_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(300);
const MIN_IMAGE_DIMENSION: f64 = 500.0;

async fn get_all_images(
    dl_path: &str,
    c: &Client,
    sp: &mut Spinner,
    index_map: Option<HashSet<&usize>>,
) -> Result<HashSet<ImageData>> {
    g_handle_popup(c).await.wrap_err(line!())?;
    let imgs = c.find_all(Locator::Css("img.chapter-img")).await?;
    let max = imgs.len();
    let mut new_imgs: HashSet<ImageData> = HashSet::with_capacity(max);

    for (i, img) in imgs.into_iter().enumerate() {
        if let Some(i_map) = &index_map {
            if !i_map.contains(&i) {
                continue;
            }
        }

        // Hide ads once per iteration
        execute_set_element_hidden_inline(c, "#adModal").await?;
        hide_elements_with_max_index(c).await?;
        sleep(Duration::from_millis(300)).await;

        // Wait for valid image to load
        match wait_for_valid_image(&img, i).await {
            Ok(()) => {
                update_progress(i, max, sp, &img).await?;

                // Capture and store image
                let bytes = img.screenshot().await?;
                let path = format!("{dl_path}/{i}.jpg");
                new_imgs.insert(ImageData { bytes, path });
            }
            Err(e) => {
                // Log error but continue with next image
                eprintln!("Failed to load image at index {i}: {e}");
                continue;
            }
        }
    }

    Ok(new_imgs)
}

async fn wait_for_valid_image(img: &Element, index: usize) -> Result<()> {
    let start = std::time::Instant::now();

    while start.elapsed() < IMAGE_LOAD_TIMEOUT {
        // Check image source
        if let Some(src) = img.attr("src").await? {
            if src.contains("gif") {
                sleep(POLL_INTERVAL).await;
                continue;
            }

            // Verify image dimensions
            let rect = img.rectangle().await?;
            if rect.2 > MIN_IMAGE_DIMENSION && rect.3 > MIN_IMAGE_DIMENSION {
                return Ok(());
            }
        }

        sleep(POLL_INTERVAL).await;
    }

    Err(eyre!("Timeout waiting for valid image at index {index}"))
}

async fn update_progress(index: usize, max: usize, sp: &mut Spinner, img: &Element) -> Result<()> {
    if matches!(PROGRAM_CLI.log, LogLevel::Trace) {
        if let Some(src) = img.attr("src").await? {
            print!(":\n  {src}");
            std::io::stdout().flush().expect("failed to flush output");
        }
    }

    let msg = downloading_panel_data_msg(index, max);
    *sp = Spinner::new(Spinners::Arc, msg);

    Ok(())
}

/// loops over every element and sets any elements with a `computed` zIndex of
/// '2147483647' to `display = 'none'`.
pub async fn hide_elements_with_max_index(c: &Client) -> Result<()> {
    let script = r#"
    var elements = document.querySelectorAll('*');
    elements.forEach(function(element) {
    var style = window.getComputedStyle(element);
    if (style.zIndex === '2147483647') {
        element.style.display = 'none';
    }});
    "#;
    c.execute(script, vec![]).await?;
    Ok(())
}

/// * selector
///
/// ".navbar" || "#navbar";
pub async fn execute_set_element_hidden_inline(c: &Client, query: &str) -> Result<()> {
    let script = format!(
        r#"
            var element = document.querySelector('{}');
            if (element) {{
                element.style.display = 'none';
            }} 
            "#,
        query
    );
    c.execute(&script, vec![]).await?;
    Ok(())
}
