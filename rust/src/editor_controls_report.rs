use super::{
    DialogAnswer, Editor, EditorWindow, Step, check, clip, edit_clip, input, open_project,
};
use anyhow::{Result, ensure};
use slint::{ComponentHandle, Model, platform::Key};
use std::{cell::RefCell, fs, path::Path};

pub(super) const CASES: [(&str, &str, usize); 9] = [
    ("editor-project", "プロジェクト作成と未保存時の確認", 7),
    ("editor-playback", "再生・一時停止・シーク", 6),
    ("editor-captions", "SRTの取り込みと字幕一覧の操作", 6),
    ("editor-panels", "アプリ情報・AI・書き出し設定の開閉", 6),
    ("editor-proxy", "プロキシの選択と切り替え", 3),
    ("editor-metrics", "計測の開始・停止・保存", 3),
    ("editor-export", "書き出し設定の選択とMP4保存", 6),
    (
        "editor-transcription",
        "ローカル文字起こしの開始・中止・保存",
        4,
    ),
    ("editor-ai", "合成AI応答の適用・設定確認・中断", 7),
];

fn flag(value: bool) -> &'static str {
    if value { "はい" } else { "いいえ" }
}

pub(super) fn execute(
    ui: &EditorWindow,
    editor: &RefCell<Editor>,
    source: &Path,
    destination: &Path,
    scenario: usize,
    stage: usize,
) -> Result<Step> {
    let (action, operations, checks): (_, &[_], _) = match (scenario, stage) {
        (0, 0) => {
            let new_project = destination.with_file_name("gui-new.json");
            super::click_with_files(
                ui,
                editor,
                "new",
                [
                    DialogAnswer::File(Some(source.with_file_name("original.mp4"))),
                    DialogAnswer::Files(vec![source.with_file_name("game.wav")]),
                    DialogAnswer::File(Some(new_project.clone())),
                ],
            )?;
            let project = super::Project::load(&new_project)?;
            (
                "新規から動画・WAVを選び、プロジェクトを作る",
                &["「新規」を押す", "合成動画とWAVを選び、保存先を指定する"],
                vec![
                    check("開いているファイル", "gui-new.json", ui.get_filename()),
                    check("保存したWAVの本数", "1", project.audio.len().to_string()),
                ],
            )
        }
        (0, 1) => {
            edit_clip(ui)?;
            input::click(ui, "open")?;
            (
                "区間編集後に「開く」を押す",
                &["終了を9秒にして区間を適用する", "「開く」を押す"],
                vec![check(
                    "未保存の確認が表示された",
                    "はい",
                    flag(ui.get_confirm_discard()),
                )],
            )
        }
        (0, 2) => {
            input::click(ui, "leave-cancel")?;
            (
                "未保存の確認をキャンセルする",
                &["「キャンセル」を押す"],
                vec![
                    check(
                        "未保存の確認が表示された",
                        "いいえ",
                        flag(ui.get_confirm_discard()),
                    ),
                    check("編集は残っている", super::EDITED_RANGE, clip(ui)),
                ],
            )
        }
        (0, 3) => {
            input::click(ui, "open")?;
            super::click_with_files(
                ui,
                editor,
                "leave-save",
                [DialogAnswer::File(Some(source.to_owned()))],
            )?;
            let saved = super::Project::load(&destination.with_file_name("gui-new.json"))?;
            (
                "編集を保存して別のプロジェクトを開く",
                &[
                    "「開く」を押し、「保存して続行」を選ぶ",
                    "別のプロジェクトを選ぶ",
                ],
                vec![
                    check("開いているファイル", "fixture.json", ui.get_filename()),
                    check(
                        "保存した区間の終了",
                        "9000000",
                        saved
                            .clip
                            .map(|clip| clip.source_end_us)
                            .unwrap_or_default()
                            .to_string(),
                    ),
                ],
            )
        }
        (0, 4) => {
            edit_clip(ui)?;
            input::click(ui, "open")?;
            super::click_with_files(
                ui,
                editor,
                "leave-discard",
                [DialogAnswer::File(Some(source.to_owned()))],
            )?;
            (
                "未保存の編集を破棄して開き直す",
                &[
                    "区間を変更し、「開く」を押す",
                    "「変更を破棄」を押して同じファイルを選ぶ",
                ],
                vec![
                    check("開き直した素材区間", super::ORIGINAL_RANGE, clip(ui)),
                    check(
                        "未保存の確認が表示された",
                        "いいえ",
                        flag(ui.get_confirm_discard()),
                    ),
                ],
            )
        }
        (0, 5) => {
            edit_clip(ui)?;
            input::close_request(ui)?;
            (
                "未保存の状態でウィンドウを閉じようとする",
                &["ウィンドウに閉じる要求を送る"],
                vec![
                    check(
                        "ウィンドウが表示されている",
                        "はい",
                        flag(ui.window().is_visible()),
                    ),
                    check(
                        "未保存の確認が表示された",
                        "はい",
                        flag(ui.get_confirm_discard()),
                    ),
                ],
            )
        }
        (0, 6) => {
            input::click(ui, "leave-cancel")?;
            (
                "終了を取り消して編集に戻る",
                &["「キャンセル」を押す"],
                vec![
                    check(
                        "未保存の確認が表示された",
                        "いいえ",
                        flag(ui.get_confirm_discard()),
                    ),
                    check("編集が残る", super::EDITED_RANGE, clip(ui)),
                ],
            )
        }
        (1, 0) => {
            open_project(ui, editor, source, destination)?;
            input::click(ui, "play")?;
            (
                "再生を開始する",
                &["「再生」を押す"],
                vec![check("再生中", "はい", flag(ui.get_playing()))],
            )
        }
        (1, 1) => {
            input::click(ui, "play")?;
            (
                "再生を一時停止する",
                &["「一時停止」を押す"],
                vec![
                    check("再生中", "いいえ", flag(ui.get_playing())),
                    check(
                        "再生位置が先頭より進んだ",
                        "はい",
                        flag(editor.borrow().position > 0),
                    ),
                ],
            )
        }
        (1, 2) => {
            input::click(ui, "timeline")?;
            (
                "タイムラインの中央をクリックする",
                &["タイムラインを中央でクリックする"],
                vec![check(
                    "中央付近の再生位置",
                    "はい",
                    flag((29_800_000..30_200_000).contains(&editor.borrow().position)),
                )],
            )
        }
        (1, 3) => {
            let before = editor.borrow().position;
            input::key(ui, Key::RightArrow)?;
            let right = editor.borrow().position;
            input::key(ui, Key::LeftArrow)?;
            (
                "矢印キーで少しずつ移動する",
                &["右矢印、左矢印の順に押す"],
                vec![
                    check("右で先に進んだ", "はい", flag(right > before)),
                    check(
                        "左で元の位置に戻った",
                        "はい",
                        flag((editor.borrow().position - before).abs() < 100_000),
                    ),
                ],
            )
        }
        (1, 4) => {
            input::key(ui, Key::End)?;
            let end = editor.borrow().position;
            input::key(ui, Key::Home)?;
            (
                "EndとHomeで端に移動する",
                &["Endで末尾へ移動し、Homeで先頭へ戻る"],
                vec![
                    check("末尾の時刻", "60000000", end.to_string()),
                    check("先頭の時刻", "0", editor.borrow().position.to_string()),
                ],
            )
        }
        (1, 5) => {
            input::drag(ui, "timeline", "timeline-late")?;
            (
                "タイムラインをドラッグする",
                &["中央から約75%の位置までドラッグして離す"],
                vec![check(
                    "ドラッグ後の時刻が45秒付近",
                    "はい",
                    flag((44_800_000..45_200_000).contains(&editor.borrow().position)),
                )],
            )
        }
        (2, 0) => {
            open_project(ui, editor, source, destination)?;
            let srt = destination.with_file_name("ui-input.srt");
            fs::write(
                &srt,
                "1\n00:00:00,000 --> 00:00:02,000\n最初の字幕\n\n2\n00:00:03,000 --> 00:00:05,000\n次の字幕\n",
            )?;
            super::click_with_files(
                ui,
                editor,
                "import",
                [DialogAnswer::Confirm(true), DialogAnswer::File(Some(srt))],
            )?;
            (
                "SRTから字幕を取り込む",
                &["「SRT 読込」を押し、置き換えを許可してSRTを選ぶ"],
                vec![
                    check(
                        "読み込みを示すメッセージ",
                        "字幕を読み込みました",
                        ui.get_status(),
                    ),
                    check("字幕の行数", "2", ui.get_captions().row_count().to_string()),
                ],
            )
        }
        (2, 1) => {
            input::click(ui, "caption-first")?;
            let first = ui.get_selected();
            input::key(ui, Key::DownArrow)?;
            let down = ui.get_selected();
            input::key(ui, Key::UpArrow)?;
            let up = ui.get_selected();
            input::key(ui, Key::End)?;
            let last = ui.get_selected();
            input::key(ui, Key::Home)?;
            (
                "字幕の行をマウスとキーで選ぶ",
                &["先頭行をクリックし、下・上・End・Homeを押す"],
                vec![check(
                    "クリック・下・上・End・Homeで選択した行",
                    "0,1,0,1,0",
                    format!("{first},{down},{up},{last},{}", ui.get_selected()),
                )],
            )
        }
        (2, 2) => {
            input::key(ui, Key::End)?;
            input::key(ui, Key::Return)?;
            (
                "Enterで選択した字幕へ移動する",
                &["Endで最後の字幕を選び、Enterを押す"],
                vec![check(
                    "再生位置（マイクロ秒）",
                    "3000000",
                    editor.borrow().position.to_string(),
                )],
            )
        }
        (2, 3) => {
            input::replace(ui, "caption-speaker", "話者Z")?;
            input::replace(ui, "caption-body", "本文更新")?;
            let model = ui.get_captions();
            let first = model
                .row_data(0)
                .ok_or_else(|| anyhow::anyhow!("字幕1行目がありません"))?;
            let second = model
                .row_data(1)
                .ok_or_else(|| anyhow::anyhow!("字幕2行目がありません"))?;
            (
                "選択中の字幕の話者と本文を編集する",
                &["話者欄と本文欄に入力する"],
                vec![
                    check("選択行の話者", "話者Z", second.speaker),
                    check("選択行の本文", "本文更新", second.body),
                    check("別の行の本文", "最初の字幕", first.body),
                ],
            )
        }
        (2, 4) => {
            input::click(ui, "caption-first")?;
            input::click(ui, "caption-first")?;
            (
                "字幕の先頭行をダブルクリックする",
                &["先頭の字幕をダブルクリックする"],
                vec![
                    check("選択行", "0", ui.get_selected().to_string()),
                    check(
                        "字幕の開始位置に移動",
                        "0",
                        editor.borrow().position.to_string(),
                    ),
                ],
            )
        }
        (2, 5) => {
            super::click_with_files(ui, editor, "import", [DialogAnswer::Confirm(false)])?;
            let second = ui
                .get_captions()
                .row_data(1)
                .ok_or_else(|| anyhow::anyhow!("字幕2行目がありません"))?;
            (
                "字幕の置き換えを拒否する",
                &["SRT 読込を押し、確認では置き換えないを選ぶ"],
                vec![check("編集済みの字幕が残る", "本文更新", second.body)],
            )
        }
        (3, 0) => {
            open_project(ui, editor, source, destination)?;
            input::click(ui, "about")?;
            (
                "アプリ情報を開く",
                &["「このアプリについて」を押す"],
                vec![check(
                    "アプリ情報が開いている",
                    "はい",
                    flag(ui.invoke_report_panel("about".into())),
                )],
            )
        }
        (3, 1) => {
            input::click(ui, "about-close")?;
            input::click(ui, "ai")?;
            (
                "アプリ情報を閉じてAI編集を開く",
                &["「閉じる」を押す", "「AI 編集」を押す"],
                vec![
                    check(
                        "アプリ情報が開いている",
                        "いいえ",
                        flag(ui.invoke_report_panel("about".into())),
                    ),
                    check(
                        "AI編集が開いている",
                        "はい",
                        flag(ui.invoke_report_panel("ai".into())),
                    ),
                ],
            )
        }
        (3, 2) => {
            input::click(ui, "ai")?;
            input::click(ui, "export")?;
            (
                "AI編集を閉じて書き出し設定を開く",
                &["「AI 編集を閉じる」を押す", "「書き出す」を押す"],
                vec![
                    check(
                        "AI編集が開いている",
                        "いいえ",
                        flag(ui.invoke_report_panel("ai".into())),
                    ),
                    check(
                        "書き出し設定が画面に出ている",
                        "はい",
                        flag(export_panel_visible(ui)?),
                    ),
                ],
            )
        }
        (3, 3) => {
            super::click_with_files(ui, editor, "export-cancel", [])?;
            (
                "書き出し設定をキャンセルする",
                &["書き出し設定の「キャンセル」を押す"],
                vec![
                    check(
                        "書き出し設定が画面に出ている",
                        "いいえ",
                        flag(export_panel_visible(ui)?),
                    ),
                    check("書き出し処理中", "いいえ", flag(ui.get_exporting())),
                ],
            )
        }
        (3, 4) => {
            input::click(ui, "subtitle-style")?;
            input::replace(ui, "style-fill", "#invalid")?;
            input::click(ui, "style-apply")?;
            (
                "不正な字幕色を拒否する",
                &["文字色に不正な値を入力して適用する"],
                vec![check(
                    "不正な色に対するエラー",
                    "文字色を確認してください: #RRGGBB 形式で入力してください",
                    ui.get_subtitle_style_error(),
                )],
            )
        }
        (3, 5) => {
            input::click(ui, "style-cancel")?;
            (
                "字幕の変更をキャンセルする",
                &["キャンセルを押す"],
                vec![check(
                    "プレビューの文字色は白のまま",
                    "はい",
                    flag(
                        ui.get_caption_appearance().text_color
                            == slint::Color::from_rgb_u8(255, 255, 255),
                    ),
                )],
            )
        }
        (4, 0) => {
            let mut project = super::Project::load(source)?;
            project.proxy.clear();
            let loaded = super::Loaded::open(project, destination.to_owned())?;
            editor.borrow_mut().install(ui, loaded);
            let before = ui.get_has_proxy();
            let proxy = source.with_file_name("proxy.mp4");
            super::click_with_files(
                ui,
                editor,
                "choose-proxy",
                [DialogAnswer::File(Some(proxy.clone()))],
            )?;
            (
                "プロキシを選ぶ",
                &["「プロキシを選択」を押し、合成プロキシを選ぶ"],
                vec![
                    check("選択前はプロキシがない", "はい", flag(!before)),
                    check(
                        "プロジェクトに選択したファイルを設定した",
                        "はい",
                        flag(
                            editor
                                .borrow()
                                .loaded
                                .as_ref()
                                .is_some_and(|loaded| loaded.project.proxy == proxy),
                        ),
                    ),
                    check("プロキシを使う", "はい", flag(ui.get_use_proxy())),
                ],
            )
        }
        (4, 1 | 2) => {
            input::click(ui, "proxy")?;
            (
                "プロキシ使用を切り替える",
                &["「スクラブにプロキシを使用」のチェックを切り替える"],
                vec![check(
                    "プロキシを使う",
                    flag(stage == 2),
                    flag(ui.get_use_proxy()),
                )],
            )
        }
        (5, 0) => {
            open_project(ui, editor, source, destination)?;
            input::click(ui, "measurement")?;
            (
                "計測を開始する",
                &["「計測開始」を押す"],
                vec![check("計測中", "はい", flag(ui.get_measuring()))],
            )
        }
        (5, 1) => {
            input::click(ui, "timeline")?;
            input::click(ui, "measurement")?;
            (
                "計測を停止する",
                &["タイムラインを操作して「計測停止」を押す"],
                vec![check("計測中", "いいえ", flag(ui.get_measuring()))],
            )
        }
        (5, 2) => {
            let metrics = destination.with_file_name("gui-metrics.json");
            super::click_with_files(
                ui,
                editor,
                "metrics-save",
                [DialogAnswer::File(Some(metrics.clone()))],
            )?;
            let json: serde_json::Value = serde_json::from_reader(fs::File::open(metrics)?)?;
            (
                "計測結果をJSONに保存する",
                &["「計測保存」を押して保存先を指定する"],
                vec![
                    check(
                        "保存完了メッセージ",
                        "計測ファイルを保存しました",
                        ui.get_status(),
                    ),
                    check(
                        "計測イベントを含む",
                        "はい",
                        flag(
                            json["events"]
                                .as_array()
                                .is_some_and(|events| !events.is_empty()),
                        ),
                    ),
                ],
            )
        }
        (6, 0) => {
            open_project(ui, editor, source, destination)?;
            input::replace(ui, "clip-end", "1")?;
            input::click(ui, "clip-apply")?;
            input::click(ui, "export")?;
            input::click(ui, "export-quality")?;
            input::key(ui, Key::DownArrow)?;
            input::key(ui, Key::Return)?;
            input::click(ui, "export-canvas")?;
            input::key(ui, Key::DownArrow)?;
            input::key(ui, Key::DownArrow)?;
            input::key(ui, Key::Return)?;
            (
                "画質と縦長キャンバスを選ぶ",
                &["書き出すを押し、「高画質」と「縦長 9:16」を選ぶ"],
                vec![check(
                    "書き出し設定が開いている",
                    "はい",
                    flag(export_panel_visible(ui)?),
                )],
            )
        }
        (6, 1) => {
            let path = destination.with_file_name("ui-export.mp4");
            super::click_with_files(ui, editor, "export-save", [DialogAnswer::File(Some(path))])?;
            (
                "MP4の保存先を指定する",
                &["保存先を選ぶボタンを押し、保存先を指定する"],
                vec![
                    check("書き出しを開始した", "はい", flag(ui.get_exporting())),
                    check(
                        "高画質と縦長が書き出しに渡る",
                        "はい",
                        flag(editor.borrow().report_export_selection == Some((1, 2))),
                    ),
                ],
            )
        }
        (6, 2) => (
            "生成したMP4の寸法と音声を照合する",
            &["書き出しの完了を待つ"],
            Vec::new(),
        ),
        (6, 3) => {
            input::click(ui, "export")?;
            let unavailable = destination.with_file_name("missing-output").join("bad.mp4");
            super::click_with_files(
                ui,
                editor,
                "export-save",
                [DialogAnswer::File(Some(unavailable))],
            )?;
            (
                "失敗する保存先で書き出す",
                &["存在しないフォルダの保存先を選ぶ"],
                vec![check(
                    "書き出しを開始した",
                    "はい",
                    flag(ui.get_exporting()),
                )],
            )
        }
        (6, 4) => (
            "失敗を表示し、書き出し中の状態を解除する",
            &["終了を待つ"],
            Vec::new(),
        ),
        (6, 5) => {
            input::replace(ui, "clip-end", "2")?;
            input::click(ui, "clip-apply")?;
            (
                "書き出し失敗後も編集を続ける",
                &["終了を2秒に変え、区間を適用する"],
                vec![check(
                    "区間の変更が反映される",
                    "0.000000 – 2.000000 秒",
                    clip(ui),
                )],
            )
        }
        (7, 0 | 3) => {
            if stage == 0 {
                open_project(ui, editor, source, destination)?;
            }
            let files = transcription_files(destination, stage == 3)?;
            super::click_with_files(ui, editor, "transcription", files)?;
            (
                "合成エンジンで文字起こしを開始する",
                &["「文字起こし」を押し、テスト用のCLI、モデル、WAV、SRT保存先を選ぶ"],
                vec![check(
                    "文字起こしを開始した",
                    "はい",
                    flag(ui.get_transcribing()),
                )],
            )
        }
        (7, 1) => {
            input::click(ui, "transcription")?;
            (
                "文字起こしを中止する",
                &["「文字起こしを中止」を押す"],
                vec![check(
                    "中止を受け付けた",
                    "はい",
                    flag(ui.get_transcription_cancelling()),
                )],
            )
        }
        (7, 2) => (
            "中止が完了し出力が残らないことを確認する",
            &["中止の完了を待つ"],
            vec![check(
                "中止したSRTが存在する",
                "いいえ",
                flag(destination.with_file_name("ui-cancelled.srt").exists()),
            )],
        ),
        (8, 0) => {
            open_project(ui, editor, source, destination)?;
            input::click(ui, "ai")?;
            input::click(ui, "ai-send")?;
            (
                "空欄のままAI編集を送信できないことを確認する",
                &["AI 編集を開き、空のまま送信位置を押す"],
                vec![check(
                    "処理は始まらない",
                    "はい",
                    flag(!ui.get_ai_busy() && ui.get_ai_status().is_empty()),
                )],
            )
        }
        (8, 1) => {
            editor.borrow_mut().report_ai_delay = Some(std::time::Duration::from_millis(800));
            input::replace(ui, "ai-prompt", "場面を縦長・高画質にする")?;
            input::click(ui, "ai-send")?;
            (
                "入力した指示を合成AIに送信する",
                &["指示を入力して「送信」を押す"],
                vec![
                    check("AI処理中", "はい", flag(ui.get_ai_busy())),
                    check("中断できる", "はい", flag(ui.get_ai_cancellable())),
                ],
            )
        }
        (8, 2) => (
            "合成応答のクリップが画面に反映される",
            &["応答を待つ"],
            Vec::new(),
        ),
        (8, 3) => {
            input::click(ui, "ai-export")?;
            let options = ui.invoke_report_ai_export_options();
            (
                "AIが提案した書き出し設定を確認する",
                &["「書き出し設定を確認」を押す"],
                vec![
                    check(
                        "書き出し設定の表示",
                        "はい",
                        flag(export_panel_visible(ui)?),
                    ),
                    check(
                        "高画質と縦長が選択される",
                        "1,2",
                        format!("{},{}", options.quality, options.canvas),
                    ),
                    check("書き出しが始まっている", "いいえ", flag(ui.get_exporting())),
                ],
            )
        }
        (8, 4) => {
            super::click_with_files(ui, editor, "export-cancel", [])?;
            input::click(ui, "undo")?;
            editor.borrow_mut().report_ai_delay = Some(std::time::Duration::from_millis(1400));
            input::replace(ui, "ai-prompt", "あとで中断する")?;
            input::click(ui, "ai-send")?;
            (
                "提案を適用前に戻し、2回目の送信を始める",
                &["設定をキャンセルしてUndoし、新しい指示を送る"],
                vec![
                    check("元の区間になっている", super::ORIGINAL_RANGE, clip(ui)),
                    check("AI処理中", "はい", flag(ui.get_ai_busy())),
                ],
            )
        }
        (8, 5) => {
            input::click(ui, "ai-cancel")?;
            (
                "処理中のAIを中断する",
                &["「中断」を押す"],
                vec![check(
                    "中断の表示",
                    "Codex の処理を中断しました",
                    ui.get_ai_status(),
                )],
            )
        }
        (8, 6) => (
            "中断後に字幕と区間を保持する",
            &["終了を待って画面を確認する"],
            vec![
                check("素材区間", super::ORIGINAL_RANGE, clip(ui)),
                check(
                    "字幕の行数",
                    "10000",
                    ui.get_captions().row_count().to_string(),
                ),
            ],
        ),
        _ => anyhow::bail!("Unknown editor scenario {scenario} stage {stage}"),
    };
    Ok(Step {
        action,
        operations,
        checks,
        rationale: "ポインター・キー入力後の画面と、操作に伴う結果を照合する。",
        screenshot: String::new(),
        spec: Vec::new(),
    })
}

