use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::*;

const LONG: Duration = Duration::from_secs(3600);

fn counting(swr: &Arc<Swr<u32>>) -> (Arc<AtomicUsize>, Arc<Mutex<Vec<String>>>) {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = heard.clone();
    swr.set_notify(Arc::new(move |topic| {
        sink.lock().unwrap().push(topic.to_owned())
    }));
    (Arc::new(AtomicUsize::new(0)), heard)
}

/// A read that takes `ms` and answers the number of reads started so far.
async fn read(reads: Arc<AtomicUsize>, ms: u64) -> Result<u32> {
    let n = u32::try_from(reads.fetch_add(1, Ordering::SeqCst)).unwrap() + 1;
    tokio::time::sleep(Duration::from_millis(ms)).await;
    Ok(n)
}

async fn settle() {
    tokio::time::sleep(Duration::from_millis(80)).await;
}

#[tokio::test]
async fn a_kept_value_is_served_without_reading_again() {
    let swr = Arc::new(Swr::new(LONG, LONG, 8));
    let (reads, _) = counting(&swr);
    let first = swr.get("k", "t", false, || read(reads.clone(), 0)).await;
    assert_eq!(first.unwrap(), 1);
    let started = Instant::now();
    for _ in 0..200 {
        assert_eq!(
            swr.get("k", "t", false, || read(reads.clone(), 0))
                .await
                .unwrap(),
            1
        );
    }
    assert!(started.elapsed() < Duration::from_millis(200));
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_stale_value_answers_at_once_and_the_refresh_is_announced() {
    let swr = Arc::new(Swr::new(Duration::ZERO, LONG, 8));
    let (reads, heard) = counting(&swr);
    assert_eq!(
        swr.get("k", "pulls", false, || read(reads.clone(), 0))
            .await
            .unwrap(),
        1
    );
    let started = Instant::now();
    // The refresh takes 150 ms, the answer must not.
    let stale = swr
        .get("k", "pulls", false, || read(reads.clone(), 150))
        .await
        .unwrap();
    assert_eq!(stale, 1);
    assert!(started.elapsed() < Duration::from_millis(50));
    assert!(heard.lock().unwrap().is_empty());
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(*heard.lock().unwrap(), ["pulls"]);
    assert_eq!(
        swr.get("k", "pulls", false, || read(reads.clone(), 0))
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn an_unchanged_refresh_announces_nothing() {
    let swr = Arc::new(Swr::new(Duration::ZERO, LONG, 8));
    let (_, heard) = counting(&swr);
    for _ in 0..3 {
        swr.get("k", "t", false, || async { Ok(7) }).await.unwrap();
        settle().await;
    }
    assert!(heard.lock().unwrap().is_empty());
}

#[tokio::test]
async fn callers_with_nothing_kept_share_one_read() {
    let swr = Arc::new(Swr::new(LONG, LONG, 8));
    let (reads, _) = counting(&swr);
    let calls = (0..20).map(|_| {
        let (swr, reads) = (swr.clone(), reads.clone());
        tokio::spawn(async move { swr.get("k", "t", false, || read(reads, 100)).await })
    });
    for call in futures_util::future::join_all(calls).await {
        assert_eq!(call.unwrap().unwrap(), 1);
    }
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn stale_callers_start_one_refresh_between_them() {
    let swr = Arc::new(Swr::new(Duration::ZERO, LONG, 8));
    let (reads, _) = counting(&swr);
    swr.get("k", "t", false, || read(reads.clone(), 0))
        .await
        .unwrap();
    for _ in 0..20 {
        assert_eq!(
            swr.get("k", "t", false, || read(reads.clone(), 150))
                .await
                .unwrap(),
            1
        );
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(reads.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn refresh_forces_a_real_read() {
    let swr = Arc::new(Swr::new(LONG, LONG, 8));
    let (reads, _) = counting(&swr);
    swr.get("k", "t", false, || read(reads.clone(), 0))
        .await
        .unwrap();
    assert_eq!(
        swr.get("k", "t", true, || read(reads.clone(), 0))
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        swr.get("k", "t", false, || read(reads.clone(), 0))
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn a_failed_refresh_keeps_the_last_good_value_and_backs_off() {
    let swr = Arc::new(Swr::new(Duration::ZERO, LONG, 8));
    let (reads, _) = counting(&swr);
    swr.get("k", "t", false, || read(reads.clone(), 0))
        .await
        .unwrap();
    let failing = |reads: Arc<AtomicUsize>| async move {
        reads.fetch_add(1, Ordering::SeqCst);
        Err::<u32, _>(CoreError::Git("forge down".into()))
    };
    for _ in 0..5 {
        assert_eq!(
            swr.get("k", "t", false, || failing(reads.clone()))
                .await
                .unwrap(),
            1
        );
        settle().await;
    }
    // One failed refresh, then the pause: the others did not ask the forge.
    assert_eq!(reads.load(Ordering::SeqCst), 2);
    // An explicit refresh still reads, and still reports the error as it is.
    let error = swr
        .get("k", "t", true, || failing(reads.clone()))
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::Git(message) if message == "forge down"));
    assert_eq!(
        swr.get("k", "t", false, || read(reads.clone(), 0))
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn with_nothing_kept_every_sharing_caller_gets_the_error_and_nothing_is_stored() {
    let swr = Arc::new(Swr::new(LONG, LONG, 8));
    let calls = (0..5).map(|_| {
        let swr = swr.clone();
        tokio::spawn(async move {
            swr.get("k", "t", false, || async {
                tokio::time::sleep(Duration::from_millis(50)).await;
                Err::<u32, _>(CoreError::NotFound("pull request".into()))
            })
            .await
        })
    });
    for call in futures_util::future::join_all(calls).await {
        let error = call.unwrap().unwrap_err();
        assert_eq!(error.code(), "not_found");
    }
    assert_eq!(
        swr.get("k", "t", false, || async { Ok(4) }).await.unwrap(),
        4
    );
}

#[tokio::test]
async fn keys_never_share_a_value() {
    let swr = Arc::new(Swr::new(LONG, LONG, 8));
    assert_eq!(
        swr.get(&key(&["p1", "1"]), "t", false, || async { Ok(1) })
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        swr.get(&key(&["p2", "1"]), "t", false, || async { Ok(2) })
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        swr.get(&key(&["p1", "1"]), "t", false, || async { Ok(9) })
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn invalidating_drops_values_and_a_read_that_started_before_is_not_kept() {
    let swr = Arc::new(Swr::new(LONG, LONG, 8));
    let (_, heard) = counting(&swr);
    swr.get(&key(&["p", "1"]), "t", false, || async { Ok(1) })
        .await
        .unwrap();
    swr.get(&key(&["p", "12"]), "t", false, || async { Ok(12) })
        .await
        .unwrap();
    swr.invalidate(&key(&["p", "1"]));
    swr.announce("pull:p:1");
    assert_eq!(*heard.lock().unwrap(), ["pull:p:1"]);
    assert_eq!(
        swr.get(&key(&["p", "1"]), "t", false, || async { Ok(2) })
            .await
            .unwrap(),
        2
    );
    // Pull request 12 is not 1.
    assert_eq!(
        swr.get(&key(&["p", "12"]), "t", false, || async { Ok(99) })
            .await
            .unwrap(),
        12
    );

    let slow = {
        let swr = swr.clone();
        tokio::spawn(async move {
            swr.get("slow", "t", false, || async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok(1)
            })
            .await
        })
    };
    tokio::time::sleep(Duration::from_millis(20)).await;
    swr.invalidate("slow");
    assert_eq!(slow.await.unwrap().unwrap(), 1);
    assert_eq!(
        swr.get("slow", "t", false, || async { Ok(5) })
            .await
            .unwrap(),
        5
    );
}

#[tokio::test]
async fn a_value_past_its_max_age_is_read_again_before_it_is_served() {
    let swr = Arc::new(Swr::new(Duration::ZERO, Duration::ZERO, 8));
    let (reads, _) = counting(&swr);
    swr.get("k", "t", false, || read(reads.clone(), 0))
        .await
        .unwrap();
    assert_eq!(
        swr.get("k", "t", false, || read(reads.clone(), 0))
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn memory_is_bounded_and_the_oldest_key_goes_first() {
    let swr = Arc::new(Swr::new(LONG, LONG, 3));
    for n in 0..10u32 {
        swr.get(&format!("k{n}"), "t", false, || async move { Ok(n) })
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert!(swr.len() <= 3);
    assert_eq!(
        swr.get("k9", "t", false, || async { Ok(0) }).await.unwrap(),
        9
    );
    assert_eq!(
        swr.get("k0", "t", false, || async { Ok(0) }).await.unwrap(),
        0
    );
}

#[tokio::test]
async fn put_keeps_a_value_a_write_already_knows() {
    let swr = Arc::new(Swr::new(LONG, LONG, 8));
    swr.put("k", 3);
    assert_eq!(
        swr.get("k", "t", false, || async { Ok(0) }).await.unwrap(),
        3
    );
}

#[tokio::test]
async fn forced_reads_that_overlap_share_one_read() {
    let swr = Arc::new(Swr::new(LONG, LONG, 8));
    let (reads, _) = counting(&swr);
    swr.get("k", "t", false, || read(reads.clone(), 0))
        .await
        .unwrap();
    let calls = (0..10).map(|_| {
        let (swr, reads) = (swr.clone(), reads.clone());
        tokio::spawn(async move { swr.get("k", "t", true, || read(reads, 100)).await })
    });
    for call in futures_util::future::join_all(calls).await {
        assert_eq!(call.unwrap().unwrap(), 2);
    }
    assert_eq!(reads.load(Ordering::SeqCst), 2);
    // A forced read after the first one landed is a new read.
    assert_eq!(
        swr.get("k", "t", true, || read(reads.clone(), 0))
            .await
            .unwrap(),
        3
    );
}
