//! Request-bound recording cues while real preview input uses an in-flight capture.

use super::*;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

struct Release {
    navigate: bool,
    safe: bool,
}

struct Fixture {
    manager: PreviewManager,
    tab: Arc<Tab>,
    captures: mpsc::Receiver<u32>,
    release: mpsc::Sender<Release>,
    server: tokio::task::JoinHandle<()>,
    pumping: Option<tokio::task::JoinHandle<()>>,
    _viewer: broadcast::Receiver<Arc<str>>,
    root: PathBuf,
}

impl Fixture {
    async fn new() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (started, captures) = mpsc::channel(8);
        let (release, mut released) = mpsc::channel::<Release>(8);
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
            let mut pending = None;
            let mut number = 0;
            let mut safe = true;
            loop {
                tokio::select! {
                    Some(release) = released.recv() => {
                        safe = release.safe;
                        if release.navigate {
                            socket.send(Message::text(json!({"sessionId":"session","method":"Page.frameNavigated","params":{"frame":{"id":"main"}}}).to_string())).await.unwrap();
                        }
                        let id = pending.take().expect("release needs a pending capture");
                        let mut pixels = image::RgbImage::new(200, 100);
                        for y in 0..20 {
                            for x in 180..200 {
                                pixels.put_pixel(x, y, image::Rgb([u8::try_from(number * 40).unwrap(), 0, 0]));
                            }
                        }
                        let mut jpeg = std::io::Cursor::new(Vec::new());
                        image::DynamicImage::ImageRgb8(pixels).write_to(&mut jpeg, image::ImageFormat::Jpeg).unwrap();
                        let data = base64::engine::general_purpose::STANDARD.encode(jpeg.into_inner());
                        if socket.send(Message::text(json!({"id":id,"result":{"data":data}}).to_string())).await.is_err() { break; }
                    }
                    frame = socket.next() => {
                        let Some(Ok(frame)) = frame else { break };
                        let Ok(text) = frame.to_text() else { continue };
                        let request: Value = serde_json::from_str(text).unwrap();
                        let result = match request["method"].as_str().unwrap() {
                            "Target.getTargets" => json!({"targetInfos":[{"type":"page","targetId":"target"}]}),
                            "Target.attachToTarget" => json!({"sessionId":"session"}),
                            "Page.getFrameTree" => json!({"frameTree":{"frame":{"id":"main"}}}),
                            "Page.createIsolatedWorld" => json!({"executionContextId":7}),
                            "Page.getNavigationHistory" => json!({"currentIndex":0,"entries":[{"id":1,"url":"http://fixture/"}]}),
                            "Runtime.evaluate" => json!({"result":{"value":safe}}),
                            "Page.captureScreenshot" => {
                                assert!(pending.replace(request["id"].clone()).is_none());
                                number += 1;
                                started.send(number).await.unwrap();
                                continue;
                            }
                            _ => json!({}),
                        };
                        if socket.send(Message::text(json!({"id":request["id"],"result":result}).to_string())).await.is_err() { break; }
                    }
                }
            }
        });
        let session = BrowserSession::connect(&format!("ws://{address}"), None)
            .await
            .unwrap();
        let tab = fixture_tab(session, "human");
        super::super::crash::watch_document(&mut *tab.session.lock().await, &tab);
        let manager = PreviewManager::default();
        lock(&manager.tabs).insert("tab".into(), tab.clone());
        let viewer = tab.frames.subscribe();
        let root = std::env::temp_dir().join(format!("recording-capture-{}", new_id()));
        let fixture = Self {
            manager,
            tab,
            captures,
            release,
            server,
            pumping: None,
            _viewer: viewer,
            root,
        };
        fixture.start_recording("first");
        fixture
    }

    fn start_recording(&self, name: &str) {
        self.manager
            .start_recording(
                "thread",
                "tab",
                self.root.join(name),
                PreviewRecordingOptions {
                    fps: 10,
                    show_mouse_presses: true,
                    show_key_presses: true,
                },
            )
            .unwrap();
    }

    fn start(&mut self) {
        self.pumping = Some(tokio::spawn(pump(self.tab.clone())));
    }

    async fn captured(&mut self, number: u32) {
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), self.captures.recv())
                .await
                .unwrap(),
            Some(number)
        );
    }

    async fn input(&self, kind: &str, text: &str) {
        let input = serde_json::from_value(json!({"kind":kind,"text":text,"x":50,"y":30})).unwrap();
        tokio::time::timeout(
            Duration::from_millis(500),
            self.manager.input("thread", "tab", &input),
        )
        .await
        .expect("input waited behind capture")
        .unwrap();
    }

    async fn frame(&self, count: u64) -> image::RgbImage {
        self.frame_in("first", count).await
    }

    async fn frame_in(&self, clip: &str, count: u64) -> image::RgbImage {
        let owner = lock(&self.tab.recording).clone().unwrap();
        tokio::time::timeout(Duration::from_secs(2), owner.wait_for_frames(count))
            .await
            .unwrap();
        let path = self.root.join(clip).join(format!("{:06}.jpg", count - 1));
        image::open(path).unwrap().to_rgb8()
    }

    async fn close(mut self) {
        self.manager.stop_recording("thread", "tab").await.unwrap();
        self.manager.close("thread", "tab").unwrap();
        tokio::time::timeout(Duration::from_secs(1), self.pumping.take().unwrap())
            .await
            .unwrap()
            .unwrap();
        drop(self.tab);
        drop(self.manager);
        tokio::time::timeout(Duration::from_secs(1), self.server)
            .await
            .unwrap()
            .unwrap();
        std::fs::remove_dir_all(self.root).unwrap();
    }
}

