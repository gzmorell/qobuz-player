use std::rc::Rc;
use std::sync::Arc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use qobuz_player_controls::client::Client;

use crate::ui::{
    album_detail_page::AlbumHeaderInfo, albums_page::AlbumsPage,
    artist_detail_page::ArtistHeaderInfo, artists_page::ArtistsPage,
    playlist_detail_page::PlaylistHeaderInfo, playlists_page::PlaylistsPage,
};

pub struct SearchPage {
    root: gtk4::Box,
}

impl SearchPage {
    pub fn new(
        client: Arc<Client>,
        on_open_album: Rc<dyn Fn(AlbumHeaderInfo)>,
        on_open_artist: Rc<dyn Fn(ArtistHeaderInfo)>,
        on_open_playlist: Rc<dyn Fn(PlaylistHeaderInfo)>,
    ) -> Self {
        let stack = adw::ViewStack::new();

        let albums_page = AlbumsPage::new();
        let artists_page = ArtistsPage::new();
        let playlists_page = PlaylistsPage::new();

        stack.add_titled(albums_page.widget(), Some("albums"), "Albums");
        stack.add_titled(artists_page.widget(), Some("artists"), "Artists");
        stack.add_titled(playlists_page.widget(), Some("playlists"), "Playlists");

        let switcher = adw::InlineViewSwitcher::builder()
            .stack(&stack)
            .css_classes(["round"])
            .halign(gtk4::Align::Center)
            .build();

        let search_entry = gtk4::SearchEntry::builder()
            .placeholder_text("Search…")
            .build();

        let top_box = gtk4::Box::builder()
            .halign(gtk4::Align::Center)
            .spacing(12)
            .build();
        top_box.append(&search_entry);
        top_box.append(&switcher);

        let spinner = gtk4::Spinner::new();
        spinner.start();
        spinner.set_visible(false);

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

        search_entry.connect_activate({
            let client = client.clone();
            let spinner = spinner.clone();

            move |entry| {
                let mut albums_page = albums_page.clone();
                let mut artists_page = artists_page.clone();
                let mut playlists_page = playlists_page.clone();

                let query = entry.text().to_string();
                if query.is_empty() {
                    return;
                }

                spinner.set_visible(true);
                spinner.start();

                albums_page.clear();
                artists_page.clear();
                playlists_page.clear();

                let client = client.clone();
                let on_open_album = on_open_album.clone();
                let on_open_artist = on_open_artist.clone();
                let on_open_playlist = on_open_playlist.clone();
                let spinner = spinner.clone();

                glib::MainContext::default().spawn_local(async move {
                    match client.search(query).await {
                        Ok(search) => {
                            let albums: Vec<_> =
                                search.albums.into_iter().map(|x| x.into()).collect();
                            albums_page.load(albums, on_open_album);

                            artists_page.load(search.artists, on_open_artist);

                            let playlists: Vec<_> =
                                search.playlists.into_iter().map(|x| x.into()).collect();
                            playlists_page.load(playlists, on_open_playlist);
                        }
                        Err(err) => {
                            tracing::error!("Search failed: {err}");
                        }
                    }

                    spinner.stop();
                    spinner.set_visible(false);
                });
            }
        });

        Self { root }
    }

    pub fn widget(&self) -> &gtk4::Box {
        &self.root
    }
}
