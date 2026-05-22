//! Small bounded HTTP/1.1 server. Only literal local IPv4 Host headers are accepted.
//! The viewer is deliberately pull-based: there is no per-client frame queue.
use crate::{
    capture::{self, StreamConfig},
    pairing::Pairing,
};
use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream},
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Frame {
    pub sequence: u64,
    pub jpeg: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}
pub struct State {
    pub pairing: Pairing,
    pub stopped: bool,
    pub frame: Option<Frame>,
    pub error: Option<String>,
}
pub struct Shared {
    pub state: Mutex<State>,
    pub wake: Condvar,
    pub config: StreamConfig,
    repaint: Arc<dyn Fn() + Send + Sync>,
}
impl Shared {
    fn changed(&self) {
        self.wake.notify_all();
        (self.repaint)();
    }
    pub fn approve(&self) {
        self.state.lock().unwrap().pairing.approve(Instant::now());
        self.changed();
    }
    pub fn disconnect(&self) {
        let mut s = self.state.lock().unwrap();
        s.pairing.disconnect(Instant::now());
        s.frame = None;
        s.error = None;
        drop(s);
        self.changed();
    }
}

pub struct Host {
    pub address: SocketAddr,
    pub shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
    listener_wake: mio::Waker,
}
impl Host {
    pub fn start(
        address: SocketAddrV4,
        config: StreamConfig,
        repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, String> {
        config.validate()?;
        if !address.ip().is_private() && !address.ip().is_loopback() {
            return Err("Choose a private LAN IPv4 address or 127.0.0.1".into());
        }
        let listener = TcpListener::bind(address).map_err(|e| format!("Cannot listen: {e}"))?;
        let address = listener.local_addr().map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let mut listener = mio::net::TcpListener::from_std(listener);
        let mut poll = mio::Poll::new().map_err(|e| e.to_string())?;
        poll.registry()
            .register(&mut listener, mio::Token(0), mio::Interest::READABLE)
            .map_err(|e| e.to_string())?;
        let listener_wake =
            mio::Waker::new(poll.registry(), mio::Token(1)).map_err(|e| e.to_string())?;
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                pairing: Pairing::default(),
                stopped: false,
                frame: None,
                error: None,
            }),
            wake: Condvar::new(),
            config,
            repaint,
        });
        let (tx, rx) = crossbeam_channel::bounded::<TcpStream>(8);
        let mut threads = Vec::new();
        for _ in 0..4 {
            let rx = rx.clone();
            let shared = shared.clone();
            threads.push(thread::spawn(move || {
                while let Ok(mut socket) = rx.recv() {
                    if shared.state.lock().unwrap().stopped {
                        break;
                    }
                    let _ = socket.set_read_timeout(Some(Duration::from_secs(2)));
                    let _ = socket.set_write_timeout(Some(Duration::from_secs(2)));
                    let ip = socket
                        .peer_addr()
                        .map(|a| a.ip())
                        .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
                    let reply = match read_request(&mut socket) {
                        Ok(request) => route(request, ip, address, &shared),
                        Err(_) => Reply::text(400, "Malformed or oversized HTTP request"),
                    };
                    let _ = reply.write(&mut socket);
                }
            }));
        }
        let accept_shared = shared.clone();
        threads.push(thread::spawn(move || {
            let mut events = mio::Events::with_capacity(8);
            loop {
                if let Err(e) = poll.poll(&mut events, None) {
                    if e.kind() == std::io::ErrorKind::Interrupted {
                        continue;
                    }
                    let mut s = accept_shared.state.lock().unwrap();
                    s.error = Some(format!("Listener failed: {e}"));
                    s.stopped = true;
                    drop(s);
                    accept_shared.changed();
                    break;
                }
                if accept_shared.state.lock().unwrap().stopped {
                    break;
                }
                for event in &events {
                    if event.token() != mio::Token(0) {
                        continue;
                    }
                    loop {
                        match listener.accept() {
                            Ok((socket, _)) => {
                                let socket: TcpStream = socket.into();
                                if socket.set_nonblocking(false).is_ok() {
                                    let _ = tx.try_send(socket);
                                }
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(e) => {
                                let mut s = accept_shared.state.lock().unwrap();
                                s.error = Some(format!("Accept failed: {e}"));
                                drop(s);
                                accept_shared.changed();
                                break;
                            }
                        }
                    }
                }
            }
        }));
        let producer = shared.clone();
        threads.push(thread::spawn(move || produce(producer)));
        Ok(Self {
            address,
            shared,
            threads,
            listener_wake,
        })
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        {
            let mut s = self.shared.state.lock().unwrap();
            s.stopped = true;
            s.pairing.disconnect(Instant::now());
            s.frame = None;
        }
        self.shared.changed();
        // Wake the OS selector even when all loopback connections are prohibited.
        let _ = self.listener_wake.wake();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

fn produce(shared: Arc<Shared>) {
    let mut source = None;
    let mut source_token = String::new();
    let mut sequence = 0u64;
    loop {
        let mut s = shared.state.lock().unwrap();
        if s.stopped {
            return;
        }
        if s.pairing.expire(Instant::now()) {
            s.frame = None;
            s.error = None;
            (shared.repaint)();
        }
        let token = s
            .pairing
            .session
            .as_ref()
            .filter(|p| p.approved)
            .map(|p| p.token.clone());
        let Some(token) = token else {
            // Virtual source Drop removes its monitor through the driver. Never do
            // that I/O while holding the UI/status mutex, and recheck before waiting.
            drop(s);
            source = None;
            source_token.clear();
            let s = shared.state.lock().unwrap();
            if s.stopped || s.pairing.session.as_ref().is_some_and(|p| p.approved) {
                continue;
            }
            // Wait indefinitely with no peer; time out only while a pending peer may expire.
            if s.pairing.session.is_some() {
                drop(shared.wake.wait_timeout(s, Duration::from_secs(1)).unwrap());
            } else {
                drop(shared.wake.wait(s).unwrap());
            }
            continue;
        };
        if s.error.is_some() {
            drop(shared.wake.wait_timeout(s, Duration::from_secs(1)).unwrap());
            continue;
        }
        drop(s);
        let started = Instant::now();
        let result = (|| {
            if source_token != token {
                source = Some(capture::open(&shared.config)?);
                source_token = token.clone();
            }
            let pixels = match source.as_mut().unwrap().poll()? {
                capture::FramePoll::Frame(pixels) => pixels,
                capture::FramePoll::Unchanged => return Ok((None, false)),
                capture::FramePoll::Inactive => return Ok((None, true)),
            };
            let (width, height) = (pixels.width(), pixels.height());
            let jpeg = capture::encode(&pixels, shared.config.quality)?;
            if jpeg.len() > 8 * 1024 * 1024 {
                return Err("Encoded frame exceeds 8 MiB limit".to_string());
            }
            sequence += 1;
            Ok((
                Some(Frame {
                    sequence,
                    jpeg: Arc::new(jpeg),
                    width,
                    height,
                }),
                false,
            ))
        })();
        if result.is_err() {
            source = None;
            source_token.clear();
        }
        let mut s = shared.state.lock().unwrap();
        let had_frame = s.frame.is_some();
        let previous_error = s.error.clone();
        // A disconnected/replaced peer must never receive an in-flight old capture.
        if s.pairing
            .session
            .as_ref()
            .is_some_and(|p| p.approved && p.token == token)
            && !s.stopped
        {
            match result {
                Ok((Some(frame), _)) => s.frame = Some(frame),
                Ok((None, true)) => s.frame = None,
                Ok((None, false)) => (),
                Err(e) => {
                    s.error = Some(e);
                    s.frame = None;
                }
            }
        }
        if had_frame != s.frame.is_some() || previous_error != s.error {
            (shared.repaint)();
        }
        let delay = Duration::from_secs_f64(1.0 / shared.config.fps as f64)
            .saturating_sub(started.elapsed());
        drop(shared.wake.wait_timeout(s, delay).unwrap());
    }
}

#[derive(Debug)]
struct Request {
    method: String,
    path: String,
    host: String,
    origin: Option<String>,
    authorization: String,
    content_type: String,
    body: Vec<u8>,
}
fn read_request(reader: &mut impl Read) -> Result<Request, String> {
    let started = Instant::now();
    let mut data = Vec::new();
    let (header_len, request, body_len) = loop {
        if started.elapsed() > Duration::from_secs(3) {
            return Err("Request deadline".into());
        }
        let mut headers = [httparse::EMPTY_HEADER; 32];
        let mut parsed = httparse::Request::new(&mut headers);
        if let httparse::Status::Complete(len) = parsed.parse(&data).map_err(|e| e.to_string())? {
            if len > 8192 {
                return Err("Headers too large".into());
            }
            let mut request = Request {
                method: parsed.method.unwrap_or("").into(),
                path: parsed.path.unwrap_or("").into(),
                host: String::new(),
                origin: None,
                authorization: String::new(),
                content_type: String::new(),
                body: Vec::new(),
            };
            let mut body_len = None;
            let mut names = std::collections::HashSet::new();
            for h in parsed.headers.iter() {
                let name = h.name.to_ascii_lowercase();
                if !names.insert(name.clone()) {
                    return Err("Duplicate headers".into());
                }
                let value = std::str::from_utf8(h.value)
                    .map_err(|e| e.to_string())?
                    .trim();
                match name.as_str() {
                    "host" => request.host = value.into(),
                    "origin" => request.origin = Some(value.into()),
                    "authorization" => request.authorization = value.into(),
                    "content-type" => request.content_type = value.into(),
                    "content-length" => {
                        body_len = Some(value.parse::<usize>().map_err(|e| e.to_string())?)
                    }
                    "transfer-encoding" => return Err("Transfer encoding unsupported".into()),
                    _ => (),
                }
            }
            let body_len = body_len.unwrap_or(0);
            if body_len > 1024 {
                return Err("Body too large".into());
            }
            break (len, request, body_len);
        }
        if data.len() >= 8192 {
            return Err("Headers too large".into());
        }
        let mut buf = [0; 1024];
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("Unexpected EOF".into());
        }
        data.extend_from_slice(&buf[..n]);
    };
    let mut request = request;
    while data.len() < header_len + body_len {
        if started.elapsed() > Duration::from_secs(3) {
            return Err("Request deadline".into());
        }
        let mut buf = [0; 1024];
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("Unexpected EOF".into());
        }
        data.extend_from_slice(&buf[..n]);
    }
    request.body = data[header_len..header_len + body_len].to_vec();
    Ok(request)
}

