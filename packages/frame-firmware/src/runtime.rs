use anyhow::Context;
use frame_core::{
    AppState, ControlEvent, ControllerPhase, DeviceCommand, OutboundStatusMessage, RenderRequest,
    ScreenStatus, parse_control_message,
};
use frame_ui::{RenderedImage, clear_rendered_image, push_rendered_image, set_controller_phase};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tungstenite::client::IntoClientRequest;
use tungstenite::{Message, WebSocket};
use url::Url;

#[cfg(not(target_os = "espidf"))]
use reqwest::blocking::Client as BlockingHttpClient;

#[cfg(target_os = "espidf")]
use embedded_svc::http::{Method, client::Client as EmbeddedHttpClient};

#[cfg(target_os = "espidf")]
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};

const MESSAGE_POLL_INTERVAL: Duration = Duration::from_millis(250);

pub trait ControlPlaneConnection: Send {
    fn read_message(&mut self, timeout: Duration) -> anyhow::Result<Option<String>>;
    fn send_status(&mut self, message: &OutboundStatusMessage) -> anyhow::Result<()>;
}

pub trait ControlPlaneTransport: Send + 'static {
    type Connection: ControlPlaneConnection;

    fn connect(&mut self) -> anyhow::Result<Self::Connection>;

    fn label(&self) -> &'static str {
        "control-plane"
    }
}

pub trait RenderExecutor: Send + 'static {
    fn render(&mut self, request: &RenderRequest) -> anyhow::Result<()>;
    fn reload_ui(&mut self) -> anyhow::Result<()>;
    fn reboot(&mut self) -> anyhow::Result<()>;
}

#[derive(Clone, Debug)]
pub struct ExponentialBackoff {
    initial_delay: Duration,
    max_delay: Duration,
    current_delay: Duration,
}

impl ExponentialBackoff {
    pub fn new(initial_delay: Duration, max_delay: Duration) -> Self {
        debug_assert!(initial_delay <= max_delay);
        Self {
            initial_delay,
            max_delay,
            current_delay: initial_delay,
        }
    }

    pub fn next_delay(&mut self) -> Duration {
        let delay = self.current_delay;
        self.current_delay = self.max_delay.min(self.current_delay.saturating_mul(2));
        delay
    }

    pub fn reset(&mut self) {
        self.current_delay = self.initial_delay;
    }
}

enum TransportCommand {
    SendStatus(OutboundStatusMessage),
    Shutdown,
}

pub struct ThinClientRuntime {
    transport_tx: Sender<TransportCommand>,
    control_join: JoinHandle<()>,
    render_join: JoinHandle<()>,
}

/// Stack for the control-plane thread.
///
/// ESP-IDF gives pthreads 3 KB by default (`CONFIG_PTHREAD_TASK_STACK_SIZE_DEFAULT`),
/// which the WebSocket handshake blows straight through -- it faults in
/// `pthread` with a stack protection error before the first message is even
/// sent. These threads must therefore size their own stacks explicitly.
const CONTROL_PLANE_STACK_BYTES: usize = 16 * 1024;

/// Stack for the render thread, which additionally decodes a JPEG.
const RENDER_CONTROLLER_STACK_BYTES: usize = 32 * 1024;

/// Put these stacks in PSRAM.
///
/// Internal RAM is roughly 400 KB and is largely spoken for by Wi-Fi and the
/// display by the time these threads start, so allocating tens of KB there
/// fails with ENOMEM. The board has 32 MB of PSRAM and both
/// `CONFIG_SPIRAM_ALLOW_STACK_EXTERNAL_MEMORY` and
/// `CONFIG_FREERTOS_TASK_CREATE_ALLOW_EXT_MEM` are enabled, so external stacks
/// are supported; neither thread takes an interrupt, which is the case that
/// would rule PSRAM out.
#[cfg(target_os = "espidf")]
fn configure_thread_stacks_in_psram(stack_size: usize, name: &'static core::ffi::CStr) {
    use enumset::enum_set;
    use esp_idf_hal::task::thread::{MallocCap, ThreadSpawnConfiguration};

    let config = ThreadSpawnConfiguration {
        name: Some(name),
        stack_size,
        priority: 5,
        inherit: false,
        pin_to_core: None,
        stack_alloc_caps: enum_set!(MallocCap::Spiram | MallocCap::Cap8bit),
    };

    if let Err(error) = config.set() {
        tracing::warn!(
            target: "frame_firmware",
            "could not place thread stack in PSRAM ({error}); falling back to internal RAM"
        );
    }
}

