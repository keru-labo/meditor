use meditor_playback::project::{
    Caption, ClipRange, MAX_DURATION, Project, Track, import_srt, parse_seconds,
};
use std::{fs, path::PathBuf};

#[test]
fn existing_project_with_10_000_captions_roundtrips_one_edit_and_clip() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("実況.json");
    let mut original = Project::new(
        temp.path().join("元動画.mp4"),
        vec![
            Track {
                path: temp.path().join("ゲーム.wav"),
                name: "ゲーム音".into(),
                offset_us: -12_000,
                gain: 0.4,
            },
            Track {
                path: temp.path().join("話者.wav"),
                name: "話者 A".into(),
                offset_us: 120_000,
                gain: 0.7,
            },
        ],
    )?;
    original.proxy = temp.path().join("proxy.mp4");
    original.captions = (0..10_000)
        .map(|row| Caption {
            id: format!("cue-{row}"),
            start_us: row * 6_000,
            end_us: row * 6_000 + 350_000,
            speaker: "話者 A".into(),
            text: format!("実況字幕 {row}"),
        })
        .collect();
    assert_eq!(original.playback_range(60_000_000).source_start_us, 0);
    assert_eq!(original.playback_range(60_000_000).duration(), 60_000_000);
    original.save(&path)?;

    let mut reopened = Project::load(&path)?;
    assert_eq!(reopened.video, original.video);
    assert_eq!(reopened.proxy, original.proxy);
    assert_eq!(reopened.audio, original.audio);
    assert_eq!(reopened.captions.len(), 10_000);
    assert_eq!(reopened.clip, None);
    let clip = ClipRange {
        source_start_us: 3_100_000,
        source_end_us: 8_100_000,
    };
    reopened.set_clip(clip, 60_000_000)?;
    reopened.captions[9999].text = "最後の字幕を編集".into();
    reopened.save(&path)?;

    let saved = Project::load(&path)?;
    assert_eq!(saved.clip, Some(clip));
    assert_eq!(saved.playback_range(60_000_000).duration(), 5_000_000);
    assert_eq!(saved.captions[..9999], original.captions[..9999]);
    assert_eq!(saved.captions[9999].id, "cue-9999");
    assert_eq!(saved.captions[9999].speaker, "話者 A");
    assert_eq!(saved.captions[9999].text, "最後の字幕を編集");
    assert_eq!(saved.audio, original.audio);
    let json: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
    assert_eq!(json["version"], 2);
    assert_eq!(json["video"], "元動画.mp4");
    assert_eq!(json["audio"][1]["path"], "話者.wav");
    Ok(())
}

#[test]
fn invalid_project_does_not_replace_the_previous_save() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.json");
    let mut project = Project::new(PathBuf::from("video.mp4"), Vec::new())?;
    project.save(&path)?;
    let saved = fs::read(&path)?;
    project.proxy = directory.path().join("proxy.mp4");
    let source = project.proxy.clone();
    let invalid = ClipRange {
        source_start_us: 3,
        source_end_us: 3,
    };
    assert!(project.set_clip(invalid, 60_000_000).is_err());
    assert_eq!(project.clip, None);
    project.captions = vec![Caption {
        id: "invalid".into(),
        start_us: 0,
        end_us: -1,
        speaker: String::new(),
        text: String::new(),
    }];
    assert!(project.save(&path).is_err());
    assert_eq!(project.proxy, source);
    assert_eq!(fs::read(path)?, saved);
    Ok(())
}

#[test]
fn malformed_and_unsupported_project_versions_are_rejected() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.json");
    for input in [
        "\"not an object\"",
        r#"{"version":3,"video":"video.mp4","audio":[],"captions":[]}"#,
        r#"{"version":2,"video":"video.mp4","audio":[],"captions":[]}"#,
        r#"{"version":1,"video":"","audio":[],"captions":[]}"#,
        r#"{"version":1,"video":"video.mp4","audio":[{"path":"game.wav","name":"game","gain":5}],"captions":[]}"#,
        r#"{"version":1,"video":"video.mp4","audio":[],"captions":[{"id":"1","start_us":0,"end_us":1,"speaker":"","text":""},{"id":"1","start_us":2,"end_us":3,"speaker":"","text":""}]}"#,
    ] {
        fs::write(&path, input)?;
        assert!(Project::load(&path).is_err(), "{input}");
    }
    Ok(())
}

#[test]
fn srt_import_preserves_bom_crlf_and_multiline_text() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("字幕.srt");
    fs::write(
        &path,
        "\u{feff}1\r\n00:00:01,005 --> 00:00:03,020\r\nこんにちは\r\n二行目\r\n\r\n2\r\n00:00:00,001 --> 00:00:01,000\r\n先に表示\r\n",
    )?;
    let captions = import_srt(&path)?;
    assert_eq!(captions.len(), 2);
    assert_eq!(captions[0].id, "2");
    assert_eq!(captions[1].id, "1");
    assert_eq!(captions[1].start_us, 1_005_000);
    assert_eq!(captions[1].end_us, 3_020_000);
    assert_eq!(captions[1].text, "こんにちは\n二行目");
    for input in [
        "1\n00:00:01,000 --> 00:00:01,000\n字幕",
        "1\n99:00:01,000 --> 99:00:02,000\n字幕",
        "1\n不正な時刻\n字幕",
        "番号なし\n00:00:00,000 --> 00:00:01,000\n字幕",
    ] {
        fs::write(&path, input)?;
        assert!(import_srt(&path).is_err());
    }
    Ok(())
}

#[test]
fn seconds_are_parsed_without_floating_point_rounding() {
    assert_eq!(parse_seconds("  13.000001 "), Some(13_000_001));
    assert_eq!(parse_seconds("8.04"), Some(8_040_000));
    assert_eq!(parse_seconds("1"), Some(1_000_000));
    assert_eq!(parse_seconds("86400.0"), Some(MAX_DURATION));
    for invalid in [
        "",
        "8.",
        "８",
        ".5",
        "-1",
        "1.0000001",
        "1.2.3",
        "86400.001",
        "99999999999999999999",
    ] {
        assert_eq!(parse_seconds(invalid), None, "{invalid}");
    }
}

#[test]
fn roundtrip_orders_by_start_and_preserves_source_order_for_equal_starts() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("caption-order.json");
    let mut project = Project::new(directory.path().join("素材.mp4"), Vec::new())?;
    let captions = [
        Caption {
            id: "first".into(),
            start_us: 0,
            end_us: 500_000,
            speaker: "話者 A".into(),
            text: "一行目\n二行目".into(),
        },
        Caption {
            id: "later".into(),
            start_us: 1_000_000,
            end_us: 2_000_000,
            speaker: "話者 A".into(),
            text: "続き".into(),
        },
        Caption {
            id: "same-start".into(),
            start_us: 0,
            end_us: 200_000,
            speaker: "話者 B".into(),
            text: "同じ開始時刻".into(),
        },
    ];

    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        project.captions = order.map(|index| captions[index].clone()).into();
        project.save(&path)?;

        let actual = Project::load(&path)?;
        let expected: Vec<_> = order
            .into_iter()
            .filter(|&index| index != 1)
            .chain([1])
            .map(|index| captions[index].clone())
            .collect();
        assert_eq!(actual.captions, expected, "input order: {order:?}");
    }
    Ok(())
}
