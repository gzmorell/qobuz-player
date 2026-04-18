use std::rc::Rc;

use gtk4::prelude::*;
use qobuz_player_controls::models::AlbumSimple;

use crate::ui::{album_detail_page::AlbumHeaderInfo, build_album_tile};

#[derive(Clone)]
pub struct AlbumsPage {
    widget: gtk4::ScrolledWindow,
    flow: gtk4::FlowBox,
    items: Vec<AlbumSimple>,
}

impl AlbumsPage {
    pub fn new() -> Self {
        let flow = gtk4::FlowBox::builder()
            .valign(gtk4::Align::Start)
            .halign(gtk4::Align::Center)
            .selection_mode(gtk4::SelectionMode::None)
            .row_spacing(12)
            .column_spacing(12)
            .build();

        let scroller = gtk4::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .child(&flow)
            .build();

        Self {
            widget: scroller,
            flow,
            items: Default::default(),
        }
    }

    pub fn widget(&self) -> &gtk4::ScrolledWindow {
        &self.widget
    }

    pub fn load(&mut self, albums: Vec<AlbumSimple>, on_open: Rc<dyn Fn(AlbumHeaderInfo)>) {
        self.clear_flowbox();

        for album in &albums {
            let tile = build_album_tile(album, on_open.clone());
            self.flow.insert(&tile, -1);
        }
        self.items = albums;

        self.flow.set_filter_func(|_| true);
        self.flow.invalidate_filter();
    }

    pub fn filter(&self, query: &str) {
        let query = query.trim().to_lowercase();

        if query.is_empty() {
            self.flow.set_filter_func(|_| true);
            self.flow.invalidate_filter();
            return;
        }

        let items = self.items.clone();

        self.flow.set_filter_func(move |child| {
            let index = child.index() as usize;

            let item = match items.get(index) {
                Some(item) => item,
                None => return false,
            };

            item.title.to_lowercase().contains(&query) || item.artist.name.contains(&query)
        });

        self.flow.invalidate_filter();
    }

    pub fn clear(&self) {
        self.clear_flowbox();
    }

    fn clear_flowbox(&self) {
        while let Some(child) = self.flow.first_child() {
            self.flow.remove(&child);
        }
    }
}