struct Reply {
    status: u16,
    mime: &'static str,
    body: Arc<Vec<u8>>,
    extra: String,
}
impl Reply {
    fn bytes(status: u16, mime: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            mime,
            body: Arc::new(body.into()),
            extra: String::new(),
        }
    }
    fn text(status: u16, body: &str) -> Self {
        Self::bytes(status, "text/plain; charset=utf-8", body.as_bytes())
    }
    fn write(&self, socket: &mut impl Write) -> std::io::Result<()> {
        write!(
            socket,
            "HTTP/1.1 {} Response\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' blob:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'\r\n{}\r\n",
            self.status,
            self.mime,
            self.body.len(),
            self.extra
        )?;
        socket.write_all(&self.body)
    }
}

fn route(request: Request, ip: IpAddr, address: SocketAddr, shared: &Shared) -> Reply {
    let expected = address.to_string();
    if request.host != expected
        || request
            .origin
            .as_ref()
            .is_some_and(|v| v != &format!("http://{expected}"))
    {
        return Reply::text(403, "Use the exact host address shown in SecondScreen");
    }
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => {
            return Reply::bytes(
                200,
                "text/html; charset=utf-8",
                include_bytes!("../viewer/index.html").as_slice(),
            );
        }
        ("GET", "/viewer.js") => {
            return Reply::bytes(
                200,
                "text/javascript; charset=utf-8",
                include_bytes!("../viewer/viewer.js").as_slice(),
            );
        }
        ("GET", "/style.css") => {
            return Reply::bytes(
                200,
                "text/css; charset=utf-8",
                include_bytes!("../viewer/style.css").as_slice(),
            );
        }
        ("POST", "/api/pair") => {
            if request.content_type != "application/json" {
                return Reply::text(415, "JSON required");
            }
            #[derive(serde::Deserialize)]
            struct Pair {
                code: String,
            }
            let Ok(pair) = serde_json::from_slice::<Pair>(&request.body) else {
                return Reply::text(400, "Invalid pairing request");
            };
            let mut s = shared.state.lock().unwrap();
            let result = s.pairing.pair(&pair.code, ip, Instant::now());
            if result.is_ok() {
                s.frame = None;
                s.error = None;
            }
            drop(s);
            shared.changed();
            return match result {
                Ok(token) => Reply::bytes(
                    200,
                    "application/json",
                    serde_json::to_vec(&serde_json::json!({"token":token})).unwrap(),
                ),
                Err(e) => Reply::text(403, e),
            };
        }
        _ => (),
    }
    let Some(token) = request.authorization.strip_prefix("Bearer ") else {
        return Reply::text(401, "Pair this viewer first");
    };
    let mut s = shared.state.lock().unwrap();
    if s.pairing.expire(Instant::now()) {
        s.frame = None;
        s.error = None;
        shared.changed();
    }
    let Some(approved) = s.pairing.authenticate(token, ip, Instant::now()) else {
        return Reply::text(401, "Pairing expired or disconnected");
    };
    match (request.method.as_str(), request.path.split('?').next().unwrap_or("")) {
        ("POST", "/api/disconnect") => {
            s.pairing.disconnect(Instant::now()); s.frame = None; s.error = None;
            drop(s); shared.changed(); Reply::text(200, "Disconnected")
        }
        ("GET", "/api/state") => Reply::bytes(200, "application/json", serde_json::to_vec(&serde_json::json!({"approved": approved, "error": s.error, "fps":shared.config.fps, "mode":shared.config.source.mode_label(), "source":shared.config.source, "hasFrame":s.frame.is_some()})).unwrap()),
        ("GET", "/api/frame") if approved => {
            if let Some(e) = &s.error { return Reply::text(503, e); }
            let Some(frame) = &s.frame else { return Reply::text(204, ""); };
            let after = request.path.split_once("?after=").and_then(|(_, n)| n.parse::<u64>().ok()).unwrap_or(0);
            if frame.sequence <= after { return Reply::text(204, ""); }
            Reply { status: 200, mime: "image/jpeg", body: frame.jpeg.clone(), extra: format!("X-Frame-Sequence: {}\r\nX-Frame-Width: {}\r\nX-Frame-Height: {}\r\n", frame.sequence, frame.width, frame.height) }
        }
        ("GET", "/api/frame") => Reply::text(403, "Approve the viewer on the host first"),
        _ => Reply::text(404, "Not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn synthetic_shared() -> Arc<Shared> {
        Arc::new(Shared {
            state: Mutex::new(State {
                pairing: Pairing::default(),
                stopped: false,
                frame: None,
                error: None,
            }),
            wake: Condvar::new(),
            config: StreamConfig {
                source: capture::SourceKind::Synthetic,
                max_width: 320,
                max_height: 240,
                ..Default::default()
            },
            repaint: Arc::new(|| {}),
        })
    }
    struct ProducerFixture {
        shared: Arc<Shared>,
        thread: Option<JoinHandle<()>>,
    }
    impl Drop for ProducerFixture {
        fn drop(&mut self) {
            self.shared.state.lock().unwrap().stopped = true;
            self.shared.changed();
            self.thread.take().unwrap().join().unwrap();
        }
    }
    #[test]
    fn steady_synthetic_stream_does_not_repaint_host_for_each_frame() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let counter = Arc::new(AtomicUsize::new(0));
        let mut state = Arc::try_unwrap(synthetic_shared()).ok().unwrap();
        let repaint_counter = counter.clone();
        state.repaint = Arc::new(move || {
            repaint_counter.fetch_add(1, Ordering::Relaxed);
        });
        let shared = Arc::new(state);
        {
            let mut s = shared.state.lock().unwrap();
            let code = s.pairing.code.clone();
            s.pairing
                .pair(&code, "127.0.0.1".parse().unwrap(), Instant::now())
                .unwrap();
            s.pairing.approve(Instant::now());
        }
        let producer = shared.clone();
        let _fixture = ProducerFixture {
            shared: shared.clone(),
            thread: Some(thread::spawn(move || produce(producer))),
        };
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let sequence = shared
                .state
                .lock()
                .unwrap()
                .frame
                .as_ref()
                .map_or(0, |f| f.sequence);
            if sequence >= 3 {
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            counter.load(Ordering::Relaxed),
            1,
            "Only the first visible frame changes host status"
        );
    }
    #[test]
    fn memory_transport_pairs_approves_decodes_and_revokes_generated_frames() {
        let shared = synthetic_shared();
        let source = shared.clone();
        let _fixture = ProducerFixture {
            shared: shared.clone(),
            thread: Some(thread::spawn(move || produce(source))),
        };
        let address = "127.0.0.1:7420".parse().unwrap();
        let ip = "127.0.0.1".parse().unwrap();
        let exchange = |method: &str,
                        path: &str,
                        token: &str,
                        body: &str,
                        host: &str,
                        origin: &str| {
            let raw = format!(
                "{method} {path} HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            let request = read_request(&mut raw.as_bytes()).unwrap();
            let reply = route(request, ip, address, &shared);
            let mut bytes = Vec::new();
            reply.write(&mut bytes).unwrap();
            bytes
        };
        let request = |method: &str, path: &str, token: &str, body: &str| {
            exchange(
                method,
                path,
                token,
                body,
                "127.0.0.1:7420",
                "http://127.0.0.1:7420",
            )
        };
        assert!(
            exchange(
                "GET",
                "/",
                "",
                "",
                "attacker.invalid:7420",
                "http://attacker.invalid:7420"
            )
            .starts_with(b"HTTP/1.1 403")
        );
        assert!(
            exchange(
                "GET",
                "/",
                "",
                "",
                "127.0.0.1:7420",
                "http://attacker.invalid"
            )
            .starts_with(b"HTTP/1.1 403")
        );
        assert!(request("GET", "/api/frame", "bad", "").starts_with(b"HTTP/1.1 401"));
        assert!(shared.state.lock().unwrap().frame.is_none());
        let code = shared.state.lock().unwrap().pairing.code.clone();
        let pair = request("POST", "/api/pair", "", &format!("{{\"code\":\"{code}\"}}"));
        let split = |bytes: &[u8]| bytes.windows(4).position(|p| p == b"\r\n\r\n").unwrap() + 4;
        let value: serde_json::Value = serde_json::from_slice(&pair[split(&pair)..]).unwrap();
        let token = value["token"].as_str().unwrap();
        assert!(request("GET", "/api/frame", token, "").starts_with(b"HTTP/1.1 403"));
        shared.approve();
        let deadline = Instant::now() + Duration::from_secs(5);
        let frame = loop {
            let frame = request("GET", "/api/frame", token, "");
            if frame.starts_with(b"HTTP/1.1 200") {
                break frame;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(20));
        };
        let decoded = image::load_from_memory(&frame[split(&frame)..]).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (320, 240));
        request("POST", "/api/disconnect", token, "");
        assert!(request("GET", "/api/frame", token, "").starts_with(b"HTTP/1.1 401"));
        assert!(shared.state.lock().unwrap().frame.is_none());
    }
    #[test]
    fn rejects_smuggling_oversize_and_truncation() {
        for input in [
            "POST / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nx",
            "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n",
            "POST / HTTP/1.1\r\nContent-Length: 1025\r\n\r\n",
            "POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\na",
        ] {
            assert!(read_request(&mut input.as_bytes()).is_err());
        }
    }
    #[test]
    #[ignore = "Binds a synthetic-only loopback listener; run explicitly when socket binding is permitted"]
    fn loopback_listener_stops_without_connecting_to_itself() {
        let host = Host::start(
            SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0),
            StreamConfig {
                source: capture::SourceKind::Synthetic,
                ..Default::default()
            },
            Arc::new(|| {}),
        )
        .unwrap();
        let started = Instant::now();
        drop(host);
        assert!(started.elapsed() < Duration::from_secs(3));
    }
    #[test]
    #[ignore = "Requires an environment that permits loopback TCP connections; no LAN or real capture"]
    fn loopback_synthetic_transport_requires_pair_and_approval() {
        let host = Host::start(
            SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0),
            StreamConfig {
                source: capture::SourceKind::Synthetic,
                max_width: 320,
                max_height: 240,
                ..Default::default()
            },
            Arc::new(|| {}),
        )
        .unwrap();
        fn request(host: &Host, method: &str, path: &str, token: &str, body: &str) -> Vec<u8> {
            let mut socket = TcpStream::connect(host.address).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            write!(socket, "{method} {path} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", host.address, body.len()).unwrap();
            let mut reply = Vec::new();
            socket.read_to_end(&mut reply).unwrap();
            reply
        }
        let no_auth = request(&host, "GET", "/api/frame", "bad", "");
        assert!(no_auth.starts_with(b"HTTP/1.1 401"));
        assert!(host.shared.state.lock().unwrap().frame.is_none());
        let code = host.shared.state.lock().unwrap().pairing.code.clone();
        let pair = request(
            &host,
            "POST",
            "/api/pair",
            "",
            &format!("{{\"code\":\"{code}\"}}"),
        );
        let body = pair.windows(4).position(|p| p == b"\r\n\r\n").unwrap() + 4;
        let value: serde_json::Value = serde_json::from_slice(&pair[body..]).unwrap();
        let token = value["token"].as_str().unwrap();
        assert!(request(&host, "GET", "/api/frame", token, "").starts_with(b"HTTP/1.1 403"));
        host.shared.approve();
        let deadline = Instant::now() + Duration::from_secs(5);
        let frame = loop {
            let frame = request(&host, "GET", "/api/frame", token, "");
            if frame.starts_with(b"HTTP/1.1 200") {
                break frame;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(30));
        };
        let body = frame.windows(4).position(|p| p == b"\r\n\r\n").unwrap() + 4;
        let decoded = image::load_from_memory(&frame[body..]).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (320, 240));
        request(&host, "POST", "/api/disconnect", token, "");
        assert!(request(&host, "GET", "/api/frame", token, "").starts_with(b"HTTP/1.1 401"));
        assert!(host.shared.state.lock().unwrap().frame.is_none());
    }
}
