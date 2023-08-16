use std::collections::BTreeSet;

use chrono::DateTime;
use getset::Getters;
use itertools::Itertools;
use maud::{html, Markup, Render};

use ssg_child::sources::ExpectedFiles;

use crate::components::CalendarEvent;
use crate::constants::DESCRIPTION;
use crate::expected_files::ExpectedFilesExt;
use crate::relative_path::RelativePathBuf;
use crate::{
    components,
    html::Class,
    mob::{self, Mob, Person},
    style::{BUTTON_CLASSES, BUTTON_GAP},
};

use super::PageBase;

#[derive(Debug, Clone, Getters)]
pub(crate) struct HomePage {
    participants: BTreeSet<Person>,
    status_legend: mob::status::Legend,
    #[getset(get = "pub(crate)")]
    events: Vec<CalendarEvent>,
    base: PageBase,
    add_page_path: RelativePathBuf,
    fullcalendar_path: RelativePathBuf,
    rrule_path: RelativePathBuf,
    fullcalendar_rrule_path: RelativePathBuf,
}

impl HomePage {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        participants: BTreeSet<Person>,
        status_legend: mob::status::Legend,
        events: Vec<CalendarEvent>,
        base: PageBase,
        add_page_path: RelativePathBuf,
        fullcalendar_path: RelativePathBuf,
        rrule_path: RelativePathBuf,
        fullcalendar_rrule_path: RelativePathBuf,
    ) -> Self {
        Self {
            participants,
            status_legend,
            events,
            base,
            add_page_path,
            fullcalendar_path,
            rrule_path,
            fullcalendar_rrule_path,
        }
    }
}

impl Render for HomePage {
    fn render(&self) -> maud::Markup {
        let calendar = components::Calendar::new(
            self.events().clone(),
            Some(self.status_legend.clone()),
            self.fullcalendar_path.clone(),
            self.rrule_path.clone(),
            self.fullcalendar_rrule_path.clone(),
        );

        let content = html! {
            (calendar)
            div class=(classes!("flex", "flex-wrap", format!("gap-x-{BUTTON_GAP}"))) {
                a
                class=(*BUTTON_CLASSES)
                    href=(self.add_page_path)
                    { "Ads d your mob" }
            }
            div class=(classes!("flex", "flex-wrap")) {
                @for person in &self.participants {
                    @if let Some(avatar_url) = person.avatar_url() {
                        a href=(person.social_url()) class=(classes!("w-20")) {
                            img alt=(person.name()) src=(avatar_url);
                        }
                    }
                }
            }
        };

        let page = self.base.clone().into_page(
            None,
            content,
            classes!("flex", "flex-col", "gap-1"),
            components::page_base::PageDescription::from(DESCRIPTION.to_owned()),
        );

        page.render()
    }
}

pub(crate) fn event_content_template(
    _start: DateTime<rrule::Tz>,
    _end: DateTime<rrule::Tz>,
    mob: &Mob,
    expected_files: &mut ExpectedFiles,
) -> Markup {
    const OFFSET_VALUES: [i8; 2] = [-1, 1];
    let mob_id = mob.id();
    let path = expected_files.insert_(format!("/mobs/{mob_id}.html"));

    let indicator_text_shadow_value = OFFSET_VALUES
        .into_iter()
        .cartesian_product(OFFSET_VALUES)
        .map(|(offset_x, offset_y)| format!("{offset_x}px {offset_y}px 3px black"))
        .join(", ");

    let indicator_text_shadow_class: Class =
        format!("[text-shadow: {indicator_text_shadow_value}]")
            .replace(' ', "_")
            .try_into()
            .unwrap();

    let content = html! {
        a class=(classes!("no-underline", "block", "h-full")) href=(path) {
            (mob.title())

            @if let Some(status_indicator) = mob.status().indicator() {
                 " " span class=(indicator_text_shadow_class) { (status_indicator) }
            }
        }
    };

    content
}
