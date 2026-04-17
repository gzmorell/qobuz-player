use std::{cell::RefCell, sync::Arc};

use gtk4::prelude::*;
use libadwaita as adw;

use qobuz_player_controls::{client::Client, controls::Controls};

use crate::ui::{format_time, set_image_from_url};

#[derive(Clone, Debug)]
pub struct AlbumHeaderInfo {
    pub id: String,
}

pub struct AlbumDetailPage {
    page: adw::NavigationPage,

    client: Arc<Client>,
    controls: Controls,
    album_id: String,

    stack: gtk4::Stack,

    cover: gtk4::Image,
    title: gtk4::Label,
    artist: gtk4::Label,
    meta: gtk4::Label,

    tracks_list: gtk4::ListBox,

    loaded: RefCell<bool>,
}

impl AlbumDetailPage {
    pub fn new(album_id: String, controls: Controls, client: Arc<Client>) -> Self {
        let empty_title = gtk4::Box::builder().hexpand(true).build();

        let nav_bar = adw::HeaderBar::builder().title_widget(&empty_title).build();

        let spinner = gtk4::Spinner::new();
        spinner.start();
        let spinner_box = gtk4::Box::builder()
            .vexpand(true)
            .hexpand(true)
            .halign(gtk4::Align::Center)
            .valign(gtk4::Align::Center)
            .build();
        spinner_box.append(&spinner);

        let cover = gtk4::Image::builder().pixel_size(400).build();

        let cover_frame = gtk4::Frame::builder().child(&cover).build();
        cover_frame.add_css_class("card");

        let title = gtk4::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .css_classes(vec!["title-1"])
            .build();

        let artist = gtk4::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .css_classes(vec!["title-3"])
            .build();

        let meta = gtk4::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .css_classes(vec!["dim-label"])
            .build();

        let play_button = gtk4::Button::builder()
            .label("Play")
            .icon_name("media-playback-start-symbolic")
            .css_classes(vec!["suggested-action", "pill"])
            .build();

        {
            let controls = controls.clone();
            let album_id = album_id.clone();
            play_button.connect_clicked(move |_| {
                controls.play_album(&album_id, 0);
            });
        }

        let header_text = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(12)
            .hexpand(true)
            .build();
        header_text.append(&title);
        header_text.append(&artist);
        header_text.append(&meta);

        play_button.set_halign(gtk4::Align::Start);
        header_text.append(&play_button);

        let header_section = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(18)
            .margin_top(18)
            .margin_bottom(18)
            .margin_start(18)
            .margin_end(18)
            .build();

        header_section.append(&cover_frame);
        header_section.append(&header_text);

        let tracks_list = gtk4::ListBox::builder()
            .selection_mode(gtk4::SelectionMode::None)
            .css_classes(vec!["boxed-list"])
            .margin_start(18)
            .margin_end(18)
            .margin_bottom(18)
            .build();

        let content = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .vexpand(true)
            .hexpand(true)
            .build();

        content.append(&header_section);
        content.append(&tracks_list);

        let clamp = adw::Clamp::builder()
            .maximum_size(900)
            .tightening_threshold(700)
            .child(&content)
            .build();

        let scroller = gtk4::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .child(&clamp)
            .build();

        let stack = gtk4::Stack::builder()
            .transition_type(gtk4::StackTransitionType::Crossfade)
            .build();
        stack.add_named(&spinner_box, Some("loading"));
        stack.add_named(&scroller, Some("content"));
        stack.set_visible_child_name("loading");

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&nav_bar);
        toolbar.set_content(Some(&stack));

        let page = adw::NavigationPage::builder()
            .title("Album")
            .child(&toolbar)
            .build();

        let s = Self {
            page,
            client,
            controls,
            album_id,
            stack,
            cover,
            title,
            artist,
            meta,
            tracks_list,
            loaded: RefCell::new(false),
        };

        s.load_album();

        s
    }

    pub fn page(&self) -> &adw::NavigationPage {
        &self.page
    }

    fn load_album(&self) {
        if *self.loaded.borrow() {
            return;
        }
        *self.loaded.borrow_mut() = true;

        let client = self.client.clone();
        let controls = self.controls.clone();
        let album_id = self.album_id.clone();

        let stack = self.stack.clone();
        let cover = self.cover.clone();
        let title = self.title.clone();
        let artist = self.artist.clone();
        let meta = self.meta.clone();
        let tracks_list = self.tracks_list.clone();

        stack.set_visible_child_name("loading");

        glib::spawn_future_local(async move {
            match client.album(&album_id).await {
                Ok(album) => {
                    title.set_label(&album.title);
                    artist.set_label(&album.artist.name);

                    let year_str = album.release_year.to_string();
                    let dur_str = format_time(album.duration_seconds);
                    meta.set_label(&format!("{year_str} • {dur_str}"));

                    set_image_from_url(Some(&album.image), &cover);

                    clear_listbox(&tracks_list);

                    for (idx, track) in album.tracks.iter().enumerate() {
                        let row =
                            build_track_row(track.number, &track.title, track.duration_seconds);

                        let controls = controls.clone();
                        let album_id = album_id.clone();
                        let click_index = idx as i32;

                        let click = gtk4::GestureClick::new();
                        click.connect_pressed(move |_, _, _, _| {
                            controls.play_album(&album_id, click_index as usize);
                        });

                        row.add_controller(click);
                        tracks_list.append(&row);
                    }

                    stack.set_visible_child_name("content");
                }
                Err(err) => {
                    eprintln!("Failed to load album {album_id}: {err}");

                    clear_listbox(&tracks_list);

                    let label = gtk4::Label::builder()
                        .label("Failed to load album.")
                        .xalign(0.0)
                        .margin_top(12)
                        .margin_bottom(12)
                        .margin_start(12)
                        .margin_end(12)
                        .css_classes(vec!["dim-label"])
                        .build();

                    let row = adw::ActionRow::builder().child(&label).build();
                    tracks_list.append(&row);

                    stack.set_visible_child_name("content");
                }
            }
        });
    }
}

fn clear_listbox(list: &gtk4::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn build_track_row(number: u32, title: &str, duration_secs: u32) -> gtk4::ListBoxRow {
    let number_label = gtk4::Label::builder()
        .label(format!("{number:>2}"))
        .xalign(0.0)
        .css_classes(vec!["dim-label"])
        .width_chars(3)
        .build();

    let title_label = gtk4::Label::builder()
        .label(title)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();

    let duration_label = gtk4::Label::builder()
        .label(format_time(duration_secs))
        .xalign(1.0)
        .css_classes(vec!["dim-label"])
        .build();

    let track_row_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(12)
        .margin_top(10)
        .margin_bottom(10)
        .margin_start(12)
        .margin_end(12)
        .build();

    track_row_box.append(&number_label);
    track_row_box.append(&title_label);
    track_row_box.append(&duration_label);

    gtk4::ListBoxRow::builder().child(&track_row_box).build()
}
