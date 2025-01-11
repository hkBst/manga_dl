#![allow(clippy::useless_format)]

use clap::Parser;
use color_eyre::{
    eyre::{eyre, Context},
    Report, Result, Section,
};
use regex::Regex;
use std::{
    fmt::Display,
    ops::{Deref, Range},
    str::FromStr,
    sync::LazyLock,
};

mod url_regex {

    use super::*;
    ///  matches the last '`\d+`' within a url.
    pub static REGEX_URL_INDEX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(\d+)(?:[^0-9]*$)").unwrap());

    #[test]
    fn regex_url_index() {
        let asserts = ["117", "25", "94"];
        let urls = [
            "https://test.to/read99/onepunchman-47/chapter-117.html",
            "https://test.to/read/chapter-25/onepunchman",
            "https://test.com/45/c94/vagbondd.html",
        ];

        let indexes: Vec<&str> = urls
            .into_iter()
            .enumerate()
            .map(|(i, url)| {
                let caps = REGEX_URL_INDEX
                    .captures(url)
                    .unwrap_or_else(|| panic!("#{} matched no index: {url}", i + 1));
                caps.iter()
                    .last()
                    .unwrap_or_else(|| panic!("#{} matched no index: {url}", i + 1))
                    .unwrap_or_else(|| panic!("#{} matched no index: {url}", i + 1))
                    .as_str()
            })
            .collect();

        for (i, index) in indexes.iter().enumerate() {
            assert_eq!(index, asserts.get(i).unwrap());
        }
    }

    /// Strictly matches any range within a url.
    pub static REGEX_URL_RANGE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(\d+)\.\.(\d+)").unwrap());

    #[test]
    fn regex_url_range() {
        let asserts = ["15..20", "3..5"];
        let urls = [
            "https://example.to/read/onepunchman-4/chapter-15..20.html",
            "https://mangafire.to/read/vagabondd.4mx/ja/chapter-3..5",
        ];
        for (i, url) in urls.iter().enumerate() {
            let mrange = REGEX_URL_RANGE.find(url).unwrap().as_str();
            assert_eq!(mrange, *asserts.get(i).unwrap());
        }
    }
}

/// arguments passed to the manga_dl cli
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[arg(required = true, short, long, num_args = 1..)]
    pub urls: Vec<MangaUrl>,
    #[clap(long, value_enum, default_value = "normal")]
    pub log: LogLevel,
    /// Downloads pages at only the indexes specified.
    /// Should be put inside the MangaUrl struct
    #[arg(long, num_args = 1..)]
    pub pages: Option<Vec<usize>>,
    /// Use an absolute path to download images to.
    /// Defaults to ./download if not specified.
    #[arg(short, long)]
    pub input_path: Option<String>,
}

/// manga_dl Url Argument
#[derive(Debug, Clone, PartialEq)]
pub struct MangaUrl {
    pub inner: String,
    pub title: Option<String>,
    pub site: SupportedSites,
    index: usize,
    /// Creates a range from two numbers to download through.  
    /// # EXAMPLE
    ///
    /// --range 112..125
    pub range: MangaUrlRange,
}

// struct RangePayload {
//     base_url: String,
//     range: String,
//     index: usize,
// }

#[derive(Debug, thiserror::Error)]
pub enum MangaUrlValidateErrorKind {
    #[error("invalid url: {s}\n  url must contain a number that indicates its chapter/volume")]
    InvalidUrl { s: String },
}

impl MangaUrl {
    /// remove the the range_str suffix from the url_str;
    ///
    /// # EXAMPLES
    /// ```
    /// let s = "https://mangafire.to/read/vagabondd.4mx/ja/chapter-33.html";
    /// let Some(caps) = REGEX_URL_SUFFIX_RANGE.captures(s) else {
    ///    return Err(MangaUrlValidateErrorKind::InvalidUrl { s: s.to_string() })?;
    /// };
    ///
    /// // the full match = "usize..usize"
    /// let range_match = caps.get(0).unwrap();
    /// let range_str = range_match.as_str().to_string();
    ///
    /// let index_match = caps.get(1).unwrap();
    /// let index = imatch.as_str().parse::<usize>().unwrap();
    ///
    /// // byte index
    /// let index_start = imatch.end();
    /// let range_end = range_match.end();
    ///
    /// // using the byte indexs, it removes "..match[2]"
    /// let inner = MangaUrl::strip_range(&user_url, index_start, range_end);
    /// ```
    fn strip_range(s: &str, start: usize, end: usize) -> String {
        let mut base_url = s.to_string();
        base_url.replace_range(start..end, "");
        base_url
    }
}

