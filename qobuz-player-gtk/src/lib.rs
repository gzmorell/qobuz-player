use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

use adw::{Application, prelude::*};
use async_channel::{Receiver, Sender};
use libadwaita as adw;
use qobuz_player_controls::{
    PositionReceiver, Status, StatusReceiver, TracklistReceiver, client::Client,
    controls::Controls, tracklist::Tracklist,
};

use crate::{
    callbacks::{CallbackHandles, build_callbacks},
    ui::{
        DetailPage,
        library_page::LibraryPage,
        now_playing_bar::{
            NowPlayingBar, update_now_playing, update_now_playing_button_icon, update_progress,
        },
        search_page::SearchPage,
    },
};

mod callbacks;
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

    let detail_pages: Rc<RefCell<Vec<Rc<dyn DetailPage>>>> = Rc::new(RefCell::new(Vec::new()));

    {
        let detail_pages = detail_pages.clone();
        app_nav.connect_popped(move |_nav, popped_page| {
            let popped_ptr = popped_page.as_ptr() as usize;

            detail_pages.borrow_mut().retain(|p| {
                let page_ptr = p.page().as_ptr() as usize;
                page_ptr != popped_ptr
            });
        });
    }

    let (sender, receiver) = async_channel::unbounded::<UiEvent>();

    let callback_handles = Rc::new(build_callbacks(
        app_nav.clone(),
        controls.clone(),
        client.clone(),
        detail_pages.clone(),
        tracklist_receiver.clone(),
        sender.clone(),
    ));

    let on_open_album = callback_handles.open_album.clone();
    let on_open_artist = callback_handles.open_artist.clone();
    let on_open_playlist = callback_handles.open_playlist.clone();

    let library_page = LibraryPage::new(
        client.clone(),
        on_open_album.clone(),
        on_open_artist.clone(),
        on_open_playlist.clone(),
    );

    tabs.add_titled(library_page.widget(), Some("library"), "Library")
        .set_icon_name(Some("audio-x-generic-symbolic"));

    let search_page = SearchPage::new(
        client.clone(),
        on_open_album.clone(),
        on_open_artist.clone(),
        on_open_playlist.clone(),
    );

    tabs.add_titled(search_page.widget(), Some("search"), "Search")
        .set_icon_name(Some("system-search-symbolic"));

    let now_playing = NowPlayingBar::new(
        controls,
        on_open_album.clone(),
        on_open_artist.clone(),
        on_open_playlist.clone(),
    );

    let vbox = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .build();
    vbox.append(&app_nav);
    vbox.append(&now_playing.revealer);

    window.set_content(Some(&vbox));

    window.present();

    let tracklist_value = tracklist_receiver.borrow().clone();
    update_now_playing(&now_playing, &tracklist_value);

    setup_tracklist_listener(
        sender,
        receiver,
        tracklist_receiver,
        status_receiver,
        position_receiver,
        now_playing,
        library_page,
        detail_pages,
        callback_handles,
    );
}

#[allow(clippy::too_many_arguments)]
fn setup_tracklist_listener(
    sender: Sender<UiEvent>,
    receiver: Receiver<UiEvent>,
    mut tracklist_receiver: TracklistReceiver,
    mut status_receiver: StatusReceiver,
    mut position_receiver: PositionReceiver,
    now_playing_bar: NowPlayingBar,
    library_page: LibraryPage,
    detail_pages: Rc<RefCell<Vec<Rc<dyn DetailPage>>>>,
    callback_handles: Rc<CallbackHandles>,
) {
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

    glib::MainContext::default().spawn_local(async move {
        let _keepalive = callback_handles;

        loop {
            match receiver.recv().await {
                Ok(update) => match update {
                    UiEvent::Tracklist(tracklist) => {
                        update_now_playing(&now_playing_bar, &tracklist);

                        if let Some(entity) = tracklist.current_playing_entity() {
                            for page in detail_pages.borrow().iter() {
                                page.update_current_playing(entity.clone());
                            }
                        }
                    }
                    UiEvent::Status(status) => {
                        update_now_playing_button_icon(&status, &now_playing_bar.play_button);
                    }
                    UiEvent::Position(duration) => {
                        update_progress(&now_playing_bar, &duration);
                    }
                    UiEvent::FavoritesChanged => {
                        library_page.reload();
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
    FavoritesChanged,
}
