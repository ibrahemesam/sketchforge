//! Native Wayland file drag-and-drop.
//!
//! winit 0.30 does not implement `WindowEvent::DroppedFile` on Wayland, so the
//! app would only accept drops when running under XWayland. This module speaks
//! `wl_data_device` on winit's own Wayland connection (both use the system
//! libwayland backend) and hands the dropped paths back to the app.
//!
//! The connection is adopted with `Backend::from_foreign_display`, so it never
//! owns or closes the display. Only the data device objects created here are
//! ours; the window surface is compared by id and never wrapped in a proxy,
//! because winit already manages it.

use std::collections::HashMap;
use std::io::Read;
use std::os::fd::AsFd;
use std::path::PathBuf;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    backend::ObjectId,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_data_device::{self, WlDataDevice},
        wl_data_device_manager::{DndAction, WlDataDeviceManager},
        wl_data_offer::{self, WlDataOffer},
        wl_registry::{self, WlRegistry},
        wl_seat::WlSeat,
        wl_surface::WlSurface,
    },
};
use winit::window::Window;

/// Mime types a file manager may use for a dragged file list.
const URI_LIST: &str = "text/uri-list";
const PLAIN: &str = "text/plain";

/// A completed drop: the surface-local position and the decoded files.
pub struct FileDrop {
    pub x: f64,
    pub y: f64,
    pub paths: Vec<PathBuf>,
}

#[derive(Default)]
struct State {
    surface: Option<ObjectId>,
    offers: HashMap<ObjectId, (WlDataOffer, Vec<String>)>,
    active: Option<Active>,
    dropped: Option<Dropped>,
    position: (f64, f64),
}

struct Active {
    offer: WlDataOffer,
    mime: String,
}

struct Dropped {
    offer: WlDataOffer,
    mime: String,
    x: f64,
    y: f64,
}

pub struct WaylandDnd {
    queue: EventQueue<State>,
    state: State,
    connection: Connection,
    _registry: WlRegistry,
    _seat: WlSeat,
    _manager: WlDataDeviceManager,
    _device: WlDataDevice,
}

impl WaylandDnd {
    /// Registers a data device for a winit Wayland window. Returns `None` on
    /// X11, or when the compositor has no `wl_data_device_manager`.
    pub fn new(window: &Window) -> Option<Self> {
        let display = match window.display_handle().ok()?.as_raw() {
            RawDisplayHandle::Wayland(display) => display.display.as_ptr(),
            other => {
                log::debug!("wayland dnd: not a Wayland display: {other:?}");
                return None;
            }
        };
        let surface = match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::Wayland(surface) => surface.surface.as_ptr(),
            other => {
                log::debug!("wayland dnd: not a Wayland surface: {other:?}");
                return None;
            }
        };

        // SAFETY: `window` owns the display and outlives this struct.
        let backend =
            unsafe { wayland_backend::sys::client::Backend::from_foreign_display(display.cast()) };
        let connection = Connection::from_backend(backend);
        let (globals, queue) = registry_queue_init::<State>(&connection)
            .inspect_err(|e| log::warn!("wayland dnd: cannot list globals: {e}"))
            .ok()?;
        let qh = queue.handle();
        let seat: WlSeat = match globals.bind(&qh, 1..=9, ()) {
            Ok(seat) => seat,
            Err(e) => {
                log::warn!("wayland dnd: cannot bind wl_seat: {e}");
                return None;
            }
        };
        let manager: WlDataDeviceManager = match globals.bind(&qh, 1..=3, ()) {
            Ok(manager) => manager,
            Err(e) => {
                log::warn!("wayland dnd: no wl_data_device_manager: {e}");
                return None;
            }
        };
        let device = manager.get_data_device(&seat, &qh, ());