#[test]
fn strip_range() {
    let asserts = [
        "https://test.to/read99/onepunchman-47/chapter-117.html",
        "https://test.to/read/chapter-25/onepunchman",
    ];
    let urls = [
        "https://test.to/read99/onepunchman-47/chapter-117..120.html",
        "https://test.to/read/chapter-25..30/onepunchman",
    ];
    for (i, url) in urls.iter().enumerate() {
        let inner = match url_regex::REGEX_URL_RANGE.captures(url) {
            Some(mrange) => {
                let start = mrange.get(1).unwrap();
                let end = mrange.get(2).unwrap();
                MangaUrl::strip_range(url, start.end(), end.end())
            }
            None => {
                url_regex::REGEX_URL_INDEX.find(url).unwrap();
                url.to_string()
            }
        };

        assert_eq!(inner, *asserts.get(i).unwrap());
    }
}

impl FromStr for MangaUrl {
    type Err = Report;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let user_url = MangaUrl::validate_url(s.to_string())?;

        let (inner, index, range) = match url_regex::REGEX_URL_RANGE.captures(&user_url) {
            Some(mrange) => {
                let start = mrange.get(1).unwrap();
                let end = mrange.get(2).unwrap();
                let inner = MangaUrl::strip_range(&user_url, start.end(), end.end());
                let range_str = format!("{}..{}", start.as_str(), end.as_str());
                let range = MangaUrlRange::from_str(&range_str).unwrap();
                let index = start.as_str().parse::<usize>().unwrap();
                (inner, index, range)
            }
            None => {
                let Some(mindex) = url_regex::REGEX_URL_INDEX.captures(&user_url) else {
                    return Err(MangaUrlValidateErrorKind::InvalidUrl { s: user_url })?;
                };
                let index_str = mindex.iter().last().unwrap().unwrap().as_str();
                let index = index_str.parse::<usize>().context(eyre!(
                    "Failed to parse string.\n  Expected url's index, found: '{}'",
                    index_str
                ))?;
                let inner = user_url.clone();
                let range = MangaUrlRange::from_single_digit_range(index);
                (inner, index, range)
            }
        };

        let site = MangaUrl::is_site_supported(&inner)?;
        let title = MangaUrl::get_title(&inner, &site);

        let url = MangaUrl {
            inner,
            title,
            site,
            range,
            index,
        };
        Ok(url)
    }
}

//  try using the `strip_prefix` method:
//  `if let Some(<stripped>) = url.strip_prefix("https://") `, `<stripped>`
impl MangaUrl {
    /// Creates a [`Vec`] from [`Self::inner`] using [`Self::range`].
    ///
    /// # EXAMPLE
    ///
    /// ```
    /// let url = MangaUrl::from_str("read.com/c1..3");
    /// let expanded = url.as_vec();
    /// ```
    pub fn to_vec(&self) -> Vec<Self> {
        let range: &Range<usize> = self.range.deref();
        let inner = self.inner.clone();
        range
            .clone()
            .map(|i| {
                MangaUrl::from_str(&inner.replace(&self.index.to_string(), &i.to_string())).unwrap()
            })
            .collect::<Vec<Self>>()
    }

    /// Removes the leading 'https://' of a Url's inner
    pub fn split_protocol(url: &str) -> Option<(&str, &str)> {
        if let Some(stripped) = url.strip_prefix("https://") {
            return Some(("https://", stripped));
        }
        None
    }

