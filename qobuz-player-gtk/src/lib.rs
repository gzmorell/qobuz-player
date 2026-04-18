use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

use adw::{Application, prelude::*};
use glib::clone;
use libadwaita as adw;
use qobuz_player_controls::{
    PositionReceiver, Status, StatusReceiver, TracklistReceiver, client::Client,
    controls::Controls, tracklist::Tracklist,
};

use crate::ui::{
    album_detail_page::{AlbumDetailPage, AlbumHeaderInfo},
    artist_detail_page::{ArtistDetailPage, ArtistHeaderInfo},
    library_page::LibraryPage,
    now_playing_bar::{
        NowPlayingBar, build_now_playing_bar, update_now_playing, update_now_playing_button_icon,
        update_progress,
    },
    playlist_detail_page::{PlaylistDetailPage, PlaylistHeaderInfo},
    search_page::SearchPage,
};

mod ui;

pub fn init(
    client: Arc<Client>,
    tracklist_receiver: TracklistReceiver,
    status_receiver: StatusReceiver,
    position_receiver: PositionReceiver,
    controls: Controls,
) {
    libadwaita::init().unwrap();

    let application = libadwaita::Application::builder()
        .application_id("com.github.sofusa.qobuz-player")
        .build();

    application.connect_activate(move |app| {
        build_ui(
            app,
            tracklist_receiver.clone(),
            status_receiver.clone(),
            position_receiver.clone(),
            controls.clone(),
            client.clone(),
        );
    });

    let args: &[&str] = &[];
    application.run_with_args(args);
}

type OpenArtistDetailCallback = Rc<RefCell<Option<Rc<dyn Fn(ArtistHeaderInfo) + 'static>>>>;

fn build_ui(
    app: &Application,
    tracklist_receiver: TracklistReceiver,
    status_receiver: StatusReceiver,
    position_receiver: PositionReceiver,
    controls: Controls,
    client: Arc<Client>,
) {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Qobuz Player")
        .default_width(800)
        .default_height(1000)
        .build();

    let tabs = adw::ViewStack::builder().vexpand(true).build();

    let view_switcher = adw::ViewSwitcher::builder()
        .stack(&tabs)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();

    let header = adw::HeaderBar::builder()
        .title_widget(&view_switcher)
        .build();

    let root_toolbar = adw::ToolbarView::new();
    root_toolbar.add_top_bar(&header);
    root_toolbar.set_content(Some(&tabs));

    let root_page = adw::NavigationPage::builder()
        .title("Qobuz Player")
        .child(&root_toolbar)
        .build();

    let app_nav = adw::NavigationView::new();
    app_nav.add(&root_page);

    let on_open_album: Rc<dyn Fn(AlbumHeaderInfo)> = Rc::new(clone!(
        #[weak]
        app_nav,
        #[strong]
        controls,
        #[strong]
        client,
        move |info: AlbumHeaderInfo| {
            let detail = AlbumDetailPage::new(info.id, controls.clone(), client.clone());
            app_nav.push(detail.page());
        }
    ));

    let on_open_playlist: Rc<dyn Fn(PlaylistHeaderInfo)> = Rc::new(clone!(
        #[weak]
        app_nav,
        #[strong]
        controls,
        #[strong]
        client,
        move |info: PlaylistHeaderInfo| {
            let detail = PlaylistDetailPage::new(info.id, controls.clone(), client.clone());
            app_nav.push(detail.page());
        }
    ));

    let on_open_artist: OpenArtistDetailCallback = Rc::new(RefCell::new(None));

    let on_open_artist_clone = on_open_artist.clone();

    let controls_clone = controls.clone();
    let callback: Rc<dyn Fn(ArtistHeaderInfo)> = Rc::new(clone!(
        #[weak]
        app_nav,
        #[strong]
        client,
        #[strong]
        on_open_album,
        move |info: ArtistHeaderInfo| {
            let detail = ArtistDetailPage::new(
                info.id,
                controls_clone.clone(),
                client.clone(),
                on_open_album.clone(),
                on_open_artist_clone
                    .borrow()
                    .as_ref()
                    .expect("on_open_artist not initialized")
                    .clone(),
            );

            app_nav.push(detail.page());
        }
    ));

    *on_open_artist.borrow_mut() = Some(callback.clone());

    let library_page = LibraryPage::new(
        client.clone(),
        on_open_album.clone(),
        on_open_artist.borrow().as_ref().unwrap().clone(),
        on_open_playlist.clone(),
    );

    tabs.add_titled(library_page.widget(), Some("library"), "Library")
        .set_icon_name(Some("audio-x-generic-symbolic"));

    let search_page = SearchPage::new(client.clone(), on_open_album.clone());

    tabs.add_titled(search_page.widget(), Some("search"), "Search")
        .set_icon_name(Some("system-search-symbolic"));

    let now_playing = build_now_playing_bar(controls);

    let vbox = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .build();
    vbox.append(&app_nav);
    vbox.append(&now_playing.revealer);

    window.set_content(Some(&vbox));

    window.present();

    let tracklist_value = tracklist_receiver.borrow().clone();
    if let Some(track) = tracklist_value.current_track() {
        update_now_playing(&now_playing, track);
    }

    setup_tracklist_listener(
        tracklist_receiver,
        status_receiver,
        position_receiver,
        now_playing,
    );
}

fn setup_tracklist_listener(
    mut tracklist_receiver: TracklistReceiver,
    mut status_receiver: StatusReceiver,
    mut position_receiver: PositionReceiver,
    now_playing_bar: NowPlayingBar,
) {
    let (sender, receiver) = async_channel::unbounded::<UiEvent>();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                Ok(_) = tracklist_receiver.changed() => {
                    let tracklist = tracklist_receiver.borrow_and_update().clone();
                    sender.send(UiEvent::Tracklist(tracklist)).await.unwrap();
                }

                Ok(_) = status_receiver.changed() => {
                    let status = *status_receiver.borrow_and_update();
                    sender.send(UiEvent::Status(status)).await.unwrap();
                }

                Ok(_) = position_receiver.changed() => {
                    let position = *position_receiver.borrow_and_update();
                    sender.send(UiEvent::Position(position)).await.unwrap();
                }
            }
        }
    });

    glib::spawn_future_local(async move {
        loop {
            match receiver.recv().await {
                Ok(update) => match update {
                    UiEvent::Tracklist(tracklist) => {
                        if let Some(track) = tracklist.current_track() {
                            update_now_playing(&now_playing_bar, track);
                        }
                    }
                    UiEvent::Status(status) => {
                        update_now_playing_button_icon(&status, &now_playing_bar.play_button);
                    }
                    UiEvent::Position(duration) => {
                        update_progress(&now_playing_bar, &duration);
                    }
                },
                Err(err) => {
                    tracing::error!("{err}");
                }
            }
        }
    });
}

enum UiEvent {
    Tracklist(Tracklist),
    Status(Status),
    Position(Duration),
}
