use std::{cell::RefCell, rc::Rc, sync::Arc};

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use qobuz_player_controls::client::Client;

use crate::ui::{
    album_detail_page::AlbumHeaderInfo, albums_page::AlbumsPage,
    artist_detail_page::ArtistHeaderInfo, artists_page::ArtistsPage,
    playlist_detail_page::PlaylistHeaderInfo, playlists_page::PlaylistsPage,
};

pub struct LibraryPage {
    root: gtk4::Box,
}

impl LibraryPage {
    pub fn new(
        client: Arc<Client>,
        on_open_album: Rc<dyn Fn(AlbumHeaderInfo)>,
        on_open_artist: Rc<dyn Fn(ArtistHeaderInfo)>,
        on_open_playlist: Rc<dyn Fn(PlaylistHeaderInfo)>,
    ) -> Self {
        let albums_page = Rc::new(RefCell::new(AlbumsPage::new()));
        let artists_page = Rc::new(RefCell::new(ArtistsPage::new()));
        let playlists_page = Rc::new(RefCell::new(PlaylistsPage::new()));

        let stack = adw::ViewStack::new();

        stack.add_titled(albums_page.borrow().widget(), Some("albums"), "Albums");
        stack.add_titled(artists_page.borrow().widget(), Some("artists"), "Artists");
        stack.add_titled(
            playlists_page.borrow().widget(),
            Some("playlists"),
            "Playlists",
        );

        let switcher = adw::InlineViewSwitcher::builder()
            .stack(&stack)
            .css_classes(["round"])
            .halign(gtk4::Align::Center)
            .build();

        let search_entry = gtk4::SearchEntry::builder()
            .placeholder_text("Search…")
            .build();

        {
            let albums_page = albums_page.clone();
            let artists_page = artists_page.clone();
            let playlists_page = playlists_page.clone();

            search_entry.connect_changed(move |entry| {
                let query = entry.text();

                albums_page.borrow().filter(&query);
                artists_page.borrow().filter(&query);
                playlists_page.borrow().filter(&query);
            });
        }

        let top_box = gtk4::Box::builder()
            .halign(gtk4::Align::Center)
            .spacing(12)
            .build();
        top_box.append(&search_entry);
        top_box.append(&switcher);

        let spinner = gtk4::Spinner::new();
        spinner.start();
        spinner.set_visible(true);

        let spinner_box = gtk4::Box::builder()
            .halign(gtk4::Align::Center)
            .valign(gtk4::Align::Center)
            .build();
        spinner_box.append(&spinner);

        let overlay = gtk4::Overlay::new();
        overlay.set_vexpand(true);
        overlay.set_hexpand(true);
        overlay.set_child(Some(&stack));
        overlay.add_overlay(&spinner_box);

        let root = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(12)
            .vexpand(true)
            .hexpand(true)
            .build();

        root.append(&top_box);
        root.append(&overlay);

        glib::spawn_future_local({
            let albums_page = albums_page.clone();
            let artists_page = artists_page.clone();
            let playlists_page = playlists_page.clone();
            let spinner = spinner.clone();

            async move {
                match client.favorites().await {
                    Ok(favorites) => {
                        spinner.set_visible(false);
                        spinner.stop();

                        albums_page
                            .borrow_mut()
                            .load(favorites.albums, on_open_album);

                        artists_page
                            .borrow_mut()
                            .load(favorites.artists, on_open_artist);

                        playlists_page.borrow_mut().load(
                            favorites.playlists.into_iter().map(|x| x.into()).collect(),
                            on_open_playlist,
                        );
                    }
                    Err(err) => {
                        spinner.set_visible(false);
                        spinner.stop();
                        tracing::error!("{err}");
                    }
                }
            }
        });

        Self { root }
    }

    pub fn widget(&self) -> &gtk4::Box {
        &self.root
    }
}
