use meditor_playback::{SeekController, SeekMode, VideoSource};

#[test]
fn only_the_latest_scrub_frame_is_accepted_even_at_the_same_position() {
    let mut controller = SeekController::default();
    let mode = SeekMode::Scrub {
        proxy_available: true,
    };
    let old = controller.request(15_000_000, mode);
    let latest = controller.request(15_000_000, mode);

    assert_eq!(latest.source(), VideoSource::Proxy);
    assert!(!latest.resumes_playback());
    assert!(!controller.accept_frame(old));
    assert!(controller.accept_frame(latest));
    assert!(!controller.accept_frame(latest));
    assert!(!controller.accept_frame(old));
}

#[test]
fn releasing_the_scrub_waits_for_the_original_at_the_release_position() {
    let mut controller = SeekController::default();
    let preview = controller.request(
        15_000_000,
        SeekMode::Scrub {
            proxy_available: true,
        },
    );
    let release = controller.request(
        16_000_000,
        SeekMode::Settle {
            resume_playback: true,
        },
    );

    assert_eq!(release.position_us(), 16_000_000);
    assert_eq!(release.source(), VideoSource::Original);
    assert!(release.resumes_playback());
    assert!(!controller.accept_frame(preview));
    assert!(controller.accept_frame(release));
}

#[test]
fn paused_scrubbing_without_a_proxy_does_not_resume_playback() {
    let mut controller = SeekController::default();
    let preview = controller.request(
        0,
        SeekMode::Scrub {
            proxy_available: false,
        },
    );
    assert_eq!(preview.source(), VideoSource::Original);

    let release = controller.request(
        0,
        SeekMode::Settle {
            resume_playback: false,
        },
    );
    assert_eq!(release.source(), VideoSource::Original);
    assert!(!release.resumes_playback());
    assert!(controller.accept_frame(release));
}

#[test]
fn old_media_completion_is_rejected_after_invalidation_and_after_reopening() {
    let mut controller = SeekController::default();
    let mode = SeekMode::Settle {
        resume_playback: true,
    };
    let from_old_media = controller.request(0, mode);
    controller.invalidate();
    assert!(!controller.accept_frame(from_old_media));

    let from_new_media = controller.request(0, mode);
    assert!(!controller.accept_frame(from_old_media));
    assert!(controller.accept_frame(from_new_media));
}