    fn is_site_supported(url: &str) -> Result<SupportedSites, ArgError> {
        let url = MangaUrl::split_protocol(url).map_or(url, |(_, t)| t);
        let site_map = [
            ("mangareader", SupportedSites::MangaReader),
            ("mangagun", SupportedSites::MangaGun),
            ("rawmanga", SupportedSites::RawManga),
            ("mangafire", SupportedSites::MangaFire),
            ("test", SupportedSites::Test),
        ];
        site_map
            .into_iter()
            .find(|(key, _)| url.starts_with(key))
            .map(|(_, site)| site)
            .ok_or(ArgError::WebsiteNotSupported(url.to_string()))
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
            _ => {
                if let Some(start) = url.rsplit_once("/") {
                    return Some(start.1.to_string());
                }
            }
        }
        None
    }

    fn validate_url(url: String) -> Result<String> {
        let mut url = url;
        if !url.starts_with("https://") {
            url = format!("https://{}", url).to_lowercase();
        }

        if !url.is_ascii() {
            return Err(ArgError::InvalidUrl(InvalidUrlError {
                url,
                kind: InvalidUrlKind::Ascii,
            }))
            .with_suggestion(|| {
                format!("use valid characters: 'mangareader.to/read/onepiece/chapter-1'")
            })?;
        }

        if url.is_empty() {
            return Err(ArgError::InvalidUrl(InvalidUrlError {
                url,
                kind: InvalidUrlKind::EmptyString,
            }))
            .with_suggestion(|| format!("mangareader.to/read/onepiece/chapter-1"))?;
        }

        if !(url.contains(".to") || url.contains(".com") || url.contains(".net")) {
            return Err(ArgError::InvalidUrl(InvalidUrlError {
                url,
                kind: InvalidUrlKind::TopLevelDomain,
            }))
            .with_suggestion(|| format!("+++ *.com | *.to | *.net"))?;
        }

        Ok(url)
    }
}

#[cfg(test)]
mod t_manga_url {
    use super::*;
    use pretty_assertions::assert_eq;

    mod from_str {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn from_str() {
            let asserts = [
                MangaUrl {
                    inner: "https://test.to/read/vagabondd.4mx/ja/chapter-3".to_string(),
                    site: SupportedSites::Test,
                    title: Some("chapter-3".to_string()),
                    index: 3,
                    range: MangaUrlRange(3..4),
                },
                MangaUrl {
                    inner: "https://test.net/read-one-piece-raw-chapter-999.html".to_string(),
                    site: SupportedSites::Test,
                    title: Some("read-one-piece-raw-chapter-999.html".to_string()),
                    index: 999,
                    range: MangaUrlRange(999..1000),
                },
            ];
            let strs = [
                "https://test.to/read/vagabondd.4mx/ja/chapter-3",
                "test.net/read-one-piece-raw-chapter-999.html",
            ];
            for (i, str) in strs.iter().enumerate() {
                let url = MangaUrl::from_str(str).unwrap();
                assert_eq!(&url, asserts.get(i).unwrap());
            }
        }
        #[test]
        fn from_str_range() {
            let asserts = [
                MangaUrl {
                    inner: "https://mangafire.to/read/vagabondd.4mx/ja/chapter-3".to_string(),
                    site: SupportedSites::MangaFire,
                    title: Some("vagabondd.4mx-ja-chapter-3".to_string()),
                    index: 3,
                    range: MangaUrlRange(3..6),
                },
                MangaUrl {
                    inner: "https://mangagun.net/read-one-piece-raw-chapter-999.html".to_string(),
                    site: SupportedSites::MangaGun,
                    title: Some("read-one-piece-raw-chapter-999.html".to_string()),
                    index: 999,
                    range: MangaUrlRange(999..1002),
                },
            ];
            let strs = [
                "https://mangafire.to/read/vagabondd.4mx/ja/chapter-3..5",
                "mangagun.net/read-one-piece-raw-chapter-999..1001.html",
            ];
            for (i, str) in strs.iter().enumerate() {
                let url = MangaUrl::from_str(str).unwrap();
                assert_eq!(&url, asserts.get(i).unwrap());
            }
        }
    }
    mod to_vec {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn vec_from_single() {
            let url = MangaUrl::from_str("mangagun.net/read-one-piece-raw-chapter-999..1001.html")
                .unwrap();
            let vec = url.to_vec();
            let vec: Vec<&str> = vec.iter().map(|url| url.inner.as_str()).collect();
            assert_eq!(
                vec,
                [
                    "https://mangagun.net/read-one-piece-raw-chapter-999.html",
                    "https://mangagun.net/read-one-piece-raw-chapter-1000.html",
                    "https://mangagun.net/read-one-piece-raw-chapter-1001.html"
                ]
            );
        }
        #[test]
        fn vec_from_multiple() {
            let url = MangaUrl::from_str("test.com/ch-1..3").unwrap();
            let vec = url.to_vec();
            let vec: Vec<&str> = vec.iter().map(|url| url.inner.as_str()).collect();
            assert_eq!(
                vec,
                [
                    "https://test.com/ch-1",
                    "https://test.com/ch-2",
                    "https://test.com/ch-3"
                ],
            );
        }
    }

