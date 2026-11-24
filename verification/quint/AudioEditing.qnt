module AudioEditing {
  type GainEdit = { track: str, beforeDb: int, afterDb: int }
  type State = {
    clipPlaced: bool,
    clipGainPct: int,
    fade: bool,
    hasB: bool,
    muteB: bool,
    soloA: bool,
    trackGainDb: int,
    gainHistory: List[GainEdit],
  }

  var state: State

  // Time is expressed in half-second ticks and levels are representative linear values.
  pure val sourceLength = 12
  pure val sourceStart = 2
  pure val sourceEnd = 8
  pure val placement = 4
  pure val clipEnd = placement + sourceEnd - sourceStart
  pure val gameEnd = 20
  pure val videoEnd = 8
  pure val fadeLength = 2

  action init = state' = {
    clipPlaced: false,
    clipGainPct: 100,
    fade: false,
    hasB: false,
    muteB: false,
    soloA: false,
    trackGainDb: 0,
    gainHistory: [],
  }

  action placeClip = state' = { ...state, clipPlaced: true }

  action setClipMix(gain, fade) = all {
    state.clipPlaced,
    state' = { ...state, clipGainPct: gain, fade: fade },
  }

  action addSpeakerB = state' = { ...state, hasB: true }

  action setMuteB(muted) = all {
    state.hasB,
    state' = { ...state, muteB: muted },
  }

  action setSoloA(solo) = all {
    state.clipPlaced,
    state' = { ...state, soloA: solo },
  }

  action chatLowerTrackGain = all {
    state.trackGainDb == 0,
    val edit = { track: "speaker-a", beforeDb: state.trackGainDb, afterDb: -6 }
    state' = { ...state, trackGainDb: edit.afterDb, gainHistory: [edit] },
  }

  action guiUndoTrackGain = all {
    state.gainHistory.length() == 1,
    state' = { ...state, trackGainDb: state.gainHistory[0].beforeDb, gainHistory: [] },
  }

  pure def sourceAt(tick) = sourceStart + tick - placement

  pure def clipActive(tick) =
    placement <= tick and sourceStart <= sourceAt(tick) and sourceAt(tick) < sourceEnd

  def speakerALevel(tick) = {
    val elapsed = tick - placement
    val remaining = clipEnd - tick
    val edgeDistance = if (elapsed < remaining) elapsed else remaining
    val fadeScale = if (state.fade and edgeDistance < fadeLength) edgeDistance else fadeLength
    val trackScale = if (state.trackGainDb == 0) 100 else 50
    if (state.clipPlaced and clipActive(tick))
      (state.clipGainPct * trackScale * fadeScale) / (100 * fadeLength)
    else 0
  }

  def mix(tick, preview) = {
    val solo = preview and state.soloA
    val gameActive = 0 <= tick and tick < gameEnd
    val game = if (gameActive and not(solo)) 100 else 0
    val voiceA = speakerALevel(tick)
    val voiceB = if (gameActive and state.hasB and not(state.muteB or solo)) 80 else 0
    { game: game, voiceA: voiceA, voiceB: voiceB, mixed: game + voiceA + voiceB,
      background: tick >= videoEnd }
  }

  action step = any {
    placeClip,
    setClipMix(50, true),
    setClipMix(100, false),
    addSpeakerB,
    setMuteB(true),
    setMuteB(false),
    setSoloA(true),
    setSoloA(false),
    chatLowerTrackGain,
    guiUndoTrackGain,
  }

  val visibleTrackGainDb = state.trackGainDb
  val visibleGainHistory = state.gainHistory

  val workflowRules = all {
    0 <= sourceStart and sourceStart < sourceEnd and sourceEnd <= sourceLength,
    state.gainHistory.length() <= 1,
    (state.trackGainDb == -6) == (state.gainHistory.length() == 1),
    (state.gainHistory.length() == 1) implies all {
      state.gainHistory[0].track == "speaker-a",
      state.gainHistory[0].beforeDb == 0,
      state.gainHistory[0].afterDb == visibleTrackGainDb,
    },
    state.muteB implies state.hasB,
    (state.fade or state.soloA) implies state.clipPlaced,
    Set(0, 3, 4, 5, 6, 8, 9, 10, 19, 20).forall(tick => {
      val preview = mix(tick, true)
      val output = mix(tick, false)
      all {
        (not(state.clipPlaced) or not(clipActive(tick))) implies output.voiceA == 0,
        (0 <= tick and tick < gameEnd) implies output.game == 100,
        state.muteB implies (preview.voiceB == 0 and output.voiceB == 0),
        state.soloA implies (preview.game == 0 and preview.voiceB == 0),
        not(state.soloA) implies preview == output,
        preview.voiceA == output.voiceA,
      }
    }),
  }

  // BDD: audio-editing.feature:4
  run placementAndTrimTest = init.then(placeClip)
    .expect(sourceLength == 12 and sourceAt(4) == 2 and sourceAt(9) == 7
      and mix(3, true).game == 100 and mix(3, true).voiceA == 0
      and mix(4, true).mixed == 200 and not(mix(4, true).background)
      and mix(8, true).mixed == 200 and mix(8, true).background
      and mix(9, true).mixed == 200 and mix(9, true).background
      and mix(10, true).game == 100 and mix(10, true).voiceA == 0)

  // BDD: audio-editing.feature:13
  run independentGainFadesAndLevelsTest = init.then(placeClip).then(setClipMix(50, true))
    .expect(mix(4, true).voiceA == 0 and mix(5, true).voiceA == 25
      and mix(6, true).voiceA == 50 and mix(9, true).voiceA == 25
      and mix(10, true).voiceA == 0
      and mix(5, true).game == 100 and mix(5, true).mixed == 125
      and Set(4, 5, 6, 8, 9, 10).forall(tick => mix(tick, true) == mix(tick, false)))

  // BDD: audio-editing.feature:21
  run soloIsPreviewOnlyTest = init.then(placeClip).then(addSpeakerB).then(setMuteB(true))
    .then(setSoloA(true))
    .expect(mix(6, true).game == 0 and mix(6, true).voiceA == 100 and mix(6, true).voiceB == 0
      and mix(6, false).game == 100 and mix(6, false).voiceA == 100 and mix(6, false).voiceB == 0)
    .then(setSoloA(false))
    .expect(mix(6, true).game == 100 and mix(6, true).voiceA == 100 and mix(6, true).voiceB == 0)

  // BDD: audio-editing.feature:30
  run chatGainAndGuiUndoTest = init.then(placeClip).then(addSpeakerB)
    .expect(visibleTrackGainDb == 0 and mix(6, true).voiceA == 100)
    .then(chatLowerTrackGain)
    .expect(visibleTrackGainDb == -6
      and visibleGainHistory == [{ track: "speaker-a", beforeDb: 0, afterDb: -6 }]
      and mix(6, true).voiceA == 50 and mix(6, true).game == 100 and mix(6, true).voiceB == 80)
    .then(guiUndoTrackGain)
    .expect(visibleTrackGainDb == 0 and visibleGainHistory == [] and mix(6, true).voiceA == 100
      and mix(6, true).game == 100 and mix(6, true).voiceB == 80)
}