fn export_panel_visible(ui: &EditorWindow) -> Result<bool> {
    let image = ui.window().take_snapshot()?;
    let (width, height) = (image.width() as usize, image.height() as usize);
    ensure!(
        width >= 360 && height >= 224,
        "Window too small to inspect export options"
    );
    let pixel = image.as_slice()[(height / 2 - 93) * width + width / 2 + 159];
    Ok((pixel.r, pixel.g, pixel.b) == (0x25, 0x2e, 0x42))
}

fn transcription_files(destination: &Path, completes: bool) -> Result<[DialogAnswer; 4]> {
    let engine = destination.with_file_name(if completes {
        "ui-whisper-complete"
    } else {
        "ui-whisper-wait"
    });
    let model = destination.with_file_name("ui-model.bin");
    let audio = destination.with_file_name("ui-voice.wav");
    let output = destination.with_file_name(if completes {
        "ui-transcript.srt"
    } else {
        "ui-cancelled.srt"
    });
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let body = if completes {
            "#!/bin/sh\nprintf '%s\\n' '1' '00:00:00,000 --> 00:00:00,500' '合成文字起こし' > transcript.srt\n"
        } else {
            "#!/bin/sh\nexec /bin/sleep 30\n"
        };
        fs::write(&engine, body)?;
        fs::set_permissions(&engine, fs::Permissions::from_mode(0o700))?;
    }
    fs::write(&model, "test fixture")?;
    let mut wave = hound::WavWriter::create(
        &audio,
        hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )?;
    for _ in 0..8_000 {
        wave.write_sample(0_i16)?;
    }
    wave.finalize()?;
    Ok([engine, model, audio, output].map(|path| DialogAnswer::File(Some(path))))
}