#[cfg(not(target_os = "espidf"))]
fn configure_thread_stacks_in_psram(_stack_size: usize, _name: &'static core::ffi::CStr) {}

impl ThinClientRuntime {
    pub fn spawn<T, E>(transport: T, render_executor: E) -> Self
    where
        T: ControlPlaneTransport,
        E: RenderExecutor,
    {
        let (control_tx, control_rx) = mpsc::channel();
        let (transport_tx, transport_rx) = mpsc::channel();
        let render_status_tx = transport_tx.clone();

        configure_thread_stacks_in_psram(CONTROL_PLANE_STACK_BYTES, c"frame-control-plane");
        let control_join = thread::Builder::new()
            .name("frame-control-plane".to_string())
            .stack_size(CONTROL_PLANE_STACK_BYTES)
            .spawn(move || run_control_plane_loop(transport, control_tx, transport_rx))
            .expect("failed to spawn control plane thread");

        configure_thread_stacks_in_psram(RENDER_CONTROLLER_STACK_BYTES, c"frame-render");
        let render_join = thread::Builder::new()
            .name("frame-render-controller".to_string())
            .stack_size(RENDER_CONTROLLER_STACK_BYTES)
            .spawn(move || {
                run_render_controller_loop(render_executor, control_rx, render_status_tx)
            })
            .expect("failed to spawn render controller thread");

        Self {
            transport_tx,
            control_join,
            render_join,
        }
    }

    pub fn send_status(&self, message: OutboundStatusMessage) -> anyhow::Result<()> {
        self.status_sender().send_status(message)
    }

    /// A cloneable handle for reporting status from another thread.
    ///
    /// The runtime itself owns join handles and so cannot be cloned; background
    /// reporters only ever need the outbound queue.
    pub fn status_sender(&self) -> StatusSender {
        StatusSender {
            transport_tx: self.transport_tx.clone(),
        }
    }
}

/// Send-only view of the control plane, for background reporters.
#[derive(Clone)]
pub struct StatusSender {
    transport_tx: Sender<TransportCommand>,
}

impl StatusSender {
    pub fn send_status(&self, message: OutboundStatusMessage) -> anyhow::Result<()> {
        self.transport_tx
            .send(TransportCommand::SendStatus(message))
            .context("failed to queue outbound status message")
    }
}

impl Drop for ThinClientRuntime {
    fn drop(&mut self) {
        let _ = self.transport_tx.send(TransportCommand::Shutdown);
        let _ = self.control_join.thread().id();
        let _ = self.render_join.thread().id();
    }
}

pub fn record_render_request(app_state: &mut AppState, request: &RenderRequest) {
    app_state.set_active_media_url(request.media_url.clone());
    app_state.set_display_brightness(request.presentation.brightness);
    app_state.set_screen_status(ScreenStatus::Rendering);
}

pub fn record_render_complete(app_state: &mut AppState) {
    app_state.set_screen_status(ScreenStatus::Idle);
}

pub fn record_render_error(app_state: &mut AppState) {
    app_state.set_screen_status(ScreenStatus::Error);
}

pub struct LoggingRenderExecutor;

impl RenderExecutor for LoggingRenderExecutor {
    fn render(&mut self, request: &RenderRequest) -> anyhow::Result<()> {
        tracing::info!(
            target: "frame_firmware",
            media_url = request.media_url,
            correlation_id = request.correlation_id.as_deref().unwrap_or_default(),
            "received render request"
        );
        Ok(())
    }

    fn reload_ui(&mut self) -> anyhow::Result<()> {
        tracing::info!(target: "frame_firmware", "received reload_ui command");
        Ok(())
    }

    fn reboot(&mut self) -> anyhow::Result<()> {
        tracing::warn!(target: "frame_firmware", "received reboot command; no-op executor does not reboot the device yet");
        Ok(())
    }
}

pub struct MediaRenderExecutor;

/// Whether the owner's own photos, copied onto the SD card, are running the
/// slideshow.
///
/// While this is set the frame ignores photos from Home Assistant entirely --
/// the card wins outright, which is what makes the frame usable with no
/// network at all. Home Assistant is still told what is going on; it simply
/// does not get to draw.
static LOCAL_PHOTOS_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn set_local_photos_active(active: bool) {
    LOCAL_PHOTOS_ACTIVE.store(active, Ordering::Relaxed);
}

pub fn local_photos_active() -> bool {
    LOCAL_PHOTOS_ACTIVE.load(Ordering::Relaxed)
}

impl RenderExecutor for MediaRenderExecutor {
    fn render(&mut self, request: &RenderRequest) -> anyhow::Result<()> {
        if local_photos_active() {
            // Acknowledged rather than failed: Home Assistant did nothing
            // wrong, and an error here would show up as a broken frame on the
            // device page when the frame is working exactly as intended.
            tracing::debug!(
                target: "frame_firmware",
                media_url = request.media_url,
                "ignoring photo from Home Assistant: the SD card's media folder has photos in it"
            );
            return Ok(());
        }

        tracing::info!(
            target: "frame_firmware",
            media_url = request.media_url,
            correlation_id = request.correlation_id.as_deref().unwrap_or_default(),
            "downloading render asset"
        );

        let bytes = download_media(&request.media_url)
            .with_context(|| format!("failed to download media from {}", request.media_url))?;
        // Home Assistant already sized and encoded this for our exact panel, so
        // decoding is the only work left here (T033 moves it onto the P4's
        // hardware JPEG decoder).
        let decoded = image::load_from_memory(&bytes)
            .with_context(|| format!("failed to decode media from {}", request.media_url))?
            .into_rgb8();
        let (width, height) = decoded.dimensions();

        // Queue rather than replace: the frame keeps a few decoded photos ready
        // so a transition never waits on a download or a decode (FR-024).
        push_rendered_image(RenderedImage::from_rgb8(width, height, decoded.as_raw())?)
            .context("failed to publish rendered image to the UI")?;

        tracing::info!(
            target: "frame_firmware",
            media_url = request.media_url,
            width,
            height,
            "rendered image published to UI"
        );
        Ok(())
    }

    fn reload_ui(&mut self) -> anyhow::Result<()> {
        clear_rendered_image().context("failed to clear rendered image during reload_ui")
    }

    fn reboot(&mut self) -> anyhow::Result<()> {
        #[cfg(target_os = "espidf")]
        unsafe {
            esp_idf_sys::esp_restart();
        }

        #[cfg(not(target_os = "espidf"))]
        tracing::warn!(
            target: "frame_firmware",
            "received reboot command; reboot is a no-op on non-embedded targets"
        );

        Ok(())
    }
}

pub struct WebSocketControlPlaneTransport {
    endpoint: String,
}

impl WebSocketControlPlaneTransport {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
        }
    }
}

