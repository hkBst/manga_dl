use color_eyre::owo_colors::OwoColorize;
use std::{fmt::Display, str::FromStr};

use clap::Parser;

use crate::{error::ArgError, style_text};

/// Todo:
/// Try out mangaraw.ma
#[derive(Debug, Clone, Default)]
pub enum SupportedSites {
    #[default]
    MangaReader,
    MangaGun,
    /// https://rawmanga.net/manga/zaziyoziyoranzu-the-jojolands/di-1hua
    RawManga,
    MangaFire,
}

impl Display for SupportedSites {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut display = |site: &str| write!(f, "{site}");
        match self {
            Self::MangaReader => display("MangaReader"),
            Self::MangaGun => display("MangaGun"),
            Self::RawManga => display("RawManga"),
            Self::MangaFire => display("MangaFire"),
        }
    }
}

/// arguments passed to the manga_dl cli
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[clap(long, value_enum, default_value = "normal")]
    pub log: LogLevel,
    #[arg(short, long, num_args = 1..)]
    pub urls: Vec<String>,
    /// Only downloads specified from a url.
    /// (ie. if they didn't download properly).
    #[arg(long, num_args = 1..)]
    pub indexes: Option<Vec<usize>>,
    /// Use an absolute path to download images to.
    /// Defaults to ./download if not specified.
    #[arg(short, long)]
    pub input_path: Option<String>,
}

impl Cli {
    pub fn check_urls(&self) -> Result<Vec<MangaUrl>, ArgError> {
        let mut urls = Vec::with_capacity(self.urls.len());
        for url in &self.urls {
            let url = MangaUrl::from_str(url)?;
            urls.push(url);
        }
        Ok(urls)
    }
}

#[derive(clap::ValueEnum, Clone, Debug, PartialEq)]
pub enum LogLevel {
    Quiet,
    Normal,
    Verbose,
    Trace,
}

pub fn get_args() -> Result<Cli, ArgError> {
    let args = Cli::parse();
    args.check_urls()?;

    Ok(args)
}

/// manga_dl Url Argument
#[derive(Debug, Clone)]
pub struct MangaUrl {
    pub inner: String,
    pub title: Option<String>,
    pub site: SupportedSites,
}

impl FromStr for MangaUrl {
    type Err = ArgError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let valid_url_str = MangaUrl::check_url(s.to_string())?;
        let site = MangaUrl::is_site_supported(&valid_url_str)?;
        let title = MangaUrl::get_title(&valid_url_str, &site);
        let url = MangaUrl {
            inner: valid_url_str,
            title,
            site,
        };
        Ok(url)
    }
}

impl MangaUrl {
    /// Removes the leading 'https://' of a Url's inner
    pub fn split_protocol(&self) -> Option<(&str, &str)> {
        if self.inner.starts_with("https://") {
            Some(("https://", &self.inner["https://".len()..]))
        } else {
            None
        }
    }

    fn is_site_supported(url: &str) -> Result<SupportedSites, ArgError> {
        if url.contains("mangareader") {
            return Ok(SupportedSites::MangaReader);
        } else if url.contains("mangagun") {
            return Ok(SupportedSites::MangaGun);
        } else if url.contains("rawmanga") {
            return Ok(SupportedSites::RawManga);
        } else if url.contains("mangafire") {
            return Ok(SupportedSites::MangaFire);
        }
        Err(ArgError::WebsiteNotSupported(url.to_string()))
    }

    /// extract title from url based on the site
    fn get_title(url: &str, site: &SupportedSites) -> Option<String> {
        match site {
            SupportedSites::MangaReader | SupportedSites::MangaFire => {
                if let Some(start) = url.split_once("/read/") {
                    // extract the part after "/read/" until the next "/"
                    return Some(start.1.replace("/", "-").to_string());
                }
            }
            SupportedSites::MangaGun => {
                if let Some(start) = url.rsplit_once("/") {
                    return Some(start.1.to_string());
                }
            }
            SupportedSites::RawManga => {
                if let Some(start) = url.split_once("/manga/") {
                    return Some(start.1.replace("/", "_").trim().to_string());
                }
            }
        }
        None
    }

    fn check_url(url: String) -> Result<String, ArgError> {
        let mut url = url;
        if !url.starts_with("https://") {
            let new_url = format!("https://{}", url);
            url = new_url;
        }
        let url = url.to_lowercase();

        if !url.is_ascii() {
            return Err(ArgError::InvalidUrl {
                url,
                reason: style_text!("URL is not valid ASCII"),
                example: style_text!("mangareader.to/read/vagabond-4/ja/chapter-6"),
            });
        }

        if url.is_empty() {
            return Err(ArgError::InvalidUrl {
                url,
                reason: style_text!("Empty string"),
                example: style_text!("*.com | *.to"),
            });
        }

        if !(url.contains(".to") || url.contains(".com") || url.contains(".net")) {
            return Err(ArgError::InvalidUrl {
                url,
                reason: style_text!("URL is missing or has an invalid top-level domain"),
                example: style_text!("*.com | *.to | *.net"),
            });
        }

        if !(url.contains("mangareader.to")
            || url.contains("mangagun.net")
            || url.contains("rawmanga")
            || url.contains("mangafire.to"))
        {
            return Err(ArgError::WebsiteNotSupported(url));
        }

        if url.contains("mangareader") || url.contains("mangafire") && !url.contains("/read") {
            return Err(ArgError::InvalidUrl {
                url,
                reason: style_text!("mangareader or mangafire URL is missing /read"),
                example: style_text!("mangareader.to/read/vagabond-4/ja/chapter-6"),
            });
        }

        Ok(url)
    }
}

#[cfg(test)]
mod url_tests {
    use super::*;

    #[test]
    fn test_strip_protocol() {
        let url = MangaUrl {
            inner: "https://mangafire.to/read/one-piece-digital-colored-comicss.06w3/en/chapter-1"
                .to_string(),
            title: None,
            site: SupportedSites::MangaFire,
        };
        let pc = url.split_protocol();
        assert_eq!(
            pc,
            Some((
                "https://",
                "mangafire.to/read/one-piece-digital-colored-comicss.06w3/en/chapter-1"
            ))
        );
    }
}