pub(super) fn awaiting(editor: &Editor, scenario: usize, stage: usize) -> bool {
    match (scenario, stage) {
        (1, 0) => editor.position == 0,
        (6, 2 | 4) => editor.export_result.is_some(),
        (7, 2 | 3) => editor.transcription.is_some(),
        (8, 2 | 6) => editor.chat.is_running(),
        _ => false,
    }
}

pub(super) fn observe(
    ui: &EditorWindow,
    editor: &Editor,
    destination: &Path,
    scenario: usize,
    stage: usize,
    capture: &mut Step,
) {
    match (scenario, stage) {
        (1, 0) => capture.checks.push(check(
            "先頭から再生が進んだ",
            "はい",
            flag(editor.position > 0),
        )),
        (1, 5) => {
            let matching_frame = editor
                .last_frame
                .as_ref()
                .is_some_and(|frame| (frame.pts - editor.position).abs() <= 50_000);
            capture.checks.push(check(
                "表示中のフレームがドラッグ先に一致",
                "はい",
                flag(matching_frame),
            ));
        }
        (2, 3) => capture.checks.push(check(
            "プレビューは本文だけ",
            "本文更新",
            ui.get_preview_caption(),
        )),
        (6, 2) => {
            let path = destination.with_file_name("ui-export.mp4");
            let metadata = std::process::Command::new("ffprobe")
                .args([
                    "-v",
                    "error",
                    "-show_entries",
                    "stream=codec_type,width,height",
                    "-of",
                    "json",
                ])
                .arg(path)
                .output()
                .ok()
                .filter(|out| out.status.success())
                .and_then(|out| serde_json::from_slice::<serde_json::Value>(&out.stdout).ok());
            let streams = metadata
                .as_ref()
                .and_then(|data| data["streams"].as_array());
            let size_ok = streams.is_some_and(|streams| {
                streams
                    .iter()
                    .any(|stream| stream["width"] == 720 && stream["height"] == 1280)
            });
            let audio_ok = streams.is_some_and(|streams| {
                streams.iter().any(|stream| stream["codec_type"] == "audio")
            });
            capture.checks.extend([
                check("縦長の寸法720×1280", "はい", flag(size_ok)),
                check("音声ストリームを含む", "はい", flag(audio_ok)),
                check(
                    "書き出し完了の画面表示",
                    "はい",
                    flag(ui.get_status().starts_with("書き出しました:")),
                ),
            ]);
        }
        (6, 4) => capture.checks.extend([
            check(
                "画面に失敗理由が表示された",
                "はい",
                flag(ui.get_status().starts_with("書き出しに失敗しました:")),
            ),
            check("書き出しが終わった", "はい", flag(!ui.get_exporting())),
        ]),
        (7, 2) => capture.checks.push(check(
            "中止完了の画面表示",
            "文字起こしを中止しました",
            ui.get_status(),
        )),
        (7, 3) => {
            let text = fs::read_to_string(destination.with_file_name("ui-transcript.srt"))
                .unwrap_or_default();
            capture.checks.extend([
                check(
                    "SRTに合成の字幕がある",
                    "はい",
                    flag(text.contains("合成文字起こし")),
                ),
                check(
                    "SRT保存完了の画面表示",
                    "はい",
                    flag(ui.get_status().starts_with("SRT を生成しました:")),
                ),
            ]);
        }
        (8, 2) => capture.checks.extend([
            check("素材区間", "8.000000 – 11.000000 秒", clip(ui)),
            check(
                "書き出し設定の確認が可能",
                "はい",
                flag(ui.get_ai_export_ready()),
            ),
        ]),
        (8, 6) => capture
            .checks
            .push(check("AI処理中", "いいえ", flag(ui.get_ai_busy()))),
        _ => {}
    }
}
