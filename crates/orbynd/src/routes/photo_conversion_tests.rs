//! Photo conversion over HTTP (docs/attachments-transcripts.md, "Photo conversion"): the answer is a
//! stored attachment the thread owns, a HEIC and anything that is not an image are refused with their
//! own codes, an oversized photo is shrunk under the image limit, the input cap is refused from the
//! declared length before the body is read, and the stored bytes carry no EXIF location.

use std::io::Cursor;
use std::sync::Arc;

use image::{DynamicImage, ImageBuffer, Rgb};
use orbyn_core::Core;
use orbyn_core::CoreConfig;
use orbyn_core::store::{NewProject, NewThread};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

struct Served {
    port: u16,
    core: Arc<Core>,
    thread: String,
    dir: std::path::PathBuf,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn serve(name: &str) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-photo-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let work = dir.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let core = Arc::new(
        Core::open(CoreConfig::new(
            dir.join("data"),
            "http://127.0.0.1:1".into(),
        ))
        .unwrap(),
    );
    let project = core
        .store
        .add_project(NewProject {
            path: &work.to_string_lossy(),
            name: "p",
            is_git: false,
            managed: false,
            scratch: false,
        })
        .unwrap();
    core.store.trust_project(&project.id, "user").unwrap();
    let thread = core
        .store
        .create_thread(NewThread {
            project_id: project.id.clone(),
            title: "Photo".into(),
            engine: "claude".into(),
            model: Some("opus".into()),
            cwd: work.to_string_lossy().into_owned(),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap();
    let app = router(AppState {
        core: core.clone(),
        access: Arc::new(Access::new("token", vec![], None)),
        remote: false,
        ui_dir: None,
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    Served {
        port,
        core,
        thread: thread.id,
        dir,
    }
}

impl Served {
    /// The route's answer and status for one photo.
    async fn convert(&self, name: &str, body: Vec<u8>) -> (u16, Value) {
        let response = reqwest::Client::new()
            .post(format!(
                "http://127.0.0.1:{}/api/threads/{}/attachments/convert?name={name}",
                self.port, self.thread
            ))
            .bearer_auth("token")
            .header("content-type", "application/octet-stream")
            .body(body)
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        (status, response.json().await.unwrap_or(Value::Null))
    }

    /// The stored bytes of one attachment, as `/api/attachments/{id}/content` serves them.
    async fn content(&self, id: &str) -> Vec<u8> {
        let response = reqwest::Client::new()
            .get(format!(
                "http://127.0.0.1:{}/api/attachments/{id}/content",
                self.port
            ))
            .bearer_auth("token")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 200);
        response.bytes().await.unwrap().to_vec()
    }

    fn attachments(&self) -> Vec<orbyn_contracts::Attachment> {
        self.core
            .store
            .list_attachments(&self.thread)
            .unwrap()
            .into_iter()
            .map(|(attachment, _)| attachment)
            .collect()
    }
}

/// An image that does not compress to nothing, so a size limit forces a real shrink.
fn photo(width: u32, height: u32) -> DynamicImage {
    let rgb: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |x, y| {
        Rgb([
            ((x * 7 + y * 13) % 256) as u8,
            ((x * 31 + y * 3) % 256) as u8,
            ((x * 5 + y * 17) % 256) as u8,
        ])
    });
    DynamicImage::ImageRgb8(rgb)
}

fn jpeg(image: &DynamicImage) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(image.to_rgb8())
        .write_to(&mut out, image::ImageFormat::Jpeg)
        .unwrap();
    out.into_inner()
}

fn png(image: &DynamicImage) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image.to_rgba8())
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