#[tokio::test]
async fn input_during_a_pending_capture_belongs_to_the_next_eligible_frame() {
    let mut fixture = Fixture::new().await;
    fixture.start();
    fixture.captured(1).await;
    fixture.input("click", "").await;
    fixture.input("key", "Enter").await;
    fixture
        .release
        .send(Release {
            navigate: false,
            safe: true,
        })
        .await
        .unwrap();
    let first = fixture.frame(1).await;
    fixture.captured(2).await;
    fixture
        .release
        .send(Release {
            navigate: false,
            safe: true,
        })
        .await
        .unwrap();
    let second = fixture.frame(2).await;
    fixture.close().await;
    assert!(
        first.get_pixel(60, 30)[2] < 70,
        "the earlier frame received a later mouse cue"
    );
    assert!(
        first.get_pixel(12, 66)[0] < 70,
        "the earlier frame received a later key cue"
    );
    assert!(
        second.get_pixel(60, 30)[2] > 100,
        "the subsequent frame lost its mouse cue"
    );
    assert!(
        second.get_pixel(12, 66)[0] > 150,
        "the subsequent frame lost its key cue"
    );
}

#[tokio::test]
async fn pending_capture_vetoes_navigation_explicit_clear_and_uncertain_focus() {
    for veto in ["navigation", "clear", "focus"] {
        let mut fixture = Fixture::new().await;
        fixture.input("click", "").await;
        fixture.input("key", "Enter").await;
        fixture.start();
        fixture.captured(1).await;
        if veto == "clear" {
            fixture.tab.clear_pointer();
        }
        fixture
            .release
            .send(Release {
                navigate: veto == "navigation",
                safe: veto != "focus",
            })
            .await
            .unwrap();
        let frame = fixture.frame(1).await;
        fixture.close().await;
        assert!(
            frame.get_pixel(12, 66)[0] < 70,
            "{veto} did not veto the frozen key cue"
        );
        if veto == "focus" {
            assert!(
                frame.get_pixel(60, 30)[2] > 100,
                "uncertain key focus removed an unrelated mouse cue"
            );
        } else {
            assert!(
                frame.get_pixel(60, 30)[2] < 70,
                "{veto} did not veto the frozen mouse cue"
            );
        }
    }
}

#[tokio::test]
async fn replacing_a_recording_while_capture_waits_cannot_receive_its_frame_or_cues() {
    let mut fixture = Fixture::new().await;
    fixture.input("click", "").await;
    fixture.input("key", "Enter").await;
    fixture.start();
    fixture.captured(1).await;
    let old = fixture
        .manager
        .stop_recording("thread", "tab")
        .await
        .unwrap();
    fixture.start_recording("second");
    fixture
        .release
        .send(Release {
            navigate: false,
            safe: true,
        })
        .await
        .unwrap();
    fixture.captured(2).await;
    fixture
        .release
        .send(Release {
            navigate: false,
            safe: true,
        })
        .await
        .unwrap();
    let frame = fixture.frame_in("second", 1).await;
    fixture.close().await;
    assert_eq!(old.frames, 0);
    assert!(
        frame.get_pixel(190, 10)[0] > 60,
        "replacement clip received the earlier request's image"
    );
    assert!(
        frame.get_pixel(60, 30)[2] < 70,
        "replacement clip received the earlier mouse cue"
    );
    assert!(
        frame.get_pixel(12, 66)[0] < 70,
        "replacement clip received the earlier key cue"
    );
}
