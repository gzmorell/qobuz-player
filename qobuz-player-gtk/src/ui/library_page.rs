use std::{rc::Rc, sync::Arc};

use gtk4::prelude::*;
use libadwaita as adw;

use qobuz_player_controls::client::Client;

use crate::ui::{
    album_detail_page::AlbumHeaderInfo, albums_page::AlbumsPage,
    artist_detail_page::ArtistHeaderInfo, artists_page::ArtistsPage,
};

pub struct LibraryPage {
    root: gtk4::Box,
}

impl LibraryPage {
    pub fn new(
        client: Arc<Client>,
        on_open_album: Rc<dyn Fn(AlbumHeaderInfo)>,
        on_open_artist: Rc<dyn Fn(ArtistHeaderInfo)>,
    ) -> Self {
        let stack = adw::ViewStack::new();

        let albums_page = AlbumsPage::new(client.clone(), on_open_album);
        albums_page.load();

        let artists_page = ArtistsPage::new(client, on_open_artist);
        artists_page.load();

        stack.add_titled(albums_page.widget(), Some("albums"), "Albums");
        stack.add_titled(artists_page.widget(), Some("artists"), "Artists");

        let switcher = adw::InlineViewSwitcher::builder()
            .stack(&stack)
            .css_classes(["round"])
            .halign(gtk4::Align::Center)
            .build();

        let root = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(12)
            .vexpand(true)
            .hexpand(true)
            .build();

        root.append(&switcher);
        root.append(&stack);

        Self { root }
    }

    pub fn widget(&self) -> &gtk4::Box {
        &self.root
    }
}
