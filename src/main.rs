mod cli;
mod error;
mod loading;
mod macros;
mod sites;

use std::{
    fs::{self, File, OpenOptions},
    future::Future,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::LazyLock,
    time::{self, Duration},
};

use clap::Parser;
use cli::{Cli, LogLevel, MangaUrl, SupportedSites};
use color_eyre::{eyre::eyre, Section};
#[allow(unused_imports)]
use color_eyre::{
    eyre::{Context, Result},
    owo_colors::OwoColorize,
    Report,
};
use error::{DownloadImageError, MainError, MangaReaderError};
use fantoccini::{elements::Element, error::NewSessionError, Client, Locator};
use loading::{print_indexes_arg, print_reqerr_count};
use reqwest::Client as ReqClient;
use serde::{Deserialize, Serialize};
use serde_json::{json, to_string_pretty};
use sites::{
    mangafire::dl_mangafire, mangagun::dl_mangagun, mangareader::dl_mangareader,
    rawmanga::dl_rawmanga,
};
use spinners::{Spinner, Spinners};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub trait WebsiteActions {
    fn new_manga_client() -> impl Future<Output = Result<Client, NewSessionError>>;

    fn count_total_pages(
        &self,
        query: &str,
        wait_for: Duration,
    ) -> impl Future<Output = Result<usize>>;
    fn click_element(&self, query: &str, wait_for: Duration) -> impl Future<Output = Result<()>>;
    fn query_selector_all(
        &self,
        query: &str,
        page_count: usize,
        retries: usize,
        sleep: Duration,
    ) -> impl Future<Output = Result<Vec<Element>, Report>>;
}

impl WebsiteActions for Client {
    async fn new_manga_client() -> Result<Client, NewSessionError> {
        let mut builder = fantoccini::ClientBuilder::native();

        if !matches!(PROGRAM_CLI.log, LogLevel::Trace) {
            let caps: serde_json::Map<String, serde_json::Value> = json!({
                "moz:firefoxOptions": {
                    "args": ["-headless"]
                }
            })
            .as_object()
            .expect("failed to serialize caps")
            .clone();

            builder.capabilities(caps);
        }

        builder.connect("http://localhost:4444").await
    }

    /// Finds the first element that matches the query and returns the text inside the element as a
    /// number
    ///
    /// # ERRORS
    ///
    /// Will return an Err if element's innerText doesn't contain a parsable number (ie. contains chars).
    async fn count_total_pages(&self, query: &str, wait_for: Duration) -> Result<usize> {
        let el = self
            .wait()
            .at_most(wait_for)
            .for_element(Locator::Css(query))
            .await
            .context(eyre!("failed to find total page element:\n   [query]='{query}' failed to match any elements."))
            .with_suggestion(|| "did you enter the right url?")
            .with_suggestion(|| "open firefox instance by running with --log trace")?;
        let count = el.text().await?;
        let count = count
            .parse::<usize>()
            .context(eyre!("failed to parse total pages: '{count}' as usize"))?;
        Ok(count)
    }

    async fn click_element(&self, query: &str, wait_for: Duration) -> Result<()> {
        self.wait()
            .at_most(wait_for)
            .for_element(Locator::Css(query))
            .await?;
        Ok(())
    }

    async fn query_selector_all(
        &self,
        query: &str,
        page_count: usize,
        retries: usize,
        sleep: Duration,
    ) -> Result<Vec<Element>> {
        let query = query.to_string();
        let mut imgs = self
            .find_all(Locator::Css(&query))
            .await
            .with_context(|| WebsiteError {
                website: SupportedSites::MangaFire,
                kind: WebsiteErrorKind::QuerySelectorAll {
                    query: query.clone(),
                },
            })?;

        let mut count = 0;
        while count < retries && imgs.len() < page_count {
            count += 1;
            tokio::time::sleep(sleep).await;
            imgs = self.find_all(Locator::Css(&query)).await?;
        }

        if imgs.is_empty() {
            Err(WebsiteError {
                website: SupportedSites::MangaFire,
                kind: WebsiteErrorKind::QuerySelectorAll { query },
            })?;
        } else if imgs.len() < page_count {
            Err(WebsiteError {
                website: SupportedSites::MangaFire,
                kind: WebsiteErrorKind::MissingImagesAfterMaxRetries {
                    img_count: imgs.len(),
                    page_count,
                    query,
                },
            })?;
        }

        Ok(imgs)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{website}:\n  {kind}")]
pub struct WebsiteError {
    website: SupportedSites,
    kind: WebsiteErrorKind,
}

#[derive(Debug, thiserror::Error)]
pub enum WebsiteErrorKind {
    #[error("the query matched 0 elements on the page.\n  [query]= '{query}'")]
    QuerySelectorAll { query: String },
    #[error(
        "{query}'\n    the query only matched {img_count} <img />s when {page_count} were expected."
    )]
    MissingImagesAfterMaxRetries {
        img_count: usize,
        page_count: usize,
        query: String,
    },
}

