// Timeline editing and render plans over small source and timeline intervals.
// A video sample is a source identifier/time, not a decoded image.
module VideoTimeline {
  type Clip = { source: str, first: int, last: int, start: int }
  type Media = { id: str, first: int, last: int }
  type Project = { clips: List[Clip], voice: int, subtitle: int, effect: int }
  type State = {
    fixture: str, project: Project, undo: List[Project], position: int,
    corner: bool, speed: int, fade: int, assets: Set[Media],
  }

  var state: State

  pure def clip(source, first, last, start) =
    { source: source, first: first, last: last, start: start }
  pure def project(clips, voice, subtitle, effect) =
    { clips: clips, voice: voice, subtitle: subtitle, effect: effect }
  pure def clipEnd(entry) = entry.start + entry.last - entry.first
  pure def active(entry, second) = entry.start <= second and second < clipEnd(entry)
  pure def sourceAt(entry, second) = entry.first + second - entry.start
  pure def positionClip(entry, second) = { ...entry, start: second }

  action start(fixture, projectValue) = state' = {
    fixture: fixture, project: projectValue, undo: [], position: 0,
    corner: false, speed: 1, fade: 0,
    assets: projectValue.clips.foldl(Set(), (assets, entry) =>
      assets.union(Set({ id: entry.source, first: 0, last: entry.last }))),
  }
  action startLayers = start("layers", project(
    [clip("game", 30, 40, 10), clip("face", 0, 4, 12)], 13, 13, 0))
  action startSplit = start("split", project([clip("game", 30, 40, 10)], 0, 0, 0))
  action startDelete = start("delete", project(
    [clip("remove", 10, 13, 2), clip("follow", 13, 17, 5)], 5, 5, 7))
  action startSnap = start("snap", project(
    [clip("first", 0, 2, 2), clip("second", 4, 6, 7), clip("anchor", 0, 2, 10)], 0, 0, 0))
  action startSpeed = start("speed", project([clip("game", 0, 10, 0)], 4, 4, 0))
  action startFade = start("fade", project(
    [clip("first", 0, 6, 0), clip("second", 10, 16, 6)], 0, 0, 0))

  action init = any { startLayers, startSplit, startDelete, startSnap, startSpeed, startFade }

  val clips = state.project.clips
  def sourceAvailable(entry) = state.assets.exists(asset =>
    asset.id == entry.source and asset.first <= entry.first and entry.last <= asset.last)
  action seek(second) = state' = { ...state, position: second }
  action putUpperRight = state' = { ...state, corner: true }

  def compositeAt(second) = {
    val lower = clips[0]
    val upper = clips[1]
    val hasLower = active(lower, second)
    val hasUpper = state.corner and active(upper, second)
    {
      outside: if (hasLower) lower.source else "",
      topRight: if (hasUpper) upper.source else if (hasLower) lower.source else "",
      videoTime: if (hasLower) sourceAt(lower, second) else -1,
      insetTime: if (hasUpper) sourceAt(upper, second) else -1,
      gameAudioTime: if (hasLower) sourceAt(lower, second) else -1,
      voiceTime: if (state.project.voice <= second and hasLower)
        33 + second - state.project.voice else -1,
      visibleSubtitle: state.project.subtitle == second and hasLower,
    }
  }

  val playbackFrame = compositeAt(state.position)
  def exportComposite(second) = compositeAt(second)

  action splitAtPlayhead = if (clips.length() == 1) {
    val entry = clips[0]
    val boundary = sourceAt(entry, state.position)
    all {
      entry.first < boundary,
      boundary < entry.last,
      state' = { ...state, project: { ...state.project,
        clips: [{ ...entry, last: boundary },
          { ...entry, first: boundary, start: state.position }] } },
    }
  } else all { false, state' = state }

  action trimLaterThenMoveFirst(sourceEnd) = if (clips.length() == 2) {
    val earlier = clips[0]
    val later = clips[1]
    val trimmed = { ...later, last: sourceEnd, start: earlier.start }
    all {
      earlier.first == 30,
      later.first < sourceEnd,
      sourceEnd <= later.last,
      state' = { ...state, project: { ...state.project,
        clips: [trimmed, positionClip(earlier, clipEnd(trimmed))] } },
    }
  } else all { false, state' = state }

  action deleteFirst(ripple) = if (clips.length() == 2) {
    val removed = clips[0]
    val following = clips[1]
    val advance = if (ripple) removed.last - removed.first else 0
    state' = { ...state, undo: [state.project], project: {
      ...state.project,
      clips: [positionClip(following, following.start - advance)],
      voice: state.project.voice - advance,
      subtitle: state.project.subtitle - advance,
    } }
  } else all { false, state' = state }

  action undoDelete = all {
    state.undo.length() == 1,
    state' = { ...state, project: state.undo[0], undo: [] },
  }

  action moveGroup(target, snap) = {
    val first = clips[0]
    val second = clips[1]
    val anchor = clips[2]
    val near = target - anchor.start >= -1 and target - anchor.start <= 1
    val landing = if (snap and near) anchor.start else target
    val delta = landing - first.start
    state' = { ...state, project: { ...state.project,
      clips: [positionClip(first, landing), positionClip(second, second.start + delta), anchor] } }
  }

  action setSpeed(rate) = state' = { ...state, speed: rate }
  def speedPlan = {
    val entry = clips[0]
    {
      duration: (entry.last - entry.first) / state.speed,
      speed: state.speed,
      frameAtCue: entry.start + (4 - entry.first) / state.speed,
      voiceAtCue: entry.start + (state.project.voice - entry.first) / state.speed,
      subtitleAtCue: entry.start + (state.project.subtitle - entry.first) / state.speed,
    }
  }
  val previewSpeed = speedPlan
  val exportSpeed = speedPlan

  action setFade(seconds) = all {
    seconds <= clips[0].last - clips[0].first,
    seconds <= clips[1].first,
    state' = { ...state, fade: seconds },
  }

  // Half-second ticks preserve the midpoint in the finite one-second fixture.
  pure def sourceTickAt(entry, tick) = entry.first * 2 + tick - entry.start * 2

  def blendAt(tick) = {
    val outgoingClip = clips[0]
    val incomingClip = clips[1]
    val end = incomingClip.start * 2
    val ticks = state.fade * 2
    val start = end - ticks
    val outgoingSample = { source: outgoingClip.source, at: sourceTickAt(outgoingClip, tick) }
    val incomingSample = { source: incomingClip.source, at: sourceTickAt(incomingClip, tick) }
    val absent = { source: "", at: -1 }
    if (tick < start)
      { outgoing: 1, incoming: 0, scale: 1, from: outgoingSample, into: absent }
    else if (tick >= end)
      { outgoing: 0, incoming: 1, scale: 1, from: absent, into: incomingSample }
    else { outgoing: end - tick, incoming: tick - start, scale: ticks,
      from: outgoingSample, into: incomingSample }
  }
  def previewBlend(tick) = blendAt(tick)
  def exportBlend(tick) = blendAt(tick)

  def validBlendSample(sample, weight) =
    if (sample.source == "") weight == 0
    else state.assets.exists(asset =>
      sample.source == asset.id and asset.first * 2 <= sample.at and sample.at < asset.last * 2)

  action step = any {
    all { state.fixture == "layers", any { putUpperRight, seek(11), seek(13), seek(16) } },
    all { state.fixture == "split", any { seek(12), seek(14), splitAtPlayhead, trimLaterThenMoveFirst(38) } },
    all { state.fixture == "delete", any { deleteFirst(false), deleteFirst(true), undoDelete } },
    all { state.fixture == "snap", any { moveGroup(9, true), moveGroup(9, false), moveGroup(8, true) } },
    all { state.fixture == "speed", any { setSpeed(1), setSpeed(2) } },
    all { state.fixture == "fade", any { setFade(0), setFade(1) } },
  }

  val workflowRules = all {
    clips.indices().forall(index =>
      clips[index].first < clips[index].last and clips[index].start >= 0 and sourceAvailable(clips[index])),
    state.undo.length() <= 1,
    (state.fixture == "split" and clips.length() == 2) implies all {
      clips[0].source == "game" and clips[1].source == "game",
      clipEnd(clips[0]) == clips[1].start,
      clips[0].first == clips[1].last or clips[0].last == clips[1].first,
      clips[0].first >= 30 and clips[1].first >= 30,
      clips[0].last <= 40 and clips[1].last <= 40,
    },
    (state.fixture == "delete" and clips.length() == 1) implies all {
      state.project.voice == clips[0].start,
      state.project.subtitle == clips[0].start,
      state.project.effect == 7,
      Set(2, 5).contains(clips[0].start),
    },
    (state.fixture == "snap") implies all {
      clips[1].start - clips[0].start == 5,
      clips[2].start == 10,
    },
    (state.fixture == "speed") implies all {
      previewSpeed.duration * state.speed == 10,
      previewSpeed.frameAtCue == previewSpeed.voiceAtCue,
      previewSpeed.frameAtCue == previewSpeed.subtitleAtCue,
    },
    (state.fixture == "fade") implies Set(9, 10, 11, 12, 13).forall(tick => {
      val blend = previewBlend(tick)
      val following = previewBlend(tick + 1)
      all {
        blend.scale > 0 and blend.outgoing >= 0 and blend.incoming >= 0,
        blend.outgoing + blend.incoming == blend.scale,
        validBlendSample(blend.from, blend.outgoing),
        validBlendSample(blend.into, blend.incoming),
        blend.from.source == "" or blend.from.source == clips[0].source,
        blend.into.source == "" or blend.into.source == clips[1].source,
        blend.incoming * following.scale <= following.incoming * blend.scale,
      }
    }),
  }

  // BDD: video-timeline.feature:4
  run compositesInsetWithSynchronizedTracksTest = startLayers.then(putUpperRight).then(seek(13))
    .expect(all {
      playbackFrame == { outside: "game", topRight: "face", videoTime: 33, insetTime: 1,
        gameAudioTime: 33, voiceTime: 33, visibleSubtitle: true },
      exportComposite(13) == playbackFrame,
    })

  // BDD: video-timeline.feature:12
  run splitsTrimsAndReordersWithoutChangingSourceTest = startSplit.then(seek(14)).then(splitAtPlayhead)
    .expect(clips == [clip("game", 30, 34, 10), clip("game", 34, 40, 14)])
    .then(trimLaterThenMoveFirst(38))
    .expect(clips == [clip("game", 34, 38, 10), clip("game", 30, 34, 14)]
      and sourceAvailable(clip("game", 30, 40, 10)))

  // BDD: video-timeline.feature:20 [30]
  run deletesKeepingGapAndUndoesTest = startDelete.then(deleteFirst(false))
    .expect(clips == [clip("follow", 13, 17, 5)]
      and state.project.voice == 5 and state.project.subtitle == 5 and state.project.effect == 7)
    .then(undoDelete)
    .expect(state.project == project([clip("remove", 10, 13, 2), clip("follow", 13, 17, 5)], 5, 5, 7))

  // BDD: video-timeline.feature:20 [31]
  run rippleDeletesAndUndoesTest = startDelete.then(deleteFirst(true))
    .expect(clips == [clip("follow", 13, 17, 2)]
      and state.project.voice == 2 and state.project.subtitle == 2 and state.project.effect == 7)
    .then(undoDelete)
    .expect(state.project == project([clip("remove", 10, 13, 2), clip("follow", 13, 17, 5)], 5, 5, 7))

  // BDD: video-timeline.feature:33
  run snapsGroupAndCanMoveFreelyTest = startSnap.then(moveGroup(9, true))
    .expect(clips[0].start == 10 and clips[1].start == 15 and clips[2].start == 10)
    .then(moveGroup(9, false))
    .expect(clips[0].start == 9 and clips[1].start == 14 and clips[2].start == 10)

  // BDD: video-timeline.feature:40
  run doublesSpeedAndPreservesSynchronizationTest = startSpeed.then(setSpeed(2))
    .expect(previewSpeed == {
      duration: 5, speed: 2, frameAtCue: 2, voiceAtCue: 2, subtitleAtCue: 2,
    } and exportSpeed == previewSpeed)

  // BDD: video-timeline.feature:48
  run crossfadesForOneSecondTest = startFade.then(setFade(1))
    .expect(all {
      previewBlend(10) == { outgoing: 2, incoming: 0, scale: 2,
        from: { source: "first", at: 10 }, into: { source: "second", at: 18 } },
      previewBlend(11) == { outgoing: 1, incoming: 1, scale: 2,
        from: { source: "first", at: 11 }, into: { source: "second", at: 19 } },
      previewBlend(12) == { outgoing: 0, incoming: 1, scale: 1,
        from: { source: "", at: -1 }, into: { source: "second", at: 20 } },
      Set(10, 11, 12).forall(tick => exportBlend(tick) == previewBlend(tick)),
    })
}