impl ControlPlaneTransport for WebSocketControlPlaneTransport {
    type Connection = WebSocketControlPlaneConnection;

    fn connect(&mut self) -> anyhow::Result<Self::Connection> {
        let endpoint = Url::parse(&self.endpoint)
            .with_context(|| format!("invalid control-plane URL {}", self.endpoint))?;
        anyhow::ensure!(
            endpoint.scheme() == "ws",
            "only plain ws:// endpoints are supported for the MVP control plane"
        );

        let host = endpoint
            .host_str()
            .context("control-plane URL did not include a host")?;
        let port = endpoint
            .port_or_known_default()
            .context("control-plane URL did not include a usable port")?;
        let address = format!("{host}:{port}");

        let stream = TcpStream::connect(&address)
            .with_context(|| format!("failed to open TCP connection to {address}"))?;
        stream
            .set_nodelay(true)
            .context("failed to enable TCP_NODELAY for control-plane socket")?;
        stream
            .set_read_timeout(Some(MESSAGE_POLL_INTERVAL))
            .context("failed to set control-plane socket read timeout")?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .context("failed to set control-plane socket write timeout")?;

        let request = endpoint
            .as_str()
            .into_client_request()
            .context("failed to build WebSocket client request")?;
        let (socket, _) = tungstenite::client(request, stream)
            .with_context(|| format!("failed to complete WebSocket handshake with {endpoint}"))?;

        Ok(WebSocketControlPlaneConnection { socket })
    }