#[derive(Eq, Hash, PartialEq, Debug)]
pub struct ImageData {
    pub bytes: Vec<u8>,
    pub path: String,
}

impl ImageData {
    fn _write_img(&self) -> Result<()> {
        std::fs::write(&self.path, &self.bytes)?;
        Ok(())
    }
}

trait WriteAllExt {
    fn write_all(self) -> Result<()>;
}

impl<I> WriteAllExt for I
where
    I: ExactSizeIterator<Item = ImageData>,
{
    fn write_all(self) -> Result<()> {
        let mut sp = Spinner::new(Spinners::Balloon, "".into());
        let len = self.len();

        self.into_iter().enumerate().for_each(|(i, d)| {
            d._write_img().unwrap();
            let msg = format!("({} .. {})", i + 1, len);
            sp = Spinner::new(Spinners::Balloon, msg);
        });

        sp.stop_with_newline();
        Ok(())
    }
}

#[derive(Eq, Hash, PartialEq, Debug)]
pub struct ReqImageData {
    pub url: String,
    pub path: String,
}

impl ReqImageData {
    pub async fn dl_src(&self, req_c: &ReqClient) -> Result<ImageData, DownloadImageError> {
        let Self { url, path } = &self;
        let res = req_c
            .get(url)
            .send()
            .await
            .map_err(|e| DownloadImageError::GetReqwest(url.to_string(), e.to_string()))?;
        let bytes = res
            .bytes()
            .await
            .wrap_err(format!("failed to decode src_url to bytes: {url}"))?
            .to_vec();
        let img = ImageData {
            bytes,
            path: path.clone(),
        };

        Ok(img)
    }
}

static PROGRAM_CLI: LazyLock<Cli> = LazyLock::new(Cli::parse);

#[tokio::main]
async fn main() -> Result<()> {
    if matches!(PROGRAM_CLI.log, LogLevel::Trace | LogLevel::Verbose) {
        let args = std::env::args();
        dbg!(&args);
    }
    //color_eyre::install()?;
    let instant = time::Instant::now();
    println!("{:#?}", PROGRAM_CLI.log);

    #[cfg(target_os = "windows")]
    let gd_data: &[u8] = include_bytes!("../bin/geckodriver-win.exe");
    #[cfg(target_os = "macos")]
    let gd_data: &[u8] = include_bytes!("../bin/geckodriver-macos");
    // only tested with arch
    #[cfg(target_os = "linux")]
    let gd_data: &[u8] = include_bytes!("../bin/geckodriver-linux");

    #[allow(clippy::zombie_processes)]
    let mut child = start_gd(gd_data).wrap_err("failed to start gecko driver")?;
    let c: Client = Client::new_manga_client()
        .await
        .expect("failed to start fantoccini");
    let mut errors: Vec<Report> = Vec::with_capacity(PROGRAM_CLI.urls.len());

    if let Some(indexes) = &PROGRAM_CLI.pages {
        print_indexes_arg(indexes);
    }

    for (i, url) in PROGRAM_CLI.urls.iter().enumerate() {
        let expanded = url.to_vec();
        for url in expanded {
            match url.site {
                cli::SupportedSites::MangaReader => {
                    if let Err(e) = dl_mangareader(&c, &url, i).await {
                        errors.push(e);
                    };
                }
                cli::SupportedSites::MangaGun => {
                    if let Err(e) = dl_mangagun(&c, &url).await {
                        errors.push(e);
                    };
                }
                cli::SupportedSites::RawManga => {
                    if let Err(e) = dl_rawmanga(&c, &url).await {
                        errors.push(e);
                    };
                }
                cli::SupportedSites::MangaFire => {
                    if let Err(e) = dl_mangafire(&c, &url).await {
                        errors.push(e);
                    }
                }
                _ => {
                    panic!("website unsupported")
                    // function to TRY download imgs for any website
                }
            }
        }
    }

    c.close().await?;
    child.kill().expect("failed to kill geckodriver");
    child
        .wait()
        .expect("panicked while waiting for geckodriver to exit after attempting to terminate");
    cleanup();

    if !errors.is_empty() {
        let titles: Vec<String> = PROGRAM_CLI
            .urls
            .to_vec()
            .iter()
            .map(|url| url.inner.clone())
            .collect();
        print_reqerr_count(errors.len(), &titles);
        eprintln!("{}", style_text!("STDERR:", error));
        for e in errors {
            eprintln!("{:?}", e);
        }
    }

    let elapsed = instant.elapsed().as_secs();
    println!("\nelapsed: {}s", elapsed);

    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LogError {
    url: String,
    index: usize,
    error: String,
}

pub fn write_log(e: LogError) -> Result<(), io::Error> {
    let mut f = OpenOptions::new()
        .append(true)
        .create(true)
        .open("manga_dl_errors.log")?;

    let err = to_string_pretty(&e)?;
    f.write_all(err.as_bytes())?;
    f.flush()?;

    Ok(())
}

/// cross platform way to get the path to the gecko driver
pub fn os_get_geckodriver_exe_path() -> PathBuf {
    if cfg!(target_os = "windows") {
        PathBuf::from("../bin/geckodriver-win.exe")
    } else if cfg!(target_os = "macos") {
        PathBuf::from("../bin/geckodriver-macos")
    } else if cfg!(target_os = "linux") {
        PathBuf::from("../bin/geckodriver-linux")
    } else {
        panic!("Unsupported operating system")
    }
}

// pub fn os_get_geckodriver_exe_path_if() -> PathBuf {
//     if cfg!(target_os = "windows") {
//         PathBuf::from("../bin/geckodriver-win.exe")
//     } else if cfg!(target_os = "macos") {
//         PathBuf::from("../bin/geckodriver-macos")
//     } else if cfg!(target_os = "linux") {
//         // Only tested with arch
//         PathBuf::from("../bin/geckodriver-linux")
//     } else {
//         unreachable!()
//     }
// }

pub fn cleanup() {
    #[cfg(target_os = "windows")]
    fs::remove_file("./temp/gd.exe").expect("failed to remove gd.exe");
    #[cfg(target_os = "macos")]
    fs::remove_file("./temp/gd").expect("failed to remove gd executable");
    #[cfg(target_os = "linux")]
    fs::remove_file("./temp/gd").expect("failed to remove gd executable");

    fs::remove_dir("temp").expect("failed to remove temp dir");
}

pub fn start_gd(gd_data: &[u8]) -> Result<Child, std::io::Error> {
    fs::create_dir_all("temp")?;
    #[cfg(target_os = "windows")]
    let temp_path = Path::new("temp/gd.exe");
    #[cfg(target_os = "macos")]
    let temp_path = Path::new("temp/gd");
    #[cfg(target_os = "linux")]
    let temp_path = Path::new("temp/gd");

    if !temp_path.exists() {
        let mut temp_file = File::create_new(temp_path)?;
        temp_file.write_all(gd_data)?;
        drop(temp_file);
    }

    #[cfg(target_os = "windows")]
    let firefox_arg = "C:/Program Files/Mozilla Firefox/firefox.exe";
    #[cfg(target_os = "linux")]
    let firefox_arg = "/mnt/c/program files/mozilla firefox/firefox.exe";

    // Set execute permission (for UNIX systems)
    #[cfg(unix)]
    {
        let metadata = std::fs::metadata(temp_path)?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o755); // Make the file executable
        std::fs::set_permissions(temp_path, permissions)?;
    }

    let child = Command::new(temp_path)
        .arg("--binary")
        .arg(firefox_arg)
        .spawn()?;

    Ok(child)
}

