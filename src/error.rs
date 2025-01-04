use base64::DecodeError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MainError {
    #[error("{0}")]
    IoErr(#[from] std::io::Error),
    #[error("{0}")]
    Fantoccini(#[from] fantoccini::error::CmdError),
    #[error("{0} could not be parsed")]
    ParseCounterElement(String),
    #[error("{0}")]
    Args(#[from] ArgError),
    #[error("{0}")]
    ColorEyre(#[from] color_eyre::Report),
    #[error("{0}")]
    JoinError(#[from] tokio::task::JoinError),
    #[error("{0}")]
    DownloadImage(#[from] DownloadImageError),
}

#[warn(dead_code)]
#[derive(Error, Debug)]
pub enum DownloadImageError {
    #[error("{0}")]
    ColorEyre(#[from] color_eyre::Report),
    #[error("{0}")]
    Base64(#[from] DecodeError),
    #[error("toDataUrl() executed on the canvas returned an invalid url: {0}")]
    InvalidDataUrl(String),
    #[error("{0}")]
    CanvasScript(String),
    #[error("{0}")]
    Fantoccini(#[from] fantoccini::error::CmdError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("get request failed for src url: {0}; reason: {1}")]
    GetReqwest(String, String),
    #[error("could not get canvas element from selector: {0}")]
    MissingCanvasElement(String),
    #[error("failed to find image: page number: {0};")]
    MissingImgElement(String),
}

#[derive(Error, Debug)]
pub enum MangaReaderError {
    #[error("{0}")]
    Fantoccini(#[from] fantoccini::error::CmdError),
    #[error("a new mangareader.to session didn't prompt with reading mode:\n {info}")]
    SelectReadingMode { info: String },
    #[error("{0}")]
    ColorEyre(#[from] color_eyre::Report),
}

#[derive(Error, Debug)]
pub enum ArgError {
    #[error("{0}")]
    Io(#[from] std::io::Error),

    #[error(
        "--url argument: {url} is not a valid url\n\
             Reason: {reason}\n\
             Example: {example}"
    )]
    InvalidUrl {
        url: String,
        reason: String,
        example: String,
    },

    #[error("--url argument: {0} is not a supported site.\n run with --help for a list of supported sites.")]
    WebsiteNotSupported(String),
}
