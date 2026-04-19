use std::rc::Rc;

use glib::object::Cast;
use qobuz_player_controls::models::Artist;

use crate::ui::artist_detail_page::ArtistHeaderInfo;
use crate::ui::build_artist_tile;
use crate::ui::grid_page::GridPage;

pub type ArtistsPage = GridPage<Artist>;

pub fn new_artists_page(on_open: Rc<dyn Fn(ArtistHeaderInfo)>) -> ArtistsPage {
    let matches_query = Rc::new(|artist: &Artist, q: &str| artist.name.to_lowercase().contains(q));

    let build_tile = Rc::new(|artist: &Artist| build_artist_tile(artist).upcast());

    let on_activate = Rc::new(move |artist: &Artist| {
        on_open(ArtistHeaderInfo { id: artist.id });
    });

    GridPage::new(2, 10, matches_query, build_tile, on_activate)
}
