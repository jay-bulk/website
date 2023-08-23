use maud::{html, Render};

use crate::components;
use crate::constants::{DEFAULT_BRANCH, GITHUB_PULL_REQUESTS_URL, MOBS_DIR, NAME, REPO_URL};
use crate::style::{PROSE_CLASSES, VERTICAL_GAP_CLASS};

use super::schema::type_::Type;
use super::PageBase;

pub(crate) struct AddPage {
    internal_types: Vec<Type>,
    base: PageBase,
}

impl AddPage {
    pub(crate) fn new(internal_types: Vec<Type>, base: PageBase) -> Self {
        Self {
            internal_types,
            base,
        }
    }
}

impl Render for AddPage {
    fn render(&self) -> maud::Markup {
        let mut existing_mobs_url = REPO_URL.clone();

        existing_mobs_url
            .path_segments_mut()
            .unwrap()
            .push("tree")
            .push(DEFAULT_BRANCH)
            .push(MOBS_DIR);

        let content = html! {
            div class=(*PROSE_CLASSES) {
                h1 { "asdfAdd a mob" }
                p {
                    "Mobs are specified in files in "
                    a href=(existing_mobs_url) {
                        "the "
                        code { (MOBS_DIR) }
                        " directory."
                    }
                    " Take a look at some of those files for examples."
                }
                p {
                    "To add a mob, submit a "
                    a href=(GITHUB_PULL_REQUESTS_URL) { "pull request" }
                    " that adds a mob file."
                }
            }
            ol class=(classes!("flex", "flex-col", VERTICAL_GAP_CLASS)) {
                @for type_ in &self.internal_types {
                    li { (type_) }
                }
            }
        };

        self.base
            .clone()
            .into_page(
                Some("Add".to_owned().into()),
                content,
                classes!("flex", "flex-col", VERTICAL_GAP_CLASS),
                components::page_base::PageDescription::from(format!(
                    "How to add your mob to {NAME}",
                )),
            )
            .render()
    }
}
