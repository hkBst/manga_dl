#[allow(unused_imports)]
use color_eyre::owo_colors::OwoColorize;

#[macro_export]
macro_rules! style_text {
    // Match URLs
    ($text:expr, url) => {
        ($text).white()
    };

    // Match file paths
    ($text:expr, path) => {
        ($text).white().bold().to_string()
    };

    // Match error messages
    ($text:expr, error) => {
        ($text).bright_red().to_string()
    };
    ($text:expr, severe) => {
        ($text).bright_red().bold().to_string()
    };

    // Default case or success messages
    ($text:expr, success) => {
        ($text).bright_green().to_string()
    };

    ($text:expr, bold) => {
        ($text).bold().to_string()
    };

    // Auto-detect pattern
    ($text:expr) => {{
        let text = $text.to_string();
        if text.contains(".to") && text.contains('/') {
            ($text).white().to_string()
        } else if text.contains('/') {
            ($text).white().bold().to_string()
        } else if text.to_lowercase().contains("error") {
            ($text).red().bold().to_string()
        } else {
            text
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_macro() {
        let text = [
            style_text!("mangareader.to/read/vagabond-4/ja/chapter-6"),
            style_text!("c:/users/arami/desktop"),
        ];

        for t in text {
            println!("{t}");
        }
    }
}
