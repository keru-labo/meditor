module ExportWorkflow {
  type Clip = { start: int, end: int }
  type Cue = { at: int, text: str }
  type Edit = { clips: List[Clip], caption: str, captionBackground: str, speakerBMuted: bool }
  type Settings = { directory: str, filename: str, quality: str, canvas: str,
    fit: bool, right: bool, background: str }
  type Job = { edit: Edit, settings: Settings }
  type FrameLayout = { width: int, height: int, cropX: int, cropWidth: int,
    shownWidth: int, shownHeight: int, padding: str }
  type Output = { path: { directory: str, filename: str }, clips: List[Clip], burned: List[Cue], audio: Set[str], crf: int,
    layout: FrameLayout, captionBackground: str }
  type State = { edit: Edit, proposal: List[Settings], dialog: List[Settings],
    job: List[Job], status: str, output: List[Output], error: str }

  var state: State

  pure def clip(first, last) = { start: first, end: last }
  pure val fullSource = [clip(0, 30)]
  pure val boss = [clip(8, 11)]
  pure val highlights = [clip(8, 11), clip(20, 23)]
  pure val trimmedWaits = [clip(0, 4), clip(8, 14), clip(20, 30)]
  pure val original = { clips: fullSource, caption: "boss commentary",
    captionBackground: "transparent", speakerBMuted: false }
  pure val defaults = { directory: "default", filename: "recording.mp4",
    quality: "standard", canvas: "source", fit: true, right: false, background: "black" }
  pure val portraitHigh = { ...defaults, quality: "high", canvas: "portrait" }
  pure val portraitStandard = { ...defaults, canvas: "portrait" }
  pure val custom = { ...portraitHigh, directory: "chosen", filename: "short.mp4" }
  pure val unavailable = { ...defaults, directory: "unwritable" }
  pure val blue = { ...portraitStandard, background: "blue" }
  pure val cropCenter = { ...portraitStandard, fit: false }
  pure val cropRight = { ...cropCenter, right: true }

  pure def proposedSettings(quality, canvas) = {
    ...defaults,
    quality: if (quality == "") defaults.quality else quality,
    canvas: if (canvas == "") defaults.canvas else canvas,
  }

  pure val initial = { edit: original, proposal: [], dialog: [], job: [],
    status: "idle", output: [], error: "" }
  action init = state' = initial

  pure def captions(edit) = {
    val sourceCues = [
      { at: 1, text: "preparation" },
      { at: 5, text: "waiting" },
      { at: 8, text: edit.caption },
      { at: 20, text: "second highlight" },
    ]
    edit.clips.foldl({ elapsed: 0, cues: [] }, (timeline, segment) => {
      val selected = sourceCues.foldl(timeline.cues, (cues, cue) =>
        if (segment.start <= cue.at and cue.at < segment.end)
          cues.append({ ...cue, at: timeline.elapsed + cue.at - segment.start })
        else cues)
      { elapsed: timeline.elapsed + segment.end - segment.start, cues: selected }
    }).cues
  }

  // Ideal rectangles for one 1280×720 source. Encoder pixel rounding is outside the model.
  pure def layout(settings) = {
    val portrait = settings.canvas == "portrait"
    val width = if (portrait) 720 else 1280
    val height = if (portrait) 1280 else 720
    val crop = portrait and not(settings.fit)
    val cropWidth = if (crop) width * 720 / height else 1280
    val shownHeight = if (crop) height else width * 720 / 1280
    {
      width: width,
      height: height,
      cropWidth: cropWidth,
      cropX: if (settings.right) 1280 - cropWidth else (1280 - cropWidth) / 2,
      shownWidth: width,
      shownHeight: shownHeight,
      padding: if (shownHeight < height) settings.background else "",
    }
  }

  pure def projectedSubject(frame, sourceX) = {
    if (sourceX < frame.cropX or sourceX >= frame.cropX + frame.cropWidth) -1
    else (sourceX - frame.cropX) * frame.shownWidth / frame.cropWidth
  }
  pure def render(job) = {
    path: { directory: job.settings.directory, filename: job.settings.filename },
    clips: job.edit.clips,
    burned: captions(job.edit),
    audio: if (job.edit.speakerBMuted) Set("game", "A") else Set("game", "A", "B"),
    crf: if (job.settings.quality == "high") 18 else 20,
    layout: layout(job.settings),
    captionBackground: job.edit.captionBackground,
  }
  pure def playedCaptions(output, _subtitleTracksEnabled) = output.burned

  action applyProposal(clips, settings) = all {
    state.status != "running",
    state' = { ...state, edit: { ...state.edit, clips: clips }, proposal: [settings], dialog: [] },
  }

  action changeCaption(text) =
    state' = { ...state, edit: { ...state.edit, caption: text } }

  action changeCaptionBackground(color) =
    state' = { ...state, edit: { ...state.edit, captionBackground: color } }

  action muteSpeakerB =
    state' = { ...state, edit: { ...state.edit, speakerBMuted: true } }

  action openProposal = all {
    state.status != "running",
    state.proposal.length() == 1,
    state' = { ...state, dialog: state.proposal },
  }

  action openManual = all {
    state.status != "running",
    state' = { ...state, dialog: [defaults] },
  }

  action updateDialog(settings) = all {
    state.dialog.length() == 1,
    state' = { ...state, dialog: [settings] },
  }

  action cancel = all {
    state.dialog.length() == 1,
    state' = { ...state, dialog: [] },
  }

  action undoProposal = all {
    state.proposal.length() == 1,
    state' = { ...state, edit: { ...state.edit, clips: fullSource }, proposal: [], dialog: [] },
  }

  action switchProject = all {
    state.status != "running",
    state' = initial,
  }

  action begin(edit, settings) = all {
    state.status != "running",
    state' = { ...state, edit: edit, job: [{ edit: edit, settings: settings }],
      status: "running", dialog: [], output: [], error: "" },
  }

  action automatic(clips, settings) = begin({ ...state.edit, clips: clips }, settings)

  action approve = all {
    state.dialog.length() == 1,
    begin(state.edit, state.dialog[0]),
  }

  action finish = all {
    state.status == "running",
    val job = state.job[0]
    if (job.settings.directory == "unwritable") {
      state' = { ...state, status: "failed", error: "destination not writable" }
    } else {
      state' = { ...state, status: "done", output: [render(job)] }
    },
  }

  val previewCaptions = captions(state.edit)
  val proposalVisible = state.proposal.length() == 1

  pure def validLayout(frame) =
    frame.shownWidth == frame.width and frame.shownHeight <= frame.height
      and frame.shownWidth * 720 == frame.shownHeight * frame.cropWidth
      and 0 <= frame.cropX and frame.cropX + frame.cropWidth <= 1280

  val workflowRules = all {
    state.proposal.length() <= 1,
    state.dialog.length() <= 1,
    state.job.length() <= 1,
    (state.status == "idle") == (state.job.length() == 0),
    (state.status == "done") == (state.output.length() == 1),
    (state.status == "failed") == (state.error != ""),
    (state.status == "running") implies state.dialog.length() == 0,
    state.job.foldl(true, (ok, job) => ok and validLayout(layout(job.settings))),
    (state.status == "done") implies state.output == state.job.foldl([], (outputs, job) => outputs.append(render(job))),
  }

  // Finite exploration of one project, one pending proposal and one tracked output.
  action step = any {
    applyProposal(boss, portraitHigh), automatic(boss, defaults), automatic(highlights, defaults),
    automatic(trimmedWaits, defaults), openManual, openProposal, cancel,
    updateDialog(custom), updateDialog(unavailable), updateDialog(portraitStandard),
    updateDialog(cropCenter), updateDialog(cropRight), updateDialog(blue), approve, finish,
    changeCaption("revised boss"), muteSpeakerB, undoProposal, switchProject,
  }

  pure def defaultBurnedOutput(output, clips, cues) =
    output.path == { directory: "default", filename: "recording.mp4" } and output.clips == clips
      and playedCaptions(output, false) == cues

  // BDD: video-export.feature:4 [14]
  run exportRequestedSceneTest = init.then(automatic(boss, defaults)).then(finish)
    .expect(defaultBurnedOutput(state.output[0], [clip(8, 11)],
      [{ at: 0, text: "boss commentary" }]) and workflowRules)
  // BDD: video-export.feature:4 [15]
  run exportRequestedHighlightsTest = init.then(automatic(highlights, defaults)).then(finish)
    .expect(defaultBurnedOutput(state.output[0], [clip(8, 11), clip(20, 23)],
      [{ at: 0, text: "boss commentary" }, { at: 3, text: "second highlight" }]) and workflowRules)
  // BDD: video-export.feature:4 [16]
  run exportWithoutWaitingTimeTest = init.then(automatic(trimmedWaits, defaults)).then(finish)
    .expect(defaultBurnedOutput(state.output[0], [clip(0, 4), clip(8, 14), clip(20, 30)],
      [{ at: 1, text: "preparation" }, { at: 4, text: "boss commentary" },
       { at: 10, text: "second highlight" }]) and workflowRules)

  // BDD: video-export.feature:18
  run confirmationUsesHumanChangesTest = init.then(applyProposal(boss, defaults))
    .expect(state.edit.clips == boss and proposalVisible and state.job.length() == 0)
    .then(changeCaption("revised boss")).then(muteSpeakerB)
    .then(openProposal).then(approve).then(finish)
    .expect(playedCaptions(state.output[0], false) == [{ at: 0, text: "revised boss" }]
      and state.output[0].audio == Set("game", "A") and workflowRules)

  // BDD: video-export.feature:27
  run oneOffSettingsDoNotChangeNextDefaultsTest = init.then(automatic(boss, custom)).then(finish)
    .expect(state.output[0].path == { directory: "chosen", filename: "short.mp4" } and state.output[0].crf == 18
      and state.output[0].layout.width == 720 and state.output[0].layout.height == 1280)
    .then(openManual).expect(state.dialog == [defaults] and workflowRules)

  // BDD: video-export.feature:33
  run failedWritePreservesEditsForRetryTest = init.then(applyProposal(boss, unavailable))
    .then(changeCaption("revised boss")).then(openProposal).then(approve).then(finish)
    .expect(state.error == "destination not writable" and state.output.length() == 0
      and state.edit.clips == boss and state.edit.caption == "revised boss")
    .then(openManual).then(updateDialog(custom)).then(approve).then(finish)
    .expect(state.output[0].path == { directory: "chosen", filename: "short.mp4" } and state.output[0].clips == boss
      and state.output[0].burned == [{ at: 0, text: "revised boss" }] and workflowRules)

  // BDD: video-export.feature:39
  run activeExportKeepsItsSnapshotTest = init.then(automatic(boss, defaults))
    .then(changeCaption("revised boss")).then(finish)
    .expect(state.output[0].burned == [{ at: 0, text: "boss commentary" }]
      and state.edit.caption == "revised boss")
    .then(openManual).then(approve).then(finish)
    .expect(state.output[0].burned == [{ at: 0, text: "revised boss" }] and workflowRules)

  // BDD: video-export.feature:45
  run portraitHighQualityFitsAgainstBlackTest = init.then(openManual)
    .then(updateDialog(portraitHigh)).then(approve).then(finish)
    .expect(state.output[0].layout.width == 720 and state.output[0].layout.height == 1280
      and state.output[0].layout.cropWidth == 1280 and state.output[0].layout.shownHeight < 1280
      and state.output[0].layout.padding == "black" and state.output[0].crf == 18 and workflowRules)

  // BDD: video-export.feature:51
  run fillsPortraitAndMovesSubjectIntoFrameTest = init.then(openManual)
    .then(updateDialog(cropCenter)).expect(projectedSubject(layout(state.dialog[0]), 1050) == -1)
    .then(updateDialog(cropRight))
    .expect(projectedSubject(layout(state.dialog[0]), 1050) == 311)
    .then(approve).then(finish)
    .expect(projectedSubject(state.output[0].layout, 1050) == 311
      and state.output[0].layout.shownHeight == state.output[0].layout.height
      and state.output[0].layout.padding == "" and workflowRules)

  // BDD: video-export.feature:57
  run blueOnlyFillsThePaddingTest = init.then(changeCaptionBackground("translucent white"))
    .then(openManual).then(updateDialog(blue))
    .expect(layout(state.dialog[0]).cropWidth == 1280 and layout(state.dialog[0]).padding == "blue")
    .then(approve).then(finish)
    .expect(state.output[0].layout == layout(blue) and state.output[0].layout.padding == "blue"
      and state.output[0].captionBackground == "translucent white" and workflowRules)

  // BDD: video-export.feature:63
  run aiSelectionCanBeReviewedAndRevisedTest = init.then(applyProposal(boss, proposedSettings("high", "portrait")))
    .expect(state.edit.clips == [clip(8, 11)] and previewCaptions == [{ at: 0, text: "boss commentary" }]
      and state.job.length() == 0 and state.output.length() == 0)
    .then(changeCaption("revised boss")).then(openProposal)
    .expect(state.dialog[0].quality == "high" and state.dialog[0].canvas == "portrait")
    .then(updateDialog({ ...portraitStandard, directory: "chosen" })).then(approve).then(finish)
    .expect(state.output[0].path == { directory: "chosen", filename: "recording.mp4" } and state.output[0].crf == 20
      and state.output[0].layout.width == 720 and state.output[0].layout.height == 1280
      and state.output[0].clips == [clip(8, 11)]
      and state.output[0].burned == [{ at: 0, text: "revised boss" }] and workflowRules)

  // BDD: video-export.feature:76
  run cancelledProposalKeepsClipButDoesNotChangeManualDefaultsTest =
    init.then(applyProposal(boss, proposedSettings("", "portrait"))).then(openProposal)
    .expect(state.dialog[0].quality == "standard" and state.dialog[0].canvas == "portrait")
    .then(cancel).expect(state.output.length() == 0 and state.job.length() == 0 and state.edit.clips == boss)
    .then(openManual).expect(state.dialog == [defaults] and workflowRules)

  // BDD: video-export.feature:85 [92]
  run undoingClipHidesItsProposalTest = init.then(applyProposal(boss, portraitHigh))
    .expect(proposalVisible)
    .then(undoProposal).expect(not(proposalVisible) and state.edit.clips == fullSource and workflowRules)
  // BDD: video-export.feature:85 [93]
  run switchingProjectHidesTheOldProposalTest = init.then(applyProposal(boss, portraitHigh))
    .expect(proposalVisible)
    .then(switchProject).expect(not(proposalVisible) and state.edit == original and workflowRules)
}