    fn label(&self) -> &'static str {
        "websocket"
    }
}

pub struct WebSocketControlPlaneConnection {
    socket: WebSocket<TcpStream>,
}

impl ControlPlaneConnection for WebSocketControlPlaneConnection {
    fn read_message(&mut self, timeout: Duration) -> anyhow::Result<Option<String>> {
        self.socket
            .get_mut()
            .set_read_timeout(Some(timeout))
            .context("failed to update control-plane read timeout")?;

        loop {
            match self.socket.read() {
                Ok(Message::Text(message)) => return Ok(Some(message.to_string())),
                Ok(Message::Binary(_)) => {
                    tracing::debug!(
                        target: "frame_firmware",
                        "dropping unexpected binary control-plane message"
                    );
                }
                Ok(Message::Ping(payload)) => {
                    self.socket
                        .send(Message::Pong(payload))
                        .context("failed to respond to control-plane ping")?;
                }
                Ok(Message::Pong(_)) => {}
                Ok(Message::Close(frame)) => {
                    anyhow::bail!("control-plane closed connection: {frame:?}");
                }
                Err(tungstenite::Error::Io(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return Ok(None);
                }
                Err(error) => return Err(error).context("failed to read control-plane message"),
                _ => {}
            }
        }
    }

    fn send_status(&mut self, message: &OutboundStatusMessage) -> anyhow::Result<()> {
        let payload = serde_json::to_string(message).context("failed to encode status payload")?;
        self.socket
            .send(Message::Text(payload.into()))
            .context("failed to send control-plane status payload")
    }
}

#[derive(Default)]
pub struct NoopControlPlaneTransport;

impl ControlPlaneTransport for NoopControlPlaneTransport {
    type Connection = NoopControlPlaneConnection;

    fn connect(&mut self) -> anyhow::Result<Self::Connection> {
        tracing::info!(target: "frame_firmware", transport = self.label(), "starting placeholder control-plane transport");
        Ok(NoopControlPlaneConnection)
    }

    fn label(&self) -> &'static str {
        "noop"
    }
}

pub struct NoopControlPlaneConnection;

impl ControlPlaneConnection for NoopControlPlaneConnection {
    fn read_message(&mut self, timeout: Duration) -> anyhow::Result<Option<String>> {
        std::thread::sleep(timeout);
        Ok(None)
    }

    fn send_status(&mut self, message: &OutboundStatusMessage) -> anyhow::Result<()> {
        tracing::debug!(target: "frame_firmware", ?message, "dropping outbound status on placeholder transport");
        Ok(())
    }
}

#[cfg(not(target_os = "espidf"))]
fn download_media(url: &str) -> anyhow::Result<Vec<u8>> {
    let response = BlockingHttpClient::new()
        .get(url)
        .send()
        .with_context(|| format!("failed to issue GET request to {url}"))?;
    let status = response.status();

    if !status.is_success() {
        anyhow::bail!("media request to {url} failed with status {status}");
    }

    response
        .bytes()
        .context("failed to read media response body")
        .map(|bytes| bytes.to_vec())
}

#[cfg(target_os = "espidf")]
fn download_media(url: &str) -> anyhow::Result<Vec<u8>> {
    let http_config = HttpConfiguration {
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        ..Default::default()
    };
    let connection = EspHttpConnection::new(&http_config)
        .with_context(|| format!("failed to build HTTP client for {url}"))?;
    let mut client = EmbeddedHttpClient::wrap(connection);
    let request = client
        .request(Method::Get, url, &[])
        .with_context(|| format!("failed to open media request to {url}"))?;
    let mut response = request
        .submit()
        .with_context(|| format!("failed to send media request to {url}"))?;
    let status = response.status();
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 1024];

    loop {
        let read = response
            .read(&mut chunk)
            .with_context(|| format!("failed to read media response from {url}"))?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
    }

    if !(200..300).contains(&status) {
        anyhow::bail!("media request to {url} failed with status {status}");
    }

    Ok(bytes)
}