/// A structurally valid EXIF APP1 segment carrying a GPS latitude, inserted after the JPEG's SOI the
/// way a camera writes one.
fn with_exif_gps(jpeg: Vec<u8>) -> Vec<u8> {
    let mut exif = b"Exif\0\0".to_vec();
    exif.extend_from_slice(b"II");
    exif.extend_from_slice(&42u16.to_le_bytes());
    exif.extend_from_slice(&8u32.to_le_bytes());
    const GPS_IFD: u32 = 26;
    exif.extend_from_slice(&1u16.to_le_bytes());
    exif.extend_from_slice(&0x8825u16.to_le_bytes());
    exif.extend_from_slice(&4u16.to_le_bytes());
    exif.extend_from_slice(&1u32.to_le_bytes());
    exif.extend_from_slice(&GPS_IFD.to_le_bytes());
    exif.extend_from_slice(&0u32.to_le_bytes());
    let rationals: u32 = GPS_IFD + 2 + 2 * 12 + 4;
    exif.extend_from_slice(&2u16.to_le_bytes());
    exif.extend_from_slice(&0x0001u16.to_le_bytes());
    exif.extend_from_slice(&2u16.to_le_bytes());
    exif.extend_from_slice(&2u32.to_le_bytes());
    exif.extend_from_slice(b"N\0\0\0");
    exif.extend_from_slice(&0x0002u16.to_le_bytes());
    exif.extend_from_slice(&5u16.to_le_bytes());
    exif.extend_from_slice(&3u32.to_le_bytes());
    exif.extend_from_slice(&rationals.to_le_bytes());
    exif.extend_from_slice(&0u32.to_le_bytes());
    for (numerator, denominator) in [(48u32, 1u32), (51, 1), (245, 10)] {
        exif.extend_from_slice(&numerator.to_le_bytes());
        exif.extend_from_slice(&denominator.to_le_bytes());
    }
    let mut segment = vec![0xFF, 0xE1];
    segment.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
    segment.extend_from_slice(&exif);
    let mut out = Vec::with_capacity(jpeg.len() + segment.len());
    out.extend_from_slice(&jpeg[..2]);
    out.extend_from_slice(&segment);
    out.extend_from_slice(&jpeg[2..]);
    out
}

#[tokio::test]
async fn a_photo_becomes_the_stored_attachment_the_run_sends_without_its_exif_location() {
    let served = serve("stored").await;
    let picked = with_exif_gps(jpeg(&photo(96, 64)));
    assert!(
        picked.windows(2).any(|pair| pair == [0xFF, 0xE1]),
        "the fixture carries an APP1 segment"
    );
    let (status, answer) = served.convert("IMG_4021.HEIC", picked.clone()).await;
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["action"], "none");
    assert_eq!(answer["name"], "IMG_4021.jpg");
    assert_eq!(answer["original_name"], "IMG_4021.HEIC");
    assert_eq!(answer["original_size"], picked.len());
    let id = answer["id"].as_str().unwrap();
    assert_eq!(
        answer["size"].as_u64().unwrap(),
        served.content(id).await.len() as u64
    );

    // The stored record is this thread's, with the converted type and the answer's name.
    let stored = served.attachments();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].id, id);
    assert_eq!(stored[0].name, "IMG_4021.jpg");
    assert_eq!(stored[0].mime, "image/jpeg");
    assert_eq!(stored[0].thread_id, served.thread);

    // The bytes the engine reads are the answer, and the camera's location did not travel with them.
    let bytes = served.content(id).await;
    assert!(
        !bytes.windows(2).any(|pair| pair == [0xFF, 0xE1]),
        "the answer carries no EXIF segment"
    );
    assert!(!bytes.windows(4).any(|window| window == b"Exif"));
    let pixels = image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg).unwrap();
    assert_eq!((pixels.width(), pixels.height()), (96, 64));
}

#[tokio::test]
async fn an_oversized_photo_is_shrunk_under_the_image_limit_and_stored() {
    let served = serve("shrunk").await;
    let picked = png(&photo(2_200, 2_200));
    let limit = 10 * 1024 * 1024;
    assert!(picked.len() > limit, "the fixture starts over the limit");
    let (status, answer) = served.convert("shot.png", picked.clone()).await;
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["action"], "shrunk");
    assert_eq!(answer["name"], "shot.png");
    let size = answer["size"].as_u64().unwrap();
    assert!(size <= limit as u64, "{size} bytes");
    let bytes = served.content(answer["id"].as_str().unwrap()).await;
    assert_eq!(bytes.len() as u64, size);
    let pixels = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).unwrap();
    assert!(pixels.width() < 2_200, "{}", pixels.width());
}

#[tokio::test]
async fn a_heic_is_refused_as_an_unsupported_type_until_a_decoder_is_chosen() {
    let served = serve("heic").await;
    let mut heic = vec![0u8, 0, 0, 0x18];
    heic.extend_from_slice(b"ftypheic");
    heic.extend_from_slice(&[0u8; 32]);
    let (status, answer) = served.convert("IMG_4021.HEIC", heic).await;
    assert_eq!(status, 415, "{answer}");
    assert_eq!(answer["code"], "unsupported_type");
    assert!(answer["message"].as_str().unwrap().contains("HEIC"));
    assert!(served.attachments().is_empty(), "nothing is stored");
}

