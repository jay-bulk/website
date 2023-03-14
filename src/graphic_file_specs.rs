use ssg::{sources::FileSource, FileSpec};

use crate::url::Url;

pub(crate) fn get() -> [FileSpec; 5] {
    const FAVICON: [u8; 0] = [];
    let favicon = FileSpec::new("/favicon.ico", FileSource::Static(FAVICON.as_slice()));

    let twitter_logo = FileSpec::new(
        "/twitter_logo.svg",
        FileSource::Http(
            Url::parse("https://upload.wikimedia.org/wikipedia/commons/4/4f/Twitter-logo.svg")
                .unwrap()
                .to_inner()
                .clone(),
        ),
    );

    let zulip_logo = FileSpec::new("/zulip_logo.svg", 
        FileSource::Http(
            Url::parse("https://raw.githubusercontent.com/zulip/zulip/main/static/images/logo/zulip-icon-square.svg")
                .unwrap().to_inner().clone(),
        )
    );

    let inverticat_logo = FileSpec::new(
        "/inverticat.svg",
        FileSource::Http(
            Url::parse(
                "https://upload.wikimedia.org/wikipedia/commons/9/91/Octicons-mark-github.svg",
            )
            .unwrap()
            .to_inner()
            .clone(),
        ),
    );

    let youtube_logo = FileSpec::new("/youtube_logo.svg",
        FileSource::Http(
            Url::parse("https://upload.wikimedia.org/wikipedia/commons/0/09/YouTube_full-color_icon_%282017%29.svg")
                .unwrap().to_inner().clone(),
        )
    );

    [
        favicon,
        twitter_logo,
        zulip_logo,
        inverticat_logo,
        youtube_logo,
    ]
}