    #[test]
    fn strip_protocol() {
        let url = MangaUrl::from_str(
            "https://mangafire.to/read/one-piece-digital-colored-comicss.06w3/en/chapter-1",
        )
        .unwrap();
        let pc = MangaUrl::split_protocol(&url.inner);
        assert_eq!(
            pc,
            Some((
                "https://",
                "mangafire.to/read/one-piece-digital-colored-comicss.06w3/en/chapter-1"
            ))
        );
    }
}

/// Todo:
/// Try out mangaraw.ma
#[derive(Debug, Clone, Default, PartialEq)]
pub enum SupportedSites {
    #[default]
    MangaReader,
    MangaGun,
    /// https://rawmanga.net/manga/zaziyoziyoranzu-the-jojolands/di-1hua
    RawManga,
    MangaFire,
    Test,
}

impl Display for SupportedSites {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut display = |site: &str| write!(f, "{site}");
        match self {
            Self::MangaReader => display("MangaReader"),
            Self::MangaGun => display("MangaGun"),
            Self::RawManga => display("RawManga"),
            Self::MangaFire => display("MangaFire"),
            Self::Test => display("test"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error(
    "invalid --url argument: '{url}'.
  Reason: {kind}"
)]
pub struct InvalidUrlError {
    url: String,
    kind: InvalidUrlKind,
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidUrlKind {
    #[error("URL is missing or has an invalid top-level domain")]
    TopLevelDomain,
    #[error("An empty string was passed")]
    EmptyString,
    #[error("The url passed contained invalid ascii characters")]
    Ascii,
}

#[derive(thiserror::Error, Debug)]
pub enum ArgError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    InvalidUrl(#[from] InvalidUrlError),
    #[error("--url argument: {0} is not a supported site")]
    WebsiteNotSupported(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MangaUrlRange(Range<usize>);

impl MangaUrlRange {
    /// Only for urls that do NOT end with a range
    /// (ie. meaning the url must only end with a single index)
    /// # EXAMPLES
    ///
    /// ```
    /// let url = MangaUrl::from_str("ex.com/read/onepiece/ch-1");
    ///
    /// ```
    /// Creates a new [`MangaUrlRange`] from a starting index
    /// # EXAMPLES
    ///
    /// ```
    /// // the first one-piece chapter
    /// let url = MangaUrl::from_str("ex.com/read/onepiece/ch-1").unwrap();
    /// let range: &std::ops::Range = MangaUrlRange::from_index(url.index);
    /// assert_eq!(range, 1..2);
    /// ```
    fn from_single_digit_range(start: usize) -> Self {
        Self(start..start + 1)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("failed to create range: {kind}")]
pub struct MangaUrlRangeError {
    kind: MangaUrlRangeErrorKind,
}
#[derive(Debug, thiserror::Error)]
pub enum MangaUrlRangeErrorKind {
    #[error("cannot create a range without specifier: `..`")]
    MissingRangeSpecifier,
}

impl FromStr for MangaUrlRange {
    type Err = MangaUrlRangeError;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let (left, right) = s.split_once("..").ok_or(MangaUrlRangeError {
            kind: MangaUrlRangeErrorKind::MissingRangeSpecifier,
        })?;
        let (left, right) = (
            left.parse::<usize>().unwrap(),
            right.parse::<usize>().unwrap(),
        );
        let range = MangaUrlRange(left..right + 1);
        Ok(range)
    }
}
impl Deref for MangaUrlRange {
    type Target = Range<usize>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(clap::ValueEnum, Clone, Debug, PartialEq)]
pub enum LogLevel {
    Quiet,
    Normal,
    Verbose,
    Trace,
}