        // SAFETY: `window` keeps the surface alive for the app's lifetime.
        let surface = match unsafe {
            wayland_backend::sys::client::ObjectId::from_ptr(
                <WlSurface as Proxy>::interface(),
                surface.cast(),
            )
        } {
            Ok(surface) => surface,
            Err(e) => {
                log::warn!("wayland dnd: cannot adopt the window surface: {e:?}");
                return None;
            }
        };

        log::info!("wayland dnd: ready on surface {surface:?}");
        Some(Self {
            queue,
            state: State {
                surface: Some(surface),
                ..State::default()
            },
            connection,
            _registry: globals.registry().clone(),
            _seat: seat,
            _manager: manager,
            _device: device,
        })
    }

    /// Pumps the data device for a completed drop. Call once per event-loop
    /// turn; returns `None` until a drop is read.
    pub fn poll(&mut self) -> Option<FileDrop> {
        // winit's event loop reads the shared display socket. Only dispatch the
        // queue for the objects created here; reading from this side as well
        // would consume wakeups winit expects.
        let _ = self.connection.flush();
        if let Err(e) = self.queue.dispatch_pending(&mut self.state) {
            log::warn!("wayland dnd: dispatch failed: {e}");
        }

        let dropped = self.state.dropped.take()?;
        let paths = self.receive(dropped.offer.clone(), &dropped.mime);
        self.forget(&dropped.offer);
        if paths.is_empty() {
            return None;
        }
        Some(FileDrop {
            x: dropped.x,
            y: dropped.y,
            paths,
        })
    }

    fn receive(&self, offer: WlDataOffer, mime: &str) -> Vec<PathBuf> {
        let (mut reader, writer) = match std::io::pipe() {
            Ok(p) => p,
            Err(e) => {
                log::warn!("wayland dnd: pipe failed: {e}");
                return Vec::new();
            }
        };
        offer.receive(mime.to_string(), writer.as_fd());
        drop(writer);
        // The receive request only leaves after a flush; the source writes as
        // soon as the compositor forwards it.
        let _ = self.connection.flush();
        let mut data = Vec::new();
        match reader.read_to_end(&mut data) {
            Ok(_) => {}
            Err(e) => log::warn!("wayland dnd: reading the drop failed: {e}"),
        }
        offer.finish();
        let text = String::from_utf8_lossy(&data);
        if mime == URI_LIST {
            parse_uri_list(&text)
        } else {
            parse_plain(&text)
        }
    }

    fn forget(&mut self, offer: &WlDataOffer) {
        if let Some((_, _)) = self.state.offers.remove(&offer.id()) {
            offer.destroy();
        }
    }
}

