use gtk4::{Image, gdk, gio, prelude::*};
use libadwaita as adw;
use qobuz_player_controls::models::{AlbumSimple, Artist, PlaylistSimple};
use std::rc::Rc;

use crate::ui::{
    album_detail_page::AlbumHeaderInfo, artist_detail_page::ArtistHeaderInfo,
    playlist_detail_page::PlaylistHeaderInfo,
};

pub mod album_detail_page;
pub mod albums_page;
pub mod artist_detail_page;
pub mod artists_page;
pub mod library_page;
pub mod now_playing_bar;
pub mod playlist_detail_page;
pub mod playlists_page;
pub mod search_page;

pub fn set_image_from_url(url: Option<&str>, image: &Image) {
    if let Some(url) = url {
        let file = gio::File::for_uri(url);

        match gdk::Texture::from_file(&file) {
            Ok(texture) => {
                image.set_paintable(Some(&texture));
            }
            Err(err) => {
                eprintln!("Failed to load image: {err}");
                image.set_paintable(None::<&gdk::Paintable>);
            }
        }
    } else {
        image.set_paintable(None::<&gdk::Paintable>);
    }
}

pub fn build_album_tile(album: &AlbumSimple, on_open: Rc<dyn Fn(AlbumHeaderInfo)>) -> adw::Bin {
    let vbox = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(6)
        .build();

    let cover = gtk4::Image::builder().pixel_size(200).build();
    set_image_from_url(Some(&album.image), &cover);
    let cover_frame = gtk4::Frame::builder().child(&cover).build();
    cover_frame.add_css_class("card");

    let title = gtk4::Label::builder()
        .label(&album.title)
        .xalign(0.0)
        .wrap(true)
        .max_width_chars(20)
        .build();

    let artist = gtk4::Label::builder()
        .label(&album.artist.name)
        .xalign(0.0)
        .css_classes(vec![String::from("dim-label")])
        .wrap(true)
        .max_width_chars(20)
        .build();

    vbox.append(&cover_frame);
    vbox.append(&title);
    vbox.append(&artist);

    let info = AlbumHeaderInfo {
        id: album.id.clone(),
    };

    let click = gtk4::GestureClick::new();
    click.connect_pressed(move |_, _, _, _| {
        (on_open)(info.clone());
    });
    vbox.add_controller(click);

    adw::Bin::builder()
        .child(&vbox)
        .margin_end(12)
        .margin_bottom(12)
        .margin_top(12)
        .margin_start(12)
        .build()
}

pub fn build_playlist_tile(
    playlist: &PlaylistSimple,
    on_open: Rc<dyn Fn(PlaylistHeaderInfo)>,
) -> adw::Bin {
    let vbox = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(6)
        .build();

    let cover = gtk4::Image::builder().pixel_size(200).build();
    set_image_from_url(playlist.image.as_deref(), &cover);
    let cover_frame = gtk4::Frame::builder().child(&cover).build();
    cover_frame.add_css_class("card");

    let title = gtk4::Label::builder()
        .label(&playlist.title)
        .xalign(0.0)
        .wrap(true)
        .max_width_chars(20)
        .build();

    vbox.append(&cover_frame);
    vbox.append(&title);

    let info = PlaylistHeaderInfo { id: playlist.id };

    let click = gtk4::GestureClick::new();
    click.connect_pressed(move |_, _, _, _| {
        (on_open)(info.clone());
    });
    vbox.add_controller(click);

    adw::Bin::builder()
        .child(&vbox)
        .margin_end(12)
        .margin_bottom(12)
        .margin_top(12)
        .margin_start(12)
        .build()
}

pub fn build_artist_tile(artist: &Artist, on_open: Rc<dyn Fn(ArtistHeaderInfo)>) -> adw::Bin {
    let vbox = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(6)
        .build();

    let cover = gtk4::Image::builder().pixel_size(200).build();
    set_image_from_url(artist.image.as_deref(), &cover);
    let cover_frame = gtk4::Frame::builder().child(&cover).build();
    cover_frame.add_css_class("card");

    let title = gtk4::Label::builder()
        .label(&artist.name)
        .xalign(0.0)
        .wrap(true)
        .max_width_chars(20)
        .build();

    vbox.append(&cover_frame);
    vbox.append(&title);

    let info = ArtistHeaderInfo { id: artist.id };

    let click = gtk4::GestureClick::new();
    click.connect_pressed(move |_, _, _, _| {
        (on_open)(info.clone());
    });
    vbox.add_controller(click);

    adw::Bin::builder()
        .child(&vbox)
        .margin_end(12)
        .margin_bottom(12)
        .margin_top(12)
        .margin_start(12)
        .build()
}

pub fn format_time(seconds: u32) -> String {
    let m = seconds / 60;
    let s = seconds % 60;
    format!("{m}:{s:02}")
}