fn run_control_plane_loop<T>(
    mut transport: T,
    control_tx: Sender<ControlEvent>,
    transport_rx: Receiver<TransportCommand>,
) where
    T: ControlPlaneTransport,
{
    let mut backoff = ExponentialBackoff::new(Duration::from_secs(1), Duration::from_secs(30));

    loop {
        match transport.connect() {
            Ok(mut connection) => {
                let _ = set_controller_phase(ControllerPhase::AwaitingConfiguration);
                tracing::info!(target: "frame_firmware", transport = transport.label(), "control-plane connected");
                backoff.reset();

                loop {
                    match transport_rx.try_recv() {
                        Ok(TransportCommand::Shutdown) => return,
                        Ok(TransportCommand::SendStatus(message)) => {
                            if let Err(error) = connection.send_status(&message) {
                                tracing::warn!(target: "frame_firmware", transport = transport.label(), "failed to send outbound status: {error:#}");
                                break;
                            }
                        }
                        Err(TryRecvError::Disconnected) => return,
                        Err(TryRecvError::Empty) => {}
                    }

                    match connection.read_message(MESSAGE_POLL_INTERVAL) {
                        Ok(Some(payload)) => match parse_control_message(&payload) {
                            Ok(ControlEvent::Registration(registration)) => {
                                let phase = if registration.claimed {
                                    ControllerPhase::Connected
                                } else {
                                    ControllerPhase::AwaitingConfiguration
                                };
                                let _ = set_controller_phase(phase);
                            }
                            Ok(event) => {
                                let _ = set_controller_phase(ControllerPhase::Connected);
                                if control_tx.send(event).is_err() {
                                    return;
                                }
                            }
                            Err(error) => {
                                tracing::warn!(target: "frame_firmware", transport = transport.label(), "dropping invalid control payload: {error}");
                            }
                        },
                        Ok(None) => {}
                        Err(error) => {
                            let _ = set_controller_phase(ControllerPhase::Searching);
                            tracing::warn!(target: "frame_firmware", transport = transport.label(), "control-plane read failed: {error:#}");
                            break;
                        }
                    }
                }
            }
            Err(error) => {
                let _ = set_controller_phase(ControllerPhase::Searching);
                tracing::warn!(target: "frame_firmware", transport = transport.label(), "control-plane connect failed: {error:#}");
            }
        }

        let delay = backoff.next_delay();
        let _ = set_controller_phase(ControllerPhase::Searching);
        tracing::info!(target: "frame_firmware", transport = transport.label(), delay_ms = delay.as_millis() as u64, "retrying control-plane connection after backoff");

        match transport_rx.recv_timeout(delay) {
            Ok(TransportCommand::Shutdown) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(TransportCommand::SendStatus(_)) | Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

fn run_render_controller_loop<E>(
    mut render_executor: E,
    control_rx: Receiver<ControlEvent>,
    transport_tx: Sender<TransportCommand>,
) where
    E: RenderExecutor,
{
    while let Ok(event) = control_rx.recv() {
        match event {
            ControlEvent::Render(request) => {
                send_transport_status(
                    &transport_tx,
                    OutboundStatusMessage::RenderStarted {
                        media_url: request.media_url.clone(),
                        correlation_id: request.correlation_id.clone(),
                    },
                );

                match render_executor.render(&request) {
                    Ok(()) => send_transport_status(
                        &transport_tx,
                        OutboundStatusMessage::RenderCompleted {
                            media_url: request.media_url,
                            correlation_id: request.correlation_id,
                        },
                    ),
                    Err(error) => send_transport_status(
                        &transport_tx,
                        OutboundStatusMessage::Error {
                            message: format!("render failed: {error:#}"),
                            correlation_id: request.correlation_id,
                        },
                    ),
                }
            }
            ControlEvent::Command(request) => {
                let result = match request.command {
                    DeviceCommand::ReloadUi => render_executor.reload_ui(),
                    DeviceCommand::Reboot => render_executor.reboot(),
                };

                match result {
                    Ok(()) => send_transport_status(
                        &transport_tx,
                        OutboundStatusMessage::CommandAcknowledged {
                            cmd: request.command,
                            correlation_id: request.correlation_id,
                        },
                    ),
                    Err(error) => send_transport_status(
                        &transport_tx,
                        OutboundStatusMessage::Error {
                            message: format!("command failed: {error:#}"),
                            correlation_id: request.correlation_id,
                        },
                    ),
                }
            }
            ControlEvent::Registration(registration) => {
                // Adoption is handled on the control channel during the claim
                // handshake, not in the render loop. Log it so an unexpected
                // mid-session registration is visible on serial, and keep the
                // slideshow running either way (T072).
                tracing::info!(
                    target: "frame_firmware",
                    claimed = registration.claimed,
                    display_name = registration.display_name.as_deref().unwrap_or_default(),
                    "received controller registration on the render channel"
                );
            }
        }
    }
}

fn send_transport_status(transport_tx: &Sender<TransportCommand>, message: OutboundStatusMessage) {
    if transport_tx
        .send(TransportCommand::SendStatus(message))
        .is_err()
    {
        tracing::debug!(target: "frame_firmware", "dropping outbound status because transport channel is closed");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ControlPlaneConnection, ControlPlaneTransport, ExponentialBackoff, LoggingRenderExecutor,
        ThinClientRuntime,
    };
    use frame_core::{DeviceCommand, OutboundStatusMessage};
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[test]
    fn backoff_doubles_until_maximum() {
        let mut backoff = ExponentialBackoff::new(Duration::from_secs(1), Duration::from_secs(8));

        assert_eq!(backoff.next_delay(), Duration::from_secs(1));
        assert_eq!(backoff.next_delay(), Duration::from_secs(2));
        assert_eq!(backoff.next_delay(), Duration::from_secs(4));
        assert_eq!(backoff.next_delay(), Duration::from_secs(8));
        assert_eq!(backoff.next_delay(), Duration::from_secs(8));

        backoff.reset();
        assert_eq!(backoff.next_delay(), Duration::from_secs(1));
    }

    #[test]
    fn runtime_accepts_outbound_status() {
        let messages = Arc::new(Mutex::new(Vec::new()));
        let transport = ScriptedTransport::new(messages.clone());
        let runtime = ThinClientRuntime::spawn(transport, LoggingRenderExecutor);

        runtime
            .send_status(OutboundStatusMessage::CommandAcknowledged {
                cmd: DeviceCommand::ReloadUi,
                correlation_id: Some("cmd-1".to_string()),
            })
            .expect("status send should be accepted");

        std::thread::sleep(Duration::from_millis(50));

        assert!(!messages.lock().expect("messages lock poisoned").is_empty());
    }

    struct ScriptedTransport {
        messages: Arc<Mutex<Vec<OutboundStatusMessage>>>,
    }

    impl ScriptedTransport {
        fn new(messages: Arc<Mutex<Vec<OutboundStatusMessage>>>) -> Self {
            Self { messages }
        }
    }

    impl ControlPlaneTransport for ScriptedTransport {
        type Connection = ScriptedConnection;

        fn connect(&mut self) -> anyhow::Result<Self::Connection> {
            Ok(ScriptedConnection {
                inbox: VecDeque::new(),
                messages: self.messages.clone(),
            })
        }
    }

    struct ScriptedConnection {
        inbox: VecDeque<String>,
        messages: Arc<Mutex<Vec<OutboundStatusMessage>>>,
    }

    impl ControlPlaneConnection for ScriptedConnection {
        fn read_message(&mut self, _timeout: Duration) -> anyhow::Result<Option<String>> {
            Ok(self.inbox.pop_front())
        }

        fn send_status(&mut self, message: &OutboundStatusMessage) -> anyhow::Result<()> {
            self.messages
                .lock()
                .expect("messages lock poisoned")
                .push(message.clone());
            Ok(())
        }
    }
}