/// handle any redirect ads by closing the newly opened tab
pub async fn g_close_open_window(c: &Client) -> Result<(), MangaReaderError> {
    let handles = c.windows().await?;
    if handles.len() > 1 {
        for handle in handles.iter().skip(1) {
            c.switch_to_window(handle.clone()).await?;
            c.close_window().await?;
        }
        c.switch_to_window(handles[0].clone()).await?;
    }

    Ok(())
}

/// hides anything with a z-index of `2147483647`.
pub async fn g_handle_popup(c: &Client) -> Result<(), MainError> {
    if let Ok(e) = c
        .find(Locator::Css("*[style*='z-index: 2147483647']"))
        .await
    {
        e.click().await?;

        c.execute(
            r#"
            var ad = document.querySelector("*[style*='z-index: 2147483647']");
            ad.style.display = 'none';  
            "#,
            vec![],
        )
        .await?;
    }

    Ok(())
}

pub type NavigateGroup = (String, String, Spinner);

/// returns [`NavigateGroup`]
pub async fn setup_nav(client: &Client, url: &MangaUrl) -> Result<NavigateGroup> {
    let title = url.title.clone().unwrap_or_else(|| gen_rand().to_string());
    let dl_path = match &PROGRAM_CLI.input_path {
        Some(p) => format!("{p}/{}", title),
        None => {
            format!("./download/{title}")
        }
    };

    if let Err(e) = std::fs::create_dir_all(&dl_path) {
        panic!("{e} \n            at: `{dl_path}`");
    }

    println!("\n{:?}", url.site);
    let message = format!("{}: {}", "", style_text!(&title, url));
    let mut sp = Spinner::new(spinners::Spinners::Arc, message);
    client.goto(&url.inner).await?;
    sp.stop_with_newline();

    Ok((title, dl_path, sp))
}

pub fn gen_rand() -> i32 {
    let num = vec![2, 3, 50, 80, 23124];
    let add = &num as *const Vec<i32>;
    add as i32
}