impl Drop for WaylandDnd {
    fn drop(&mut self) {
        if let Some(active) = self.state.active.take() {
            active.offer.destroy();
        }
        for (_, (offer, _)) in self.state.offers.drain() {
            offer.destroy();
        }
        let _ = self.connection.flush();
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _state: &mut Self,
        _registry: &WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(
        _state: &mut Self,
        _seat: &WlSeat,
        _event: wayland_client::protocol::wl_seat::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlDataDeviceManager, ()> for State {
    fn event(
        _state: &mut Self,
        _manager: &WlDataDeviceManager,
        _event: wayland_client::protocol::wl_data_device_manager::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlDataOffer, ()> for State {
    fn event(
        state: &mut Self,
        offer: &WlDataOffer,
        event: wl_data_offer::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_data_offer::Event::Offer { mime_type } = event
            && let Some((_, mimes)) = state.offers.get_mut(&offer.id())
        {
            mimes.push(mime_type);
        }
    }
}

impl Dispatch<WlDataDevice, ()> for State {
    fn event(
        state: &mut Self,
        _device: &WlDataDevice,
        event: wl_data_device::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::DataOffer { id } => {
                state.offers.insert(id.id(), (id, Vec::new()));
            }
            wl_data_device::Event::Enter {
                serial,
                surface,
                x,
                y,
                id,
            } => {
                state.position = (x, y);
                if state.surface.as_ref() != Some(&surface.id()) {
                    return;
                }
                let Some(id) = id else { return };
                let Some(mime) = state
                    .offers
                    .get(&id.id())
                    .and_then(|(_, mimes)| pick_mime(mimes))
                else {
                    return;
                };
                log::debug!("wayland dnd: enter at {x:.0},{y:.0} ({mime})");
                id.accept(serial, Some(mime.clone()));
                if id.version() >= 3 {
                    id.set_actions(DndAction::Copy, DndAction::Copy);
                }
                state.active = Some(Active { offer: id, mime });
            }
            wl_data_device::Event::Leave => {
                if let Some(active) = state.active.take() {
                    state.offers.remove(&active.offer.id());
                    active.offer.destroy();
                }
            }
            wl_data_device::Event::Motion { x, y, .. } => {
                if state.active.is_some() {
                    state.position = (x, y);
                }
            }
            wl_data_device::Event::Drop => {
                if let Some(active) = state.active.take() {
                    let (x, y) = state.position;
                    log::debug!("wayland dnd: drop at {x:.0},{y:.0} ({})", active.mime);
                    state.dropped = Some(Dropped {
                        offer: active.offer,
                        mime: active.mime,
                        x,
                        y,
                    });
                }
            }
            wl_data_device::Event::Selection { id: Some(id) } => {
                if let Some((_, mimes)) = state.offers.get(&id.id()) {
                    log::debug!("wayland dnd: selection offer {mimes:?}");
                }
                id.destroy();
            }
            wl_data_device::Event::Selection { id: None } => {}
            _ => {}
        }
    }

    wayland_client::event_created_child!(State, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, ())
    ]);
}

fn pick_mime(mimes: &[String]) -> Option<String> {
    mimes
        .iter()
        .find(|m| m.as_str() == URI_LIST || m.starts_with("text/uri-list;"))
        .or_else(|| {
            mimes
                .iter()
                .find(|m| m.as_str() == PLAIN || m.starts_with("text/plain;"))
        })
        .cloned()
}

/// Decode the `text/uri-list` format (RFC 2483) into local paths.
fn parse_uri_list(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(uri_to_path)
        .collect()
}

fn parse_plain(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // `file://host/path` is not local unless the host is empty or localhost.
    let path = if let Some(path) = rest.strip_prefix('/') {
        format!("/{path}")
    } else {
        let (host, path) = rest.split_once('/')?;
        if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
            return None;
        }
        format!("/{path}")
    };
    let bytes = percent_decode(path.as_bytes());
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Some(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
    }
    #[cfg(not(unix))]
    {
        String::from_utf8(bytes).ok().map(PathBuf::from)
    }
}

fn percent_decode(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] == b'%' && i + 2 < input.len() {
            let hi = (input[i + 1] as char).to_digit(16);
            let lo = (input[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(input[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_list_decodes_paths() {
        let text = "# comment\r\nfile:///tmp/a.png\r\nfile:///tmp/with%20space.jpg\r\n\r\n";
        assert_eq!(
            parse_uri_list(text),
            vec![
                PathBuf::from("/tmp/a.png"),
                PathBuf::from("/tmp/with space.jpg")
            ]
        );
    }

    #[test]
    fn uri_list_skips_remote_hosts() {
        assert!(parse_uri_list("file://example.com/a.png\n").is_empty());
        assert_eq!(
            parse_uri_list("file://localhost/tmp/x\n"),
            vec![PathBuf::from("/tmp/x")]
        );
    }

    #[test]
    fn percent_decodes_utf8() {
        assert_eq!(
            percent_decode(b"/tmp/caf%C3%A9.png"),
            "/tmp/café.png".as_bytes()
        );
    }

    #[test]
    fn picks_uri_list_first() {
        let mimes = vec!["text/plain".to_string(), "text/uri-list".to_string()];
        assert_eq!(pick_mime(&mimes).as_deref(), Some(URI_LIST));
    }
}