#[tokio::test]
async fn something_that_is_not_an_image_is_refused_and_stores_nothing() {
    let served = serve("text").await;
    let (status, answer) = served
        .convert("notes.txt", b"hello, not a photo".to_vec())
        .await;
    assert_eq!(status, 422, "{answer}");
    assert_eq!(answer["code"], "unreadable_image");
    assert!(served.attachments().is_empty(), "nothing is stored");
}

#[tokio::test]
async fn a_body_over_the_cap_is_refused_from_its_declared_length() {
    let served = serve("cap").await;
    // A head-only request: the service must answer before it tries to read 50 MiB, so the body is
    // never sent.
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", served.port))
        .await
        .unwrap();
    let head = format!(
        "POST /api/threads/{}/attachments/convert?name=big.jpg HTTP/1.1\r\n\
         Host: 127.0.0.1\r\n\
         Authorization: Bearer token\r\n\
         Content-Type: image/jpeg\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n",
        served.thread,
        50 * 1024 * 1024 + 1
    );
    stream.write_all(head.as_bytes()).await.unwrap();
    // The answer arrives without a body; stop reading rather than wait for a close the server keeps
    // open for the body it never received.
    let mut response = Vec::new();
    let mut buffer = [0u8; 1024];
    while !String::from_utf8_lossy(&response).contains("file_too_large") {
        let read =
            tokio::time::timeout(std::time::Duration::from_secs(10), stream.read(&mut buffer))
                .await
                .expect("the answer arrives without the body")
                .unwrap();
        if read == 0 {
            break;
        }
        response.extend_from_slice(&buffer[..read]);
    }
    let response = String::from_utf8_lossy(&response);
    assert!(response.starts_with("HTTP/1.1 413"), "{response}");
    assert!(response.contains("file_too_large"), "{response}");
    assert!(served.attachments().is_empty(), "nothing is stored");
}

#[tokio::test]
async fn an_unknown_thread_is_not_found() {
    let served = serve("missing").await;
    let response = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{}/api/threads/nope/attachments/convert?name=a.png",
            served.port
        ))
        .bearer_auth("token")
        .body(png(&photo(8, 8)))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 404);
    let answer: Value = response.json().await.unwrap();
    assert_eq!(answer["code"], "not_found");
}

#[tokio::test]
async fn simultaneous_photo_uploads_wait_for_a_global_admission_slot() {
    use std::time::Duration;

    let served = serve("admission").await;
    let (events, mut observed) = tokio::sync::mpsc::unbounded_channel();
    *super::composer::PHOTO_ADMISSION_OBSERVER.lock().unwrap() = Some(events);

    let first = super::composer::PHOTO_WORKERS.acquire().await.unwrap();
    let second = super::composer::PHOTO_WORKERS.acquire().await.unwrap();

    // An incomplete upload must not reach the conversion admission queue.
    let mut partial = tokio::net::TcpStream::connect(("127.0.0.1", served.port))
        .await
        .unwrap();
    let header = format!(
        "POST /api/threads/{}/attachments/convert?name=concurrency-test-partial HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer token\r\nContent-Length: 100\r\nConnection: close\r\n\r\nx",
        served.thread
    );
    partial.write_all(header.as_bytes()).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(200), observed.recv())
            .await
            .is_err()
    );
    drop(partial);

    let url = format!(
        "http://127.0.0.1:{}/api/threads/{}/attachments/convert?name=concurrency-test-three",
        served.port, served.thread
    );
    let third = tokio::spawn(async move {
        reqwest::Client::new()
            .post(url)
            .bearer_auth("token")
            .body(png(&photo(8, 8)))
            .send()
            .await
            .unwrap()
    });
    let event = tokio::time::timeout(Duration::from_secs(10), observed.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event, ("concurrency-test-three".into(), false));
    assert!(
        tokio::time::timeout(Duration::from_millis(200), observed.recv())
            .await
            .is_err()
    );

    drop(first);
    let event = tokio::time::timeout(Duration::from_secs(10), observed.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event, ("concurrency-test-three".into(), true));
    let response = tokio::time::timeout(Duration::from_secs(10), third)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    drop(second);
    *super::composer::PHOTO_ADMISSION_OBSERVER.lock().unwrap() = None;
}
