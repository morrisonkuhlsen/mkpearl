//! Files dragged onto a window. Added for Pearl, because winit 0.30's Wayland backend has no drag and
//! drop; see `vendor/README.md`. Reports the same events as the X11 backend does.
//!
//! Only copying is agreed to: a source told "move" may delete the file once the drop is finished.

use std::io::{ErrorKind, Read};

use sctk::data_device_manager::data_device::{DataDeviceData, DataDeviceHandler};
use sctk::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
use sctk::data_device_manager::data_source::DataSourceHandler;
use sctk::data_device_manager::WritePipe;
use sctk::reexports::calloop::PostAction;
use sctk::reexports::client::protocol::wl_data_device::WlDataDevice;
use sctk::reexports::client::protocol::wl_data_device_manager::DndAction;
use sctk::reexports::client::protocol::wl_data_source::WlDataSource;
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{Connection, Proxy, QueueHandle};
use tracing::warn;

use super::uri_list;
use crate::event::WindowEvent;
use crate::platform_impl::wayland::state::WinitState;
use crate::platform_impl::wayland::{make_wid, WindowId};

const URI_LIST: &str = "text/uri-list";

fn offer_of(device: &WlDataDevice) -> Option<DragOffer> {
    device.data::<DataDeviceData>()?.drag_offer()
}

fn offers_files(offer: &DragOffer) -> bool {
    offer.with_mime_types(|mimes| mimes.iter().any(|mime| mime == URI_LIST))
}

impl DataDeviceHandler for WinitState {
    fn enter(&mut self, conn: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice, _: f64, _: f64, surface: &WlSurface) {
        let Some(offer) = offer_of(device) else { return };
        let window_id = make_wid(surface);
        // The frame's subsurfaces aren't windows, and a drag of anything else isn't for us.
        if !self.windows.get_mut().contains_key(&window_id) || !offers_files(&offer) {
            offer.accept_mime_type(offer.serial, None);
            let _ = conn.flush();
            return;
        }
        offer.set_actions(DndAction::Copy, DndAction::Copy);
        offer.accept_mime_type(offer.serial, Some(URI_LIST.to_string()));
        // The drop may come at once, and must find the compositor knowing we accepted.
        let _ = conn.flush();
        self.drag_window = Some(window_id);
        // The list is read when the drop comes, for not every source sends it sooner, so a hover
        // names no file. All egui asks of it is that something is being carried.
        self.events_sink.push_window_event(WindowEvent::HoveredFile(Default::default()), window_id);
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
        if let Some(window_id) = self.drag_window.take() {
            self.events_sink.push_window_event(WindowEvent::HoveredFileCancelled, window_id);
        }
    }

    fn motion(&mut self, conn: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice, _: f64, _: f64) {
        // KWin wants the acceptance renewed. The actions aren't: that restarts the negotiation.
        if self.drag_window.is_some() {
            if let Some(offer) = offer_of(device) {
                offer.accept_mime_type(offer.serial, Some(URI_LIST.to_string()));
                let _ = conn.flush();
            }
        }
    }

    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn drop_performed(&mut self, conn: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice) {
        let Some(window_id) = self.drag_window.take() else { return };
        // A source that never sends its list must not leave the hint up, or block the window.
        self.events_sink.push_window_event(WindowEvent::HoveredFileCancelled, window_id);
        let Some(offer) = offer_of(device) else { return };
        self.read_dropped_files(conn, offer, window_id);
    }
}

impl WinitState {
    /// Reads the list as it arrives, between the event loop's other work.
    fn read_dropped_files(&mut self, conn: &Connection, offer: DragOffer, window_id: WindowId) {
        let pipe = match offer.receive(URI_LIST.to_string()) {
            Ok(pipe) => pipe,
            Err(error) => {
                warn!("Dropped files: {error}");
                offer.destroy();
                return;
            }
        };
        // The source hears of the request only when it leaves here.
        let _ = conn.flush();
        let reader_conn = conn.clone();
        let (finished, mut list) = (offer.clone(), Vec::new());
        let inserted = self.loop_handle.insert_source(pipe, move |(), file, state: &mut WinitState| {
            let mut chunk = [0; 8192];
            match (&**file).read(&mut chunk) {
                Ok(0) => {}
                Ok(read) if list.len() + read <= uri_list::MAX_BYTES => {
                    list.extend_from_slice(chunk.get(..read).unwrap_or_default());
                    return PostAction::Continue;
                }
                Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {
                    return PostAction::Continue;
                }
                _ => {
                    warn!("Dropped files: the list couldn't be read");
                    list.clear();
                }
            }
            for path in uri_list::files(&list) {
                state.events_sink.push_window_event(WindowEvent::DroppedFile(path), window_id);
            }
            finish(&reader_conn, &finished);
            PostAction::Remove
        });
        if inserted.is_err() {
            warn!("Dropped files: the list couldn't be waited for");
            finish(conn, &offer);
        }
    }
}

/// Finishing without an agreed action is a protocol error, which would end the connection. The offer
/// is as it was at the drop, the last the action could change.
fn finish(conn: &Connection, offer: &DragOffer) {
    if offer.selected_action == DndAction::Copy {
        offer.finish();
    }
    offer.destroy();
    let _ = conn.flush();
}

impl DataOfferHandler for WinitState {
    fn source_actions(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}

    fn selected_action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}
}

/// Nothing starts a drag from here; the data device's delegate asks for this all the same.
impl DataSourceHandler for WinitState {
    fn accept_mime(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: Option<String>) {}

    fn send_request(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: String, _: WritePipe) {}

    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}

sctk::delegate_data_device!(WinitState);
